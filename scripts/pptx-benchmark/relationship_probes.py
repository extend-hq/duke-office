import argparse
import hashlib
import json
from pathlib import Path
import posixpath
import zipfile
from xml.etree import ElementTree as ET

from artwork_math_gate import artwork_hashes
from comments_probes import package
from feedback import OUT, invoke as compact
from object_gate import invoke

RELS = 'http://schemas.openxmlformats.org/package/2006/relationships'
OFFICE = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships/'


def rel_part(part):
    return posixpath.join(posixpath.dirname(part), '_rels', posixpath.basename(part) + '.rels')


def source_part(path):
    if path == '_rels/.rels':
        return ''
    return posixpath.join(posixpath.dirname(posixpath.dirname(path)), posixpath.basename(path)[:-5])


def resolve(part, target):
    return posixpath.normpath(posixpath.join(posixpath.dirname(part), target)).lstrip('/')


def write_package(path, parts):
    with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
        for name, value in sorted(parts.items()):
            info = zipfile.ZipInfo(name, (2000, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, value)


def relocate(source, destination, mode):
    with zipfile.ZipFile(source) as archive:
        parts = {name: archive.read(name) for name in archive.namelist() if not name.endswith('/')}
    mapping = {}
    for index, name in enumerate(sorted(parts)):
        if name == '[Content_Types].xml' or name.endswith('.rels'):
            continue
        selected = (mode != 'root_only' or name == 'ppt/presentation.xml') and (mode != 'related_only' or name != 'ppt/presentation.xml')
        mapping[name] = f'parts/{index:05}/{posixpath.basename(name)}' if selected else name
    if '_rels/.rels' not in parts:
        root = ET.Element(f'{{{RELS}}}Relationships')
        ET.SubElement(root, f'{{{RELS}}}Relationship', Id='main', Type=OFFICE + 'officeDocument', Target='ppt/presentation.xml')
        parts['_rels/.rels'] = ET.tostring(root)
    moved = {}; edges = 0
    for name, data in parts.items():
        if name.endswith('.rels'):
            owner = source_part(name); next_owner = mapping.get(owner, owner)
            root = ET.fromstring(data)
            for relation in root:
                if relation.get('TargetMode') == 'External' or not relation.get('Target'):
                    continue
                old = resolve(owner, relation.get('Target'))
                target = mapping.get(old, old)
                if mode == 'absolute':
                    relative = '/' + target
                else:
                    relative = posixpath.relpath(target, posixpath.dirname(next_owner) or '.')
                    relative = './' + relative
                relation.set('Target', relative)
                assert resolve(next_owner, relative) == target
                edges += 1
            moved[rel_part(next_owner)] = ET.tostring(root)
        elif name == '[Content_Types].xml':
            root = ET.fromstring(data)
            for node in root:
                old = node.get('PartName', '').lstrip('/')
                if old in mapping:
                    node.set('PartName', '/' + mapping[old])
            moved[name] = ET.tostring(root)
        else:
            moved[mapping[name]] = data
    write_package(destination, moved)
    assert all(parts[old] == moved[new] for old, new in mapping.items())
    return mapping, dict(parts=len(mapping), relationship_edges=edges)


def canonical(value, reverse):
    if isinstance(value, dict):
        return {canonical(k, reverse): canonical(v, reverse) for k, v in value.items()}
    if isinstance(value, list):
        return [canonical(v, reverse) for v in value]
    if isinstance(value, str):
        for new, old in reverse:
            value = value.replace(new, old)
    return value


def differences(before, after, at='$'):
    if type(before) != type(after):
        return [at + ':type']
    if isinstance(before, dict):
        result = [at + ':keys'] if before.keys() != after.keys() else []
        for key in before.keys() & after.keys():
            result.extend(differences(before[key], after[key], at + '.' + key))
        return result[:30]
    if isinstance(before, list):
        result = [at + ':length'] if len(before) != len(after) else []
        for i, (a, b) in enumerate(zip(before, after)):
            result.extend(differences(a, b, f'{at}[{i}]'))
        return result[:30]
    return [] if before == after else [at]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    directory = run / 'relationship-probes'; directory.mkdir()
    binary = run / 'candidate'; baseline = OUT / 'experiments/comments3/candidate'
    corpus = {r['id']: r for r in json.loads((OUT / 'bom-source-cohort.json').read_text())}
    selected = [corpus[k] for k in ['7cad26da3b390ba3', '78d5f10913eb67c8', '8c5c47e428ee2eaa', 'ded7504bedae9311']]
    for strict in [False, True]:
        path = directory / ('strict-source.pptx' if strict else 'transitional-source.pptx')
        package(path, strict=strict)
        selected.append(dict(id='strict' if strict else 'transitional', path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
    receipts, manifest, records = [], [], []
    for source in selected:
        assert hashlib.sha256(Path(source['path']).read_bytes()).hexdigest() == source['sha256']
        old, before = invoke(baseline, source, directory / 'before.json.gz')
        current, original = invoke(binary, source, directory / 'original.json.gz')
        assert old['status'] == current['status'] == 'ok'
        assert before == original, source['id']
        for mode in ['root_only', 'related_only', 'relative', 'absolute']:
            name = source['id'] + '-' + mode
            path = directory / (name + '.pptx')
            mapping, counts = relocate(source['path'], path, mode)
            row = dict(id=name, path=str(path.resolve()), sha256=hashlib.sha256(path.read_bytes()).hexdigest())
            manifest.append(row)
            old, _ = invoke(baseline, row, directory / 'relocated-before.json.gz')
            current, after = invoke(binary, row, directory / 'after.json.gz')
            reverse = sorted(((v, k) for k, v in mapping.items() if k != v), key=lambda pair: -len(pair[0]))
            issues = differences(before, canonical(after, reverse)) if current['status'] == 'ok' else ['status']
            passed = not issues
            receipt = dict(id=name, passed=passed, source_id=source['id'], source_sha256=source['sha256'], input_sha256=row['sha256'], counts=counts, baseline_status=old['status'], candidate_status=current['status'], issues=issues)
            receipts.append(receipt)
            print(name, passed, issues, flush=True)
            record = compact(binary, 'duke-slides', row, directory / (name + '.compact.json.gz'), 0)
            record.update(id=name)
            if current['status'] == 'ok':
                assert record['full_sha256'] == current['full_sha256']
                record['artwork_hashes'] = artwork_hashes(after)
            records.append(record)
    base_path = directory / 'guard-source.pptx'
    package(base_path)
    with zipfile.ZipFile(base_path) as archive:
        parts = {name: archive.read(name) for name in archive.namelist()}
    slide_rel = 'ppt/slides/_rels/slide1.xml.rels'
    parts[slide_rel] = ET.tostring(ET.Element(f'{{{RELS}}}Relationships'))
    write_package(base_path, parts)
    _, expected = invoke(binary, dict(path=str(base_path)), directory / 'guard-source.json.gz')
    p = 'http://schemas.openxmlformats.org/presentationml/2006/main'
    a = 'http://schemas.openxmlformats.org/drawingml/2006/main'
    guards = {
        'slideLayout': ('ppt/slideLayouts/guard.xml', f'<p:sldLayout xmlns:p="{p}"><p:cSld><p:spTree/></p:cSld></p:sldLayout>'),
        'notesSlide': ('ppt/notesSlides/guard.xml', f'<p:notes xmlns:p="{p}" xmlns:a="{a}"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>Guard text</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:notes>'),
        'comments': ('ppt/comments/guard.xml', f'<p:cmLst xmlns:p="{p}"><p:cm authorId="1" idx="1"><p:text>Guard text</p:text></p:cm></p:cmLst>'),
    }
    for kind, (target, data) in guards.items():
        for mode in ['external', 'foreign_type']:
            name = kind + '-' + mode
            mutated = dict(parts)
            mutated[target] = data.encode()
            root = ET.Element(f'{{{RELS}}}Relationships')
            attrs = dict(Id='guard', Type=(OFFICE if mode == 'external' else 'urn:guard/') + kind, Target='/' + target)
            if mode == 'external':
                attrs['TargetMode'] = 'External'
            ET.SubElement(root, f'{{{RELS}}}Relationship', attrs)
            mutated[slide_rel] = ET.tostring(root)
            path = directory / (name + '.pptx')
            write_package(path, mutated)
            row = dict(id=name, path=str(path.resolve()), sha256=hashlib.sha256(path.read_bytes()).hexdigest())
            manifest.append(row)
            current, after = invoke(binary, row, directory / 'guard-after.json.gz')
            issues = differences(expected, after) if current['status'] == 'ok' else ['status']
            receipts.append(dict(id=name, passed=not issues, input_sha256=row['sha256'], issues=issues))
            print(name, not issues, issues, flush=True)
            record = compact(binary, 'duke-slides', row, directory / (name + '.compact.json.gz'), 0)
            record.update(id=name, artwork_hashes=artwork_hashes(after))
            records.append(record)
    (directory / 'manifest.json').write_text(json.dumps(manifest, indent=2))
    records_path = directory / 'records.jsonl'
    records_path.write_text(''.join(json.dumps(r) + '\n' for r in records))
    summary = dict(total=len(receipts), passed=sum(r['passed'] for r in receipts), receipts=receipts, candidate_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), baseline_sha256=hashlib.sha256(baseline.read_bytes()).hexdigest(), records_sha256=hashlib.sha256(records_path.read_bytes()).hexdigest(), harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), scope='Full model equality after reversing only relocated part identities. All non-relationship/non-content-type part bytes are independently identical. Includes package root, related parts, relative/absolute targets, strict/transitional classic and modern comments, real charts, notes, layouts, themes, metadata and images. External and foreign-type layout/notes/comments relationships cannot load internal decoys. Does not establish complete OPC conformance.')
    (run / 'relationship-probes-summary.json').write_text(json.dumps(summary, indent=2))
    if summary['passed'] != summary['total']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()

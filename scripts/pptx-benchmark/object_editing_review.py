import argparse
from collections import Counter, defaultdict
import copy
import hashlib
import json
from pathlib import Path
import re
import subprocess
import gzip
import zipfile
from xml.etree import ElementTree as ET
from xml.parsers import expat

from feedback import OUT, invoke as compact
from object_gate import invoke

D = {'http://schemas.openxmlformats.org/drawingml/2006/main', 'http://purl.oclc.org/ooxml/drawingml/main'}
P = {'http://schemas.openxmlformats.org/presentationml/2006/main', 'http://purl.oclc.org/ooxml/presentationml/main'}
A14 = 'http://schemas.microsoft.com/office/drawing/2010/main'
HOSTS = D | P | {'http://schemas.microsoft.com/office/drawing/2008/diagram', 'http://schemas.microsoft.com/office/powerpoint/2010/main'}
KINDS = {'sp': ('shape', 'nvSpPr', 'cNvSpPr', 'spLocks'), 'cxnSp': ('connector', 'nvCxnSpPr', 'cNvCxnSpPr', 'cxnSpLocks'), 'pic': ('picture', 'nvPicPr', 'cNvPicPr', 'picLocks'), 'grpSp': ('group', 'nvGrpSpPr', 'cNvGrpSpPr', 'grpSpLocks'), 'spTree': ('group', 'nvGrpSpPr', 'cNvGrpSpPr', 'grpSpLocks'), 'graphicFrame': ('graphicFrame', 'nvGraphicFramePr', 'cNvGraphicFramePr', 'graphicFrameLocks'), 'contentPart': ('contentPart', 'nvContentPartPr', 'cNvContentPartPr', 'cpLocks')}
LOCKS = dict(noGrp='noGrouping', noUngrp='noUngrouping', noSelect='noSelection', noRot='noRotation', noChangeAspect='noChangeAspect', noMove='noMove', noResize='noResize', noEditPoints='noEditPoints', noAdjustHandles='noAdjustHandles', noChangeArrowheads='noChangeArrowheads', noChangeShapeType='noChangeShapeType', noTextEdit='noTextEdit', noCrop='noCrop', noDrilldown='noDrilldown')
COMMON = set('noGrp noSelect noRot noChangeAspect noMove noResize noEditPoints noAdjustHandles noChangeArrowheads noChangeShapeType'.split())
ALLOWED = dict(shape=COMMON | {'noTextEdit'}, connector=COMMON, picture=COMMON | {'noCrop'}, contentPart=COMMON, group=set('noGrp noUngrp noSelect noRot noChangeAspect noMove noResize'.split()), graphicFrame=set('noGrp noDrilldown noSelect noChangeAspect noMove noResize'.split()))
sha = lambda p: hashlib.sha256(Path(p).read_bytes()).hexdigest()


def name(tag):
    return tuple(tag[1:].split('}', 1)) if tag.startswith('{') else ('', tag)


def boolean(value):
    return {'true': True, '1': True, 'false': False, '0': False}.get(value.strip(' \t\r\n')) if value is not None else None


def uint(value):
    if value is None or not re.fullmatch(r'[ \t\r\n]*[+-]?[0-9]+[ \t\r\n]*', value):
        return None
    value = int(value)
    return value if 0 <= value <= 4294967295 else None


def signature(node):
    return (node.tag, sorted(node.attrib.items()), node.text or '', [(signature(c), c.tail or '') for c in node])


def source_fragments(data, tree):
    elements = iter(tree.iter())
    parser = expat.ParserCreate(namespace_separator='}')
    stack, result = [], {}

    def start(tag, attributes):
        node = next(elements)
        assert node.tag == ('{' + tag if '}' in tag else tag)
        stack.append((node, parser.CurrentByteIndex))

    def end(tag):
        node, offset = stack.pop()
        stop = parser.CurrentByteIndex
        if data[stop:stop + 2] == b'</':
            stop = data.index(b'>', stop) + 1
        if name(node.tag)[1] in KINDS:
            result[id(node)] = data[offset:stop].decode('utf-8')

    parser.StartElementHandler = start
    parser.EndElementHandler = end
    parser.Parse(data, True)
    return result


def audit_invoke(binary, row, output):
    result = subprocess.run([str(binary), 'duke-slides', row['path'], str(output), '0', 'editing'], capture_output=True, text=True, timeout=120, check=True)
    record = json.loads(result.stdout.splitlines()[-1])
    return record, json.load(gzip.open(output, 'rt')) if record['status'] == 'ok' else None


def expected(node):
    namespace, tag = name(node.tag)
    if namespace not in HOSTS or tag not in KINDS:
        return None, None
    kind, nv, prop, lock = KINDS[tag]
    container = node.find('{' + namespace + '}' + nv)
    properties = container.find('{' + namespace + '}' + prop) if container is not None else None
    if properties is None:
        return None, None
    result = dict(kind=kind)
    if kind == 'connector':
        result['connector'] = {}
    attr, output = {'shape': ('txBox', 'textBox'), 'picture': ('preferRelativeResize', 'preferRelativeResize'), 'contentPart': ('isComment', 'isComment')}.get(kind, (None, None))
    if attr and (value := boolean(properties.get(attr))) is not None:
        result[output] = value
    extension = False
    for child in properties:
        uri, child_name = name(child.tag)
        if child_name == lock and (uri in D or (kind == 'contentPart' and uri == A14)):
            result['locks'] = {LOCKS[k]: value for k in ALLOWED[kind] if (value := boolean(child.get(k))) is not None}
        elif uri in D and child_name == 'extLst':
            extension = True
        elif kind == 'connector' and uri in D and child_name in {'stCxn', 'endCxn'}:
            ident, site = uint(child.get('id')), uint(child.get('idx'))
            key = 'start' if child_name == 'stCxn' else 'end'
            if ident is not None and site is not None:
                result['connector'][key] = dict(shapeId=ident, connectionSite=site)
            else:
                result['connector'].pop(key, None)
    return (result, signature(properties)) if len(result) > 1 or extension else (None, None)


def nodes(value):
    if isinstance(value, dict):
        if 'type' in value and 'id' in value and 'transform' in value:
            yield value
        for key, child in value.items():
            if key not in {'data', 'bytes'} and isinstance(child, (dict, list)):
                yield from nodes(child)
    elif isinstance(value, list):
        for child in value:
            if isinstance(child, (dict, list)):
                yield from nodes(child)


def without_editing(document):
    result = copy.deepcopy(document)
    for node in nodes(result):
        node.pop('editing', None)
    return result


def verify(path, document):
    counts = Counter()
    if not zipfile.is_zipfile(path):
        assert not any('editing' in node for node in nodes(document))
        return counts
    with zipfile.ZipFile(path) as archive:
        cache, trees, fragments = {}, {}, {}
        for node in nodes(document):
            part = node.get('sourcePart')
            if not part or 'sourceId' not in node:
                assert 'editing' not in node
                continue
            if part not in cache:
                index = defaultdict(list)
                trees[part] = ET.fromstring(archive.read(part))
                for source in trees[part].iter():
                    if name(source.tag)[1] == 'control' and source.get('spid') is not None:
                        index[source.get('spid')].append(source)
                    for nv in source:
                        if name(nv.tag)[1] not in {v[1] for v in KINDS.values()}:
                            continue
                        identity = next((c.get('id') for c in nv if name(c.tag)[1] == 'cNvPr'), None)
                        index[identity].append(source)
                cache[part] = index
            sources = cache[part].get(node['sourceId'], [])
            assert sources, (part, node['sourceId'], 'missing source')
            source_count = len(sources)
            source_tags = {'shape': {'sp', 'cxnSp'}, 'group': {'grpSp', 'graphicFrame'}, 'chart': {'graphicFrame'}, 'table': {'graphicFrame'}, 'image': {'pic', 'contentPart', 'graphicFrame', 'model3d'}, 'media': {'pic', 'graphicFrame', 'control', 'model3d'}, 'unknown': {'graphicFrame'}}
            if node['type'] in source_tags:
                sources = [source for source in sources if name(source.tag)[1] in source_tags[node['type']]]
            assert sources, (part, node['sourceId'], 'missing typed source')
            if len(sources) > 1 and node.get('sourceXml'):
                if part not in fragments:
                    fragments[part] = source_fragments(archive.read(part), trees[part])
                matched = [source for source in sources if fragments[part].get(id(source)) == node['sourceXml']]
                if matched:
                    sources = matched
            observed = dict(node['editing']) if 'editing' in node else None
            raw = observed.pop('sourceXml') if observed is not None else None
            source_signature = None
            if raw is not None:
                wrapper = ET.fromstring(raw)
                assert len(wrapper) == 1
                source_signature = signature(wrapper[0])
            candidates = [expected(source) for source in sources]
            assert (observed, source_signature) in candidates, (part, node['sourceId'], observed, [c[0] for c in candidates])
            if observed is None:
                assert all(value is None for value, _ in candidates), (part, node['sourceId'], 'ambiguous absence')
            counts['nodes_checked'] += 1
            counts['duplicate_source_ids'] += source_count > 1
            if observed is not None:
                counts['editing_nodes'] += 1
                counts[observed['kind']] += 1
                counts['source_xml_bytes'] += len(raw.encode())
                counts['explicit_lock_values'] += len(observed.get('locks', {}))
                counts['explicit_false_locks'] += sum(v is False for v in observed.get('locks', {}).values())
                counts['endpoints'] += len(observed.get('connector', {}))
    return counts


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--label', required=True)
    p.add_argument('--manifest', required=True, type=Path)
    p.add_argument('--reference-records', required=True, type=Path)
    p.add_argument('--name', default='editing-reviewed')
    p.add_argument('--resume', action='store_true')
    p.add_argument('--audit-binary', type=Path)
    a = p.parse_args()
    run = OUT/'experiments'/a.label
    baseline = OUT/'experiments/xml-indexed1/candidate'
    assert sha(run/'candidate') == json.loads((run/'identity.json').read_text())['binary_sha256']
    rows = json.loads(a.manifest.read_text())
    previous = {r['id']: r for r in map(json.loads, a.reference_records.read_text().splitlines())}
    destination = run/(a.name+'.jsonl')
    totals, results, changed = Counter(), [], []
    checkpoint = None
    if a.resume:
        checkpoint_path = run/(a.name+'-checkpoint.json')
        checkpoint = json.loads(checkpoint_path.read_text())
        assert checkpoint['records_sha256'] == sha(destination)
        assert checkpoint['candidate_sha256'] == sha(run/'candidate')
        assert checkpoint['manifest_sha256'] == sha(a.manifest)
        assert checkpoint['reference_records_sha256'] == sha(a.reference_records)
        cursor = checkpoint
        while cursor:
            assert cursor['harness_sha256'] == sha(run/cursor.get('harness_archive', 'review-before-resume.py'))
            cursor = cursor.get('prior_checkpoint')
        results = [json.loads(line) for line in destination.read_text().splitlines()]
        assert len(results) == checkpoint['completed']
        assert [r['id'] for r in results] == [r['id'] for r in rows[:len(results)]]
        assert all(r['passed'] for r in results)
        for row, record in zip(rows, results):
            assert sha(row['path']) == row['sha256']
            totals.update(record.get('source_counts', {}))
            if not record['equal']:
                changed.append(row)
    else:
        assert not destination.exists()
    fields = lambda r: (r['status'], r.get('full_sha256'), r.get('error'))
    start = len(results)
    with destination.open('a' if a.resume else 'w', buffering=1) as log:
        for i, row in enumerate(rows[start:], start):
            assert row.get('split') != 'validation' and sha(row['path']) == row['sha256']
            if a.audit_binary:
                record = compact(run/'candidate', 'duke-slides', row, run/(a.name+'.compact.json.gz'), 0)
                audited, artifact = audit_invoke(a.audit_binary, row, run/(a.name+'.audit.json.gz'))
                assert fields(audited) == fields(record), row['id']
                after = dict(nodes=[value['node'] for value in artifact['nodes']]) if artifact else None
            else:
                record, after = invoke(run/'candidate', row, run/(a.name+'.after.json.gz'))
            equal = fields(record) == fields(previous[row['id']])
            if record['status'] == 'ok':
                if a.audit_binary:
                    assert audited['without_editing_sha256'] == previous[row['id']]['full_sha256'], row['id']
                counts = verify(row['path'], after)
                if not equal:
                    if a.audit_binary:
                        assert audited['without_editing_sha256'] == previous[row['id']]['full_sha256'], row['id']
                    else:
                        prior, before = invoke(baseline, row, run/(a.name+'.before.json.gz'))
                        assert fields(prior) == fields(previous[row['id']]) and prior['status'] == 'ok'
                        for node in nodes(after):
                            node.pop('editing', None)
                        assert after == before, row['id']
                    assert counts['editing_nodes'] > 0
                    changed.append(row)
                record['source_counts'] = dict(counts)
                totals.update(counts)
            else:
                assert equal, (row['id'], record)
            record.update(id=row['id'], equal=equal, passed=True)
            log.write(json.dumps(record)+'\n'); results.append(record)
            if i % 25 == 0:
                print(i, len(rows), dict(totals), flush=True)
    summary = dict(total=len(results), passed=len(results), exact=sum(r['equal'] for r in results), changed=len(changed), source_counts=dict(totals), candidate_sha256=sha(run/'candidate'), baseline_sha256=sha(baseline), records_sha256=sha(destination), manifest_sha256=sha(a.manifest), reference_records_sha256=sha(a.reference_records), harness_sha256=sha(__file__), scope='Qualified source properties and explicit booleans/endpoints independently checked on every successful model. All preexisting model values, artwork and payloads exact. Counts include inherited instances; duplicate source IDs require a matching complete property subtree and ambiguous absences fail.')
    if checkpoint:
        summary['resumed_checkpoint'] = checkpoint
        summary['checkpoint_sha256'] = sha(run/(a.name+'-checkpoint.json'))
    if a.audit_binary:
        summary['audit_binary_sha256'] = sha(a.audit_binary)
        summary['audit_mode'] = 'New complete output hash matches the frozen candidate; removing only node editing metadata reproduces the baseline complete output hash. Qualified source checks inspect the emitted metadata independently.'
    (run/(a.name+'-summary.json')).write_text(json.dumps(summary, indent=2)+'\n')
    (run/(a.name+'-changed.json')).write_text(json.dumps(changed, indent=2)+'\n')
    for suffix in ['compact.json.gz', 'before.json.gz', 'after.json.gz', 'audit.json.gz']:
        (run/(a.name+'.'+suffix)).unlink(missing_ok=True)
    print(json.dumps(summary), flush=True)


if __name__ == '__main__':
    main()

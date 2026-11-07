import argparse
import collections
import hashlib
import json
from pathlib import Path
import zipfile
from xml.etree import ElementTree as ET

from artwork_math_gate import artwork_hashes
from common import rels
from feedback import OUT
from object_gate import changes, invoke


PRESENTATION = {'http://schemas.openxmlformats.org/presentationml/2006/main', 'http://purl.oclc.org/ooxml/presentationml/main'}
DRAWING = {'http://schemas.openxmlformats.org/drawingml/2006/main', 'http://purl.oclc.org/ooxml/drawingml/main'}
RELATIONSHIP = {'http://schemas.openxmlformats.org/officeDocument/2006/relationships', 'http://purl.oclc.org/ooxml/officeDocument/relationships'}
SVG = 'http://schemas.microsoft.com/office/drawing/2016/SVG/main'


def qualified(node, name, namespaces):
    return node.tag in {'{' + ns + '}' + name for ns in namespaces}


def child(node, name, namespaces):
    return next((n for n in node if qualified(n, name, namespaces)), None)


def descendants(node, name, namespaces):
    return (n for n in node.iter() if qualified(n, name, namespaces))


def visit(value, inherited=None):
    if isinstance(value, dict):
        metadata = dict(inherited or {})
        metadata.update({k: value[k] for k in ('sourcePart', 'sourceId') if k in value})
        if value.get('type') == 'image' and 'fallbackAssetId' in value:
            yield value, metadata
        for key, part in value.items():
            if key not in {'data', 'bytes'}:
                yield from visit(part, metadata)
    elif isinstance(value, list):
        for part in value:
            yield from visit(part, inherited)


def placeholder(node):
    element = next(descendants(node, 'ph', PRESENTATION), None)
    if element is None:
        return None
    kind = element.get('type')
    kind = 'title' if kind in ('title', 'ctrTitle') else 'body' if kind in ('body', 'obj', 'subTitle') else kind
    return element.get('idx'), kind


def inherited_picture(root, source):
    key = placeholder(source)
    if key is None:
        return None
    tree = next(descendants(root, 'spTree', PRESENTATION), None)
    candidates = [n for n in tree if any(qualified(n, name, PRESENTATION) for name in ('sp', 'pic', 'cxnSp', 'graphicFrame')) and placeholder(n) is not None]
    for index in (0, 1):
        if key[index] is not None:
            found = next((n for n in candidates if placeholder(n)[index] == key[index]), None)
            if found is not None:
                return found
    return None


def verify_sources(path, document):
    counts = collections.Counter()
    with zipfile.ZipFile(path) as archive:
        roots = {}
        def root(part):
            if part not in roots:
                roots[part] = ET.fromstring(archive.read(part))
            return roots[part]
        for image, metadata in visit(document):
            part, ident = metadata['sourcePart'], metadata['sourceId']
            source = next((n for n in descendants(root(part), 'pic', PRESENTATION) if any(p.get('id') == ident for p in descendants(n, 'cNvPr', PRESENTATION))), None)
            assert source is not None, metadata
            candidates = [(part, source)]
            for role in ('slideLayout', 'slideMaster'):
                relationship = next((r for r in rels(archive, part).values() if r['type'] == role and not r['external']), None)
                if relationship is not None:
                    part = relationship['target']
                    inherited = inherited_picture(root(part), source)
                    if inherited is not None:
                        source = inherited
                        candidates.append((part, inherited))
            matched = None
            for part, source in candidates:
                fill = child(source, 'blipFill', PRESENTATION)
                blip = child(fill, 'blip', DRAWING) if fill is not None else None
                if blip is None:
                    continue
                relationships = rels(archive, part)
                def target(element, attribute):
                    ident = next((element.get('{' + ns + '}' + attribute) for ns in RELATIONSHIP if element.get('{' + ns + '}' + attribute) is not None), None)
                    relationship = relationships.get(ident, {})
                    return relationship.get('target') if not relationship.get('external', True) else None
                original = target(blip, 'embed') or target(blip, 'link')
                alternative = next(descendants(blip, 'svgBlip', {SVG}), None)
                selected = target(alternative, 'embed') if alternative is not None else None
                if selected == image['assetId'] and original == image['fallbackAssetId']:
                    assert selected != original
                    for asset_id in (selected, original):
                        asset = document['assets'][asset_id]
                        assert bytes(asset['data']) == archive.read(asset_id)
                    matched = part
                    break
            assert matched is not None, (metadata, image['assetId'], image['fallbackAssetId'])
            counts['associations'] += 1
            counts['inherited'] += matched != metadata['sourcePart']
            counts['strict'] += root(matched).tag.startswith('{http://purl.oclc.org/')
    return counts


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--label', required=True); parser.add_argument('--manifest', required=True, type=Path); args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    proof = json.loads((run / 'parity-summary.json').read_text())
    raw = run / 'parity.jsonl'
    assert digest(raw) == proof['records_sha256'] and digest(args.manifest) == proof['manifest_sha256']
    candidate, baseline = run / 'candidate', Path(proof['baseline_path'])
    assert digest(candidate) == proof['candidate_sha256'] and digest(baseline) == proof['baseline_sha256']
    rows = {r['id']: r for r in json.loads(args.manifest.read_text())}
    records = [json.loads(line) for line in raw.read_text().splitlines()]
    destination = run / 'image-fallback-reviewed.jsonl'
    assert not destination.exists(), 'Preserve existing audit receipts.'
    totals, reviewed = collections.Counter(), []
    with destination.open('w', buffering=1) as stream:
        for index, record in enumerate(records):
            row = rows[record['id']]
            assert digest(Path(row['path'])) == row['sha256']
            current = dict(record, passed=record['equal'])
            if not record['equal']:
                old, before = invoke(baseline, row, run / 'before.json.gz')
                new, after = invoke(candidate, row, run / 'after.json.gz')
                assert old['status'] == new['status'] == 'ok' and new['full_sha256'] == record['full_sha256']
                current['source_counts'] = dict(verify_sources(row['path'], after))
                totals.update(current['source_counts'])
                current['artwork_hashes'] = artwork_hashes(after)
                assert artwork_hashes(before) == current['artwork_hashes']
                for image, _ in visit(after):
                    del image['fallbackAssetId']
                current['differences'] = changes(before, after)
                current['passed'] = not current['differences']
            stream.write(json.dumps(current) + '\n'); reviewed.append(current)
            if index % 25 == 0 or not current['passed']:
                print(index, len(records), current['passed'], flush=True)
    summary = dict(total=len(records), passed=sum(r['passed'] for r in reviewed), exact=sum(r['equal'] for r in reviewed), source_counts=dict(totals), candidate_sha256=proof['candidate_sha256'], baseline_sha256=proof['baseline_sha256'], baseline_path=str(baseline), records_sha256=digest(destination), raw_records_sha256=digest(raw), manifest_sha256=digest(args.manifest), harness_sha256=digest(Path(__file__)), scope='Namespace-qualified source picture and inherited placeholder checks for every emitted fallback association; original and selected payloads exact against package bytes. Removing only fallbackAssetId leaves the full baseline model, payloads and artwork unchanged. This is not source-wide image recall or visual fidelity.')
    (run / 'image-fallback-reviewed-summary.json').write_text(json.dumps(summary, indent=2)); print(json.dumps(summary)); assert summary['passed'] == summary['total']


if __name__ == '__main__':
    main()

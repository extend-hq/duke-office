import argparse
import collections
import copy
import gzip
import hashlib
import json
import math
from pathlib import Path
import re
import shutil
import zipfile
from xml.etree import ElementTree as ET

from common import local, rels
from object_gate import OUT, ROOT, attrs, child, descend, invoke, changes

MATH = 'http://schemas.openxmlformats.org/officeDocument/2006/math'
INK = 'http://www.w3.org/2003/InkML'


def structure(node):
    namespace, name = node.tag[1:].split('}', 1) if node.tag.startswith('{') else (None, node.tag)
    attributes = []
    for key, value in node.attrib.items():
        ns, key = key[1:].split('}', 1) if key.startswith('{') else (None, key)
        attributes.append(dict(name=key, namespace=ns, value=value))
    return dict(name=name, namespace=namespace, attributes=attributes, children=[structure(n) for n in node], text=node.text or '')


def normalized_structure(node):
    return dict(name=node['name'], namespace=node.get('namespace'), attributes=[dict(name=a['name'], namespace=a.get('namespace'), value=a['value']) for a in node.get('attributes', [])],
                children=[normalized_structure(n) for n in node.get('children', [])], text=node.get('text', ''))


def walk_objects(value):
    if isinstance(value, dict):
        yield value
        for child_value in value.values():
            yield from walk_objects(child_value)
    elif isinstance(value, list):
        for child_value in value:
            yield from walk_objects(child_value)


def close(a, b):
    return math.isclose(a, b, abs_tol=1e-8, rel_tol=1e-9)


def ink_points(xml):
    root = ET.fromstring(xml)
    traces = root.findall('.//{' + INK + '}trace')
    expected = []
    for trace in traces:
        context_id = trace.get('contextRef', '').lstrip('#')
        context = next(n for n in root.iter() if local(n) == 'context' and n.get('{http://www.w3.org/XML/1998/namespace}id') == context_id)
        channels = context.findall('.//{' + INK + '}channel')
        resolution = {n.get('channel'): float(n.get('value')) for n in context.findall('.//{' + INK + '}channelProperty') if n.get('name') == 'resolution'}
        assert [n.get('name') for n in channels] == ['X', 'Y', 'F']
        assert all(n.get('units') == 'cm' for n in channels[:2])
        points = []
        for sample in (trace.text or '').split(','):
            if not sample.strip():
                continue
            x, y, pressure = map(float, sample.split())
            points.append(dict(x=x * 96 / 2.54 / resolution['X'], y=y * 96 / 2.54 / resolution['Y'], pressure=pressure / float(channels[2].get('max')), contact=True))
        expected.append(points)
    return expected


def audit(before, after, package):
    issues = []
    counts = collections.Counter()
    current = copy.deepcopy(after)
    allowed_assets = set()
    expected_math = []
    source_nodes = {}
    source_controls = {}
    relationship_cache = {}
    for part in package.namelist():
        if not (part.startswith('ppt/slides/') and part.endswith('.xml')):
            continue
        root = ET.fromstring(package.read(part))
        relationship_cache[part] = rels(package, part)
        for node in root.iter():
            if node.tag == '{' + MATH + '}oMathPara':
                expected_math.append(structure(node))
            elif local(node) == 'contentPart':
                source_nodes[part, descend(node, 'cNvPr').get('id')] = node
            elif local(node) == 'control':
                source_controls[part, node.get('spid')] = node
    math_found = []
    ink_seen = set()
    controls_seen = set()

    def payload(path):
        allowed_assets.add(path)
        if bytes(after['assets'].get(path, {}).get('data', [])) != package.read(path):
            issues.append('inserted payload mismatch')
        else:
            counts['payloads'] += 1

    def validate_ink(node):
        key = node.get('sourcePart'), node.get('sourceId')
        source = source_nodes.get(key)
        if source is None:
            issues.append('ink source not found')
            return
        ink_seen.add(key)
        relationship = relationship_cache[key[0]][attrs(source)['id']]
        path = relationship['target']
        ink = node['ink']
        xml = package.read(path).decode('utf-8')
        if ink.get('assetId') != path or ink.get('sourceXml') != xml:
            issues.append('ink source payload differs')
        payload(path)
        expected = ink_points(xml)
        strokes = ink.get('drawing', {}).get('strokes', [])
        if len(expected) != len(strokes):
            issues.append('ink stroke count differs')
        for points, stroke in zip(expected, strokes):
            actual = stroke.get('points', [])
            if len(points) != len(actual) or any(not all(close(point[k], found[k]) for k in ['x', 'y', 'pressure']) or point['contact'] != found['contact'] for point, found in zip(points, actual)):
                issues.append('ink coordinates differ')
            counts['ink_points'] += len(points)
            counts['ink_strokes'] += 1
        xfrm = descend(source, 'xfrm'); off = child(xfrm, 'off'); ext = child(xfrm, 'ext')
        transform = dict(x=int(off.get('x')), y=int(off.get('y')), width=int(ext.get('cx')), height=int(ext.get('cy')))
        if node['transform'] != transform:
            issues.append('ink frame differs')
        if not node.get('pictureSvg') or not node.get('paintBounds'):
            issues.append('missing ink artwork')
        counts['ink_objects'] += 1

    def validate_control(node):
        key = node.get('sourcePart'), node.get('sourceId')
        source = source_controls.get(key)
        if source is None:
            issues.append('control source not found')
            return
        controls_seen.add(key)
        rr = relationship_cache[key[0]]
        path = rr[attrs(source)['id']]['target']
        xml = package.read(path).decode('utf-8')
        root = ET.fromstring(xml)
        definition = node.get('embeddedObject', {}).get('control', {})
        attributes = attrs(root)
        if definition.get('classId') != attributes.get('classid') or definition.get('persistence') != attributes.get('persistence') or definition.get('sourceXml') != xml:
            issues.append('control definition differs')
        properties = [dict(name=attrs(n)['name'], value=attrs(n)['value']) for n in root if local(n) == 'ocxPr']
        if definition.get('properties') != properties:
            issues.append('control properties differ')
        if node.get('assetId') != path:
            issues.append('control relationship differs')
        payload(path)
        preview = node.get('posterAssetId')
        if not preview or not node.get('poster'):
            issues.append('control preview missing')
        else:
            payload(preview)
        for relationship in rr.values():
            if relationship['type'] != 'vmlDrawing':
                continue
            vml = package.read(relationship['target']).decode('utf-8')
            root = ET.fromstring(re.sub(r'<!\[(?:if [^>]*|endif)\]>', '', vml))
            shape = next((n for n in root if local(n) == 'shape' and attrs(n).get('spid') == '_x0000_s' + key[1]), None)
            if shape is None:
                continue
            style = dict(part.split(':', 1) for part in shape.get('style', '').split(';') if ':' in part)
            expected = {field: round(float(style[prop].strip().removesuffix('pt')) * 12700) for field, prop in [('x', 'left'), ('y', 'top'), ('width', 'width'), ('height', 'height')]}
            if node['transform'] != expected or node['poster']['transform'] != expected:
                issues.append('control preview geometry differs')
            image_id = attrs(child(shape, 'imagedata')).get('relid')
            if preview != rels(package, relationship['target'])[image_id]['target']:
                issues.append('control preview relationship differs')
        counts['controls'] += 1
        counts['control_properties'] += len(properties)

    def normalize(old, new):
        if isinstance(old, dict) and isinstance(new, dict):
            if new.get('ink'):
                validate_ink(new)
                if old.get('type') != 'image' or old.get('assetId') != 'missing-asset' or old.get('id') != new.get('id'):
                    issues.append('unexpected ink replacement')
                new.clear(); new.update(copy.deepcopy(old))
                return
            if 'runs' in new and any(run.get('math') for run in new['runs']):
                retained = []
                for run in new['runs']:
                    if not run.get('math'):
                        retained.append(run); continue
                    equation = run['math']
                    if equation.get('root') is None:
                        issues.append('unparsed source equation'); continue
                    math_found.append(normalized_structure(equation['root']))
                    source = ET.fromstring(equation['sourceXml'])
                    wanted = ''.join(n.text or '' for n in source.iter() if n.tag == '{' + MATH + '}t')
                    if run['text'] != wanted or equation['text'] != wanted or not equation.get('mathMl'):
                        issues.append('equation text or MathML missing')
                    if equation.get('justification') in ('center', 'centerGroup') and new.get('alignment') != 'center':
                        issues.append('equation paragraph alignment differs')
                    counts['math'] += 1
                new['runs'] = retained
                if 'alignment' in old: new['alignment'] = old['alignment']
                else: new.pop('alignment', None)
            if 'nodes' in new:
                retained = []
                for node in new['nodes']:
                    if node.get('mediaType') == 'control': validate_control(node)
                    else: retained.append(node)
                new['nodes'] = retained
            for key in old.keys() & new.keys():
                normalize(old[key], new[key])
        elif isinstance(old, list) and isinstance(new, list):
            for a, b in zip(old, new): normalize(a, b)

    normalize(before, current)
    if collections.Counter(json.dumps(n, sort_keys=True) for n in expected_math) != collections.Counter(json.dumps(n, sort_keys=True) for n in math_found):
        issues.append('structured equation source coverage differs')
    if ink_seen != set(source_nodes): issues.append('ink occurrence coverage differs')
    if controls_seen != set(source_controls): issues.append('control occurrence coverage differs')
    for path in list(current['assets']):
        if path not in before['assets']:
            if path not in allowed_assets: issues.append('unexpected added asset')
            else: counts['new_assets'] += 1
            current['assets'].pop(path)
    expected = copy.deepcopy(before)
    for slide in expected['slides']:
        ids = {key[1] for key in ink_seen if key[0] == slide.get('sourcePart')}
        slide['warnings'] = [w for w in slide.get('warnings', []) if not (w.get('feature') == 'contentPart' and w['code'] == 'unsupported-feature') and not (w['code'] == 'missing-asset' and w['message'] in {f'Image {ident} has no resolvable embedded asset.' for ident in ids})]
    differences = changes(expected, current)
    issues.extend(differences[:30])
    return issues, counts


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--label', required=True); args = parser.parse_args()
    run = OUT / 'experiments' / args.label; run.mkdir(parents=True, exist_ok=True)
    binary = run / 'candidate'; shutil.copy2(ROOT / 'scripts/pptx-benchmark/target/release/pptx-benchmark', binary)
    baseline = OUT / 'experiments/opt5/candidate'
    originals = {r['id']: r for r in map(json.loads, (OUT / 'experiments/opt5/parity.jsonl').read_text().splitlines())}
    results = []; totals = collections.Counter()
    with (run / 'inserted.jsonl').open('w', buffering=1) as log:
        for index, row in enumerate(json.loads((OUT / 'manifest.json').read_text())):
            record, after = invoke(binary, row, run / 'after.json.gz')
            original = originals[row['id']]
            equal = record['status'] == original['status'] and (record['status'] != 'ok' or record['full_sha256'] == original['full_sha256'])
            issues = []; counts = collections.Counter()
            if not equal:
                previous, before = invoke(baseline, row, run / 'before.json.gz')
                if before is None or after is None: issues.append('acceptance changed')
                else:
                    with zipfile.ZipFile(row['path']) as package: issues, counts = audit(before, after, package)
                shutil.copy2(run / 'after.json.gz', run / (row['id'] + '.json.gz'))
            result = dict(record, id=row['id'], unchanged=equal, passed=not issues, issues=issues, counts=dict(counts))
            results.append(result); totals.update(counts); log.write(json.dumps(result) + '\n')
            if issues: print(row['id'], issues, flush=True)
            if index % 50 == 0: print(index, '/ 318', flush=True)
    summary = dict(total=len(results), passed=sum(r['passed'] for r in results), unchanged=sum(r['unchanged'] for r in results), counts=dict(totals),
                   candidate_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), baseline_sha256=hashlib.sha256(baseline.read_bytes()).hexdigest())
    (run / 'inserted-summary.json').write_text(json.dumps(summary, indent=2)); print(json.dumps(summary))
    if summary['passed'] != summary['total']: raise SystemExit(1)


if __name__ == '__main__':
    main()

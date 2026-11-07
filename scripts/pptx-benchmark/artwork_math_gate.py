import argparse
import base64
import collections
import gzip
import hashlib
import json
import math
from pathlib import Path
import re
from xml.etree import ElementTree as ET

from feedback import OUT, ROOT, invoke
from object_gate import invoke as full_model


ART = {'shapeSvg', 'pictureSvg', 'chartSvg', 'svg'}
NUMBER = re.compile(r'[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?')
NUMERIC = set('d points transform x y x1 x2 y1 y2 cx cy r rx ry width height viewBox opacity fill-opacity stroke-opacity stroke-width stroke-miterlimit stroke-dasharray stroke-dashoffset offset stdDeviation dx dy font-size font-weight letter-spacing values slope intercept amplitude exponent scale'.split())


def near(a, b):
    return math.isfinite(a) and math.isfinite(b) and math.isclose(a, b, rel_tol=1e-13, abs_tol=1e-7)


def numeric_text(a, b):
    if NUMBER.sub('#', a) != NUMBER.sub('#', b):
        return False
    x, y = NUMBER.findall(a), NUMBER.findall(b)
    return len(x) == len(y) and all(near(float(i), float(j)) for i, j in zip(x, y))


def svg_changes(before, after, changes, location):
    def walk(a, b, path, root=False):
        if a.tag != b.tag or a.text != b.text or a.tail != b.tail or len(a) != len(b) or a.attrib.keys() != b.attrib.keys():
            return [path + '/structure']
        issues = []
        for key in a.attrib:
            x, y = a.attrib[key], b.attrib[key]
            if x == y:
                continue
            if key in NUMERIC and numeric_text(x, y):
                changes['numeric_attributes'] += 1
            elif root and key in {'width', 'height', 'viewBox'}:
                old, new = NUMBER.findall(x), NUMBER.findall(y)
                if NUMBER.sub('#', x) == NUMBER.sub('#', y) and len(old) == len(new) and all(abs(float(i) - float(j)) <= 2 for i, j in zip(old, new)):
                    changes['viewport_changes'].append(dict(path=path + '/@' + key, before=x, after=y))
                else:
                    issues.append(path + '/@' + key)
            elif x.startswith('data:image/svg+xml;base64,') and y.startswith('data:image/svg+xml;base64,'):
                issues.extend(svg_changes(base64.b64decode(x.split(',', 1)[1]), base64.b64decode(y.split(',', 1)[1]), changes, path + '/@' + key))
            else:
                issues.append(path + '/@' + key)
        for index, (x, y) in enumerate(zip(a, b)):
            issues.extend(walk(x, y, path + '/' + str(index)))
        return issues
    try:
        return walk(ET.fromstring(before), ET.fromstring(after), location, True)
    except (ET.ParseError, ValueError):
        return [location + '/invalid-svg']


def artwork_hashes(value, path='', result=None):
    if result is None:
        result = []
    items = value.items() if isinstance(value, dict) else enumerate(value) if isinstance(value, list) else []
    for key, child in items:
        if key in {'data', 'bytes'}:
            continue
        location = path + '/' + str(key)
        if key in ART and isinstance(child, str):
            result.append([location, hashlib.sha256(child.encode()).hexdigest()])
        elif isinstance(child, (dict, list)):
            artwork_hashes(child, location, result)
    return result


def compare(before, after, changes, path=''):
    if before == after:
        return []
    if type(before) != type(after):
        return [path + '/type']
    if isinstance(before, dict):
        issues = []
        for key in before.keys() | after.keys():
            location = path + '/' + key
            if key == 'paintBounds':
                changes['paint_bounds'].append(dict(path=location, before=before.get(key), after=after.get(key)))
            elif key not in before or key not in after:
                issues.append(location + '/key')
            elif key in ART and isinstance(before[key], str) and before[key] != after[key]:
                changes['changed_artwork'] += 1
                issues.extend(svg_changes(before[key], after[key], changes, location))
            else:
                issues.extend(compare(before[key], after[key], changes, location))
        return issues
    if isinstance(before, list):
        if len(before) != len(after):
            return [path + '/length']
        return [p for i, (a, b) in enumerate(zip(before, after)) for p in compare(a, b, changes, path + '/' + str(i))]
    return [path]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--baseline-label', default='chart-group1')
    parser.add_argument('--manifest', type=Path)
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    baseline_run = OUT / 'experiments' / args.baseline_label
    binary, baseline = run / 'candidate', baseline_run / 'candidate'
    if (baseline_run / 'artwork-math-summary.json').exists():
        proof = json.loads((baseline_run / 'artwork-math-summary.json').read_text())
        bounds = json.loads((baseline_run / 'bounds-review.json').read_text())['summary']
        reviewed_path = baseline_run / 'artwork-reviewed-summary.json'
        if reviewed_path.exists():
            reviewed = json.loads(reviewed_path.read_text())
            assert reviewed['total'] == reviewed['passed'] == proof['total']
            assert reviewed['records_sha256'] == proof['records_sha256']
            assert reviewed['candidate_sha256'] == proof['candidate_sha256']
            for name, expected in reviewed['review_sha256'].items():
                assert hashlib.sha256((baseline_run / name).read_bytes()).hexdigest() == expected
        else:
            assert proof['numerical_only'] == proof['total']
        assert proof['total'] == bounds['inputs']
        assert bounds['passed'] == bounds['changed_bounds']
        assert bounds['records_sha256'] == proof['records_sha256']
        previous_path = baseline_run / 'artwork-math.jsonl'
        assert hashlib.sha256(previous_path.read_bytes()).hexdigest() == proof['records_sha256']
    elif (baseline_run / 'parity-summary.json').exists():
        proof = json.loads((baseline_run / 'parity-summary.json').read_text())
        assert proof['equal'] == proof['total']
        previous_path = baseline_run / 'parity.jsonl'
        assert hashlib.sha256(previous_path.read_bytes()).hexdigest() == proof['records_sha256']
    else:
        proof = json.loads((baseline_run / 'semantic-summary.json').read_text())
        assert proof['passed'] == proof['total']
        previous_path = baseline_run / 'semantic.jsonl'
    assert hashlib.sha256(baseline.read_bytes()).hexdigest() == proof['candidate_sha256']
    previous = {r['id']: r for r in map(json.loads, previous_path.read_text().splitlines())}
    rows = json.loads(args.manifest.read_text()) if args.manifest else json.loads((OUT / 'manifest.json').read_text()) + json.loads((OUT / 'expanded-optimized/cohort.json').read_text()) + [r for r in json.loads((OUT / 'corpus-followup/manifest.json').read_text()) if r['split'] == 'tuning']
    assert len(rows) == len({r['id'] for r in rows})
    assert all(hashlib.sha256(Path(r['path']).read_bytes()).hexdigest() == r['sha256'] for r in rows)
    destination = run / 'artwork-math.jsonl'
    if destination.exists():
        raise SystemExit('Preserve previous receipts; select a new output directory.')
    records = []
    with destination.open('w', buffering=1) as stream:
        for index, row in enumerate(rows):
            current = invoke(binary, 'rust-pptx', row, run / (row['id'] + '.compact.json.gz'), 0)
            old = previous[row['id']]
            fields = lambda r: (r['status'], r.get('error'), r.get('full_sha256'))
            exact = fields(current) == fields(old)
            changes = dict(changed_artwork=0, numeric_attributes=0, viewport_changes=[], paint_bounds=[])
            issues, hashes = [], []
            if exact and 'artwork_hashes' in old:
                hashes = old['artwork_hashes']
            elif current['status'] == 'ok':
                after_path = run / (row['id'] + '.after.json.gz')
                replay, after = full_model(binary, row, after_path)
                assert fields(replay) == fields(current)
                hashes = artwork_hashes(after)
                if not exact:
                    before_path = run / (row['id'] + '.before.json.gz')
                    replay, before = full_model(baseline, row, before_path)
                    assert fields(replay) == fields(old)
                    issues = compare(before, after, changes) if old['status'] == 'ok' else ['acceptance changed']
                    if not issues:
                        before_path.unlink()
                    del before
                if not issues:
                    after_path.unlink()
                del after
            elif not exact:
                issues.append('acceptance or error changed')
            record = dict(id=row['id'], **current, exact=exact, numerical_only=not issues, changes=changes, artwork_hashes=hashes, issues=issues)
            records.append(record)
            stream.write(json.dumps(record) + '\n')
            if issues:
                print('MISMATCH', row['id'], issues[:8], flush=True)
            if index % 25 == 0:
                print(index, len(rows), flush=True)
    summary = dict(total=len(records), exact=sum(r['exact'] for r in records), numerical_only=sum(r['numerical_only'] for r in records), changed_artwork=sum(r['changes']['changed_artwork'] for r in records), changed_numeric_attributes=sum(r['changes']['numeric_attributes'] for r in records), viewport_changes=sum(len(r['changes']['viewport_changes']) for r in records), paint_bounds=sum(len(r['changes']['paint_bounds']) for r in records), failures=[r['id'] for r in records if r['issues']], pending='Pixel-bound changes require a separate review; this audit alone does not approve them.', candidate_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), baseline_sha256=proof['candidate_sha256'], baseline_label=args.baseline_label, records_sha256=hashlib.sha256(destination.read_bytes()).hexdigest(), source_sha256={str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for folder in ['rust-pptx', 'rust-office-art', 'rust-office-charts'] for p in (ROOT / 'crates' / folder).rglob('*.rs')})
    (run / 'artwork-math-summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps({k: v for k, v in summary.items() if k != 'source_sha256'}))
    if summary['failures']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()

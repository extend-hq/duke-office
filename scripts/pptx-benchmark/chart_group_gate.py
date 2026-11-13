import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import math
from pathlib import Path
import re
import shutil
from xml.etree import ElementTree as ET

from feedback import OUT, ROOT, invoke
from object_gate import invoke as full_model


def dimensions(svg):
    root = ET.fromstring(svg)
    if root.tag != '{http://www.w3.org/2000/svg}svg':
        raise ValueError('Expected an SVG root')
    result = tuple(float(root.get(key)) for key in ['width', 'height'])
    if not all(math.isfinite(value) and value > 0 for value in result):
        raise ValueError('Invalid SVG dimensions')
    viewbox = tuple(map(float, root.get('viewBox').split()))
    if viewbox != (0, 0, *result):
        raise ValueError('SVG viewBox disagrees with viewport')
    for node in root.iter():
        for key, value in node.attrib.items():
            if key in {'d', 'points', 'transform', 'x', 'y', 'x1', 'y1', 'x2', 'y2', 'cx', 'cy', 'r', 'rx', 'ry', 'width', 'height', 'font-size'} and re.search(r'(?<![A-Za-z])(?:NaN|[+-]?inf(?:inity)?)(?![A-Za-z])', value, re.I):
                raise ValueError('Nonfinite SVG geometry')
    return result


def compare(before, after, changes, scale=(1., 1.), path=''):
    if before == after:
        return []
    if type(before) != type(after):
        return [path]
    if isinstance(before, dict):
        child_scale = scale
        if before.get('type') == 'group':
            frame = before['transform']
            child = before.get('childTransform') or frame
            values = []
            for axis, parent in zip(['width', 'height'], scale):
                factor = parent * frame[axis] / child[axis] if child[axis] else float('nan')
                values.append(factor if math.isfinite(factor) and factor > 0 else parent)
            child_scale = tuple(values)
        issues = []
        for key in before.keys() | after.keys():
            location = path + '/' + key
            if key not in before or key not in after:
                issues.append(location)
            elif key == 'chartSvg' and before.get('type') == 'chart' and before[key] != after[key]:
                try:
                    old = dimensions(before[key])
                    new = dimensions(after[key])
                    raw = tuple(before['transform'][axis] / 9525 for axis in ['width', 'height'])
                    expected = tuple(value * factor for value, factor in zip(raw, scale))
                    valid = scale != (1., 1.) and all(math.isclose(a, b, rel_tol=1e-10, abs_tol=1e-9) for a, b in zip(old, raw)) and all(math.isclose(a, b, rel_tol=1e-10, abs_tol=1e-9) for a, b in zip(new, expected))
                    if not valid:
                        issues.append(location + '/dimensions')
                    else:
                        changes.append(dict(path=location, before=old, after=new, scale=scale))
                except (ValueError, TypeError, ET.ParseError) as error:
                    issues.append(location + '/' + str(error))
            else:
                issues.extend(compare(before[key], after[key], changes, child_scale if key == 'children' else scale, location))
        return issues
    if isinstance(before, list):
        if len(before) != len(after):
            return [path + '/length']
        return [issue for index, (a, b) in enumerate(zip(before, after)) for issue in compare(a, b, changes, scale, path + '/' + str(index))]
    return [path]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--baseline-label', default='chart-bounds1')
    args = parser.parse_args()
    baseline_run = OUT / 'experiments' / args.baseline_label
    proof = json.loads((baseline_run / 'parity-summary.json').read_text())
    baseline = baseline_run / 'candidate'
    if proof['equal'] != proof['total'] or hashlib.sha256(baseline.read_bytes()).hexdigest() != proof['candidate_sha256']:
        raise SystemExit('The baseline exact-output gate must pass and remain unchanged.')
    old = {r['id']: r for r in map(json.loads, (baseline_run / 'parity.jsonl').read_text().splitlines())}
    run = OUT / 'experiments' / args.label
    run.mkdir(exist_ok=True)
    binary = run / 'candidate'
    if binary.exists():
        raise SystemExit('Preserve existing candidate receipts; choose a new label.')
    shutil.copy2(ROOT / 'scripts/pptx-benchmark/target/release/pptx-benchmark', binary)
    rows = json.loads((OUT / 'manifest.json').read_text()) + json.loads((OUT / 'expanded-optimized/cohort.json').read_text())
    rows += [r for r in json.loads((OUT / 'corpus-followup/manifest.json').read_text()) if r['split'] == 'tuning']
    if any(hashlib.sha256(Path(r['path']).read_bytes()).hexdigest() != r['sha256'] for r in rows):
        raise SystemExit('Input identity changed.')

    def check(row):
        current = invoke(binary, 'duke-pptx', row, run / (row['id'] + '.compact.json.gz'), 0)
        previous = old.get(row['id'])
        if previous is None:
            previous = invoke(baseline, 'duke-pptx', row, run / (row['id'] + '.baseline.compact.json.gz'), 0)
        fields = lambda record: (record['status'], record.get('full_sha256'), record.get('error'))
        exact = fields(current) == fields(previous)
        issues, changes = [], []
        if not exact:
            if current['status'] != 'ok' or previous['status'] != 'ok':
                issues.append('acceptance or error changed')
            else:
                old_record, before = full_model(baseline, row, run / (row['id'] + '.before.json.gz'))
                new_record, after = full_model(binary, row, run / (row['id'] + '.after.json.gz'))
                if fields(old_record) != fields(previous) or fields(new_record) != fields(current):
                    issues.append('full-output replay changed')
                else:
                    issues = compare(before, after, changes)
        return dict(id=row['id'], **current, exact=exact, passed=not issues, changed_charts=changes, issues=issues)

    records = []
    with ThreadPoolExecutor(max_workers=2) as pool, (run / 'semantic.jsonl').open('w', buffering=1) as stream:
        for index, record in enumerate(pool.map(check, rows)):
            records.append(record)
            stream.write(json.dumps(record) + '\n')
            if not record['passed']: print('MISMATCH', record['id'], record['issues'][:10], flush=True)
            if index % 50 == 0: print(index, len(rows), flush=True)
    summary = dict(total=len(records), passed=sum(r['passed'] for r in records), exact=sum(r['exact'] for r in records), changed_charts=sum(len(r['changed_charts']) for r in records), baseline_label=args.baseline_label, baseline_sha256=proof['candidate_sha256'], candidate_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), failures=[r['id'] for r in records if not r['passed']], scope='All non-SVG model fields remain exact; changed grouped-chart SVGs must use positive finite viewport dimensions derived from the unchanged group frames.', source_sha256={str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for folder in ['duke-pptx', 'duke-office-art', 'duke-office-charts'] for p in (ROOT / 'crates' / folder).rglob('*.rs')})
    (run / 'semantic-summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps({key: value for key, value in summary.items() if key != 'source_sha256'}))
    if summary['passed'] != summary['total']: raise SystemExit(1)


if __name__ == '__main__':
    main()

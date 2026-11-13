import argparse
from concurrent.futures import ThreadPoolExecutor
import collections
import hashlib
import json
from pathlib import Path
import shutil
from xml.etree import ElementTree as ET

from object_gate import OUT, ROOT, invoke

SVG = '{http://www.w3.org/2000/svg}'


def equivalent_svg(before, after):
    root = ET.fromstring(before)
    definitions = root.find(SVG + 'defs')
    candidates = [] if definitions is None else [n for n in definitions if n.tag == SVG + 'g' and n.attrib == {'id': 'picture-body'}]
    if len(candidates) != 1:
        return False
    definitions.remove(candidates[0])
    if any('#picture-body' in value for node in root.iter() for value in node.attrib.values()):
        return False
    return ET.tostring(root) == ET.tostring(ET.fromstring(after))


def compare(before, after, counts, path=''):
    if type(before) != type(after):
        return [path]
    if before == after:
        return []
    if isinstance(before, dict):
        issues = []
        for key in before.keys() | after.keys():
            location = path + '/' + key
            if key not in before or key not in after:
                issues.append(location)
            elif key in {'shapeSvg', 'pictureSvg', 'chartSvg', 'svg'} and isinstance(before[key], str) and before[key] != after[key]:
                if equivalent_svg(before[key], after[key]):
                    counts['unused_definitions_removed'] += 1
                    counts['svg_bytes_removed'] += len(before[key].encode()) - len(after[key].encode())
                else:
                    issues.append(location)
            else:
                issues.extend(compare(before[key], after[key], counts, location))
        return issues
    if isinstance(before, list):
        if len(before) != len(after):
            return [path + '/length']
        return [p for i, (a, b) in enumerate(zip(before, after)) for p in compare(a, b, counts, path + '/' + str(i))]
    return [] if before == after else [path]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--baseline-label', default='xml-memory1')
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    run.mkdir(exist_ok=True)
    binary = run / 'candidate'
    if binary.exists():
        raise SystemExit('Preserve completed or partial gates; choose a new label.')
    shutil.copy2(ROOT / 'scripts/pptx-benchmark/target/release/pptx-benchmark', binary)
    baseline_run = OUT / 'experiments' / args.baseline_label
    proof = json.loads((baseline_run / 'parity-summary.json').read_text())
    if proof['equal'] != proof['total']:
        raise SystemExit('The complete baseline equivalence gate must pass.')
    baseline = baseline_run / 'candidate'
    if hashlib.sha256(baseline.read_bytes()).hexdigest() != proof['candidate_sha256']:
        raise SystemExit('Baseline executable changed.')
    old_records = {r['id']: r for r in map(json.loads, (baseline_run / 'parity.jsonl').read_text().splitlines())}
    rows = json.loads((OUT / 'manifest.json').read_text()) + json.loads((OUT / 'expanded-optimized/cohort.json').read_text())
    if any(hashlib.sha256(Path(r['path']).read_bytes()).hexdigest() != r['sha256'] for r in rows):
        raise SystemExit('Input identity changed.')

    def check(row):
        after_path = run / (row['id'] + '.after.json.gz')
        current, after = invoke(binary, row, after_path)
        previous = old_records[row['id']]
        issues = []
        counts = collections.Counter()
        exact = current['status'] == previous['status'] and current.get('full_sha256') == previous.get('full_sha256') and current.get('error') == previous.get('error')
        if not exact:
            before_path = run / (row['id'] + '.before.json.gz')
            old, before = invoke(baseline, row, before_path)
            if current['status'] != old['status'] or current['status'] != 'ok':
                issues.append('acceptance or error changed')
            else:
                issues = compare(before, after, counts)
            if not issues:
                before_path.unlink(missing_ok=True)
        if not issues:
            after_path.unlink(missing_ok=True)
        return dict(id=row['id'], **current, exact=exact, passed=not issues, counts=dict(counts), issues=issues)

    records = []
    with ThreadPoolExecutor(max_workers=2) as pool, (run / 'semantic.jsonl').open('w', buffering=1) as stream:
        for i, record in enumerate(pool.map(check, rows)):
            records.append(record)
            stream.write(json.dumps(record) + '\n')
            if not record['passed']:
                print('MISMATCH', record['id'], record['issues'][:5], flush=True)
            if i % 50 == 0:
                print(i, len(rows), flush=True)
    counts = sum((collections.Counter(r['counts']) for r in records), collections.Counter())
    summary = dict(total=len(records), passed=sum(r['passed'] for r in records), exact=sum(r['exact'] for r in records), counts=dict(counts), baseline_label=args.baseline_label, baseline_sha256=proof['candidate_sha256'], candidate_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), failures=[r['id'] for r in records if not r['passed']], source_sha256={str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for folder in ['duke-pptx', 'duke-office-art', 'duke-office-charts'] for p in (ROOT / 'crates' / folder).rglob('*.rs')})
    (run / 'semantic-summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps({k: v for k, v in summary.items() if k != 'source_sha256'}))
    assert summary['passed'] == summary['total']


if __name__ == '__main__':
    main()

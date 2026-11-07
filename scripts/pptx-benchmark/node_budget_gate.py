import argparse
from concurrent.futures import ThreadPoolExecutor
import gzip
import hashlib
import json
from pathlib import Path
import shutil

from assess import score
from common import core
from feedback import OUT, ROOT, invoke


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--reference-label', default='xml-memory2')
    args = parser.parse_args()
    baseline_run = OUT / 'experiments' / args.reference_label
    proof = json.loads((baseline_run / 'semantic-reviewed-summary.json').read_text())
    old_path = baseline_run / 'semantic-reviewed.jsonl'
    if proof['passed'] != proof['total'] or hashlib.sha256(old_path.read_bytes()).hexdigest() != proof['reviewed_records_sha256']:
        raise SystemExit('The complete SVG equivalence audit must pass and remain unchanged.')
    if hashlib.sha256((baseline_run / 'candidate').read_bytes()).hexdigest() != proof['candidate_sha256']:
        raise SystemExit('Baseline executable changed.')
    old = {r['id']: r for r in map(json.loads, old_path.read_text().splitlines())}
    run = OUT / 'experiments' / args.label
    run.mkdir(exist_ok=True)
    binary = run / 'candidate'
    if binary.exists():
        raise SystemExit('Choose a new label; preserve existing verification artifacts.')
    shutil.copy2(ROOT / 'scripts/pptx-benchmark/target/release/pptx-benchmark', binary)
    rows = json.loads((OUT / 'manifest.json').read_text()) + json.loads((OUT / 'expanded-optimized/cohort.json').read_text())
    if any(hashlib.sha256(Path(r['path']).read_bytes()).hexdigest() != r['sha256'] for r in rows):
        raise SystemExit('Input identity changed.')

    def check(row):
        output = run / (row['id'] + '.compact.json.gz')
        current = invoke(binary, 'rust-pptx', row, output, 0)
        before = old[row['id']]
        exact = (before['status'], before.get('full_sha256'), before.get('error')) == (current['status'], current.get('full_sha256'), current.get('error'))
        recovered = False
        metrics = None
        if not exact and before.get('error') == 'resource limit exceeded: XML node count' and current['status'] == 'ok':
            reference = json.loads((OUT / 'oracle' / (row['id'] + '.json')).read_text())
            if reference['status'] == 'ok':
                metrics = score(reference, core(json.load(gzip.open(output, 'rt'))))
                recovered = all(metrics[key]['recall'] == 1 for key in ['body_text', 'slide_count'])
        return dict(id=row['id'], **current, exact=exact, recovered=recovered, passed=exact or recovered, metrics=metrics)

    records = []
    with ThreadPoolExecutor(max_workers=2) as pool, (run / 'semantic.jsonl').open('w', buffering=1) as stream:
        for i, record in enumerate(pool.map(check, rows)):
            records.append(record)
            stream.write(json.dumps(record) + '\n')
            if not record['exact']:
                print('RECOVERED' if record['recovered'] else 'MISMATCH', record['id'], flush=True)
            if i % 50 == 0:
                print(i, len(rows), flush=True)
    summary = dict(total=len(records), passed=sum(r['passed'] for r in records), exact=sum(r['exact'] for r in records), recoveries=[dict(id=r['id'], metrics=r['metrics']) for r in records if r['recovered']], failures=[r['id'] for r in records if not r['passed']], baseline_label=args.reference_label, baseline_sha256=proof['candidate_sha256'], candidate_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), source_sha256={str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for folder in ['rust-pptx', 'rust-office-art', 'rust-office-charts'] for p in (ROOT / 'crates' / folder).rglob('*.rs')})
    (run / 'semantic-summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps({k: v for k, v in summary.items() if k != 'source_sha256'}))
    assert summary['passed'] == summary['total']


if __name__ == '__main__':
    main()

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path

from feedback import OUT, invoke


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--baseline', required=True)
    parser.add_argument('--baseline-proof', default='legacy-features-summary.json')
    parser.add_argument('--baseline-records', default='legacy-features.jsonl')
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--jobs', type=int, choices=range(1, 5), default=1)
    args = parser.parse_args()
    run, prior = [OUT / 'experiments' / label for label in [args.label, args.baseline]]
    binary, baseline = run / 'candidate', prior / 'candidate'
    proof = json.loads((prior / args.baseline_proof).read_text())
    assert proof.get('passed', proof.get('equal')) == proof['total']
    assert hashlib.sha256(baseline.read_bytes()).hexdigest() == proof['candidate_sha256']
    records_path = prior / args.baseline_records
    assert hashlib.sha256(records_path.read_bytes()).hexdigest() == proof['records_sha256']
    for name, expected in proof.get('review_sha256', {}).items():
        assert hashlib.sha256((prior / name).read_bytes()).hexdigest() == expected
    previous = {row['id']: row for row in map(json.loads, records_path.read_text().splitlines())}
    identity = json.loads((run / 'identity.json').read_text())
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == identity['binary_sha256']
    rows = json.loads(args.manifest.read_text())
    assert len(rows) == len({row['id'] for row in rows})
    assert all(row['id'] in previous for row in rows)
    destination = run / 'parity.jsonl'
    if destination.exists():
        raise SystemExit('Preserve existing audit receipts.')
    fields = lambda row: (row['status'], row.get('full_sha256'), row.get('error'))
    def evaluate(row):
        assert hashlib.sha256(Path(row['path']).read_bytes()).hexdigest() == row['sha256']
        current = invoke(binary, 'duke-pptx', row, run / (row['id'] + '.compact.json.gz'), 0)
        old = previous[row['id']]
        equal = fields(current) == fields(old)
        current.update(id=row['id'], equal=equal)
        if equal and 'artwork_hashes' in old:
            current['artwork_hashes'] = old['artwork_hashes']
        return current

    records = []
    with destination.open('w', buffering=1) as stream, ThreadPoolExecutor(max_workers=args.jobs) as pool:
        for index, current in enumerate(pool.map(evaluate, rows)):
            records.append(current)
            stream.write(json.dumps(current) + '\n')
            if not current['equal']:
                print('MISMATCH', current['id'], flush=True)
            if index % 25 == 0:
                print(index, len(rows), flush=True)
    summary = dict(total=len(rows), equal=sum(row['equal'] for row in records), candidate_sha256=identity['binary_sha256'], baseline_sha256=proof['candidate_sha256'], baseline_path=str(baseline), baseline_manifest_count=proof['total'], manifest_path=str(args.manifest), manifest_sha256=hashlib.sha256(args.manifest.read_bytes()).hexdigest(), records_sha256=hashlib.sha256(destination.read_bytes()).hexdigest(), failures=[row['id'] for row in records if not row['equal']], scope='Exact full canonical model hash and exact failure status/error on the explicitly selected manifest. A tuning pilot is not complete-corpus validation.')
    summary.update(jobs=args.jobs, harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())
    (run / 'parity-summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary))
    if summary['failures']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()

import argparse
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
    args = parser.parse_args()
    run, prior = [OUT / 'experiments' / label for label in [args.label, args.baseline]]
    binary, baseline = run / 'candidate', prior / 'candidate'
    proof = json.loads((prior / args.baseline_proof).read_text())
    assert proof.get('passed', proof.get('equal')) == proof['total']
    assert hashlib.sha256(baseline.read_bytes()).hexdigest() == proof['candidate_sha256']
    records_path = prior / args.baseline_records
    assert hashlib.sha256(records_path.read_bytes()).hexdigest() == proof['records_sha256']
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
    records = []
    with destination.open('w', buffering=1) as stream:
        for index, row in enumerate(rows):
            assert hashlib.sha256(Path(row['path']).read_bytes()).hexdigest() == row['sha256']
            current = invoke(binary, 'rust-pptx', row, run / (row['id'] + '.compact.json.gz'), 0)
            old = previous[row['id']]
            equal = fields(current) == fields(old)
            current.update(id=row['id'], equal=equal)
            if equal and 'artwork_hashes' in old:
                current['artwork_hashes'] = old['artwork_hashes']
            records.append(current)
            stream.write(json.dumps(current) + '\n')
            if not equal:
                print('MISMATCH', row['id'], flush=True)
            if index % 25 == 0:
                print(index, len(rows), flush=True)
    summary = dict(total=len(rows), equal=sum(row['equal'] for row in records), candidate_sha256=identity['binary_sha256'], baseline_sha256=proof['candidate_sha256'], baseline_path=str(baseline), baseline_manifest_count=proof['total'], manifest_path=str(args.manifest), manifest_sha256=hashlib.sha256(args.manifest.read_bytes()).hexdigest(), records_sha256=hashlib.sha256(destination.read_bytes()).hexdigest(), failures=[row['id'] for row in records if not row['equal']], scope='Exact full canonical model hash and exact failure status/error on the explicitly selected manifest. A tuning pilot is not complete-corpus validation.')
    (run / 'parity-summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary))
    if summary['failures']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()

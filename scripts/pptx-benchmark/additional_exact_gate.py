import argparse
import hashlib
import json
from pathlib import Path

from feedback import OUT, invoke


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--label', required=True); parser.add_argument('--baseline', required=True); parser.add_argument('--manifest', required=True, type=Path); parser.add_argument('--name', required=True); args = parser.parse_args()
    run = OUT / 'experiments' / args.label; target = run / args.name; target.mkdir()
    previous = OUT / 'experiments' / args.baseline
    binary, baseline = run / 'candidate', previous / 'candidate'
    digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    for directory in [run, previous]:
        assert digest(directory / 'candidate') == json.loads((directory / 'identity.json').read_text())['binary_sha256']
    rows = [r for r in json.loads(args.manifest.read_text()) if r['split'] == 'tuning']
    records = []
    output = target / 'records.jsonl'
    with output.open('w', buffering=1) as stream:
        for index, row in enumerate(rows):
            assert digest(Path(row['path'])) == row['sha256']
            old = invoke(baseline, 'duke-slides', row, target / (row['id'] + '.baseline.json.gz'), 0)
            current = invoke(binary, 'duke-slides', row, target / (row['id'] + '.compact.json.gz'), 0)
            identity = lambda r: (r['status'], r.get('full_sha256'), r.get('error'))
            current.update(id=row['id'], equal=identity(current) == identity(old), baseline=old)
            records.append(current); stream.write(json.dumps(current) + '\n')
            if index % 25 == 0 or not current['equal']:
                print(index, len(rows), current['equal'], flush=True)
    summary = dict(total=len(rows), equal=sum(r['equal'] for r in records), candidate_sha256=digest(binary), baseline_sha256=digest(baseline), manifest_sha256=digest(args.manifest), records_sha256=digest(output), harness_sha256=digest(Path(__file__)), scope='Additional tuning-only corpus: exact complete canonical output and error comparison against the frozen baseline. This is regression equivalence, not independently established extraction accuracy.')
    (target / 'summary.json').write_text(json.dumps(summary, indent=2)); (target / 'manifest.json').write_text(json.dumps(rows, indent=2)); print(json.dumps(summary)); assert summary['equal'] == summary['total']


if __name__ == '__main__':
    main()

import argparse
import gzip
import hashlib
import json
from pathlib import Path

from feedback import OUT, ROOT, invoke


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--baseline', required=True)
    parser.add_argument('--manifest', type=Path, required=True)
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    prior = OUT / 'experiments' / args.baseline
    binary = run / 'candidate'
    proof = json.loads((prior / 'parity-summary.json').read_text())
    previous_path = prior / 'parity.jsonl'
    assert proof['equal'] == proof['total']
    assert digest(prior / 'candidate') == proof['candidate_sha256']
    assert digest(previous_path) == proof['records_sha256']
    previous = {row['id']: row for row in map(json.loads, previous_path.read_text().splitlines())}
    identity = json.loads((run / 'identity.json').read_text())
    assert digest(binary) == identity['binary_sha256']
    assert all(digest(ROOT / path) == value for path, value in identity['sources'].items())
    rows = json.loads(args.manifest.read_text())
    assert len(rows) == len({row['id'] for row in rows})
    assert all(row['id'] in previous for row in rows)
    harness = {
        str(path.relative_to(ROOT)): digest(path)
        for path in [Path(__file__).resolve(), ROOT / 'scripts/pptx-benchmark/src/main.rs',
                     ROOT / 'scripts/pptx-benchmark/Cargo.lock']
    }
    destination = run / 'source-identity.jsonl'
    records = []
    with destination.open('x', buffering=1) as stream:
        for index, row in enumerate(rows):
            assert digest(Path(row['path'])) == row['sha256']
            output = run / 'identity-current.json.gz'
            current = invoke(binary, 'duke-slides', row, output, 0, 'identity')
            old = previous[row['id']]
            passed = current['status'] == old['status']
            if current['status'] == 'ok':
                with gzip.open(output, 'rt') as source:
                    model = json.load(source)
                expected = 'pptx-v1:sha256:' + row['sha256']
                passed = passed and model.get('documentId') == expected
                passed = passed and current.get('source_identity_matches') is True
                passed = passed and current.get('without_document_identity_sha256') == old.get('full_sha256')
            else:
                passed = passed and current.get('error') == old.get('error')
            current.update(id=row['id'], passed=passed)
            records.append(current)
            stream.write(json.dumps(current) + '\n')
            if output.exists():
                if passed:
                    output.unlink()
                else:
                    output.rename(run / (row['id'] + '.identity-mismatch.json.gz'))
            if not passed:
                print('MISMATCH', row['id'], flush=True)
            if index % 25 == 0:
                print(index, len(rows), flush=True)
    assert digest(binary) == identity['binary_sha256']
    assert all(digest(ROOT / path) == value for path, value in identity['sources'].items())
    assert all(digest(ROOT / path) == value for path, value in harness.items())
    summary = {
        'total': len(records),
        'passed': sum(row['passed'] for row in records),
        'candidate_sha256': identity['binary_sha256'],
        'baseline_sha256': proof['candidate_sha256'],
        'baseline_path': str(prior / 'candidate'),
        'manifest_path': str(args.manifest),
        'manifest_sha256': digest(args.manifest),
        'records_sha256': digest(destination),
        'harness_sha256': harness,
        'failures': [row['id'] for row in records if not row['passed']],
        'scope': 'The input-byte identity is independently checked. Removing only the new root documentId must restore the exact prior complete model hash; failures and error messages must remain exact. No timing or extraction completeness claim.',
    }
    (run / 'source-identity-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary))
    if summary['failures']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()

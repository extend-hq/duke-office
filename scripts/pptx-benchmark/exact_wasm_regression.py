import argparse
import gzip
import hashlib
import json
from pathlib import Path

from feedback import OUT


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--manifest', required=True, type=Path)
    parser.add_argument('--baseline-check', required=True)
    parser.add_argument('--current-check', required=True)
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    before_dir, after_dir = [OUT / name for name in [args.baseline_check, args.current_check]]
    destination = after_dir / 'exact-regression.json'
    if destination.exists():
        raise SystemExit('Preserve existing regression receipts.')
    proof = json.loads((run / 'parity-summary.json').read_text())
    assert proof['equal'] == proof['total']
    assert sha(run / 'candidate') == proof['candidate_sha256']
    assert sha(run / 'parity.jsonl') == proof['records_sha256']
    native = {row['id']: row for row in map(json.loads, (run / 'parity.jsonl').read_text().splitlines())}
    manifest = json.loads(args.manifest.read_text())
    expected_ids = {row['id'] for row in manifest}
    assert len(expected_ids) == len(manifest) and expected_ids <= native.keys()
    identities, records = [], []
    for directory in [before_dir, after_dir]:
        identity = json.loads((directory / 'identity.json').read_text())
        assert identity['manifest_sha256'] == sha(args.manifest)
        rows = json.loads((directory / 'records.json').read_text())
        assert len(rows) == len(manifest) == identity['inputs']
        keyed = {row['id']: row for row in rows}
        assert set(keyed) == expected_ids
        identities.append(identity)
        records.append(keyed)
    receipts = []
    for row in manifest:
        ident = row['id']
        assert sha(row['path']) == row['sha256']
        before, after = [values[ident] for values in records]
        assert before == after
        assert after['status'] == native[ident]['status']
        receipt = dict(id=ident, passed=True, status=after['status'])
        if after['status'] == 'ok':
            paths = [directory / (ident + '.json.gz') for directory in [before_dir, after_dir]]
            models = [json.load(gzip.open(path, 'rt')) for path in paths]
            assert models[0] == models[1]
            assert 'artwork_hashes' in before and 'artwork_hashes' in after
            receipt.update(artwork_hashes=len(after['artwork_hashes']),
                           compact_sha256=sha(paths[1]), previous_compact_sha256=sha(paths[0]))
        receipts.append(receipt)
    result = dict(total=len(receipts), passed=len(receipts),
                  exact_to_previous_wasm=len(receipts),
                  successful_models=sum(row['status'] == 'ok' for row in receipts),
                  artwork_hashes=sum(row.get('artwork_hashes', 0) for row in receipts),
                  baseline_identity=identities[0], candidate_identity=identities[1],
                  manifest_sha256=sha(args.manifest), native_proof_sha256=sha(run / 'parity-summary.json'),
                  baseline_records_sha256=sha(before_dir / 'records.json'),
                  candidate_records_sha256=sha(after_dir / 'records.json'),
                  harness_sha256=sha(__file__), receipts=receipts,
                  scope='Exact compact models, all generated-artwork hashes and error strings against the previous WASM build. Native acceptance also agrees. This regression gate does not waive any prior strict native/WASM difference or establish rendered fidelity.')
    destination.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({key: value for key, value in result.items() if key != 'receipts'}))


if __name__ == '__main__':
    main()

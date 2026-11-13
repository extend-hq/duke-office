import argparse
import gzip
import json

from artwork_math_gate import artwork_hashes
from compare_wasm import differences
from feedback import OUT, invoke as compact
from object_gate import invoke
from table_grid_review import sha, without_rows

read = lambda path: json.load(gzip.open(path, 'rt'))


def main():
    p = argparse.ArgumentParser(); p.add_argument('--label', required=True); a = p.parse_args()
    run = OUT / 'experiments' / a.label
    native = run / 'candidate'; prior_native = OUT / 'experiments/modern-comments2/candidate'
    proof = json.loads((run/'table-reviewed-summary.json').read_text())
    assert proof['passed'] == proof['total'] and sha(native) == proof['candidate_sha256']
    assert sha(run/'table-reviewed.jsonl') == proof['records_sha256']
    records = {r['id']: r for r in map(json.loads, (run/'table-reviewed.jsonl').read_text().splitlines())}
    previous = OUT/'wasm-check-modern-comments2'
    current = OUT/('wasm-check-' + a.label)
    old = {r['id']: r for r in json.loads((previous/'records.json').read_text())}
    new = {r['id']: r for r in json.loads((current/'records.json').read_text())}
    manifest = {r['id']: r for r in json.loads((OUT/'wasm-bom-cohort.json').read_text())}
    assert set(old) == set(new) == set(manifest)
    receipts, known = [], []
    for ident, row in manifest.items():
        assert old[ident]['status'] == new[ident]['status'] == records[ident]['status']
        equal = True
        if new[ident]['status'] == 'ok':
            before, after = read(previous/(ident+'.json.gz')), read(current/(ident+'.json.gz'))
            assert dict(old[ident]['artwork_hashes']) == dict(new[ident]['artwork_hashes'])
            equal = before == after
            if not equal:
                assert not records[ident]['equal'] and without_rows(before) == without_rows(after)
                result = compact(native, 'duke-slides', row, run/'wasm-native.current.json.gz', 0)
                assert result['full_sha256'] == records[ident]['full_sha256']
                diff = differences(read(run/'wasm-native.current.json.gz'), after)
                if diff:
                    compact(prior_native, 'duke-slides', row, run/'wasm-native.previous.json.gz', 0)
                    previous_diff = differences(read(run/'wasm-native.previous.json.gz'), before)
                    assert sorted(diff) == sorted(previous_diff)
                    known.append(ident)
            else:
                assert records[ident]['equal']
        else:
            assert old[ident]['error'] == new[ident]['error']
        receipts.append(dict(id=ident, exact_to_previous_wasm=equal, passed=True))
    assert known == ['0bd000a429c378fe']
    result = dict(total=len(receipts), passed=len(receipts), exact_to_previous_wasm=sum(r['exact_to_previous_wasm'] for r in receipts), changed_tables=sum(not r['exact_to_previous_wasm'] for r in receipts), strict_native_wasm_known_failures=known, native_review_sha256=sha(run/'table-reviewed-summary.json'), wasm_identity=json.loads((current/'identity.json').read_text()), candidate_sha256=sha(native), harness_sha256=sha(__file__), receipts=receipts, scope='Exact prior WASM models, artwork and errors except native-matched table grids. The existing strict native/WASM curved-3D numerical mismatch is independently replayed and remains unchanged, not waived.')
    (current/'regression.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k != 'receipts'}))
    for cohort, folder in [(run/'table-probes/manifest.json', OUT/('wasm-check-'+a.label+'-probes')), (run/'wasm-extra-cohort.json', OUT/('wasm-check-'+a.label+'-extra'))]:
        observed = {r['id']: r for r in json.loads((folder/'records.json').read_text())}
        checks = []
        for row in json.loads(cohort.read_text()):
            assert sha(row['path']) == row['sha256']
            full, document = invoke(native, row, run/'wasm-extra.full.json.gz')
            result = compact(native, 'duke-slides', row, run/'wasm-extra.compact.json.gz', 0)
            assert full['status'] == result['status'] == observed[row['id']]['status'] == 'ok'
            assert full['full_sha256'] == result['full_sha256']
            assert read(run/'wasm-extra.compact.json.gz') == read(folder/(row['id']+'.json.gz'))
            hashes = dict(artwork_hashes(document))
            assert hashes == dict(observed[row['id']]['artwork_hashes'])
            checks.append(dict(id=row['id'], passed=True, artwork=len(hashes), full_sha256=full['full_sha256']))
        summary = dict(total=len(checks), passed=len(checks), receipts=checks, wasm_identity=json.loads((folder/'identity.json').read_text()), candidate_sha256=sha(native), cohort_sha256=sha(cohort), harness_sha256=sha(__file__))
        (folder/'comparison.json').write_text(json.dumps(summary,indent=2)+'\n')
        print(json.dumps({k:v for k,v in summary.items() if k != 'receipts'}))
    for path in run.glob('wasm-*.json.gz'):
        path.unlink()


if __name__ == '__main__':
    main()

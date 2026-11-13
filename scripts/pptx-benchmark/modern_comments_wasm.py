import argparse
import gzip
import hashlib
import json
from pathlib import Path

from artwork_math_gate import artwork_hashes
from comments_review import without_comments
from feedback import OUT, invoke

sha = lambda p: hashlib.sha256(Path(p).read_bytes()).hexdigest()
read = lambda p: json.load(gzip.open(p, 'rt'))


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--label', required=True); a = parser.parse_args()
    run = OUT / 'experiments' / a.label; native = run / 'candidate'
    assert sha(native) == json.loads((run/'identity.json').read_text())['binary_sha256']
    probes = run/'modern-probes'; wasm_probes = OUT/('wasm-check-' + a.label + '-probes')
    wasm_records = {r['id']: r for r in json.loads((wasm_probes/'records.json').read_text())}
    probe_receipts = []
    for row in json.loads((probes/'manifest.json').read_text()):
        result = invoke(native, 'duke-pptx', row, probes/(row['id']+'.compact.json.gz'), 0)
        other = wasm_records[row['id']]; assert result['status'] == other['status']
        if result['status'] == 'ok':
            assert read(probes/(row['id']+'.compact.json.gz')) == read(wasm_probes/(row['id']+'.json.gz'))
            assert dict(artwork_hashes(read(probes/(row['id']+'.json.gz')))) == dict(other['artwork_hashes'])
        probe_receipts.append(dict(id=row['id'], passed=True))
    (run/'wasm-probe-comparison.json').write_text(json.dumps(dict(total=len(probe_receipts), passed=len(probe_receipts), candidate_sha256=sha(native), wasm_identity=json.loads((wasm_probes/'identity.json').read_text()), receipts=probe_receipts),indent=2)+'\n')
    before_dir = OUT/'wasm-check-xml-recent1'; after_dir = OUT/('wasm-check-' + a.label)
    baseline = {r['id']: r for r in json.loads((before_dir/'records.json').read_text())}
    current = {r['id']: r for r in json.loads((after_dir/'records.json').read_text())}
    manifest = {r['id']: r for r in json.loads((OUT/'wasm-bom-cohort.json').read_text())}
    proof = json.loads((run/'modern-reviewed-summary.json').read_text()); assert proof['passed'] == proof['total'] and sha(run/'modern-reviewed.jsonl') == proof['records_sha256']
    native_records = {r['id']: r for r in map(json.loads,(run/'modern-reviewed.jsonl').read_text().splitlines())}
    assert set(baseline) == set(current) == set(manifest) and set(current) <= set(native_records)
    receipts = []
    for ident in current:
        old, new = baseline[ident], current[ident]; assert old['status'] == new['status'] == native_records[ident]['status']
        equal = True
        if new['status'] == 'ok':
            before, after = read(before_dir/(ident+'.json.gz')), read(after_dir/(ident+'.json.gz'))
            assert dict(old['artwork_hashes']) == dict(new['artwork_hashes'])
            equal = before == after
            if not equal:
                assert not native_records[ident]['equal']
                assert without_comments(before) == without_comments(after)
                result = invoke(native, 'duke-pptx', manifest[ident], run/(ident+'.compact.json.gz'), 0)
                assert result['full_sha256'] == native_records[ident]['full_sha256']
                assert read(run/(ident+'.compact.json.gz')) == after
            else:
                assert native_records[ident]['equal']
        else:
            assert old['error'] == new['error']
        receipts.append(dict(id=ident, exact_to_previous_wasm=equal, passed=True))
    prior = json.loads((before_dir/'comparison.json').read_text())
    known = [r['id'] for r in prior['rows'] if not r['status_equal'] or r.get('semantic_equal') is False]
    assert not set(known) & {r['id'] for r in receipts if not r['exact_to_previous_wasm']}
    result = dict(total=len(receipts), passed=len(receipts), exact_to_previous_wasm=sum(r['exact_to_previous_wasm'] for r in receipts), changed_comments=sum(not r['exact_to_previous_wasm'] for r in receipts), strict_native_wasm_known_failures=known, prior_native_wasm_comparison_sha256=sha(before_dir/'comparison.json'), native_review_sha256=sha(run/'modern-reviewed-summary.json'), wasm_identity=json.loads((after_dir/'identity.json').read_text()), candidate_sha256=sha(native), harness_sha256=sha(__file__), receipts=receipts, scope='Exact prior WASM models, artwork and errors, except native-matched modern comments. Native full-output source audit covers every input. The preceding strict native/WASM numerical failure remains unchanged, not waived.')
    (after_dir/'regression.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k!='receipts'}))


if __name__ == '__main__':
    main()

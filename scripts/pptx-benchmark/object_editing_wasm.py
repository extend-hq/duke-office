import argparse
import gzip
import hashlib
import json
from pathlib import Path

from artwork_math_gate import artwork_hashes
from feedback import OUT, invoke as compact
from object_gate import invoke
from object_editing_review import audit_invoke, sha, verify, without_editing

read = lambda p: json.load(gzip.open(p, 'rt'))


def changes(before, after, path=''):
    number = lambda value: type(value) in (int, float)
    if number(before) and number(after):
        return [] if before == after else [path]
    if type(before) is not type(after):
        return [path]
    if isinstance(before, dict):
        return [p for key in before.keys() | after.keys() for p in (changes(before[key], after[key], path+'/'+key) if key in before and key in after else [path+'/'+key])]
    if isinstance(before, list):
        if len(before) != len(after):
            return [path+'/length']
        return [p for i, (a, b) in enumerate(zip(before, after)) for p in changes(a, b, path+'/'+str(i))]
    return [] if before == after else [path]


def editing_hashes(value, location='', result=None):
    result = [] if result is None else result
    if isinstance(value, dict):
        for key, child in value.items():
            if key in {'data', 'bytes'}:
                continue
            if key == 'editing':
                result.append([location+'/editing/sourceXml', hashlib.sha256(child['sourceXml'].encode()).hexdigest()])
            elif isinstance(child, (dict, list)):
                editing_hashes(child, location+'/'+key, result)
    elif isinstance(value, list):
        for index, child in enumerate(value):
            editing_hashes(child, location+'/'+str(index), result)
    return result


def compact_model(value):
    if isinstance(value, dict):
        for key in ['sourceXml', 'shapeSvg', 'pictureSvg', 'chartXml', 'chartSvg', 'svg', 'source_xml']:
            if key in value:
                raw = value.pop(key)
                value[key+'Bytes'] = len(raw.encode()) if isinstance(raw, str) else 0
        for key in ['data', 'bytes']:
            raw = value.get(key)
            if isinstance(raw, list) and all(type(x) is int and 0 <= x < 256 for x in raw):
                value[key+'Sha256'] = hashlib.sha256(bytes(raw)).hexdigest()
                del value[key]
        for child in value.values():
            if isinstance(child, (dict, list)):
                compact_model(child)
    elif isinstance(value, list):
        for child in value:
            if isinstance(child, (dict, list)):
                compact_model(child)
    return value


def main():
    p = argparse.ArgumentParser(); p.add_argument('--label', required=True)
    p.add_argument('--probes-only', action='store_true'); a = p.parse_args()
    run = OUT/'experiments'/a.label
    assert sha(run/'candidate') == json.loads((run/'identity.json').read_text())['binary_sha256']
    audit_identity = json.loads((run/'audit-build-identity.json').read_text())
    assert sha(run/'editing-audit') == audit_identity['binary_sha256']
    assert audit_identity['source_sha256'] == json.loads((run/'identity.json').read_text())['sources']
    probes = run/'editing-probes'
    wasm_probes = OUT/('wasm-check-'+a.label+'-probes')
    records = {r['id']: r for r in json.loads((wasm_probes/'records.json').read_text())}
    receipts = []
    for row in json.loads((probes/'manifest.json').read_text()):
        assert sha(row['path']) == row['sha256']
        native = read(probes/(row['id']+'.json.gz'))
        current = records[row['id']]
        assert current['status'] == 'ok'
        assert read(probes/(row['id']+'.compact.json.gz')) == read(wasm_probes/(row['id']+'.json.gz'))
        assert dict(artwork_hashes(native)) == dict(current['artwork_hashes'])
        assert dict(editing_hashes(native)) == dict(current['editing_source_hashes'])
        counts = verify(row['path'], native)
        assert compact_model(native) == read(probes/(row['id']+'.compact.json.gz'))
        receipts.append(dict(id=row['id'], passed=True, counts=dict(counts)))
    (run/'wasm-probes-comparison.json').write_text(json.dumps(dict(total=len(receipts), passed=len(receipts), receipts=receipts, native_sha256=sha(run/'candidate'), wasm_identity=json.loads((wasm_probes/'identity.json').read_text()), review_harness_sha256=sha(Path(__file__).with_name('object_editing_review.py'))),indent=2)+'\n')
    if a.probes_only:
        print(json.dumps(dict(total=len(receipts), passed=len(receipts))))
        return
    native_records = {}
    for prefix in ['editing-reviewed', 'sdk-reviewed']:
        proof = json.loads((run/(prefix+'-summary.json')).read_text())
        assert proof['passed'] == proof['total'] and proof['candidate_sha256'] == sha(run/'candidate')
        assert proof['records_sha256'] == sha(run/(prefix+'.jsonl'))
        native_records.update({r['id']: r for r in map(json.loads, (run/(prefix+'.jsonl')).read_text().splitlines())})
    prior_comparison = json.loads((OUT/'experiments/xml-indexed1/native-wasm-compact.json').read_text())
    known = {r['id']: r['differences'] for r in prior_comparison['strict_compact_failures']}
    receipts, strict_failures = [], []
    path = run/'wasm-regression.jsonl'; assert not path.exists()
    artwork_count, editing_count, success = 0, 0, 0
    with path.open('w', buffering=1) as stream:
        for suffix, manifest in [('', OUT/'wasm-bom-cohort.json'), ('-extra', OUT/'experiments/table-grid1/wasm-extra-cohort.json')]:
            before_dir = OUT/('wasm-check-xml-indexed1'+suffix)
            after_dir = OUT/('wasm-check-'+a.label+suffix)
            before_records = {r['id']: r for r in json.loads((before_dir/'records.json').read_text())}
            after_records = {r['id']: r for r in json.loads((after_dir/'records.json').read_text())}
            rows = json.loads(manifest.read_text())
            assert set(before_records) == set(after_records) == {r['id'] for r in rows}
            for index, row in enumerate(rows):
                ident = row['id']; old, new = before_records[ident], after_records[ident]
                assert sha(row['path']) == row['sha256']
                assert old['status'] == new['status'] == native_records[ident]['status'], ident
                receipt = dict(id=ident, status=new['status'], passed=True)
                if new['status'] == 'ok':
                    before, after = read(before_dir/(ident+'.json.gz')), read(after_dir/(ident+'.json.gz'))
                    assert without_editing(after) == before, ident
                    assert dict(old['artwork_hashes']) == dict(new['artwork_hashes']), ident
                    native_record, artifact = audit_invoke(run/'editing-audit', row, run/'wasm-native.audit.json.gz')
                    assert native_record['full_sha256'] == native_records[ident]['full_sha256'], ident
                    hashes = {}
                    def at(value, location):
                        for key in location.split('/')[1:]:
                            value = value[int(key)] if isinstance(value, list) else value[key]
                        return value
                    for exported in artifact['nodes']:
                        node, location = exported['node'], exported['path']
                        other = at(after, location)
                        if 'editing' in node:
                            raw = node['editing']['sourceXml']
                            hashes[location+'/editing/sourceXml'] = hashlib.sha256(raw.encode()).hexdigest()
                            assert json.dumps(compact_model(dict(node['editing'])), sort_keys=True) == json.dumps(other.get('editing'), sort_keys=True), (ident, location)
                        else:
                            assert 'editing' not in other, (ident, location)
                    assert hashes == dict(new['editing_source_hashes']), ident
                    editing_count += len(new['editing_source_hashes'])
                    artwork_count += len(new['artwork_hashes'])
                    native_compact = artifact['model']
                    without_new = without_editing(after)
                    equal = native_compact == without_new
                    receipt['strict_native_compact_equal'] = equal
                    if not equal:
                        differences = changes(native_compact, without_new)
                        assert ident in known and differences == [r[0] for r in known[ident]], (ident, differences)
                        values = [[location,at(native_compact,location),at(without_new,location)] for location in differences]
                        assert values == known[ident]
                        strict_failures.append(dict(id=ident, differences=values, unchanged_from_baseline=True))
                    success += 1
                else:
                    assert old.get('error') == new.get('error') == native_records[ident].get('error'), ident
                receipts.append(receipt); stream.write(json.dumps(receipt)+'\n')
                if index % 25 == 0:
                    print(suffix or 'main', index, len(rows), flush=True)
    assert {r['id'] for r in strict_failures} == set(known)
    summary = dict(total=len(receipts), passed=len(receipts), successful_models=success, strict_compact_matches=success-len(strict_failures), strict_compact_failures=strict_failures, prior_wasm_artwork_hashes_exact=artwork_count, native_wasm_editing_source_hashes_exact=editing_count, candidate_sha256=sha(run/'candidate'), audit_binary_sha256=sha(run/'editing-audit'), wasm_sha256=sha(OUT/('wasm-'+a.label)/'pptx_wasm_bg.wasm'), records_sha256=sha(path), harness_sha256=sha(__file__), scope='Every preexisting WASM field, artwork hash and error is exact. New typed editing values and complete retained XML match native; full native outputs are source-reviewed. The audit binary matches every complete frozen-candidate output hash. The prior strict native/WASM artwork-size discrepancy remains unchanged and is reported as a failure, not waived.')
    (run/'wasm-regression-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    for suffix in ['full', 'compact', 'audit']:
        (run/('wasm-native.'+suffix+'.json.gz')).unlink(missing_ok=True)
    print(json.dumps(summary), flush=True)


if __name__ == '__main__':
    main()

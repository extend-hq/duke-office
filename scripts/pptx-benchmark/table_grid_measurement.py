import argparse
import copy
import gzip
import json
from pathlib import Path

from feedback import OUT, ROOT, invoke
from table_grid_review import sha

read = lambda p: json.load(gzip.open(p, 'rt'))


def table_order_only(before, after):
    a, b = copy.deepcopy(before), copy.deepcopy(after)
    counts = []
    for first, second in zip(a['sections'], b['sections']):
        positions = [i for i, block in enumerate(first['content']) if block['type'] == 'Table']
        other = [i for i, block in enumerate(second['content']) if block['type'] == 'Table']
        assert positions == other
        left = sorted((first['content'][i] for i in positions), key=lambda x: json.dumps(x, sort_keys=True))
        right = sorted((second['content'][i] for i in positions), key=lambda x: json.dumps(x, sort_keys=True))
        assert left == right
        for i in positions:
            first['content'][i] = second['content'][i] = None
        counts.append(len(positions))
    assert a == b
    return sum(counts)


def main():
    p = argparse.ArgumentParser(); p.add_argument('--phase', required=True); p.add_argument('--label', required=True)
    p.add_argument('--baseline-phase', default='expanded-modern-comments')
    p.add_argument('--exact-gate', action='store_true')
    p.add_argument('--editing-source-label')
    a = p.parse_args()
    run, experiment = OUT/a.phase, OUT/'experiments'/a.label
    previous = OUT/a.baseline_phase
    identity = json.loads((run/'identity.json').read_text())
    assert 'end' in identity and sha(run/'parser-benchmark') == identity['binary_sha256'] == sha(experiment/'candidate')
    assert sha(run/'peer-benchmark') == identity['peer_binary_sha256']
    assert all(sha(ROOT/name) == digest for name,digest in identity['source_sha256'].items())
    assert all(sha(Path(__file__).parent/name) == digest for name,digest in identity['harness_sha256'].items())
    rows = [json.loads(s) for s in (run/'records.jsonl').read_text().splitlines()]
    old = {(r['id'],r['engine']):r for r in map(json.loads,(previous/'records.jsonl').read_text().splitlines())}
    current = {(r['id'],r['engine']):r for r in rows}
    assert len(current) == len(rows) == len(old) and set(current) == set(old)
    if a.exact_gate and a.editing_source_label:
        raise SystemExit('Choose exact previous output or the source-reviewed editing change.')
    review_name = 'parity' if a.exact_gate or a.editing_source_label else 'table-reviewed'
    audit = json.loads((experiment/(review_name+'-summary.json')).read_text())
    assert audit.get('equal', audit.get('passed')) == audit['total']
    assert audit['candidate_sha256'] == identity['binary_sha256']
    assert sha(experiment/(review_name+'.jsonl')) == audit['records_sha256']
    reviewed = {r['id']:r for r in map(json.loads,(experiment/(review_name+'.jsonl')).read_text().splitlines())}
    fields = lambda r: (r['status'],r.get('full_sha256'),r.get('error'))
    editing_proof = None
    if a.editing_source_label:
        source_run = OUT/'experiments'/a.editing_source_label
        editing_proof = json.loads((source_run/'editing-reviewed-summary.json').read_text())
        source_path = source_run/'editing-reviewed.jsonl'
        assert editing_proof['passed'] == editing_proof['total']
        assert sha(source_path) == editing_proof['records_sha256']
        assert sha(source_run/'candidate') == editing_proof['candidate_sha256']
        assert sha(previous/'parser-benchmark') == editing_proof['baseline_sha256']
        source_rows = [json.loads(line) for line in source_path.read_text().splitlines()]
        assert len(source_rows) == editing_proof['total'] and all(r['passed'] for r in source_rows)
        assert {r['id']:fields(r) for r in source_rows} == {ident:fields(r) for ident,r in reviewed.items()}
        prior_reference = OUT/'experiments/xml-indexed1/parity.jsonl'
        assert sha(prior_reference) == editing_proof['reference_records_sha256']
        prior_rows = {r['id']:r for r in map(json.loads,prior_reference.read_text().splitlines())}
        assert all(fields(r) == fields(prior_rows[ident]) for (ident,engine),r in old.items() if engine == 'rust-pptx')
    changes, native_changes = [], []
    for (ident,engine), record in current.items():
        before = old[ident,engine]
        assert before['status'] == record['status']
        if engine == 'rust-pptx':
            assert fields(record) == fields(reviewed[ident])
            if a.exact_gate:
                assert fields(record) == fields(before)
            if fields(record) != fields(before):
                native_changes.append(ident)
        elif record['status'] == 'ok':
            left, right = previous/'native'/(ident+'.'+engine+'.json.gz'), run/'native'/(ident+'.'+engine+'.json.gz')
            if left.stat().st_ino == right.stat().st_ino:
                continue
            x, y = read(left), read(right)
            if x != y:
                assert engine == 'undoc'
                changes.append(dict(id=ident,only_table_block_permutation=True,table_blocks=table_order_only(x,y)))
    (run/'undoc-permutation-review.json').write_text(json.dumps(changes,indent=2)+'\n')
    manifest = {r['id']:r for r in json.loads((run/'cohort.json').read_text())}
    replay_dir = run/'undoc-replays'; replay_dir.mkdir(exist_ok=False)
    replay_receipts = []
    for change in changes:
        ident = change['id']; reference = read(run/'native'/(ident+'.undoc.json.gz'))
        observed = {json.dumps(reference,sort_keys=True)}
        for i in range(8):
            output = replay_dir/(ident+'.'+str(i)+'.json.gz')
            record = invoke(run/'parser-benchmark','undoc',manifest[ident],output,0)
            assert record['status'] == 'ok'
            result = read(output); table_order_only(reference,result)
            observed.add(json.dumps(result,sort_keys=True))
            replay_receipts.append(dict(id=ident,attempt=i,full_sha256=record['full_sha256']))
            if i >= 3 and len(observed) > 1:
                break
        assert len(observed) > 1
    replay = dict(inputs=len(changes),variable_inputs=len(changes),invocations=len(replay_receipts),receipts=replay_receipts,candidate_sha256=identity['binary_sha256'],scope='Untimed replays only. Table blocks vary in order; full table content and every other field remain unchanged. Raw measured output is retained.')
    (run/'undoc-replay-summary.json').write_text(json.dumps(replay,indent=2)+'\n')
    proof = dict(records=len(rows),unique_records=len(current),statuses_equal_to_previous=True,native_outputs_match_source_review=sum(r['engine']=='rust-pptx' for r in rows),native_changed_inputs=native_changes,peer_output_changes={'undoc_table_order_only':[r['id'] for r in changes]},sources_and_harness_unchanged_at_measurement_completion=True,candidate_sha256=identity['binary_sha256'],identity_sha256=sha(run/'identity.json'),records_sha256=sha(run/'records.jsonl'),harness_sha256=sha(__file__))
    proof.update(baseline_phase=a.baseline_phase, exact_native_output_gate=a.exact_gate,
                 native_review_sha256=sha(experiment/(review_name+'-summary.json')))
    if editing_proof:
        proof['editing_source_label'] = a.editing_source_label
        proof['editing_source_review_sha256'] = sha(source_run/'editing-reviewed-summary.json')
        proof['editing_only_native_changes'] = True
    (run/'measurement-verification.json').write_text(json.dumps(proof,indent=2)+'\n')
    print(json.dumps(proof))


if __name__ == '__main__':
    main()

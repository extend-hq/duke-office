import argparse
import collections
import gzip
import hashlib
import json
from pathlib import Path
import time

import svg_definition_gate as gate
from svg_definition_equivalence import equivalent_svg


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--follow', action='store_true')
    args = parser.parse_args()
    run = gate.OUT / 'experiments' / args.label
    path = run / 'recursive-review-partial.json'
    reviewed = {r['id']: r for r in json.loads(path.read_text())} if path.exists() else {}
    gate.equivalent_svg = equivalent_svg
    while True:
        records = [json.loads(line) for line in (run / 'semantic.jsonl').read_text().splitlines()]
        for record in records:
            if record['passed'] or record['id'] in reviewed:
                continue
            before_path = run / (record['id'] + '.before.json.gz')
            after_path = run / (record['id'] + '.after.json.gz')
            before = json.load(gzip.open(before_path, 'rt'))
            after = json.load(gzip.open(after_path, 'rt'))
            counts = collections.Counter()
            issues = gate.compare(before, after, counts)
            reviewed[record['id']] = dict(id=record['id'], issues=issues, counts=dict(counts), before_gzip_sha256=hashlib.sha256(before_path.read_bytes()).hexdigest(), after_gzip_sha256=hashlib.sha256(after_path.read_bytes()).hexdigest())
            path.write_text(json.dumps(list(reviewed.values()), indent=2))
            print(record['id'], 'remaining issues', len(issues), flush=True)
        if (run / 'semantic-summary.json').exists():
            break
        if not args.follow:
            raise SystemExit('Initial audit is still running; use --follow to wait for completion.')
        time.sleep(5)
    proof = json.loads((run / 'semantic-summary.json').read_text())
    for filename, expected in [(run / 'candidate', proof['candidate_sha256']), (run.parent / proof['baseline_label'] / 'candidate', proof['baseline_sha256'])]:
        if hashlib.sha256(filename.read_bytes()).hexdigest() != expected:
            raise SystemExit('Executable identity changed.')
    final = []
    counts = collections.Counter()
    for record in records:
        row = dict(record)
        if not row['passed']:
            review = reviewed[row['id']]
            row.update(passed=not review['issues'], counts=review['counts'], issues=review['issues'], recursive_review=True)
        c = row['counts']
        if 'unused_definitions_removed' in c:
            c['svg_fields_changed'] = c.pop('unused_definitions_removed')
        counts.update(c)
        final.append(row)
    (run / 'semantic-reviewed.jsonl').write_text(''.join(json.dumps(r) + '\n' for r in final))
    proof.update(passed=sum(r['passed'] for r in final), counts=dict(counts), recursive_reviews=len(reviewed), failures=[r['id'] for r in final if not r['passed']], review_scripts_sha256={name: hashlib.sha256(Path(__file__).with_name(name).read_bytes()).hexdigest() for name in ['svg_definition_gate.py', 'svg_definition_equivalence.py', 'review_svg_gate.py']}, initial_records_sha256=hashlib.sha256((run / 'semantic.jsonl').read_bytes()).hexdigest(), reviewed_records_sha256=hashlib.sha256((run / 'semantic-reviewed.jsonl').read_bytes()).hexdigest())
    if len(final) != proof['total'] or len({r['id'] for r in final}) != len(final):
        raise SystemExit('Audit is incomplete or contains duplicates.')
    (run / 'semantic-reviewed-summary.json').write_text(json.dumps(proof, indent=2))
    print(json.dumps({k: v for k, v in proof.items() if k != 'source_sha256'}))
    assert proof['passed'] == proof['total']


if __name__ == '__main__':
    main()

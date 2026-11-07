import argparse
import hashlib
import json

from feedback import OUT


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    raw = run / 'artwork-math.jsonl'
    summary = json.loads((run / 'artwork-math-summary.json').read_text())
    records = [json.loads(line) for line in raw.read_text().splitlines()]
    bounds = json.loads((run / 'bounds-review.json').read_text())['summary']
    assert len(records) == len({r['id'] for r in records}) == summary['total'] == bounds['inputs']
    assert hashlib.sha256(raw.read_bytes()).hexdigest() == summary['records_sha256'] == bounds['records_sha256']
    assert bounds['passed'] == bounds['changed_bounds']
    approved = set()
    reviewed_paths = [run / 'bounds-review.json']
    arc_path = run / 'degenerate-arc-review.json'
    if arc_path.exists():
        arc = json.loads(arc_path.read_text())
        assert arc['summary']['reviewed'] == arc['summary']['passed'] == len(arc['rows'])
        approved = {(r['id'], r['issue']) for r in arc['rows'] if r['passed']}
        reviewed_paths.append(arc_path)
    unexpected = [(r['id'], issue) for r in records for issue in r['issues'] if (r['id'], issue) not in approved]
    observed = {(r['id'], issue) for r in records for issue in r['issues']}
    assert approved <= observed
    result = dict(total=len(records), passed=sum(not r['issues'] or all((r['id'], issue) in approved for issue in r['issues']) for r in records), numerical_only=summary['numerical_only'], exact=summary['exact'], candidate_sha256=summary['candidate_sha256'], baseline_sha256=summary['baseline_sha256'], baseline_label=summary['baseline_label'], records_sha256=summary['records_sha256'], reviewed_changes=[dict(id=ident, path=issue, kind='independently checked zero-radius bracket geometry') for ident, issue in sorted(approved)], unexpected=unexpected, review_sha256={p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in reviewed_paths}, scope='All model changes are audited artwork numerical differences, reviewed paint-bound changes, or independently checked zero-radius bracket corrections. Native PowerPoint visual parity is not established.')
    (run / 'artwork-reviewed-summary.json').write_text(json.dumps(result, indent=2))
    print(json.dumps(result))
    if unexpected:
        raise SystemExit(1)


if __name__ == '__main__':
    main()

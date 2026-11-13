import argparse
import json
import os
import random
import statistics

from feedback import OUT, invoke
from table_grid_review import sha


def main():
    p = argparse.ArgumentParser(); p.add_argument('--label', required=True); a = p.parse_args()
    run = OUT/'experiments'/a.label
    baseline, candidate = OUT/'experiments/modern-comments2/candidate', run/'candidate'
    proof = json.loads((run/'table-reviewed-summary.json').read_text())
    assert proof['passed'] == proof['total']
    assert sha(candidate) == proof['candidate_sha256'] and sha(baseline) == proof['baseline_sha256']
    rows = json.loads((run/'table-reviewed-changed.json').read_text())
    assert len(rows) == proof['changed'] and all(sha(r['path']) == r['sha256'] for r in rows)
    plan = dict(selection='Every changed real input from the full table source review; no latency-based selection.', ids=[r['id'] for r in rows], repeats=7, seed=20261107, baseline_sha256=sha(baseline), candidate_sha256=sha(candidate), review_sha256=sha(run/'table-reviewed-summary.json'))
    path = run/'table-timings.jsonl'; assert not path.exists()
    (run/'table-timing-plan.json').write_text(json.dumps(plan,indent=2)+'\n')
    rng = random.Random(20261107); rng.shuffle(rows); records = []
    with path.open('w',buffering=1) as stream:
        for row in rows:
            jobs = [('baseline',baseline),('candidate',candidate)]; rng.shuffle(jobs)
            for label,binary in jobs:
                r = invoke(binary,'duke-pptx',row,'-',7); assert r['status'] == 'ok'
                r.update(id=row['id'],label=label,load=os.getloadavg()); records.append(r); stream.write(json.dumps(r)+'\n')
            print(row['id'],flush=True)
    values = {(r['id'],r['label']):statistics.median(r['samples_ms']) for r in records}
    ratios = [values[r['id'],'baseline']/values[r['id'],'candidate'] for r in rows]
    bootstrap = sorted(statistics.median(rng.choices(ratios,k=len(ratios))) for _ in range(4000))
    summary = dict(inputs=len(rows), before_ms=sum(values[r['id'],'baseline'] for r in rows), after_ms=sum(values[r['id'],'candidate'] for r in rows), median_paired_speedup=statistics.median(ratios), median_speedup_ci95=[bootstrap[100],bootstrap[3899]], plan=plan, records_sha256=sha(path), harness_sha256=sha(__file__), scope='All changed table-bearing tuning inputs. Complete parsing and model construction with paired warm medians. This cohort measures the cost of retained covered cells and corrected styling, not general or untouched-validation performance.')
    (run/'table-timing-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    print(json.dumps(summary))


if __name__ == '__main__':
    main()

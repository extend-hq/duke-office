import argparse
import hashlib
import json
import os
import random
import statistics
from pathlib import Path
from feedback import OUT, invoke

p = argparse.ArgumentParser(); p.add_argument('--label', required=True); a = p.parse_args()
run = OUT/'experiments'/a.label; baseline = OUT/'experiments/xml-recent1/candidate'; candidate = run/'candidate'
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
proof = json.loads((run/'modern-reviewed-summary.json').read_text())
assert proof['passed'] == proof['total'] and sha(candidate) == proof['candidate_sha256'] and sha(baseline) == proof['baseline_sha256']
rows = json.loads((run/'modern-cohort.json').read_text()); assert len(rows) == proof['changed']
assert all(sha(Path(r['path'])) == r['sha256'] for r in rows)
plan = dict(selection='Every real input changed by the independently reviewed modern-comment implementation; no latency-based selection.', ids=[r['id'] for r in rows], repeats=7, seed=20261107, baseline_sha256=sha(baseline), candidate_sha256=sha(candidate), review_sha256=sha(run/'modern-reviewed-summary.json'))
assert not (run/'comment-timings.jsonl').exists()
(run/'comment-timing-plan.json').write_text(json.dumps(plan,indent=2)+'\n')
rng = random.Random(20261107); rng.shuffle(rows); records=[]
with (run/'comment-timings.jsonl').open('w',buffering=1) as stream:
    for row in rows:
        jobs=[('baseline',baseline),('candidate',candidate)]; rng.shuffle(jobs)
        for label,binary in jobs:
            r=invoke(binary,'rust-pptx',row,'-',7);assert r['status']=='ok';r.update(id=row['id'],label=label,load=os.getloadavg());records.append(r);stream.write(json.dumps(r)+'\n')
values={(r['id'],r['label']):statistics.median(r['samples_ms']) for r in records}
summary=dict(inputs=len(rows),before_ms=sum(values[r['id'],'baseline'] for r in rows),after_ms=sum(values[r['id'],'candidate'] for r in rows),rows=[dict(id=r['id'],baseline_ms=values[r['id'],'baseline'],candidate_ms=values[r['id'],'candidate'],ratio=values[r['id'],'baseline']/values[r['id'],'candidate']) for r in rows],plan=plan,harness_sha256=sha(Path(__file__)),scope='Four feature-bearing tuning inputs. Complete parsing and full model construction; paired warm medians. This small cohort is not a general performance estimate.')
(run/'comment-timing-summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary))

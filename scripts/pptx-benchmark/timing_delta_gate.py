import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import shutil

from feedback import invoke, OUT, ROOT


def main():
    p=argparse.ArgumentParser();p.add_argument('--label',default='timing2');p.add_argument('--reference-label',default='timing1');args=p.parse_args()
    prior=OUT/'experiments'/args.reference_label;proof=json.loads((prior/'semantic-summary.json').read_text())
    if proof['passed']!=proof['total']:raise SystemExit('Source semantic gate must pass first.')
    run=OUT/'experiments'/args.label;run.mkdir(exist_ok=True);binary=run/'candidate';shutil.copy2(ROOT/'scripts/pptx-benchmark/target/release/pptx-benchmark',binary)
    originals={r['id']:r for r in map(json.loads,(prior/'semantic.jsonl').read_text().splitlines())}
    rows=json.loads((OUT/'manifest.json').read_text())+json.loads((OUT/'expanded-optimized/cohort.json').read_text());records=[]
    if hashlib.sha256((prior/'candidate').read_bytes()).hexdigest()!=proof['candidate_sha256']:raise SystemExit('Validated executable identity changed.')
    if any(hashlib.sha256(Path(r['path']).read_bytes()).hexdigest()!=r['sha256'] for r in rows):raise SystemExit('Input identity changed.')
    def compare(row):
        current=invoke(binary,'rust-pptx',row,run/(row['id']+'.compact.json.gz'),0);old=originals[row['id']]
        equal=current['status']==old['status'] and (current['status']!='ok' or current['full_sha256']==old['full_sha256'])
        if current['status']!='ok':
            old_error=invoke(prior/'candidate','rust-pptx',row,'-',0);equal=equal and old_error.get('error')==current.get('error')
        current.update(id=row['id'],equal=equal)
        return current
    with ThreadPoolExecutor(max_workers=2) as pool, (run/'parity.jsonl').open('w',buffering=1) as stream:
        for i,current in enumerate(pool.map(compare,rows)):
            records.append(current);stream.write(json.dumps(current)+'\n')
            if not current['equal']:print('MISMATCH',current['id'],flush=True)
            if i%50==0:print(i,len(rows),flush=True)
    result=dict(total=len(records),equal=sum(r['equal'] for r in records),candidate_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),baseline_sha256=hashlib.sha256((prior/'candidate').read_bytes()).hexdigest(),baseline_path=str(prior/'candidate'),semantic_reference_label=args.reference_label,failures=[r['id'] for r in records if not r['equal']],source_sha256={str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for folder in ['rust-pptx','rust-office-art','rust-office-charts'] for p in (ROOT/'crates'/folder).rglob('*.rs')})
    (run/'parity-summary.json').write_text(json.dumps(result,indent=2));print(json.dumps({k:v for k,v in result.items() if k!='source_sha256'}));assert result['equal']==result['total']

if __name__=='__main__':main()

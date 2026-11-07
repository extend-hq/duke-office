import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import shutil
import statistics
import subprocess
import time

ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'output/pptx-benchmark-20261106'

def invoke(binary,engine,row,output,repeats):
    result=subprocess.run([str(binary),engine,row['path'],str(output),str(repeats)],capture_output=True,text=True,timeout=120)
    if not result.stdout.strip():return dict(status='process_error',error=result.stderr[-2000:])
    return json.loads(result.stdout.splitlines()[-1])

def main():
    p=argparse.ArgumentParser();p.add_argument('--label',required=True);p.add_argument('--stage',choices=['parity','timing'],required=True);p.add_argument('--count',type=int,default=12);p.add_argument('--split',choices=['tuning','holdout','full'],default='tuning');p.add_argument('--skip-build',action='store_true');p.add_argument('--reference',type=Path);p.add_argument('--reference-records',type=Path);args=p.parse_args()
    run=OUT/'experiments'/args.label;run.mkdir(parents=True,exist_ok=True)
    candidate=run/'candidate';baseline=args.reference or OUT/'baseline-parity-binary';built=ROOT/'scripts/pptx-benchmark/target/release/pptx-benchmark'
    manifest=json.loads((OUT/'manifest.json').read_text());by_id={r['id']:r for r in manifest};split=json.loads((OUT/'split.json').read_text())
    if args.stage=='parity':
        if not args.skip_build:
            subprocess.run(['cargo','build','--release','--locked','--manifest-path',str(ROOT/'scripts/pptx-benchmark/Cargo.toml')],check=True)
        shutil.copy2(built,candidate)
        originals={r['id']:r for r in map(json.loads,(args.reference_records or OUT/'parity-baseline/records.jsonl').read_text().splitlines())}
        records=[]
        with (run/'parity.jsonl').open('w',buffering=1) as f:
            for index,row in enumerate(manifest):
                output=run/'current.json.gz';record=invoke(candidate,'rust-pptx',row,output,0);old=originals[row['id']]
                equal=(record['status']==old['status'] and (record['status']!='ok' or record['full_sha256']==old['full_sha256']))
                record.update(id=row['id'],equal=equal,baseline_sha256=old.get('full_sha256'));records.append(record);f.write(json.dumps(record)+'\n')
                if not equal:
                    if output.exists():output.rename(run/(row['id']+'.json.gz'))
                    print('MISMATCH',row['id'],flush=True)
                if index%50==0:print(index,len(manifest),flush=True)
        sources={str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for name in ['rust-pptx','rust-office-art','rust-office-charts'] for p in (ROOT/'crates'/name).rglob('*.rs')}
        summary=dict(total=len(records),equal=sum(r['equal'] for r in records),candidate_sha256=hashlib.sha256(candidate.read_bytes()).hexdigest(),baseline_sha256=hashlib.sha256(baseline.read_bytes()).hexdigest(),baseline_path=str(baseline.resolve()),source_sha256=sources)
        (run/'parity-summary.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary));return
    parity=json.loads((run/'parity-summary.json').read_text())
    baseline=Path(parity.get('baseline_path',str(baseline)))
    if parity['equal']!=parity['total']:raise SystemExit('parity must pass before timing')
    if hashlib.sha256(candidate.read_bytes()).hexdigest()!=parity['candidate_sha256']:raise SystemExit('candidate changed after parity')
    if hashlib.sha256(baseline.read_bytes()).hexdigest()!=parity['baseline_sha256']:raise SystemExit('baseline changed after parity')
    previous=json.loads((OUT/'comparison/assessment.json').read_text())
    eligible=split['tuning']+split['holdout'] if args.split=='full' else split[args.split]
    rows=sorted([r for r in previous['rows'] if r['engine']=='rust-pptx' and r['id'] in eligible],key=lambda r:r['median_ms'])
    count=min(len(rows),args.count or len(rows))
    selected=[rows[round(i*(len(rows)-1)/max(1,count-1))]['id'] for i in range(count)]
    rng=random.Random(20261106);rng.shuffle(selected);measurements=[]
    for ident in selected:
        engines=[('baseline',baseline,'rust-pptx'),('candidate',candidate,'rust-pptx'),('office_oxide',candidate,'office_oxide')];rng.shuffle(engines)
        for label,binary,engine in engines:
            record=invoke(binary,engine,by_id[ident],'-',7);record.update(id=ident,label=label,load=os.getloadavg());measurements.append(record)
        print(ident,flush=True)
    suffix='' if args.split=='tuning' else '-'+args.split
    (run/('timings'+suffix+'.json')).write_text(json.dumps(measurements,indent=2));indexed={(r['id'],r['label']):statistics.median(r['samples_ms']) for r in measurements if r['status']=='ok'}
    ratios=[indexed[i,'baseline']/indexed[i,'candidate'] for i in selected if all((i,e) in indexed for e in ['baseline','candidate','office_oxide'])]
    bootstrap=sorted(statistics.median(rng.choices(ratios,k=len(ratios))) for _ in range(2000))
    summary=dict(inputs=len(ratios),split=args.split,median_speedup=statistics.median(ratios),median_speedup_ci95=[bootstrap[50],bootstrap[1949]],candidate_wins=sum(indexed[i,'candidate']<indexed[i,'office_oxide'] for i in selected),baseline_total=sum(indexed[i,'baseline'] for i in selected),candidate_total=sum(indexed[i,'candidate'] for i in selected),oxide_total=sum(indexed[i,'office_oxide'] for i in selected),candidate_sha256=parity['candidate_sha256'],baseline_sha256=parity['baseline_sha256'])
    (run/('timing-summary'+suffix+'.json')).write_text(json.dumps(summary,indent=2));print(json.dumps(summary))

if __name__=='__main__':main()

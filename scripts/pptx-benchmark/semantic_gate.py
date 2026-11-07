import argparse
import collections
import gzip
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

from common import core, norm
from assess import score

ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'output/pptx-benchmark-20261106'

def invoke(binary,row,path,full=False):
    args=[str(binary),'rust-pptx',row['path'],str(path),'0']
    if full:args.append('full')
    result=subprocess.run(args,capture_output=True,text=True,timeout=120,check=True)
    return json.loads(result.stdout.splitlines()[-1])

def permitted_changes(document):
    charts={}
    def walk(nodes,prefix):
        for i,node in enumerate(nodes):
            path=prefix+'/'+str(i)
            if node.get('type')=='chart':
                charts[path]=node.pop('series')
                node.pop('chartType')
            if node.get('type')=='group':walk(node['children'],path+'/children')
    for kind in ['slides','masters','layouts']:
        for i,part in enumerate(document.get(kind,[])):
            if kind=='slides':part.pop('notes',None)
            walk(part.get('nodes',[]),kind+'/'+str(i)+'/nodes')
    return charts

def main():
    ap=argparse.ArgumentParser();ap.add_argument('--label',required=True);args=ap.parse_args()
    run=OUT/'experiments'/args.label;run.mkdir(parents=True,exist_ok=True)
    candidate=run/'candidate';shutil.copy2(ROOT/'scripts/pptx-benchmark/target/release/pptx-benchmark',candidate)
    baseline=OUT/'baseline-parity-binary'
    manifest=json.loads((OUT/'manifest.json').read_text())
    originals={r['id']:r for r in map(json.loads,(OUT/'parity-baseline/records.jsonl').read_text().splitlines())}
    results=[]
    with (run/'semantic.jsonl').open('w',buffering=1) as log:
        for i,row in enumerate(manifest):
            ident=row['id'];old=originals[ident];record=invoke(candidate,row,run/'current.json.gz')
            check=dict(id=ident,status=record['status'],full_sha256=record.get('full_sha256'),issues=[])
            if old['status']==record['status'] and (record['status']!='ok' or old['full_sha256']==record['full_sha256']):
                check['result']='identical'
            elif record['status']!='ok':
                check['result']='regression';check['issues'].append(record)
            else:
                check['result']='expected_change'
                refpath=OUT/'oracle'/(ident+'.json')
                ref=json.loads(refpath.read_text()) if refpath.exists() else {}
                actual=json.load(gzip.open(run/'current.json.gz','rt'));mapped=core(actual)
                if old['status']!='ok':
                    if ref.get('status')!='ok' or ref['slides'] or actual['slides']:
                        check['issues'].append('new acceptance is not a reference-valid empty presentation')
                else:
                    a=invoke(baseline,row,run/'before.json.gz',True)
                    b=invoke(candidate,row,run/'after.json.gz',True)
                    if a.get('full_sha256')!=old['full_sha256'] or b.get('full_sha256')!=record['full_sha256']:
                        check['issues'].append('nondeterministic full output')
                    before=json.load(gzip.open(run/'before.json.gz','rt'));after=json.load(gzip.open(run/'after.json.gz','rt'))
                    old_charts=permitted_changes(before);new_charts=permitted_changes(after)
                    if before!=after:check['issues'].append('difference outside chart type/series or slide notes')
                    for path,series in old_charts.items():
                        a=collections.Counter(json.dumps(s,sort_keys=True) for s in series)
                        b=collections.Counter(json.dumps(s,sort_keys=True) for s in new_charts.get(path,[]))
                        if a-b:check['issues'].append('previously retained chart series changed: '+path)
                if ref.get('status')=='ok':
                    metrics=score(ref,mapped)
                    for key in ['chart_series','notes']:
                        check[key]=metrics.get(key)
                        if metrics.get(key,{}).get('recall',1)!=1:check['issues'].append('incomplete source recall: '+key)
                    for source,parsed in zip(ref['slides'],mapped['slides']):
                        if norm('\n'.join(n['text'] for n in source['notes']))!=norm('\n'.join(n['text'] for n in parsed['notes'])):
                            check['issues'].append('note text differs from independent source body')
                if check['issues']:check['result']='review_required'
            results.append(check);log.write(json.dumps(check)+'\n')
            if check['issues']:print('REVIEW',ident,check['issues'],flush=True)
            elif i%50==0:print(i,len(manifest),flush=True)
    summary=dict(total=len(results),counts=dict(collections.Counter(r['result'] for r in results)),candidate_sha256=hashlib.sha256(candidate.read_bytes()).hexdigest(),baseline_sha256=hashlib.sha256(baseline.read_bytes()).hexdigest(),permitted=['valid zero-slide acceptance','classic chart type and complete series','notes body and run boundaries'])
    (run/'semantic-summary.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary))
    if any(r['issues'] for r in results):raise SystemExit(1)

if __name__=='__main__':main()

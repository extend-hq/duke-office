import argparse
import collections
import copy
from concurrent.futures import ThreadPoolExecutor
import gzip
import hashlib
import io
import json
from pathlib import Path
import shutil
import zipfile
from xml.etree import ElementTree as ET

from object_gate import OUT, ROOT, invoke, changes
from common import rels

PML = {'http://schemas.openxmlformats.org/presentationml/2006/main','http://purl.oclc.org/ooxml/presentationml/main'}
DML = {'http://schemas.openxmlformats.org/drawingml/2006/main','http://purl.oclc.org/ooxml/drawingml/main'}
REL = {'http://schemas.openxmlformats.org/officeDocument/2006/relationships','http://purl.oclc.org/ooxml/officeDocument/relationships'}
P14 = 'http://schemas.microsoft.com/office/powerpoint/2010/main'
MC = 'http://schemas.openxmlformats.org/markup-compatibility/2006'
SUPPORTED = PML | DML | REL | {P14,'http://schemas.microsoft.com/office/powerpoint/2012/main','http://schemas.microsoft.com/office/powerpoint/2015/09/main'}
KINDS={'par','seq','excl','anim','animClr','animEffect','animMotion','animRot','animScale','cmd','set','audio','video'}


def name(n):return n.tag.rsplit('}',1)[-1]
def ns(n):return n.tag[1:].split('}')[0] if n.tag.startswith('{') else ''
def child(n,k):return next((c for c in n if ns(c) in PML and name(c)==k),None) if n is not None else None
def structure(n):return (n.tag,dict(n.attrib),n.text or '',[structure(c) for c in n])
def frac(s):return float(s[:-1])/100 if s.endswith('%') else float(s)/100000
def time(s,scale=1):return s if s=='indefinite' else int(s)/scale
def boolean(s):return s in ('true','1')

def select(data):
    scopes={};stack=[];pending={};tree=None
    for event,node in ET.iterparse(io.BytesIO(data),events=('start-ns','start','end')):
        if event=='start-ns':pending[node[0]]=node[1]
        elif event=='start':
            scope=dict(stack[-1] if stack else {});scope.update(pending);pending={};stack.append(scope);scopes[id(node)]=scope
            if tree is None:tree=node
        else:stack.pop()
    def flatten(n):
        for c in n:
            if c.tag=='{'+MC+'}AlternateContent':
                branch=next((b for b in c if b.tag=='{'+MC+'}Choice' and b.get('Requires','').strip() and all(scopes[id(b)].get(p) in SUPPORTED for p in b.get('Requires').split())),None)
                if branch is None:branch=next((b for b in c if b.tag=='{'+MC+'}Fallback'),None)
                if branch is not None:yield from flatten(branch)
            else:yield c
    selected={};candidates={}
    for candidate in tree:
        nodes=list(flatten(candidate)) if candidate.tag=='{'+MC+'}AlternateContent' else [candidate]
        if candidate.tag=='{'+MC+'}AlternateContent':
            holder=ET.Element('holder');holder.append(candidate);nodes=list(flatten(holder))
        for node in nodes:
            if ns(node) in PML and name(node) in ('transition','timing') and name(node) not in selected:
                selected[name(node)]=node;candidates[name(node)]=candidate
    def strip(n):
        x=ET.Element(n.tag,n.attrib);x.text=n.text;x.extend(strip(c) for c in flatten(n));return x
    return {k:strip(v) for k,v in selected.items()},candidates

def model_nodes(t):
    def walk(nodes):
        for n in nodes:
            yield n
            yield from walk(n.get('time',{}).get('children',[]));yield from walk(n.get('time',{}).get('subNodes',[]))
    yield from walk(t.get('nodes',[]))
    for b in t.get('builds',[]):
        for template in b.get('templates',[]):yield from walk(template.get('nodes',[]))

def audit(package,model):
    counts=collections.Counter();issues=[]
    for group in ['slides','masters','layouts']:
        for item in model[group]:
            path=item.get('sourcePart') if group=='slides' else item['id']
            if path not in package.namelist():continue
            source,candidates=select(package.read(path));rr=rels(package,path)
            for key in ['transition','timing']:
                expected=source.get(key);actual=item.get(key)
                if (expected is not None)!=(actual is not None):issues.append(f'{group}: {key} presence');continue
                if actual is None:continue
                counts[key]+=1;counts[group+'_'+key]+=1
                retained=ET.fromstring(actual['sourceXml'])
                if len(retained)!=1 or structure(retained[0])!=structure(candidates[key]):issues.append('source preservation')
                if key=='transition':
                    effect=next((n for n in expected if not(ns(n) in PML and name(n) in ('sndAc','extLst'))),None)
                    if effect is not None and (actual.get('effect',{}).get('kind')!=name(effect) or actual['effect']['namespace']!=ns(effect) or actual['effect']['parameters']!=effect.attrib):issues.append('transition effect')
                    for attr,field,convert in [('spd','speed',str),('advClick','advanceOnClick',boolean),('advTm','advanceAfterMs',int),('{'+P14+'}dur','durationMs',int)]:
                        if attr in expected.attrib and actual.get(field)!=convert(expected.get(attr)):issues.append('transition '+field)
                    continue
                sources=[n for n in expected.iter() if ns(n) in PML and name(n) in KINDS];nodes=list(model_nodes(actual));counts['time_nodes']+=len(sources)
                if len(sources)!=len(nodes):issues.append('time node count');continue
                for s,n in zip(sources,nodes):
                    if name(s)!=n['kind']:issues.append('node order or kind')
                    c=child(s,'cTn');b=child(s,'cBhvr');m=child(s,'cMediaNode')
                    if c is None:c=child(b,'cTn')
                    if c is None:c=child(m,'cTn')
                    if c is not None:
                        a=n.get('time',{})
                        for attr,field,convert in [('id','id',int),('dur','durationMs',time),('repeatCount','repeatCount',lambda s:time(s,1000)),('repeatDur','repeatDurationMs',time),('spd','speed',frac),('accel','acceleration',frac),('decel','deceleration',frac),('autoRev','autoReverse',boolean),('presetID','presetId',int),('presetClass','presetClass',str),('presetSubtype','presetSubtype',int),('grpId','groupId',int),('nodeType','nodeType',str),('fill','fill',str),('restart','restart',str),('masterRel','masterRelation',str),('tmFilter','timeFilter',str),('evtFilter','eventFilter',str),('display','display',boolean),('nodePh','placeholder',boolean)]:
                            if attr in c.attrib and a.get(field)!=convert(c.get(attr)):issues.append('time '+field)
                        for source_name,field in [('stCondLst','startConditions'),('endCondLst','endConditions')]:
                            source_conditions=list(child(c,source_name)) if child(c,source_name) is not None else []
                            got=a.get(field,[])
                            if len(source_conditions)!=len(got):issues.append('condition count');continue
                            counts['conditions']+=len(got)
                            for e,g in zip(source_conditions,got):
                                if e.get('evt')!=g.get('event') or (e.get('delay') is not None and time(e.get('delay'))!=g.get('delayMs')):issues.append('condition event/delay')
                                tn=child(e,'tn')
                                if tn is not None and int(tn.get('val'))!=g.get('timeNodeId'):issues.append('condition graph link')
                    if b is not None:
                        target=child(b,'tgtEl');target=next(iter(target),None) if target is not None else None
                        got=n.get('behavior',{}).get('target',{})
                        if target is not None and (name(target)!=got.get('kind') or target.get('spid')!=got.get('shapeId')):issues.append('behavior target')
                        names=child(b,'attrNameLst');names=[c.text or '' for c in names] if names is not None else []
                        if names!=n.get('behavior',{}).get('attributeNames',[]):issues.append('animated property')
                        counts['behavior_targets']+=target is not None
                    if m is not None:
                        for attr,field,convert in [('vol','volume',frac),('mute','mute',boolean),('numSld','slideCount',int),('showWhenStopped','showWhenStopped',boolean)]:
                            if attr in m.attrib and n['media'].get(field)!=convert(m.get(attr)):issues.append('media '+field)
                        counts['media_nodes']+=1
                    p=n['parameters']
                    for attr,field in [('path','path'),('ptsTypes','pointTypes'),('pathEditMode','pathEditMode'),('origin','origin'),('filter','filter'),('prLst','properties'),('cmd','command'),('calcmode','calculationMode'),('valueType','valueType')]:
                        if attr in s.attrib and p.get(field)!=s.get(attr):issues.append('parameter '+field)
                    if name(s)=='anim':
                        keyframes=child(s,'tavLst');keyframes=list(keyframes) if keyframes is not None else []
                        got=p.get('keyframes',[])
                        if len(keyframes)!=len(got):issues.append('keyframe count');continue
                        counts['keyframes']+=len(got)
                        for e,g in zip(keyframes,got):
                            if e.get('tm') is not None and (e.get('tm') if e.get('tm')=='indefinite' else frac(e.get('tm')))!=g['time']:issues.append('keyframe time')
                            if e.get('fmla','')!=g['formula']:issues.append('keyframe formula')
                builds=child(expected,'bldLst');builds=list(builds) if builds is not None else []
                got=actual.get('builds',[]);counts['builds']+=len(builds)
                if [(name(b),b.get('spid'),int(b.get('grpId','0'))) for b in builds]!=[(b['kind'],b.get('shapeId'),b.get('groupId',0)) for b in got]:issues.append('build targets')
    return counts,issues

def main():
    p=argparse.ArgumentParser();p.add_argument('--label',default='timing1');p.add_argument('--resume',action='store_true');a=p.parse_args();run=OUT/'experiments'/a.label;run.mkdir(exist_ok=True,parents=True)
    binary=run/'candidate'
    if not a.resume:
        if (run/'semantic.jsonl').exists():raise SystemExit('Use a new label or resume the preserved executable.')
        shutil.copy2(ROOT/'scripts/pptx-benchmark/target/release/pptx-benchmark',binary)
    baseline=OUT/'experiments/opt8/candidate'
    rows=json.loads((OUT/'manifest.json').read_text())+json.loads((OUT/'expanded-optimized/cohort.json').read_text());totals=collections.Counter();records=[]
    if a.resume:
        records=[json.loads(line) for line in (run/'semantic.jsonl').read_text().splitlines()]
        for record in records:totals.update(record['counts'])
    done={r['id'] for r in records}
    with ThreadPoolExecutor(max_workers=2) as pool, (run/'semantic.jsonl').open('a' if a.resume else 'w',buffering=1) as log:
        for i,row in enumerate(rows):
            if row['id'] in done:continue
            reference=pool.submit(invoke,baseline,row,run/'before.json.gz');candidate=pool.submit(invoke,binary,row,run/'after.json.gz')
            a,before=reference.result();b,after=candidate.result();issues=[];counts=collections.Counter();exact=False
            if a['status']!=b['status']:issues.append('acceptance changed')
            elif a['status']=='ok':
                exact=before==after
                if after['format']=='pptx':
                    with zipfile.ZipFile(row['path']) as package:counts,issues=audit(package,after)
                cleaned=copy.deepcopy(after)
                for key in ['slides','masters','layouts']:
                    for item in cleaned[key]:item.pop('transition',None);item.pop('timing',None)
                if before!=cleaned:issues.extend(changes(before,cleaned)[:12])
            elif a.get('error')!=b.get('error'):issues.append('error changed')
            record=dict(id=row['id'],status=b['status'],full_sha256=b.get('full_sha256'),equal=not issues,exact=exact,counts=counts,issues=issues);records.append(record);totals.update(counts);log.write(json.dumps(record)+'\n')
            if issues:print('FAIL',row['id'],issues,flush=True);shutil.copy2(run/'after.json.gz',run/(row['id']+'.json.gz'))
            if i%50==0:print(i,len(rows),flush=True)
    summary=dict(total=len(records),passed=sum(r['equal'] for r in records),exact=sum(r['exact'] for r in records),counts=totals,failures=[r for r in records if r['issues']],candidate_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),baseline_sha256=hashlib.sha256(baseline.read_bytes()).hexdigest())
    (run/'semantic-summary.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary));assert summary['passed']==summary['total']

if __name__=='__main__':main()

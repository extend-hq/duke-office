import argparse
import collections
import copy
import gzip
import hashlib
import html
import json
from pathlib import Path
import posixpath
import shutil
import subprocess
import zipfile
from xml.etree import ElementTree as ET

from common import local, rels

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'output/pptx-benchmark-20261106'
META = {'sourceId', 'sourcePart', 'hidden', 'description', 'accessibilityTitle', 'altText', 'decorative', 'hyperlink', 'clickAction', 'hoverAction'}
NV = {'nvSpPr', 'nvPicPr', 'nvGrpSpPr', 'nvGraphicFramePr', 'nvCxnSpPr'}

def child(node, name):
    return next((n for n in node if local(n) == name), None) if node is not None else None

def descend(node, name):
    return next((n for n in node.iter() if local(n) == name), None) if node is not None else None

def attrs(node):
    return {k.rsplit('}', 1)[-1]: v for k, v in node.attrib.items()} if node is not None else {}

def boolean(value):
    return value in ('1', 'true')

def action(node, relationships):
    a = attrs(node); out = {}
    for key, target in [('id','relationshipId'),('action','action'),('tooltip','tooltip'),('tgtFrame','targetFrame'),('invalidUrl','invalidUrl')]:
        if key in a: out[target] = a[key]
    for key, target in [('history','history'),('highlightClick','highlightClick'),('endSnd','endSound')]:
        if key in a: out[target] = boolean(a[key])
    relationship = relationships.get(a.get('id'))
    if relationship: out.update(target=relationship['target'], external=relationship['external'])
    sound = attrs(child(node, 'snd'))
    if sound.get('embed') in relationships and not relationships[sound['embed']]['external']:
        out['soundAssetId'] = relationships[sound['embed']]['target']
    if 'name' in sound: out['soundName'] = sound['name']
    if 'builtIn' in sound: out['soundBuiltin'] = boolean(sound['builtIn'])
    return out

def metadata(node, part, relationships):
    nv = next((n for n in node if local(n) in NV), None)
    prop = child(nv, 'cNvPr')
    if prop is None: return {}
    a = attrs(prop); out = {'sourcePart': part}
    if 'id' in a: out['sourceId'] = a['id']
    for key, target in [('hidden','hidden'),('descr','description'),('title','accessibilityTitle')]:
        if key in a: out[target] = boolean(a[key]) if key == 'hidden' else a[key]
    alt = a.get('descr') or a.get('title')
    if alt: out['altText'] = alt
    decorative = descend(child(prop, 'extLst'), 'decorative')
    if decorative is not None and 'val' in decorative.attrib: out['decorative'] = boolean(decorative.get('val'))
    for name, target in [('hlinkClick','clickAction'),('hlinkHover','hoverAction')]:
        link = child(prop, name)
        if link is not None: out[target] = action(link, relationships)
    if out.get('clickAction', {}).get('target') is not None: out['hyperlink'] = out['clickAction']['target']
    return out

def invoke(binary, row, output):
    result = subprocess.run([str(binary),'rust-pptx',row['path'],str(output),'0','full'], capture_output=True,text=True,timeout=120,check=True)
    record = json.loads(result.stdout.splitlines()[-1])
    return record, json.load(gzip.open(output,'rt')) if record['status'] == 'ok' else None

def changes(before, after, path=''):
    if type(before) != type(after): return [path]
    if isinstance(before,dict):
        return [p for key in before.keys() | after.keys() for p in (changes(before[key],after[key],path+'/'+key) if key in before and key in after else [path+'/'+key])]
    if isinstance(before,list):
        if len(before)!=len(after): return [path+'/length']
        return [p for i,(a,b) in enumerate(zip(before,after)) for p in changes(a,b,path+'/'+str(i))]
    return [] if before==after else [path]

def main():
    parser=argparse.ArgumentParser(); parser.add_argument('--label',required=True); args=parser.parse_args()
    run=OUT/'experiments'/args.label; run.mkdir(parents=True,exist_ok=True)
    binary=run/'candidate'; shutil.copy2(ROOT/'scripts/pptx-benchmark/target/release/pptx-benchmark',binary)
    baseline=OUT/'experiments/accuracy2/candidate'
    totals=collections.Counter(); results=[]
    with (run/'objects.jsonl').open('w',buffering=1) as log:
        for i,row in enumerate(json.loads((OUT/'manifest.json').read_text())):
            a,before=invoke(baseline,row,run/'before.json.gz'); b,after=invoke(binary,row,run/'after.json.gz')
            issues=[]; counts=collections.Counter(); record={'id':row['id'],'status':b['status'],'full_sha256':b.get('full_sha256')}
            if a['status']!=b['status']: issues.append('acceptance changed')
            elif before is not None:
                if row['format']=='ppt':
                    if before!=after: issues.append('legacy output changed')
                else:
                    with zipfile.ZipFile(row['path']) as z:
                        source={}; relationships={}
                        def source_nodes(part):
                            if part not in source:
                                tree=ET.fromstring(z.read(part)); relationships[part]=rels(z,part); source[part]={}
                                for n in tree.iter():
                                    nv=next((c for c in n if local(c) in NV),None); props=child(nv,'cNvPr')
                                    if props is not None: source[part].setdefault(props.get('id'),n)
                            return source[part]
                        def walk(old_nodes,new_nodes):
                            if len(old_nodes)!=len(new_nodes): issues.append('node count changed'); return
                            for index,(old,new) in enumerate(zip(old_nodes,new_nodes)):
                                src=None; part=new.get('sourcePart')
                                if part:
                                    src=source_nodes(part).get(new.get('sourceId',new['id'].rsplit(':',1)[-1]))
                                    if src is None: issues.append('unresolved object source '+new['id']+' '+part)
                                    else:
                                        expected=metadata(src,part,relationships[part]); actual={k:new[k] for k in META if k in new}
                                        if actual!=expected: issues.append('object metadata differs '+new['id']+' '+part)
                                        counts.update(expected.keys())
                                if new['type']=='media':
                                    counts['media_'+new['mediaType']]+=1
                                    if src is None: issues.append('unresolved media source')
                                    else:
                                        rr=relationships[part]; ole=descend(src,'oleObj') if new['mediaType']=='ole' else None
                                        if ole is not None:
                                            properties=attrs(ole); rid=properties.get('id'); expected={ 'linked':child(ole,'link') is not None }
                                            for k,t in [('progId','programId'),('name','name'),('id','relationshipId'),('showAsIcon','showAsIcon'),('imgW','previewWidthEmu'),('imgH','previewHeightEmu')]:
                                                if k in properties: expected[t]=boolean(properties[k]) if k=='showAsIcon' else int(properties[k]) if k in ('imgW','imgH') else properties[k]
                                            embed=attrs(child(ole,'embed')); link=attrs(child(ole,'link'))
                                            if 'followColorScheme' in embed: expected['followColorScheme']=embed['followColorScheme']
                                            if 'updateAutomatic' in link: expected['updateAutomatic']=boolean(link['updateAutomatic'])
                                            if new.get('embeddedObject')!=expected: issues.append('embedded object properties differ')
                                            if not new.get('sourceXml'): issues.append('missing embedded object XML')
                                        else:
                                            nv=next((n for n in src if local(n) in NV),None); props=child(nv,'nvPr'); ext=descend(child(props,'extLst'),'media')
                                            classic=next((n for n in props if local(n) in ('audioCd','wavAudioFile','audioFile','videoFile','quickTimeFile')),None) if props is not None else None
                                            selected=attrs(ext) or attrs(classic); rid=selected.get('link') or selected.get('embed')
                                            if classic is not None:
                                                expected_type='video' if local(classic) in ('videoFile','quickTimeFile') else 'audio'
                                                if new['mediaType']!=expected_type: issues.append('media type differs')
                                        relation=rr.get(rid)
                                        if relation:
                                            key='sourceUrl' if relation['external'] else 'assetId'
                                            if new.get(key)!=relation['target']: issues.append('media relationship differs')
                                            if not relation['external'] and relation['target'] in z.namelist():
                                                retained=after['assets'].get(new.get('assetId'),{}).get('data',[])
                                                if bytes(retained)!=z.read(relation['target']): issues.append('media payload differs')
                                                else: counts['media_payloads']+=1
                                    if old['type']=='image':
                                        replacement=new.get('poster')
                                        if replacement is None: issues.append('lost poster'); continue
                                        if new['id']!=old['id']: issues.append('media wrapper identity changed')
                                        if replacement['id']!=old['id']:
                                            if replacement['id']!=old['id'].rsplit(':',1)[-1]: issues.append('poster identity changed')
                                            else: counts['scoped_poster_ids']+=1; replacement['id']=old['id']
                                        new_nodes[index]=replacement; new=replacement
                                    elif old['type']=='unknown' and new['mediaType']=='ole':
                                        new_nodes[index]=copy.deepcopy(old); new=new_nodes[index]
                                    else: issues.append('unexpected media conversion from '+old['type'])
                                for key in META: new.pop(key,None)
                                if old['type']=='group' and new['type']=='group': walk(old['children'],new['children'])
                        for kind in ['slides','masters','layouts']:
                            if len(before.get(kind,[]))!=len(after.get(kind,[])): issues.append(kind+' count changed'); continue
                            for old,new in zip(before.get(kind,[]),after.get(kind,[])): walk(old.get('nodes',[]),new.get('nodes',[]))
                        for key in list(after['assets']):
                            if key not in before['assets']:
                                if key not in z.namelist() or bytes(after['assets'][key].get('data',[]))!=z.read(key): issues.append('invalid added asset '+key)
                                counts['new_assets']+=1; after['assets'].pop(key)
                        def warnings(document):
                            for container in [document]+document.get('slides',[]):
                                container['warnings']=[w for w in container.get('warnings',[]) if not (w.get('code') in ('missing-embedded-preview','missing-media-asset','missing-media-source') or w.get('code')=='unsupported-feature' and w.get('feature')=='graphic-frame')]
                        warnings(before); warnings(after)
                        diffs=changes(before,after)
                        if diffs: issues.extend(diffs[:15])
            record.update(issues=issues,counts=dict(counts)); log.write(json.dumps(record)+'\n'); results.append(record); totals.update(counts)
            if issues: print('REVIEW',row['id'],issues,flush=True)
            elif i%25==0: print(i,flush=True)
    summary=dict(total=len(results),passed=sum(not r['issues'] for r in results),counts=dict(totals),candidate_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),baseline_sha256=hashlib.sha256(baseline.read_bytes()).hexdigest())
    (run/'objects-summary.json').write_text(json.dumps(summary,indent=2)); print(json.dumps(summary))
    if summary['passed']!=summary['total']: raise SystemExit(1)

if __name__=='__main__': main()

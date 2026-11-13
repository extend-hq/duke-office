import collections
import argparse
import difflib
import gzip
import json
import re
from pathlib import Path
import statistics
import sys
from common import norm,core,ir,markdown_model
from peer_models import ENGINES as PEER_ENGINES, model as peer_model

OUT=Path(__file__).resolve().parents[2]/'output/pptx-benchmark-20261106'
ENGINES=['duke-slides','office_oxide','undoc','python-pptx','markitdown']

def grams(strings):
    c=collections.Counter()
    for text in strings:
        text=norm(text)
        if text:c.update([text] if len(text)<5 else (text[i:i+5] for i in range(len(text)-4)))
    return c

def counts(expected,found):
    a,b=collections.Counter(expected),collections.Counter(found)
    return [sum((a&b).values()),sum(a.values()),sum(b.values())]

def prf(count):
    hit,expected,found=count
    recall=hit/expected if expected else 1
    precision=hit/found if found else (1 if not expected else 0)
    return dict(recall=recall,precision=precision,f1=2*recall*precision/(recall+precision) if recall+precision else 0,matched=hit,expected=expected,found=found)

def style_text(ps,key):
    result=[]
    for p in ps:
        value=''
        for r in [*p.get('runs',[]),{}]:
            if r.get(key):value+=r.get('text','')
            elif value:result.append(value);value=''
    return result

def score(ref,actual):
    totals=collections.defaultdict(lambda:[0,0,0])
    def add(key,counts):totals[key]=[a+b for a,b in zip(totals[key],counts)]
    add('slide_count',[min(len(ref['slides']),len(actual['slides'])),len(ref['slides']),len(actual['slides'])])
    for i in range(max(len(ref['slides']),len(actual['slides']))):
        s=ref['slides'][i] if i<len(ref['slides']) else {k:[] for k in ['paragraphs','notes','tables','images','charts','shapes','comments']}
        a=actual['slides'][i] if i<len(actual['slides']) else {k:[] for k in ['paragraphs','notes','tables','images','charts','shapes','comments']}
        rt=[norm(p['text']) for p in s['paragraphs'] if norm(p['text'])];at=[norm(p['text']) for p in a['paragraphs'] if norm(p['text'])]
        add('body_text',counts(grams(rt),grams(at)))
        x,y=' '.join(rt),' '.join(at)
        matched=sum(m.size for m in difflib.SequenceMatcher(None,x,y,autojunk=False).get_matching_blocks())
        add('ordered_text',[matched,len(x),len(y)])
        add('paragraphs',counts(rt,at))
        add('notes',counts(grams([p['text'] for p in s['notes']]),grams([p['text'] for p in a['notes']])))
        add('comments',counts([norm(t) for t in s['comments']],[norm(t) for t in a['comments']]))
        add('images',counts(s['images'],a['images']))
        for key in ['bold','italic','link']:
            add(key,counts(grams(style_text(s['paragraphs'],key)),grams(style_text(a['paragraphs'],key))))
        for key in ['tables','cells','horizontal_spans','vertical_spans']:
            def items(slide):
                if key=='tables':return [norm(''.join(c['text'] for c in t)) for t in slide['tables']]
                cells=[c for t in slide['tables'] for c in t if c['text']]
                if key=='cells':return [c['text'] for c in cells]
                field='col_span' if key=='horizontal_spans' else 'row_span'
                return [(c['text'],c.get(field,1)) for c in cells if (c.get(field) or 1)>1]
            add(key,counts(items(s),items(a)))
        def literal_tables(slide):return [re.sub(r'\s+','',''.join(c['text'] for c in table)) for table in slide['tables']]
        expected=literal_tables(s);found=literal_tables(a);available=list(found);matched=0
        for text in expected:
            for j,candidate in enumerate(available):
                iterator=iter(candidate)
                if all(char in iterator for char in text):
                    matched+=1;available.pop(j);break
        add('tables_literal',[matched,len(expected),len(found)])
        add('chart_count',[min(len(s['charts']),len(a['charts'])),len(s['charts']),len(a['charts'])])
        def series(slide):return [tuple(round(v,7) if isinstance(v,(int,float)) else None for v in series) for chart in slide['charts'] for series in chart]
        add('chart_series',counts(series(s),series(a)))
        def positions(slide):return [(str(n['id']),tuple(n['transform'])) for n in slide['shapes']]
        add('shape_positions',counts(positions(s),positions(a)))
        if s.get('hidden'):add('hidden_slides',[int(bool(a.get('hidden'))),1,int(bool(a.get('hidden')))])
    if ref.get('size'):add('slide_size',[int(ref['size']==actual.get('size')),1,int(bool(actual.get('size')))])
    return {k:prf(v) for k,v in totals.items()}

def percentile(xs,q):
    xs=sorted(xs)
    if not xs:return None
    pos=(len(xs)-1)*q;lo=int(pos);hi=min(lo+1,len(xs)-1)
    return xs[lo]+(xs[hi]-xs[lo])*(pos-lo)

def main(phase, fixed_cohort=None, exclude_empty=False, output_name='assessment.json'):
    run=OUT/phase;cohort=json.loads((run/'cohort.json').read_text());records=[json.loads(x) for x in (run/'records.jsonl').read_text().splitlines()]
    by={(r['id'],r['engine']):r for r in records};rows=[]
    identity=json.loads((run/'identity.json').read_text())
    engines=identity.get('selected_engines',ENGINES)
    if len(engines)!=len(set(engines)) or any(e not in ENGINES+PEER_ENGINES for e in engines):
        raise SystemExit('Invalid parser selection.')
    expected={(row['id'],engine) for row in cohort for engine in engines}
    if 'end' not in identity or set(by)!=expected or len(by)!=len(records):
        raise SystemExit('Assessment requires a completed run with exactly one record per input and selected engine.')
    common={r['id'] for r in cohort if r['reference_status']=='ok' and all(by.get((r['id'],e),{}).get('status')=='ok' for e in engines)}
    if exclude_empty:
        common={ident for ident in common if json.loads((OUT/'oracle'/(ident+'.json')).read_text())['slides']}
    if fixed_cohort:
        fixed=set(json.loads((OUT/fixed_cohort/'assessment.json').read_text())['common_ids'])
        if not fixed.issubset(common):raise SystemExit('previously timed input failed; report the regression before changing the cohort')
        common=fixed
    for item in cohort:
        if item['reference_status']!='ok':continue
        ref=json.loads((OUT/'oracle'/(item['id']+'.json')).read_text())
        for e in engines:
            record=by.get((item['id'],e),dict(status='pending'))
            if record['status']=='pending':continue
            actual={'slides':[]}
            if record['status']=='ok':
                data=json.load(gzip.open(run/'native'/(item['id']+'.'+e+'.json.gz'),'rt'))
                actual=core(data) if e=='duke-slides' else ir(e,data) if e in ['office_oxide','undoc'] else peer_model(e,data) if e in PEER_ENGINES else markdown_model(data['markdown']) if e=='markitdown' else data
            metrics=score(ref,actual)
            rows.append(dict(id=item['id'],engine=e,status=record['status'],metrics=metrics,median_ms=statistics.median(record['samples_ms']) if record.get('samples_ms') else None,bytes=item['bytes'],uncompressed_bytes=ref['uncompressed_bytes'],slides=len(ref['slides']),paragraphs=sum(len(s['paragraphs']) for s in ref['slides']),features=sum(sum(len(s[k]) for k in ['tables','images','charts','shapes']) for s in ref['slides']),characters=sum(len(p['text']) for s in ref['slides'] for p in s['paragraphs'])))
        print(item['id'],flush=True)
    summaries={}
    for e in engines:
        er=[r for r in rows if r['engine']==e];timings=[r['median_ms'] for r in er if r['id'] in common and r['median_ms'] is not None]
        keys=sorted({k for r in er for k in r['metrics']});features={}
        for k in keys:
            ms=[r['metrics'][k] for r in er if k in r['metrics']]
            features[k]=prf([sum(m[n] for m in ms) for n in ['matched','expected','found']])
            bearing=[m for m in ms if m['expected']]
            features[k]['macro_f1']=statistics.mean(m['f1'] for m in bearing) if bearing else None
            features[k]['macro_recall']=statistics.mean(m['recall'] for m in bearing) if bearing else None
            features[k]['documents']=len(bearing)
        common_features={}
        for key in ['body_text','ordered_text']:
            ms=[r['metrics'][key] for r in er if r['id'] in common and key in r['metrics']]
            common_features[key]=prf([sum(m[n] for m in ms) for n in ['matched','expected','found']])
        summaries[e]=dict(success=sum(by.get((r['id'],e),{}).get('status')=='ok' for r in cohort),total=len(cohort),reference_success=sum(r['status']=='ok' for r in er),reference_total=len(er),common_n=len(timings),median_ms=statistics.median(timings) if timings else None,p95_ms=percentile(timings,.95),features=features,common_features=common_features)
    result=dict(phase=phase,exclude_empty_from_timing=exclude_empty,timing_cohort=fixed_cohort or phase,summary=summaries,rows=rows,common_ids=sorted(common),excluded_reference=[r for r in cohort if r['reference_status']!='ok'],identity=json.loads((run/'identity.json').read_text()))
    (run/output_name).write_text(json.dumps(result,indent=2))
    print(json.dumps(summaries,indent=2))

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('phase',nargs='?',default='baseline');parser.add_argument('--fixed-cohort');parser.add_argument('--exclude-empty',action='store_true');parser.add_argument('--output-name',default='assessment.json');args=parser.parse_args();main(args.phase,args.fixed_cohort,args.exclude_empty,args.output_name)

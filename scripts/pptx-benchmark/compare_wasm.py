import argparse
import gzip
import json
from pathlib import Path

OUT=Path(__file__).resolve().parents[2]/'output/pptx-benchmark-20261106'

def differences(a,b,path='',result=None):
    if result is None:result=[]
    if isinstance(a,dict) and isinstance(b,dict):
        for key in a.keys()|b.keys():
            if key not in a or key not in b:result.append([path+'/'+key,'key'])
            else:differences(a[key],b[key],path+'/'+key,result)
    elif isinstance(a,list) and isinstance(b,list):
        if len(a)!=len(b):result.append([path,'length'])
        else:
            for i,(x,y) in enumerate(zip(a,b)):differences(x,y,path+'/'+str(i),result)
    elif a!=b:result.append([path,a,b])
    return result

def main():
    ap=argparse.ArgumentParser();ap.add_argument('--phase',default='optimized');ap.add_argument('--semantic-label',default='accuracy2');ap.add_argument('--record-file',default='semantic.jsonl');ap.add_argument('--wasm-dir',default='wasm-check');ap.add_argument('--native-experiment');ap.add_argument('--manifest',type=Path);ap.add_argument('--exact-artwork',action='store_true');args=ap.parse_args()
    wasm_dir=OUT/args.wasm_dir
    wasm={r['id']:r for r in json.loads((wasm_dir/'records.json').read_text())}
    native={r['id']:r for r in map(json.loads,(OUT/'experiments'/args.semantic_label/args.record_file).read_text().splitlines())}
    results=[]
    for row in json.loads((args.manifest or OUT/'manifest.json').read_text()):
        ident=row['id'];record=dict(id=ident,status_equal=wasm[ident]['status']==native[ident]['status'])
        if wasm[ident]['status']=='ok':
            path=OUT/'experiments'/args.native_experiment/(ident+'.compact.json.gz') if args.native_experiment else OUT/args.phase/'native'/(ident+'.rust-pptx.json.gz') if row['format']=='pptx' else OUT/'parity-baseline'/(ident+'.json.gz')
            a=json.load(gzip.open(path,'rt'));b=json.load(gzip.open(wasm_dir/(ident+'.json.gz'),'rt'))
            record['differences']=differences(a,b)
            record['semantic_equal']=all(r[0].endswith('/shapeSvgBytes') for r in record['differences'])
            if args.exact_artwork:
                if 'artwork_hashes' not in wasm[ident] or 'artwork_hashes' not in native[ident]:
                    raise SystemExit('Both builds must record complete artwork hashes.')
                old=dict(native[ident]['artwork_hashes']);new=dict(wasm[ident]['artwork_hashes'])
                record['artwork_differences']=[path for path in old.keys()|new.keys() if old.get(path)!=new.get(path)]
                record['artwork_equal']=not record['artwork_differences']
                record['semantic_equal']=not record['differences'] and record['artwork_equal']
        results.append(record)
    summary=dict(total=len(results),status_matches=sum(r['status_equal'] for r in results),model_comparisons=sum('differences' in r for r in results),exact_compact_matches=sum(r.get('differences')==[] for r in results),semantic_matches=sum(r.get('semantic_equal',False) for r in results),different_svg_lengths=sum(bool(r.get('differences')) for r in results),wasm_identity=json.loads((wasm_dir/'identity.json').read_text()))
    summary.update(strict_artwork=args.exact_artwork,artwork_comparisons=sum('artwork_equal' in r for r in results),exact_artwork_matches=sum(r.get('artwork_equal',False) for r in results))
    (wasm_dir/'comparison.json').write_text(json.dumps(dict(summary=summary,rows=results),indent=2));print(json.dumps(summary))
    if any(not r['status_equal'] or r.get('semantic_equal') is False for r in results):raise SystemExit(1)

if __name__=='__main__':main()

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import shutil
import subprocess
import time
import zipfile
from xml.etree import ElementTree as ET
from common import oracle
from peer_models import ENGINES as PEER_ENGINES

ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'output/pptx-benchmark-20261106'
ENGINES=['duke-slides','office_oxide','undoc','python-pptx','markitdown']

if __name__=='__main__':
    ap=argparse.ArgumentParser();ap.add_argument('--phase',default='baseline');ap.add_argument('--repeats',type=int,default=5);ap.add_argument('--limit',type=int);ap.add_argument('--legacy',action='store_true');ap.add_argument('--binary',type=Path);ap.add_argument('--manifest',type=Path);ap.add_argument('--split',choices=['tuning','validation','all'],default='all');ap.add_argument('--containers');ap.add_argument('--engines');ap.add_argument('--peer-binary',type=Path);ap.add_argument('--deduplicate-phase');args=ap.parse_args()
    run=OUT/args.phase;run.mkdir(exist_ok=True);(run/'native').mkdir(exist_ok=True);(OUT/'oracle').mkdir(exist_ok=True)
    manifest_path=args.manifest or OUT/'manifest.json'
    manifest=json.loads(manifest_path.read_text())
    cohort=[r for r in manifest if (r.get('container') in args.containers.split(',') if args.containers else r['format']==('ppt' if args.legacy else 'pptx')) and (args.split=='all' or r.get('split')==args.split)]
    selected_engines=args.engines.split(',') if args.engines else ENGINES[:3] if args.legacy else ENGINES[:]
    if len(selected_engines)!=len(set(selected_engines)) or any(e not in ENGINES+PEER_ENGINES for e in selected_engines):raise SystemExit('unknown or duplicate engine')
    if any(e in PEER_ENGINES for e in selected_engines) and not args.peer_binary:raise SystemExit('peer parsers require a frozen peer binary')
    rng=random.Random(20261106);rng.shuffle(cohort)
    if args.limit:cohort=cohort[:args.limit]
    if not cohort:raise SystemExit('No inputs match the selected format, container and split; no benchmark was run.')
    reference=[]
    for row in cohort:
        if hashlib.sha256(Path(row['path']).read_bytes()).hexdigest()!=row['sha256']:
            raise SystemExit('input identity changed: '+row['id'])
        p=OUT/'oracle'/(row['id']+'.json')
        if not p.exists() and not args.legacy:
            try:
                with zipfile.ZipFile(row['path']) as package:
                    if ET.fromstring(package.read('ppt/presentation.xml')).tag!='{http://schemas.openxmlformats.org/presentationml/2006/main}presentation':raise ValueError('reference does not yet score this PresentationML namespace')
                ref=oracle(row['path']);ref['status']='ok'
            except Exception as e:ref=dict(status='error',error=str(e))
            p.write_text(json.dumps(ref))
        reference.append(dict(row,reference_status=json.loads(p.read_text())['status'] if p.exists() else 'none'))
    (run/'cohort.json').write_text(json.dumps(reference,indent=2))
    source_binary=args.binary or ROOT/'scripts/pptx-benchmark/target/release/pptx-benchmark'
    binary=run/'parser-benchmark'
    if binary.exists() and hashlib.sha256(binary.read_bytes()).digest()!=hashlib.sha256(source_binary.read_bytes()).digest():raise SystemExit('phase binary changed; choose a new phase')
    if not binary.exists():shutil.copy2(source_binary,binary)
    identity=dict(commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),platform=platform.platform(),cpu=platform.processor(),seed=20261106,repeats=args.repeats,start=time.time(),load=os.getloadavg())
    identity['manifest_sha256']=hashlib.sha256(manifest_path.read_bytes()).hexdigest()
    identity['manifest_path']=str(manifest_path.resolve())
    identity['reserved_validation_inputs']=sum(row.get('split')=='validation' for row in json.loads(manifest_path.read_text()))
    identity['selected_engines']=selected_engines
    identity['selected_ids']=[r['id'] for r in reference]
    identity['rustc']=subprocess.check_output(['rustc','--version'],text=True).strip()
    identity['source_sha256']={str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for folder in ['crates/duke-slides','crates/duke-office-art','crates/duke-office-charts'] for p in (ROOT/folder).rglob('*.rs')}
    identity['harness_sha256']={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in Path(__file__).parent.iterdir() if p.suffix in ['.py','.toml','.lock','.txt']}
    identity['harness_sha256']['src/main.rs']=hashlib.sha256((Path(__file__).parent/'src/main.rs').read_bytes()).hexdigest()
    if args.peer_binary:
        peer_binary=run/'peer-benchmark'
        peer_hash=hashlib.sha256(args.peer_binary.read_bytes()).hexdigest()
        if peer_binary.exists() and hashlib.sha256(peer_binary.read_bytes()).hexdigest()!=peer_hash:raise SystemExit('phase peer binary changed')
        if not peer_binary.exists():shutil.copy2(args.peer_binary,peer_binary)
        identity['peer_binary_sha256']=peer_hash
        identity['peer_threads']=1
        for pattern in ['Cargo.toml','Cargo.lock','src/*.rs']:
            for p in (Path(__file__).parent/'peers').glob(pattern):
                identity['harness_sha256'][str(p.relative_to(Path(__file__).parent))]=hashlib.sha256(p.read_bytes()).hexdigest()
    deduplicate={}
    same_compactor=False
    if args.deduplicate_phase:
        previous=OUT/args.deduplicate_phase
        deduplicate={(r['id'],r['engine']):r for r in map(json.loads,(previous/'records.jsonl').read_text().splitlines())}
        previous_identity=json.loads((previous/'identity.json').read_text())
        same_compactor=previous_identity['harness_sha256'].get('src/main.rs')==identity['harness_sha256']['src/main.rs']
        identity['output_deduplication']=dict(phase=args.deduplicate_phase,rule='Fresh parse and serialization required. Identical full native-model hashes with an unchanged compactor, or identical complete serialized peer-artifact hashes, permit compressed-artifact hard links.')
    if (run/'identity.json').exists():
        old=json.loads((run/'identity.json').read_text())
        for key in ['binary_sha256','peer_binary_sha256','peer_threads','repeats','source_sha256','harness_sha256','manifest_sha256','selected_engines','selected_ids','output_deduplication']:
            if old.get(key)!=identity[key]:raise SystemExit('phase identity changed: '+key)
        identity['start']=old['start']
    archive = run / 'harness-at-measurement'
    for name, expected in identity['harness_sha256'].items():
        source = Path(__file__).parent / name
        target = archive / name
        target.parent.mkdir(parents=True, exist_ok=True)
        if not target.exists():
            shutil.copy2(source, target)
        if hashlib.sha256(target.read_bytes()).hexdigest() != expected:
            raise SystemExit('Archived measurement harness changed: ' + name)
    (run/'identity.json').write_text(json.dumps(identity,indent=2))
    records=run/'records.jsonl';done=set()
    if records.exists():done={(r['id'],r['engine']) for r in map(json.loads,records.read_text().splitlines())}
    with records.open('a',buffering=1) as f:
        for index,row in enumerate(reference):
            engines=selected_engines[:];rng.shuffle(engines)
            for engine in engines:
                if (row['id'],engine) in done:continue
                native=run/'native'/(row['id']+'.'+engine+'.json.gz')
                if engine in PEER_ENGINES:cmd=[str(peer_binary),engine,row['path'],str(native),str(args.repeats)]
                elif engine in ENGINES[:3]:cmd=[str(binary),engine,row['path'],str(native),str(args.repeats)]
                else:cmd=[str(OUT/'venv/bin/python'),str(ROOT/'scripts/pptx-benchmark/python_worker.py'),engine,row['path'],str(native),str(args.repeats)]
                start=time.time()
                try:
                    result=subprocess.run(cmd,capture_output=True,text=True,timeout=60)
                    record=json.loads(result.stdout.splitlines()[-1]) if result.stdout.strip() else dict(status='process_error',error=result.stderr[-1200:],returncode=result.returncode)
                except subprocess.TimeoutExpired:record=dict(status='timeout')
                except Exception as e:record=dict(status='harness_error',error=str(e))
                record.update(id=row['id'],engine=engine,elapsed_s=time.time()-start,load=os.getloadavg(),binary_sha256=identity.get('peer_binary_sha256') if engine in PEER_ENGINES else identity['binary_sha256'])
                prior=deduplicate.get((row['id'],engine),{})
                digest_key='output_sha256' if engine in PEER_ENGINES else 'full_sha256'
                if record.get('status')=='ok' and record.get(digest_key) and record[digest_key]==prior.get(digest_key) and (digest_key=='output_sha256' or same_compactor):
                    existing=previous/'native'/native.name
                    if existing.exists():
                        temporary=native.with_suffix('.deduplicated')
                        os.link(existing,temporary);os.replace(temporary,native)
                        record['artifact_deduplicated_from']=args.deduplicate_phase
                        record['artifact_deduplication_hash_kind']=digest_key
                f.write(json.dumps(record)+'\n')
            print(f'{index+1}/{len(reference)} {row["id"]}',flush=True)
    identity['end']=time.time();identity['load_end']=os.getloadavg();(run/'identity.json').write_text(json.dumps(identity,indent=2))

import argparse
import collections
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

from expand import OUT, ROOT


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--split', choices=['tuning', 'validation', 'all'], default='tuning')
    parser.add_argument('--manifest', type=Path)
    parser.add_argument('--output-root', type=Path)
    args = parser.parse_args()
    manifest_bytes = (args.manifest or OUT / 'manifest.json').read_bytes()
    cohort = [r for r in json.loads(manifest_bytes) if args.split == 'all' or r['split'] == args.split]
    run = (args.output_root or OUT) / args.label
    run.mkdir(exist_ok=True)
    binary = run / 'candidate'
    digest = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    identity = dict(binary_sha256=digest, manifest_sha256=hashlib.sha256(manifest_bytes).hexdigest(), split=args.split,
                    harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), scope='acceptance-only')
    if (run / 'identity.json').exists() and json.loads((run / 'identity.json').read_text()) != identity:
        raise ValueError('check identity changed')
    if not binary.exists():
        shutil.copy2(args.binary, binary)
    if hashlib.sha256(binary.read_bytes()).hexdigest() != digest:
        raise ValueError('binary identity changed')
    (run / 'identity.json').write_text(json.dumps(identity, indent=2))
    (run / 'cohort.json').write_text(json.dumps(cohort, indent=2))
    path = run / 'records.jsonl'
    records = list(map(json.loads, path.read_text().splitlines())) if path.exists() else []
    done = {(r['id'], r['engine']) for r in records}
    with path.open('a', buffering=1) as stream:
        for index, row in enumerate(cohort):
            if hashlib.sha256(Path(row['path']).read_bytes()).hexdigest() != row['sha256']:
                raise ValueError('input identity changed')
            for engine in ['duke-slides', 'office_oxide', 'undoc']:
                if (row['id'], engine) in done:
                    continue
                start = time.time()
                try:
                    result = subprocess.run([str(binary), engine, row['path'], '-', '0'], capture_output=True, text=True, timeout=30)
                    record = json.loads(result.stdout.splitlines()[-1]) if result.stdout.strip() else dict(status='process_error', returncode=result.returncode)
                except subprocess.TimeoutExpired:
                    record = dict(status='timeout')
                except Exception as exc:
                    record = dict(status='harness_error', error=type(exc).__name__)
                record.update(id=row['id'], engine=engine, format=row['format'], container=row['container'], elapsed_s=time.time() - start, load=os.getloadavg())
                stream.write(json.dumps(record) + '\n')
                records.append(record)
            if index % 25 == 0:
                print(index + 1, '/', len(cohort), flush=True)
    summary = {}
    for engine in ['duke-slides', 'office_oxide', 'undoc']:
        matching = [r for r in records if r['engine'] == engine]
        summary[engine] = {kind: dict(collections.Counter(r['status'] for r in matching if r['format'] == kind)) for kind in sorted({r['format'] for r in matching})}
    (run / 'summary.json').write_text(json.dumps(dict(identity=identity, total=len(cohort), engines=summary), indent=2))
    print(json.dumps(summary))


if __name__ == '__main__':
    main()

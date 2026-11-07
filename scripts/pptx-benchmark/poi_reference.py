import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

from feedback import OUT, ROOT


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--repeats', type=int, default=0)
    parser.add_argument('--java', type=Path)
    args = parser.parse_args()
    runtime = OUT / 'poi-runtime'
    identity = json.loads((runtime / 'identity.json').read_text())
    source = ROOT / 'scripts/pptx-benchmark/poi/PowerPointReference.java'
    assert hashlib.sha256(source.read_bytes()).hexdigest() == identity['source_sha256']
    if 'class_sha256' in identity:
        assert hashlib.sha256((runtime / 'classes/PowerPointReference.class').read_bytes()).hexdigest() == identity['class_sha256']
    for dependency in identity['dependencies']:
        assert hashlib.sha256(Path(dependency['path']).read_bytes()).hexdigest() == dependency['sha256']
    java = args.java or Path(identity.get('java_executable', shutil.which('java') or 'java'))
    classpath = str(runtime / 'classes') + os.pathsep + (runtime / 'classpath.txt').read_text().strip()
    args.output.mkdir(parents=True, exist_ok=True)
    records_path = args.output / 'records.jsonl'
    if records_path.exists():
        raise SystemExit('Preserve existing results; choose a new output directory.')
    rows = json.loads(args.manifest.read_text())
    records = []
    with records_path.open('w', buffering=1) as stream:
        for index, row in enumerate(rows):
            assert hashlib.sha256(Path(row['path']).read_bytes()).hexdigest() == row['sha256']
            command = [str(java), '-Xmx2g', '-Djava.awt.headless=true', '-cp', classpath, 'PowerPointReference', row['path'], str(args.output / (row['id'] + '.json')), str(args.repeats)]
            try:
                process = subprocess.run(command, capture_output=True, text=True, timeout=120)
                record = json.loads(process.stdout.splitlines()[-1]) if process.returncode == 0 and process.stdout.strip() else dict(status='process_error', error=process.stderr[-2000:], exit_code=process.returncode)
                (args.output / (row['id'] + '.stderr.txt')).write_text(process.stderr)
            except subprocess.TimeoutExpired:
                record = dict(status='timeout', error='120 seconds')
            record.update(id=row['id'], input_sha256=row['sha256'])
            records.append(record)
            stream.write(json.dumps(record) + '\n')
            print(index + 1, len(rows), row['id'], record['status'], flush=True)
    summary = dict(total=len(records), accepted=sum(r['status'] == 'ok' for r in records), engine='apache_poi', version='6.0.0', identity=identity, manifest_sha256=hashlib.sha256(args.manifest.read_bytes()).hexdigest(), repeats=args.repeats, timing_scope='Parse and text extraction inside the JVM; process startup and file reading excluded. Zero-repeat runs are acceptance/content diagnostics and do not establish latency rankings.')
    (args.output / 'summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps({k: v for k, v in summary.items() if k != 'identity'}))


if __name__ == '__main__':
    main()

import argparse
import gzip
import hashlib
import json
from pathlib import Path
import subprocess
import zipfile
from xml.etree import ElementTree as ET

import common
from assess import score

OUT = Path(__file__).resolve().parents[2] / 'output/pptx-benchmark-20261106'


def strict_check(phase):
    run = OUT / phase
    records = {(r['id'], r['engine']): r for r in map(json.loads, (run / 'records.jsonl').read_text().splitlines())}
    result = []
    previous = common.NS.copy()
    try:
        common.NS.update(p='http://purl.oclc.org/ooxml/presentationml/main', a='http://purl.oclc.org/ooxml/drawingml/main', r='http://purl.oclc.org/ooxml/officeDocument/relationships', c='http://purl.oclc.org/ooxml/drawingml/chart')
        for row in json.loads((run / 'cohort.json').read_text()):
            if row['reference_status'] == 'ok':
                continue
            with zipfile.ZipFile(row['path']) as package:
                if ET.fromstring(package.read('ppt/presentation.xml')).tag != '{' + common.NS['p'] + '}presentation':
                    continue
            reference = common.oracle(row['path'])
            engines = {}
            for engine in ['rust-pptx', 'office_oxide', 'undoc', 'python-pptx', 'markitdown']:
                record = records[row['id'], engine]
                actual = {'slides': []}
                if record['status'] == 'ok':
                    data = json.load(gzip.open(run / 'native' / (row['id'] + '.' + engine + '.json.gz'), 'rt'))
                    actual = common.core(data) if engine == 'rust-pptx' else common.ir(engine, data) if engine in ['office_oxide', 'undoc'] else common.markdown_model(data['markdown']) if engine == 'markitdown' else data
                engines[engine] = dict(status=record['status'], metrics=score(reference, actual))
            result.append(dict(id=row['id'], reference=reference, results=engines))
    finally:
        common.NS.clear()
        common.NS.update(previous)
    output = dict(phase=phase, scope='Separate namespace-aware source check; frozen reference data and denominators remain unchanged.', records=result)
    (OUT / 'expanded/strict-supplement.json').write_text(json.dumps(output, indent=2))
    return len(result)


def legacy_check(phase):
    binary = OUT / phase / 'parser-benchmark'
    manifest = {r['id']: r for r in json.loads((OUT / 'expanded/manifest.json').read_text())}
    rows = [json.loads(line) for line in (OUT / 'expanded/baseline/records.jsonl').read_text().splitlines()]
    indexed = {(r['id'], r['engine']): r for r in rows}
    directory = OUT / 'expanded/legacy-gap-audit'
    directory.mkdir(exist_ok=True)
    results = []
    for row in rows:
        if row['engine'] != 'rust-pptx' or row['format'] != 'ppt' or row['status'] == 'ok' or indexed[row['id'], 'office_oxide']['status'] != 'ok':
            continue
        source = manifest[row['id']]
        if hashlib.sha256(Path(source['path']).read_bytes()).hexdigest() != source['sha256']:
            raise SystemExit('Input identity changed.')
        output = directory / (row['id'] + '.json.gz')
        process = subprocess.run([str(binary), 'office_oxide', source['path'], str(output), '0'], capture_output=True, text=True, check=True, timeout=120)
        record = json.loads(process.stdout.splitlines()[-1])
        result = dict(id=row['id'], rust_error=row['error'], oxide_status=record['status'])
        if record['status'] == 'ok':
            parsed = common.ir('office_oxide', json.load(gzip.open(output, 'rt')))
            result.update(slides=len(parsed['slides']), body_characters=sum(len(p['text']) for slide in parsed['slides'] for p in slide['paragraphs']), images=sum(len(slide['images']) for slide in parsed['slides']), tables=sum(len(slide['tables']) for slide in parsed['slides']))
        results.append(result)
    summary = dict(phase=phase, binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), scope='Returned content counts on the preceding acceptance-gap set; no independent legacy semantic oracle.', records=results)
    (directory / 'summary.json').write_text(json.dumps(summary, indent=2))
    return dict(inputs=len(results), without_body_images_tables=sum(r.get('body_characters') == r.get('images') == r.get('tables') == 0 for r in results))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('phase')
    args = parser.parse_args()
    print(json.dumps(dict(strict_inputs=strict_check(args.phase), legacy=legacy_check(args.phase))))

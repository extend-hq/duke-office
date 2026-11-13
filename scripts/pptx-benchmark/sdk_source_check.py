import argparse
import collections
import gzip
import hashlib
import json
from pathlib import Path
import shutil
import statistics
import subprocess
import zipfile
from xml.etree import ElementTree as ET

import common
from assess import ENGINES, prf, score
from feedback import OUT, ROOT


STRICT = {
    'p': 'http://purl.oclc.org/ooxml/presentationml/main',
    'a': 'http://purl.oclc.org/ooxml/drawingml/main',
    'r': 'http://purl.oclc.org/ooxml/officeDocument/relationships',
    'c': 'http://purl.oclc.org/ooxml/drawingml/chart',
}
TRANSITIONAL = common.NS.copy()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def reference(path):
    with zipfile.ZipFile(path) as archive:
        root = ET.fromstring(archive.read('ppt/presentation.xml'))
        families = {'{' + ns['p'] + '}presentation': (name, ns) for name, ns in [('strict', STRICT), ('transitional', TRANSITIONAL)]}
        if root.tag not in families:
            raise ValueError('Unsupported presentation namespace')
        family, namespaces = families[root.tag]
        slide_ids = root.findall('./p:sldIdLst/p:sldId', namespaces)
        relationships = common.rels(archive, 'ppt/presentation.xml')
        for node in slide_ids:
            relation = relationships[node.get('{' + namespaces['r'] + '}id')]
            if relation['external'] or relation['type'] != 'slide':
                raise ValueError('Unresolved internal slide relationship')
            slide_root = ET.fromstring(archive.read(relation['target']))
            if slide_root.tag != '{' + namespaces['p'] + '}sld':
                raise ValueError('Mixed or unsupported slide namespace')
    previous = common.NS.copy()
    try:
        common.NS.clear()
        common.NS.update(namespaces)
        result = common.oracle(path)
    finally:
        common.NS.clear()
        common.NS.update(previous)
    assert len(result['slides']) == len(slide_ids)
    result.update(namespace_family=family, status='ok')
    return result


def aggregate(rows):
    output = {}
    for engine in ENGINES:
        selected = [r for r in rows if r['engine'] == engine]
        features = {}
        for key in sorted({k for r in selected for k in r['metrics']}):
            values = [r['metrics'][key] for r in selected if key in r['metrics']]
            item = prf([sum(v[k] for v in values) for k in ['matched', 'expected', 'found']])
            bearing = [v for v in values if v['expected']]
            item.update(documents=len(bearing), macro_f1=statistics.mean(v['f1'] for v in bearing) if bearing else None)
            features[key] = item
        output[engine] = dict(total=len(selected), accepted=sum(r['status'] == 'ok' for r in selected), features=features)
    return output


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    corpus = OUT / 'corpus-followup/openxml-sdk'
    manifest = corpus / 'manifest.json'
    cohort = [r for r in json.loads(manifest.read_text()) if r['split'] == 'tuning']
    assert len(cohort) == len({r['id'] for r in cohort}) == 156
    run = corpus / args.output
    run.mkdir()
    for name in ['native', 'reference', 'harness']:
        (run / name).mkdir()
    source = OUT / 'experiments' / args.label
    binary = source / 'candidate'
    source_identity = json.loads((source / 'identity.json').read_text())
    assert digest(binary) == source_identity['binary_sha256']
    harness = Path(__file__).parent
    frozen = {}
    for name in [Path(__file__).name, 'common.py', 'assess.py', 'feedback.py', 'python_worker.py']:
        shutil.copy2(harness / name, run / 'harness' / name)
        frozen[name] = digest(run / 'harness' / name)
    versions = subprocess.check_output([str(OUT / 'venv/bin/python'), '-c', "import importlib.metadata,json; print(json.dumps({k:importlib.metadata.version(k) for k in ['python-pptx','markitdown']}))"], text=True)
    identity = dict(label=args.label, binary_path=str(binary), binary_sha256=digest(binary), source_identity_sha256=digest(source / 'identity.json'), manifest_sha256=digest(manifest), harness_sha256=frozen, python_versions=json.loads(versions), scope='Tuning-only accuracy supplement. Existing source-literal scoring with the actual Strict or Transitional namespaces. Failed parses remain in accuracy denominators. No latency ranking or rendered-fidelity claim; prior references and headline denominators remain unchanged.')
    (run / 'identity.json').write_text(json.dumps(identity, indent=2))
    (run / 'cohort.json').write_text(json.dumps(cohort, indent=2))
    rows, excluded = [], []
    records_path = run / 'records.jsonl'
    with records_path.open('w', buffering=1) as stream:
        for index, row in enumerate(cohort):
            assert digest(Path(row['path'])) == row['sha256']
            try:
                ref = reference(row['path'])
            except Exception as error:
                ref = dict(status='error', error=type(error).__name__ + ': ' + str(error))
                excluded.append(dict(id=row['id'], **ref))
            (run / 'reference' / (row['id'] + '.json')).write_text(json.dumps(ref))
            for engine in ENGINES:
                output = run / 'native' / (row['id'] + '.' + engine + '.json.gz')
                command = [str(binary), engine, row['path'], str(output), '0'] if engine in ENGINES[:3] else [str(OUT / 'venv/bin/python'), str(run / 'harness/python_worker.py'), engine, row['path'], str(output), '0']
                try:
                    process = subprocess.run(command, capture_output=True, text=True, timeout=120)
                    record = json.loads(process.stdout.splitlines()[-1]) if process.stdout.strip() else dict(status='process_error', error=process.stderr[-1500:])
                except subprocess.TimeoutExpired:
                    record = dict(status='timeout')
                record.update(id=row['id'], engine=engine, reference_status=ref['status'])
                if output.exists():
                    record['output_sha256'] = digest(output)
                stream.write(json.dumps(record) + '\n')
                if ref['status'] != 'ok':
                    continue
                actual = {'slides': []}
                if record['status'] == 'ok':
                    data = json.load(gzip.open(output, 'rt'))
                    actual = common.core(data) if engine == 'duke-slides' else common.ir(engine, data) if engine in ENGINES[1:3] else common.markdown_model(data['markdown']) if engine == 'markitdown' else data
                rows.append(dict(id=row['id'], engine=engine, status=record['status'], namespace_family=ref['namespace_family'], metrics=score(ref, actual)))
            if index % 20 == 0:
                print(index + 1, len(cohort), flush=True)
    assert digest(binary) == identity['binary_sha256']
    assert all(digest(run / 'harness' / name) == value for name, value in frozen.items())
    references = {r['id']: r['namespace_family'] for r in rows}
    result = dict(identity=identity, inputs=len(cohort), reference_inputs=len(references), namespaces=dict(collections.Counter(references.values())), excluded_reference=excluded, summary=aggregate(rows), by_namespace={family: aggregate([r for r in rows if r['namespace_family'] == family]) for family in sorted(set(references.values()))}, records_sha256=digest(records_path), reference_sha256={p.name:digest(p) for p in sorted((run / 'reference').glob('*.json'))}, rows=rows)
    (run / 'assessment.json').write_text(json.dumps(result, indent=2))
    print(json.dumps(dict(inputs=len(cohort), namespaces=result['namespaces'], excluded_reference=excluded, engines={engine:dict(accepted=value['accepted'], body_recall=value['features']['body_text']['recall'], body_macro_f1=value['features']['body_text']['macro_f1']) for engine,value in result['summary'].items()})))


if __name__ == '__main__':
    main()

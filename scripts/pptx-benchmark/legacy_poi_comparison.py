import argparse
import collections
from concurrent.futures import ThreadPoolExecutor
import gzip
import hashlib
import json
from pathlib import Path
import re

from assess import counts, grams, prf
from common import core, ir, norm
from feedback import OUT, invoke


def normalize(text):
    return re.sub(r'\s+', '', norm(text))


def agreement(reference, actual, key):
    total = [0, 0, 0]
    for index in range(max(len(reference['slides']), len(actual['slides']))):
        source = reference['slides'][index] if index < len(reference['slides']) else {}
        parsed = actual['slides'][index] if index < len(actual['slides']) else {}
        expected = normalize(source.get('body_text' if key == 'body_text' else 'notes_text', ''))
        observed = normalize(' '.join(p['text'] for p in parsed.get('paragraphs' if key == 'body_text' else 'notes', [])))
        total = [a + b for a, b in zip(total, counts(grams([expected]), grams([observed])))]
    return prf(total)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', default='legacy-poi-comparison')
    parser.add_argument('--candidate', default='chart-group1')
    parser.add_argument('--manifest', type=Path, default=OUT / 'poi-runtime/legacy-tuning-cohort.json')
    parser.add_argument('--reference', type=Path, default=OUT / 'poi-legacy-tuning')
    parser.add_argument('--engines', nargs='+', choices=['duke-slides', 'office_oxide', 'undoc'], default=['duke-slides', 'office_oxide', 'undoc'])
    args = parser.parse_args()
    poi = args.reference
    run = OUT / args.output
    rows = json.loads(args.manifest.read_text())
    reference_records = {r['id']: r for r in map(json.loads, (poi / 'records.jsonl').read_text().splitlines())}
    reference_summary = json.loads((poi / 'summary.json').read_text())
    assert len(rows) == len(reference_records) == reference_summary['total']
    assert {r['id'] for r in rows} == reference_records.keys()
    binary = OUT / 'experiments' / args.candidate / 'candidate'
    run.mkdir(exist_ok=True)
    destination = run / 'records.jsonl'
    if destination.exists():
        raise SystemExit('Preserve existing comparison results.')
    engines = args.engines

    def check(row):
        assert hashlib.sha256(Path(row['path']).read_bytes()).hexdigest() == row['sha256']
        reference = json.loads((poi / (row['id'] + '.json')).read_text()) if reference_records[row['id']]['status'] == 'ok' else None
        records = []
        for engine in engines:
            target = run / (row['id'] + '.' + engine + '.json.gz')
            record = invoke(binary, engine, row, target, 0)
            record.update(id=row['id'], engine=engine, poi_status=reference_records[row['id']]['status'])
            if reference is not None:
                if record['status'] == 'ok':
                    data = json.load(gzip.open(target, 'rt'))
                    actual = core(data) if engine == 'duke-slides' else ir(engine, data)
                else:
                    actual = dict(slides=[])
                record['metrics'] = {key: agreement(reference, actual, key) for key in ['body_text', 'notes']}
                record['slide_counts'] = [len(reference['slides']), len(actual['slides'])]
            records.append(record)
        return records

    records = []
    with ThreadPoolExecutor(max_workers=2) as pool, destination.open('w', buffering=1) as stream:
        for index, result in enumerate(pool.map(check, rows)):
            for record in result:
                records.append(record)
                stream.write(json.dumps(record) + '\n')
            if index % 25 == 0:
                print(index, len(rows), flush=True)
    summary = dict(inputs=len(rows), poi_accepted=reference_summary['accepted'], engines={}, binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), poi_identity=reference_summary['identity'], reference_records_sha256=hashlib.sha256((poi / 'records.jsonl').read_bytes()).hexdigest(), manifest_sha256=hashlib.sha256(args.manifest.read_bytes()).hexdigest(), methodology='Per-slide NFKC-normalized, whitespace-free character five-gram agreement against Apache POI text extraction. Failures on POI-accepted inputs count as empty. This is cross-parser agreement, not independently adjudicated accuracy. JVM diagnostics and concurrent native runs do not establish latency rankings.')
    for engine in engines:
        selected = [r for r in records if r['engine'] == engine]
        metrics = {}
        for key in ['body_text', 'notes']:
            values = [r['metrics'][key] for r in selected if 'metrics' in r]
            metrics[key] = prf([sum(v[field] for v in values) for field in ['matched', 'expected', 'found']])
        summary['engines'][engine] = dict(accepted=sum(r['status'] == 'ok' for r in selected), accepted_poi_reference=sum(r['status'] == 'ok' and r['poi_status'] == 'ok' for r in selected), metrics=metrics, slide_counts_equal=sum(r.get('slide_counts', [None, False])[0] == r.get('slide_counts', [None, False])[1] for r in selected))
    (run / 'summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps({k: v for k, v in summary.items() if k != 'poi_identity'}))


if __name__ == '__main__':
    main()

import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import statistics

from feedback import OUT, invoke


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--label', required=True); parser.add_argument('--select-only', action='store_true'); args = parser.parse_args()
    run = OUT / 'experiments' / args.label; selection_path = run / 'field-timing-selection.json'
    digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
    source = OUT / 'experiments/fields1/fields-reviewed.jsonl'
    source_proof = json.loads(source.with_name('fields-reviewed-summary.json').read_text())
    assert digest(source) == source_proof['records_sha256']
    assessment_path = OUT / 'expanded-images/assessment.json'
    assessment = json.loads(assessment_path.read_text())
    cohort_path = assessment_path.with_name('cohort.json')
    cohort = {r['id']: r for r in json.loads(cohort_path.read_text())}
    field_counts = {r['id']: r.get('source_counts', {}).get('fields', 0) for r in map(json.loads, source.read_text().splitlines())}
    selection = dict(rule='All common nonempty tuning inputs in expanded-images with at least 50 independently source-reviewed emitted fields; no latency selection.', minimum_fields=50, rows=[dict(cohort[i], emitted_fields=field_counts[i]) for i in sorted(assessment['common_ids']) if field_counts.get(i, 0) >= 50], source_records_sha256=digest(source), assessment_sha256=digest(assessment_path), cohort_sha256=digest(cohort_path))
    assert all(r['split'] == 'tuning' for r in selection['rows'])
    if args.select_only:
        assert not selection_path.exists(), 'Preserve the predeclared selection.'
        selection_path.write_text(json.dumps(selection, indent=2)); print('Selected', len(selection['rows']), 'field-heavy inputs'); return
    assert selection == json.loads(selection_path.read_text()), 'Predeclared selection changed.'
    gate = json.loads((run / 'parity-summary.json').read_text())
    assert gate['equal'] == gate['total']
    assert digest(run / 'parity.jsonl') == gate['records_sha256']
    sdk = json.loads((run / 'sdk-exact/summary.json').read_text())
    assert sdk['equal'] == sdk['total'] and sdk['candidate_sha256'] == gate['candidate_sha256'] and sdk['baseline_sha256'] == gate['baseline_sha256']
    assert digest(run / 'sdk-exact/records.jsonl') == sdk['records_sha256']
    binaries = {'baseline': Path(gate['baseline_path']), 'candidate': run / 'candidate'}
    assert all(digest(binary) == gate[label + '_sha256'] for label, binary in binaries.items())
    rows = selection['rows']; rng = random.Random(20261107); rng.shuffle(rows)
    records = []; output = run / 'field-timings.jsonl'; assert not output.exists(), 'Preserve timing receipts.'
    with output.open('w', buffering=1) as stream:
        for row in rows:
            assert digest(Path(row['path'])) == row['sha256']
            engines = list(binaries); rng.shuffle(engines)
            for label in engines:
                record = invoke(binaries[label], 'rust-pptx', row, '-', 7)
                assert record['status'] == 'ok'
                record.update(id=row['id'], label=label, load=os.getloadavg()); records.append(record); stream.write(json.dumps(record) + '\n')
            print(row['id'], flush=True)
    medians = {(r['id'], r['label']): statistics.median(r['samples_ms']) for r in records}
    ratios = [medians[r['id'], 'baseline'] / medians[r['id'], 'candidate'] for r in rows]
    bootstrap = sorted(statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(10000))
    summary = dict(inputs=len(rows), baseline_sha256=gate['baseline_sha256'], candidate_sha256=gate['candidate_sha256'], median_paired_speedup=statistics.median(ratios), median_speedup_ci95=[bootstrap[250], bootstrap[9749]], candidate_wins=sum(x > 1 for x in ratios), summed_medians={label:sum(medians[r['id'],label] for r in rows) for label in binaries}, selection_sha256=digest(selection_path), records_sha256=digest(output), harness_sha256=digest(Path(__file__)), scope='Predeclared high-field-count subset, paired single-process parsing with seven warm samples and randomized baseline/candidate order. Not a full-corpus or peer ranking.')
    (run / 'field-timing-summary.json').write_text(json.dumps(summary, indent=2)); print(json.dumps(summary))


if __name__ == '__main__':
    main()

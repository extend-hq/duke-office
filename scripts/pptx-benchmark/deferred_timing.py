"""Paired timing of the deferred-artwork parse mode.

Selects the same even-quantile tuning inputs as `expanded_timing.py`, then
interleaves the candidate's eager mode, its deferred mode, Office Oxide and
undoc with seven warm samples each. The deferred mode is a different workload
(no generated artwork); its ratios are reported separately from the eager
default and must not be presented as the default-mode result.
"""
import argparse
import hashlib
import json
import os
import random
import statistics
from pathlib import Path

from feedback import OUT, invoke


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--count', type=int, default=40)
    parser.add_argument('--cohort-phase', default='expanded-xml-indexed')
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    candidate = run / 'candidate'
    identity = json.loads((run / 'identity.json').read_text())
    if hashlib.sha256(candidate.read_bytes()).hexdigest() != identity['binary_sha256']:
        raise SystemExit('Executable identity changed.')
    proof = json.loads((run / 'artwork-deferral-summary.json').read_text())
    if proof['passed'] != proof['total'] or proof['candidate_sha256'] != identity['binary_sha256']:
        raise SystemExit('The deferred-artwork semantic gate must pass for this executable first.')
    reference = json.loads((OUT / args.cohort_phase / 'assessment.json').read_text())
    manifest = {r['id']: r for r in json.loads((OUT / args.cohort_phase / 'cohort.json').read_text())}
    eligible = sorted([r for r in reference['rows'] if r['engine'] == 'rust-pptx' and r['id'] in reference['common_ids']], key=lambda r: r['median_ms'])
    count = min(args.count, len(eligible))
    selected = [eligible[round(i * (len(eligible) - 1) / max(1, count - 1))]['id'] for i in range(count)]
    gated = {json.loads(line)['id'] for line in (run / 'artwork-deferral.jsonl').read_text().splitlines()}
    if not set(selected).issubset(gated):
        raise SystemExit('Every timed input must be covered by the deferred-artwork gate.')
    if any(manifest[ident]['split'] != 'tuning' for ident in selected):
        raise SystemExit('Only tuning inputs are allowed.')
    if any(hashlib.sha256(Path(manifest[ident]['path']).read_bytes()).hexdigest() != manifest[ident]['sha256'] for ident in selected):
        raise SystemExit('An input identity changed.')
    rng = random.Random(20261107)
    rng.shuffle(selected)
    output = run / 'deferred-timings.jsonl'
    if output.exists():
        raise SystemExit('Timing output already exists; preserve the original run.')
    records = []
    with output.open('w', buffering=1) as stream:
        for ident in selected:
            engines = [('eager', 'rust-pptx'), ('deferred', 'rust-pptx-deferred'), ('office_oxide', 'office_oxide'), ('undoc', 'undoc')]
            rng.shuffle(engines)
            for label, engine in engines:
                record = invoke(candidate, engine, manifest[ident], '-', 7)
                record.update(id=ident, label=label, load=os.getloadavg())
                records.append(record)
                stream.write(json.dumps(record) + '\n')
                if record['status'] != 'ok':
                    raise SystemExit('A selected timing input failed.')
            print(ident, flush=True)
    times = {(r['id'], r['label']): statistics.median(r['samples_ms']) for r in records}
    def paired(numerator, denominator):
        ratios = [times[ident, numerator] / times[ident, denominator] for ident in selected]
        bootstrap = sorted(statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(4000))
        return dict(median=statistics.median(ratios), ci95=[bootstrap[100], bootstrap[3899]])
    summary = dict(inputs=count, split='expanded tuning', selection='even quantiles of the preceding full-model latency distribution', seed=20261107, cohort_phase=args.cohort_phase,
                   eager_over_deferred=paired('eager', 'deferred'), oxide_over_deferred=paired('office_oxide', 'deferred'), oxide_over_eager=paired('office_oxide', 'eager'),
                   deferred_wins_vs_oxide=sum(times[i, 'deferred'] < times[i, 'office_oxide'] for i in selected), eager_wins_vs_oxide=sum(times[i, 'eager'] < times[i, 'office_oxide'] for i in selected),
                   summed_medians={label: sum(times[i, label] for i in selected) for label in ['eager', 'deferred', 'office_oxide', 'undoc']},
                   candidate_sha256=identity['binary_sha256'], gate_sha256=hashlib.sha256((run / 'artwork-deferral-summary.json').read_bytes()).hexdigest(), timing_harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                   scope='Deferred mode omits generated artwork and is a different workload from the eager default; ratios are tuning results on the fixed selection, not untouched validation.')
    (run / 'deferred-timing-summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary))


if __name__ == '__main__':
    main()

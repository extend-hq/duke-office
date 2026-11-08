"""Paired timing of on-demand node rendering through the retained parse handle.

Selects the same even-quantile tuning inputs as `expanded_timing.py`. For each,
the candidate runner's `rust-pptx-render-timing` engine collects the first
`--targets` nodes with eager artwork and times, interleaved within one process
and alternating in order, N renders through `render_node_artwork` (one parse per
render) against one `parse_presentation_retained` plus the same N renders through
the handle. The handle's creation is a deferred parse and is included in its
time and also reported alone. Load averages are recorded per input. Ratios are
paired per input; inputs without eager artwork are listed but carry no ratio.
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
    parser.add_argument('--targets', type=int, default=12)
    parser.add_argument('--samples', type=int, default=7)
    parser.add_argument('--cohort-phase', default='expanded-xml-indexed')
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    candidate = run / 'candidate'
    identity = json.loads((run / 'identity.json').read_text())
    if hashlib.sha256(candidate.read_bytes()).hexdigest() != identity['binary_sha256']:
        raise SystemExit('Executable identity changed.')
    proof = json.loads((run / 'artwork-deferral-summary.json').read_text())
    if proof['passed'] != proof['total'] or proof['candidate_sha256'] != identity['binary_sha256'] or proof.get('retained_matched') != proof['checked']:
        raise SystemExit('The deferred-artwork semantic gate, including the retained handle, must pass for this executable first.')
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
    rng = random.Random(20261108)
    rng.shuffle(selected)
    output = run / 'retained-render-timings.jsonl'
    if output.exists():
        raise SystemExit('Timing output already exists; preserve the original run.')
    records = []
    with output.open('w', buffering=1) as stream:
        for ident in selected:
            record = invoke(candidate, 'rust-pptx-render-timing', manifest[ident], '-', args.samples, args.targets)
            record.update(id=ident, load=os.getloadavg())
            records.append(record)
            stream.write(json.dumps(record) + '\n')
            if record['status'] != 'ok':
                raise SystemExit('A selected timing input failed.')
            print(ident, record['targets'], flush=True)
    timed = [r for r in records if r['targets'] > 0]
    medians = {r['id']: {key: statistics.median(r[key]) for key in ['reparse_ms', 'retained_ms', 'handle_ms', 'handle_render_ms']} for r in timed}

    def paired(numerator, denominator):
        ratios = [medians[r['id']][numerator] / medians[r['id']][denominator] for r in timed]
        bootstrap = sorted(statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(4000))
        return dict(median=statistics.median(ratios), ci95=[bootstrap[100], bootstrap[3899]])

    per_render = {key: sum(medians[r['id']][key] for r in timed) / sum(r['targets'] for r in timed) for key in ['reparse_ms', 'handle_render_ms']}
    loads = [value for r in records for value in r['load'][:1]]
    summary = dict(inputs=count, timed_inputs=len(timed), targets_per_input=args.targets, targets=sum(r['targets'] for r in timed), samples=args.samples, split='expanded tuning',
                   selection='even quantiles of the preceding full-model latency distribution', seed=20261108, cohort_phase=args.cohort_phase,
                   reparse_over_retained=paired('reparse_ms', 'retained_ms'), reparse_over_handle_renders=paired('reparse_ms', 'handle_render_ms'),
                   retained_wins=sum(medians[r['id']]['retained_ms'] < medians[r['id']]['reparse_ms'] for r in timed),
                   summed_medians={key: sum(medians[r['id']][key] for r in timed) for key in ['reparse_ms', 'retained_ms', 'handle_ms', 'handle_render_ms']},
                   mean_ms_per_render=per_render, load_one_minute=dict(min=min(loads), max=max(loads)),
                   candidate_sha256=identity['binary_sha256'], gate_sha256=hashlib.sha256((run / 'artwork-deferral-summary.json').read_bytes()).hexdigest(), timing_harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                   scope='N on-demand renders per input through the retained handle (one deferred parse plus N renders) against N renders through render_node_artwork (N parses). Both methods render the same nodes; ratios are tuning results on the fixed selection, not untouched validation, and say nothing about the eager default.')
    (run / 'retained-render-timing-summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary))


if __name__ == '__main__':
    main()

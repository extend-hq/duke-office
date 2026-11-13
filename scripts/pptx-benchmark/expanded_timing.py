import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import statistics

from feedback import invoke

OUT = Path(__file__).resolve().parents[2] / 'output/pptx-benchmark-20261106'


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--count', type=int, default=40)
    parser.add_argument('--cohort-phase', default='expanded-comparison')
    parser.add_argument('--run-tag', default='')
    parser.add_argument('--semantic-reference-label')
    parser.add_argument('--baseline-label')
    parser.add_argument('--peer-binary', type=Path)
    parser.add_argument('--svg-definition-gate', action='store_true')
    parser.add_argument('--exact-gate', action='store_true')
    parser.add_argument('--artwork-math-gate', action='store_true')
    parser.add_argument('--fields-gate', action='store_true')
    parser.add_argument('--fields-source-label')
    parser.add_argument('--image-fallback-gate', action='store_true')
    parser.add_argument('--picture-fallback-gate', action='store_true')
    parser.add_argument('--modern-comments-gate', action='store_true')
    parser.add_argument('--table-grid-gate', action='store_true')
    parser.add_argument('--object-editing-gate', action='store_true')
    parser.add_argument('--object-editing-source-label')
    args = parser.parse_args()
    if args.run_tag and (not args.run_tag.replace('-', '').replace('_', '').isalnum()):
        raise SystemExit('The run tag must contain letters, digits, underscores or hyphens.')
    if args.count <= 0:
        raise SystemExit('The requested input count must be positive.')
    run = OUT / 'experiments' / args.label
    candidate = run / 'candidate'
    original_gate = None
    if args.fields_source_label and not args.fields_gate:
        raise SystemExit('A field source label requires the field review gate.')
    if args.object_editing_source_label and not args.object_editing_gate:
        raise SystemExit('An object editing source label requires its review gate.')
    if args.object_editing_gate:
        if any([args.table_grid_gate, args.modern_comments_gate, args.image_fallback_gate, args.picture_fallback_gate, args.baseline_label, args.semantic_reference_label, args.svg_definition_gate, args.exact_gate, args.artwork_math_gate, args.fields_gate]):
            raise SystemExit('The object editing review fixes its own baseline.')
        editing_source_run = OUT / 'experiments' / (args.object_editing_source_label or args.label)
        proof = json.loads((editing_source_run / 'editing-reviewed-summary.json').read_text())
        records_path = editing_source_run / 'editing-reviewed.jsonl'
        records = [json.loads(line) for line in records_path.read_text().splitlines()]
        if proof['passed'] != proof['total'] or len(records) != proof['total'] or not all(r['passed'] for r in records):
            raise SystemExit('The complete object editing source review must pass.')
        if hashlib.sha256(records_path.read_bytes()).hexdigest() != proof['records_sha256']:
            raise SystemExit('Object editing audit records changed.')
        identity = json.loads((run / 'identity.json').read_text())
        root = Path(__file__).resolve().parents[2]
        if any(hashlib.sha256((root / name).read_bytes()).hexdigest() != digest for name, digest in identity['sources'].items()):
            raise SystemExit('Source changed after the object editing build.')
        baseline = OUT / 'experiments/xml-indexed1/candidate'
        gate = {key: proof[key] for key in ['candidate_sha256', 'baseline_sha256']}
        if editing_source_run != run:
            current_proof = json.loads((run / 'parity-summary.json').read_text())
            current_path = run / 'parity.jsonl'
            current_records = [json.loads(line) for line in current_path.read_text().splitlines()]
            if current_proof['equal'] != current_proof['total'] or current_proof['total'] != proof['total'] or not all(r['equal'] for r in current_records):
                raise SystemExit('The subsequent exact-output audit must pass the entire source-reviewed corpus.')
            if hashlib.sha256(current_path.read_bytes()).hexdigest() != current_proof['records_sha256']:
                raise SystemExit('The subsequent exact-output records changed.')
            chain = current_proof
            visited = {run.resolve()}
            while Path(chain['baseline_path']).resolve() != (editing_source_run / 'candidate').resolve():
                prior_run = Path(chain['baseline_path']).resolve().parent
                if prior_run in visited:
                    raise SystemExit('The exact-output chain contains a cycle.')
                visited.add(prior_run)
                prior_proof = json.loads((prior_run / 'parity-summary.json').read_text())
                prior_path = prior_run / 'parity.jsonl'
                prior_records = [json.loads(line) for line in prior_path.read_text().splitlines()]
                if prior_proof['candidate_sha256'] != chain['baseline_sha256'] or hashlib.sha256((prior_run / 'candidate').read_bytes()).hexdigest() != chain['baseline_sha256']:
                    raise SystemExit('An intermediate exact-output executable differs.')
                if prior_proof['equal'] != proof['total'] or prior_proof['total'] != proof['total'] or len(prior_records) != proof['total'] or not all(r['equal'] for r in prior_records):
                    raise SystemExit('An intermediate exact-output audit is incomplete.')
                if hashlib.sha256(prior_path.read_bytes()).hexdigest() != prior_proof['records_sha256']:
                    raise SystemExit('Intermediate exact-output records changed.')
                fields = lambda row: (row['status'], row.get('full_sha256'), row.get('error'))
                if {r['id']: fields(r) for r in records} != {r['id']: fields(r) for r in prior_records}:
                    raise SystemExit('Intermediate outputs differ from the source-reviewed corpus.')
                chain = prior_proof
            if chain['baseline_sha256'] != proof['candidate_sha256'] or hashlib.sha256((editing_source_run / 'candidate').read_bytes()).hexdigest() != proof['candidate_sha256']:
                raise SystemExit('The exact-output chain does not reach the source-reviewed candidate.')
            fields = lambda row: (row['status'], row.get('full_sha256'), row.get('error'))
            if {r['id']: fields(r) for r in records} != {r['id']: fields(r) for r in current_records}:
                raise SystemExit('Subsequent outputs differ from the source-reviewed corpus.')
            gate['candidate_sha256'] = current_proof['candidate_sha256']
    elif args.table_grid_gate:
        if any([args.modern_comments_gate, args.image_fallback_gate, args.picture_fallback_gate, args.baseline_label, args.semantic_reference_label, args.svg_definition_gate, args.exact_gate, args.artwork_math_gate, args.fields_gate]):
            raise SystemExit('The table grid review fixes its own baseline.')
        proof = json.loads((run / 'table-reviewed-summary.json').read_text())
        records_path = run / 'table-reviewed.jsonl'
        records = [json.loads(line) for line in records_path.read_text().splitlines()]
        if proof['passed'] != proof['total'] or len(records) != proof['total'] or not all(r['passed'] for r in records):
            raise SystemExit('The complete table grid source review must pass.')
        if hashlib.sha256(records_path.read_bytes()).hexdigest() != proof['records_sha256']:
            raise SystemExit('Table grid audit records changed.')
        identity = json.loads((run / 'identity.json').read_text())
        root = Path(__file__).resolve().parents[2]
        if any(hashlib.sha256((root / name).read_bytes()).hexdigest() != digest for name, digest in identity['sources'].items()):
            raise SystemExit('Source changed after the table grid build.')
        baseline = OUT / 'experiments/modern-comments2/candidate'
        gate = {key: proof[key] for key in ['candidate_sha256', 'baseline_sha256']}
    elif args.modern_comments_gate:
        if any([args.image_fallback_gate, args.picture_fallback_gate, args.baseline_label, args.semantic_reference_label, args.svg_definition_gate, args.exact_gate, args.artwork_math_gate, args.fields_gate]):
            raise SystemExit('The modern comment source review fixes its own baseline.')
        proof = json.loads((run / 'modern-reviewed-summary.json').read_text())
        records_path = run / 'modern-reviewed.jsonl'
        records = [json.loads(line) for line in records_path.read_text().splitlines()]
        if proof['passed'] != proof['total'] or len(records) != proof['total'] or not all(r['passed'] for r in records):
            raise SystemExit('The complete modern comment source review must pass.')
        if hashlib.sha256(records_path.read_bytes()).hexdigest() != proof['records_sha256']:
            raise SystemExit('Modern comment audit records changed.')
        identity = json.loads((run / 'identity.json').read_text())
        root = Path(__file__).resolve().parents[2]
        if any(hashlib.sha256((root / name).read_bytes()).hexdigest() != digest for name, digest in identity['sources'].items()):
            raise SystemExit('Source changed after the modern comment build.')
        baseline = OUT / 'experiments/xml-recent1/candidate'
        gate = {key: proof[key] for key in ['candidate_sha256', 'baseline_sha256']}
    elif args.image_fallback_gate or args.picture_fallback_gate:
        if args.image_fallback_gate and args.picture_fallback_gate:
            raise SystemExit('Select one fallback source review.')
        if args.baseline_label or args.semantic_reference_label or args.svg_definition_gate or args.exact_gate or args.artwork_math_gate or args.fields_gate:
            raise SystemExit('The image fallback review fixes its own baseline.')
        review_name = 'picture-fallback' if args.picture_fallback_gate else 'image-fallback'
        proof = json.loads((run / (review_name + '-reviewed-summary.json')).read_text())
        records_path = run / (review_name + '-reviewed.jsonl')
        records = [json.loads(line) for line in records_path.read_text().splitlines()]
        raw = json.loads((run / 'parity-summary.json').read_text())
        raw_path = run / 'parity.jsonl'
        if proof['passed'] != proof['total'] or len(records) != proof['total'] or not all(r['passed'] for r in records):
            raise SystemExit('The complete image fallback source review must pass.')
        if hashlib.sha256(records_path.read_bytes()).hexdigest() != proof['records_sha256'] or hashlib.sha256(raw_path.read_bytes()).hexdigest() != proof['raw_records_sha256']:
            raise SystemExit('Image fallback audit records changed.')
        if raw['records_sha256'] != proof['raw_records_sha256'] or raw['total'] != proof['total'] or any(raw[k] != proof[k] for k in ['candidate_sha256', 'baseline_sha256', 'baseline_path', 'manifest_sha256']):
            raise SystemExit('Image fallback review refers to another raw audit.')
        baseline = Path(proof['baseline_path'])
        gate = {key: proof[key] for key in ['candidate_sha256', 'baseline_sha256']}
    elif args.fields_gate:
        if args.baseline_label or args.semantic_reference_label or args.svg_definition_gate or args.exact_gate or args.artwork_math_gate:
            raise SystemExit('The field source review fixes its own baseline.')
        proof_run = OUT / 'experiments' / (args.fields_source_label or args.label)
        proof = json.loads((proof_run / 'fields-reviewed-summary.json').read_text())
        records_path = proof_run / 'fields-reviewed.jsonl'
        records = [json.loads(line) for line in records_path.read_text().splitlines()]
        if proof['passed'] != proof['total'] or len(records) != proof['total'] or not all(r['passed'] for r in records):
            raise SystemExit('The complete field source review must pass.')
        if hashlib.sha256(records_path.read_bytes()).hexdigest() != proof['records_sha256']:
            raise SystemExit('Field source review records changed.')
        raw = json.loads((run / 'parity-summary.json').read_text())
        raw_path = run / 'parity.jsonl'
        if hashlib.sha256(raw_path.read_bytes()).hexdigest() != raw['records_sha256']:
            raise SystemExit('The underlying exact-output audit changed.')
        if raw['manifest_sha256'] != proof['manifest_sha256']:
            raise SystemExit('Field source review manifests do not match.')
        if proof_run == run:
            for key in ['candidate_sha256', 'baseline_sha256']:
                if raw[key] != proof[key]:
                    raise SystemExit('Field source review executable identities do not match.')
            if raw['records_sha256'] != proof['raw_records_sha256']:
                raise SystemExit('Field source review refers to another raw audit.')
        else:
            current = [json.loads(line) for line in raw_path.read_text().splitlines()]
            if raw['equal'] != raw['total'] or raw['total'] != proof['total'] or len(current) != len(records) or not all(r['equal'] for r in current):
                raise SystemExit('The subsequent exact-output gate must pass the entire reviewed corpus.')
            if raw['baseline_sha256'] != proof['candidate_sha256'] or Path(raw['baseline_path']) != proof_run / 'candidate':
                raise SystemExit('The subsequent exact-output gate has a different baseline.')
            identity = lambda row: (row['status'], row.get('full_sha256'), row.get('error'))
            if {r['id']: identity(r) for r in current} != {r['id']: identity(r) for r in records}:
                raise SystemExit('Subsequent outputs differ from the source-reviewed corpus.')
        baseline = Path(proof['baseline_path'])
        gate = dict(candidate_sha256=raw['candidate_sha256'], baseline_sha256=proof['baseline_sha256'])
    elif args.artwork_math_gate:
        if args.baseline_label or args.semantic_reference_label or args.svg_definition_gate or args.exact_gate:
            raise SystemExit('The artwork review fixes its own baseline.')
        proof = json.loads((run / 'artwork-reviewed-summary.json').read_text())
        if proof['passed'] != proof['total']:
            raise SystemExit('The complete artwork review must pass.')
        if hashlib.sha256((run / 'artwork-math.jsonl').read_bytes()).hexdigest() != proof['records_sha256']:
            raise SystemExit('Artwork audit records changed.')
        for name, expected in proof['review_sha256'].items():
            if hashlib.sha256((run / name).read_bytes()).hexdigest() != expected:
                raise SystemExit('Artwork review changed.')
        baseline = OUT / 'experiments' / proof['baseline_label'] / 'candidate'
        gate = {key: proof[key] for key in ['candidate_sha256', 'baseline_sha256']}
    elif args.exact_gate:
        if args.baseline_label or args.semantic_reference_label or args.svg_definition_gate:
            raise SystemExit('The exact-output gate fixes its own baseline.')
        original_gate = json.loads((run / 'parity-summary.json').read_text())
        records = [json.loads(line) for line in (run / 'parity.jsonl').read_text().splitlines()]
        if original_gate['equal'] != original_gate['total'] or len(records) != original_gate['total'] or not all(r['equal'] for r in records):
            raise SystemExit('The complete exact-output gate must pass.')
        if hashlib.sha256((run / 'parity.jsonl').read_bytes()).hexdigest() != original_gate['records_sha256']:
            raise SystemExit('Exact-output audit records changed.')
        baseline = Path(original_gate['baseline_path'])
        gate = original_gate
    elif args.svg_definition_gate:
        if args.baseline_label or args.semantic_reference_label:
            raise SystemExit('The SVG equivalence audit fixes its own baseline.')
        proof = json.loads((run / 'semantic-reviewed-summary.json').read_text())
        records_path = run / 'semantic-reviewed.jsonl'
        records = [json.loads(line) for line in records_path.read_text().splitlines()]
        if proof['passed'] != proof['total'] or len(records) != proof['total'] or not all(r['passed'] for r in records):
            raise SystemExit('The complete SVG equivalence audit must pass.')
        if hashlib.sha256(records_path.read_bytes()).hexdigest() != proof['reviewed_records_sha256']:
            raise SystemExit('SVG equivalence records changed.')
        baseline = OUT / 'experiments' / proof['baseline_label'] / 'candidate'
        gate = {key: proof[key] for key in ['candidate_sha256', 'baseline_sha256']}
    else:
        original_gate = json.loads((run / 'parity-summary.json').read_text())
    if args.semantic_reference_label:
        semantic = json.loads((OUT / 'experiments' / args.semantic_reference_label / 'semantic-summary.json').read_text())
        if semantic['passed'] != semantic['total'] or original_gate['equal'] != original_gate['total'] or original_gate['total'] != semantic['total']:
            raise SystemExit('The complete semantic and subsequent exact-output gates must pass.')
        if original_gate['baseline_sha256'] != semantic['candidate_sha256']:
            raise SystemExit('The validated semantic build does not match the equivalence baseline.')
        baseline = OUT / 'experiments' / 'opt8' / 'candidate'
        gate = dict(candidate_sha256=original_gate['candidate_sha256'], baseline_sha256=semantic['baseline_sha256'])
    elif not any([args.svg_definition_gate, args.exact_gate, args.artwork_math_gate, args.fields_gate, args.image_fallback_gate, args.picture_fallback_gate, args.modern_comments_gate, args.table_grid_gate, args.object_editing_gate]):
        gate = json.loads((run / 'expanded-parity-summary.json').read_text())
        baseline = Path(original_gate['baseline_path'])
        if gate['passed'] != gate['total'] or original_gate['equal'] != original_gate['total']:
            raise SystemExit('Both parity gates must pass.')
    if args.baseline_label:
        baseline_run = OUT / 'experiments' / args.baseline_label
        baseline_proof = json.loads((baseline_run / 'parity-summary.json').read_text())
        if baseline_proof['equal'] != baseline_proof['total'] or baseline_proof['baseline_sha256'] != original_gate['baseline_sha256']:
            raise SystemExit('The alternate baseline must share the validated exact-output reference.')
        fields = lambda record: (record['status'], record.get('full_sha256'), record.get('error'))
        before = {r['id']: fields(r) for r in map(json.loads, (baseline_run / 'parity.jsonl').read_text().splitlines())}
        after = {r['id']: fields(r) for r in map(json.loads, (run / 'parity.jsonl').read_text().splitlines())}
        if before != after:
            raise SystemExit('Alternate baseline and candidate outputs are not identical on the complete audited corpus.')
        baseline = baseline_run / 'candidate'
        gate['baseline_sha256'] = baseline_proof['candidate_sha256']
    for binary, key in [(candidate, 'candidate_sha256'), (baseline, 'baseline_sha256')]:
        if hashlib.sha256(binary.read_bytes()).hexdigest() != gate[key]:
            raise SystemExit('Executable identity changed.')
    peer_binary = args.peer_binary or candidate
    peer_sha256 = hashlib.sha256(peer_binary.read_bytes()).hexdigest()
    identity_path = run / 'identity.json'
    identity = json.loads(identity_path.read_text()) if identity_path.exists() else {}
    build_files = identity.get('build_files', {})
    source_files = identity.get('sources', {})
    root = Path(__file__).resolve().parents[2]
    if any(hashlib.sha256((root / path).read_bytes()).hexdigest() != digest for path, digest in source_files.items()):
        raise SystemExit('Parser source changed after the candidate snapshot.')
    if any(hashlib.sha256((root / path).read_bytes()).hexdigest() != digest for path, digest in build_files.items()):
        raise SystemExit('Build configuration changed after the candidate snapshot.')
    harness_sha256 = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    reference = json.loads((OUT / args.cohort_phase / 'assessment.json').read_text())
    manifest = {r['id']: r for r in json.loads((OUT / args.cohort_phase / 'cohort.json').read_text())}
    eligible = sorted([r for r in reference['rows'] if r['engine'] == 'duke-slides' and r['id'] in reference['common_ids']], key=lambda r: r['median_ms'])
    count = min(args.count, len(eligible))
    selected = [eligible[round(i * (len(eligible) - 1) / max(1, count - 1))]['id'] for i in range(count)]
    if (args.exact_gate or args.object_editing_gate) and not set(selected).issubset({record['id'] for record in records}):
        raise SystemExit('Every timed input must be covered by the selected output gate.')
    if any(manifest[ident]['split'] != 'tuning' for ident in selected):
        raise SystemExit('Only tuning inputs are allowed.')
    if any(hashlib.sha256(Path(manifest[ident]['path']).read_bytes()).hexdigest() != manifest[ident]['sha256'] for ident in selected):
        raise SystemExit('An input identity changed.')
    rng = random.Random(20261107)
    rng.shuffle(selected)
    records = []
    suffix = '-vs-' + args.baseline_label if args.baseline_label else ''
    if args.run_tag:
        suffix += '-' + args.run_tag
    output = run / ('expanded-timings' + suffix + '.jsonl')
    if output.exists():
        raise SystemExit('Timing output already exists; preserve the original run.')
    with output.open('w', buffering=1) as stream:
        for ident in selected:
            engines = [('baseline', baseline, 'duke-slides'), ('candidate', candidate, 'duke-slides'), ('office_oxide', peer_binary, 'office_oxide'), ('undoc', peer_binary, 'undoc')]
            rng.shuffle(engines)
            for label, binary, engine in engines:
                record = invoke(binary, engine, manifest[ident], '-', 7)
                record.update(id=ident, label=label, load=os.getloadavg())
                records.append(record)
                stream.write(json.dumps(record) + '\n')
                if record['status'] != 'ok':
                    raise SystemExit('A selected timing input failed.')
            print(ident, flush=True)
    if hashlib.sha256(peer_binary.read_bytes()).hexdigest() != peer_sha256:
        raise SystemExit('Peer executable changed during timing.')
    if any(hashlib.sha256((root / path).read_bytes()).hexdigest() != digest for path, digest in source_files.items()):
        raise SystemExit('Parser source changed during timing.')
    if any(hashlib.sha256((root / path).read_bytes()).hexdigest() != digest for path, digest in build_files.items()):
        raise SystemExit('Build configuration changed during timing.')
    times = {(r['id'], r['label']): statistics.median(r['samples_ms']) for r in records}
    ratios = [times[ident, 'baseline'] / times[ident, 'candidate'] for ident in selected]
    bootstrap = sorted(statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(4000))
    summary = dict(baseline_label=baseline.parent.name, semantic_reference_label=args.semantic_reference_label, svg_definition_equivalence=args.svg_definition_gate, inputs=count, split='expanded tuning', selection='even quantiles of the preceding full-model latency distribution', seed=20261107, median_paired_speedup=statistics.median(ratios), median_speedup_ci95=[bootstrap[100], bootstrap[3899]], candidate_wins_vs_oxide=sum(times[ident, 'candidate'] < times[ident, 'office_oxide'] for ident in selected), candidate_wins_vs_undoc=sum(times[ident, 'candidate'] < times[ident, 'undoc'] for ident in selected), summed_medians={label: sum(times[ident, label] for ident in selected) for label in ['baseline', 'candidate', 'office_oxide', 'undoc']}, baseline_sha256=gate['baseline_sha256'], candidate_sha256=gate['candidate_sha256'], selected_ids=selected)
    summary['artwork_math_review'] = args.artwork_math_gate
    summary['field_source_review'] = args.fields_gate
    summary['cohort_phase'] = args.cohort_phase
    summary['run_tag'] = args.run_tag
    summary['entire_eligible_cohort'] = count == len(eligible)
    summary['peer_binary_path'] = str(peer_binary)
    summary['peer_binary_sha256'] = peer_sha256
    summary['harness_sha256'] = harness_sha256
    if args.object_editing_gate:
        summary['object_editing_source_review'] = True
        summary['object_editing_source_label'] = editing_source_run.name
        summary['object_editing_review_sha256'] = hashlib.sha256((editing_source_run / 'editing-reviewed-summary.json').read_bytes()).hexdigest()
        if editing_source_run != run:
            summary['subsequent_exact_proof_sha256'] = hashlib.sha256((run / 'parity-summary.json').read_bytes()).hexdigest()
    if args.table_grid_gate:
        summary['table_grid_source_review'] = True
        summary['table_grid_review_sha256'] = hashlib.sha256((run / 'table-reviewed-summary.json').read_bytes()).hexdigest()
    if args.modern_comments_gate:
        summary['modern_comment_source_review'] = True
        summary['modern_comment_review_sha256'] = hashlib.sha256((run / 'modern-reviewed-summary.json').read_bytes()).hexdigest()
    if args.picture_fallback_gate:
        summary['picture_source_review'] = True
        summary['picture_review_summary_sha256'] = hashlib.sha256((run / 'picture-fallback-reviewed-summary.json').read_bytes()).hexdigest()
    summary['timing_harness_sha256'] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    if args.fields_gate:
        summary['field_review_summary_sha256'] = hashlib.sha256((proof_run / 'fields-reviewed-summary.json').read_bytes()).hexdigest()
        summary['field_source_label'] = proof_run.name
        summary['subsequent_exact_records_sha256'] = raw['records_sha256']
    (run / ('expanded-timing' + suffix + '-summary.json')).write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary))


if __name__ == '__main__':
    main()

"""Compare a completed phase's assessment with its predecessor's.

The common timing cohort and every engine's feature aggregates (matched, expected and found units)
must be identical; timings are not compared. Writes an immutable assessment-verification.json receipt.
"""
import argparse
import hashlib
import json
from pathlib import Path

OUT = Path(__file__).resolve().parents[2] / 'output/pptx-benchmark-20261106'
COUNTS = ['matched', 'expected', 'found']


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('phase')
    parser.add_argument('--baseline-phase', required=True)
    args = parser.parse_args()
    current_path, prior_path = OUT / args.phase / 'assessment.json', OUT / args.baseline_phase / 'assessment.json'
    current, prior = json.loads(current_path.read_text()), json.loads(prior_path.read_text())
    engines = current['identity']['selected_engines']
    if engines != prior['identity']['selected_engines']:
        raise SystemExit('The phases measured different engines.')
    if current['common_ids'] != prior['common_ids'] or current['exclude_empty_from_timing'] != prior['exclude_empty_from_timing']:
        raise SystemExit('The common timing cohort differs; report the regression before comparing.')
    differences = []
    for engine in engines:
        features, before = current['summary'][engine]['features'], prior['summary'][engine]['features']
        if set(features) != set(before):
            raise SystemExit('Feature rows differ for ' + engine)
        for key, value in features.items():
            if any(value[name] != before[key][name] for name in COUNTS):
                differences.append(dict(engine=engine, feature=key, before={name: before[key][name] for name in COUNTS}, after={name: value[name] for name in COUNTS}))
    receipt = dict(phase=args.phase, baseline=args.baseline_phase, common_inputs=len(current['common_ids']),
                   all_feature_aggregates_exact=not differences, feature_differences=differences, engines=engines,
                   reference_success={engine: f"{current['summary'][engine]['reference_success']}/{current['summary'][engine]['reference_total']}" for engine in engines},
                   assessment_sha256=sha(current_path), prior_assessment_sha256=sha(prior_path), harness_sha256=sha(__file__))
    output = OUT / args.phase / 'assessment-verification.json'
    if output.exists():
        raise SystemExit('Receipt exists; it is immutable: ' + str(output))
    output.write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt, indent=2))


if __name__ == '__main__':
    main()

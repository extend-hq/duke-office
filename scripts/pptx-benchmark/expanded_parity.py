import argparse
import gzip
import hashlib
import json
from pathlib import Path
import subprocess

OUT = Path(__file__).resolve().parents[2] / 'output/pptx-benchmark-20261106'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_records(phase):
    return {r['id']: r for r in map(json.loads, (OUT / phase / 'records.jsonl').read_text().splitlines()) if r['engine'] == 'duke-slides'}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--phase', required=True)
    parser.add_argument('--reference-phase', default='expanded-comparison')
    parser.add_argument('--baseline', type=Path, default=OUT / 'experiments/inserted2/candidate')
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    candidate = run / 'candidate'
    original = load_records(args.reference_phase)
    current = load_records(args.phase)
    manifest = {r['id']: r for r in json.loads((OUT / args.phase / 'cohort.json').read_text())}
    if original.keys() != current.keys() or current.keys() != manifest.keys():
        raise SystemExit('The complete cohort is required.')
    candidate_hash = digest(candidate)
    baseline_hash = digest(args.baseline)
    if any(r['binary_sha256'] != candidate_hash for r in current.values()) or any(r['binary_sha256'] != baseline_hash for r in original.values()):
        raise SystemExit('The recorded executable identity changed.')
    exact = 0
    warning_order = []
    failures = []
    for ident, record in current.items():
        old = original[ident]
        if record['status'] != old['status']:
            failures.append(dict(id=ident, reason='status'))
            continue
        if record['status'] != 'ok':
            if record.get('error') == old.get('error'):
                exact += 1
            else:
                failures.append(dict(id=ident, reason='error'))
            continue
        if record['full_sha256'] == old['full_sha256']:
            exact += 1
            continue
        models = []
        replay_hashes = []
        for index, binary in enumerate([args.baseline, args.baseline, candidate, candidate, candidate]):
            output = run / f'{ident}-full-replay-{index}.json.gz'
            process = subprocess.run([str(binary), 'duke-slides', manifest[ident]['path'], str(output), '0', 'full'], capture_output=True, text=True, check=True, timeout=120)
            replay = json.loads(process.stdout.splitlines()[-1])
            if replay['status'] != 'ok':
                raise SystemExit('A full replay failed.')
            replay_hashes.append(replay['full_sha256'])
            model = json.load(gzip.open(output, 'rt'))
            model['warnings'] = sorted(model.get('warnings', []), key=lambda x: json.dumps(x, sort_keys=True))
            models.append(model)
        frozen = json.load(gzip.open(OUT / args.reference_phase / 'native' / (ident + '.duke-slides.json.gz'), 'rt'))
        recent = json.load(gzip.open(OUT / args.phase / 'native' / (ident + '.duke-slides.json.gz'), 'rt'))
        for model in [frozen, recent]:
            model['warnings'] = sorted(model.get('warnings', []), key=lambda x: json.dumps(x, sort_keys=True))
        if all(model == models[0] for model in models[1:]) and frozen == recent and len(set(replay_hashes[2:])) == 1:
            warning_order.append(dict(id=ident, replay_hashes=replay_hashes, baseline_order_varies=replay_hashes[0] != replay_hashes[1], candidate_deterministic=True))
        else:
            failures.append(dict(id=ident, reason='full output differs beyond document warning order', replay_hashes=replay_hashes))
    summary = dict(total=len(current), exact=exact, warning_order_only=len(warning_order), passed=exact + len(warning_order), candidate_sha256=candidate_hash, baseline_sha256=baseline_hash, reference_phase=args.reference_phase, phase=args.phase, warning_order_cases=warning_order, failures=failures)
    (run / 'expanded-parity-summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary))
    if failures:
        raise SystemExit(1)


if __name__ == '__main__':
    main()

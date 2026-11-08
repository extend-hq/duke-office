"""Stages the built benchmark runner as a new experiment label.

Copies `scripts/pptx-benchmark/target/release/pptx-benchmark` to
`output/pptx-benchmark-20261106/experiments/<label>/candidate` and writes
`identity.json` with the executable's SHA-256, the SHA-256 of every Rust source
file of the parser crates, the baseline label and a description of the change.
An existing label is never overwritten.
"""
import argparse
import hashlib
import json
import shutil

from feedback import OUT, ROOT


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--baseline', required=True)
    parser.add_argument('--change', required=True)
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    if run.exists():
        raise SystemExit('Preserve the existing experiment directory.')
    built = ROOT / 'scripts/pptx-benchmark/target/release/pptx-benchmark'
    run.mkdir(parents=True)
    shutil.copy2(built, run / 'candidate')
    sources = {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
               for name in ['rust-pptx', 'rust-office-art', 'rust-office-charts'] for path in sorted((ROOT / 'crates' / name).rglob('*.rs'))}
    identity = dict(binary_sha256=hashlib.sha256((run / 'candidate').read_bytes()).hexdigest(), sources=sources, baseline_label=args.baseline, change=args.change)
    (run / 'identity.json').write_text(json.dumps(identity, indent=2) + '\n')
    print(json.dumps({key: value for key, value in identity.items() if key != 'sources'}))


if __name__ == '__main__':
    main()

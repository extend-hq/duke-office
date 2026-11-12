"""Audit shared ODF parsing and artwork availability without treating it as visual fidelity."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('parser', type=Path)
    p.add_argument('corpus', type=Path)
    p.add_argument('output', type=Path)
    p.add_argument('--workers', type=int, default=4)
    args = p.parse_args()
    binary = args.parser.resolve()
    paths = sorted(path for path in args.corpus.rglob('*') if path.is_file())

    def inspect(path):
        result = dict(path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest())
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'audit.json'
            try:
                run = subprocess.run([str(binary), '--audit', str(path), str(target)],
                                     capture_output=True, text=True, timeout=45)
                if run.returncode:
                    result.update(status='failed', error=run.stderr.strip())
                else:
                    result.update(status='parsed', audit=json.loads(target.read_text()))
            except subprocess.TimeoutExpired:
                result.update(status='timeout')
        return result

    with ThreadPoolExecutor(max_workers=args.workers) as pool:
        results = list(pool.map(inspect, paths))
    parsed = [r for r in results if r['status'] == 'parsed']
    charts = [c for r in parsed for c in r['audit']['charts']]
    drawings = [d for r in parsed for d in r['audit']['drawings']]
    custom = [d for d in drawings if d['custom'] is not None]
    summary = dict(inputs=len(results), parsed=len(parsed), failed=len(results)-len(parsed),
                   charts=len(charts), chartsRendered=sum(c['rendered'] for c in charts),
                   drawings=len(drawings), customGeometry=len(custom),
                   customEvaluated=sum(d['custom']['evaluated'] for d in custom),
                   customRendered=sum(d['rendered'] for d in custom),
                   embeddedFailures=sum(len(r['audit']['failures']) for r in parsed))
    result = dict(summary=summary, parserSha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                  note='Availability and structural audit only; no visual score is inferred.', results=results)
    args.output.write_text(json.dumps(result, indent=2))
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()

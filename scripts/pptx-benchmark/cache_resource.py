import argparse
import hashlib
import json
import platform
import random
import re
import statistics
import subprocess
from pathlib import Path

from feedback import OUT


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--selection', type=Path, required=True)
    parser.add_argument('--manifest', type=Path, required=True)
    args = parser.parse_args()
    if platform.system() != 'Darwin':
        raise SystemExit('This resource runner requires the macOS time utility.')
    run = OUT / 'experiments' / args.label
    output = run / 'resource'
    output.mkdir()
    sha = lambda path: hashlib.sha256(Path(path).read_bytes()).hexdigest()
    proof = json.loads((run / 'parity-summary.json').read_text())
    assert proof['equal'] == proof['total']
    assert sha(run / 'parity.jsonl') == proof['records_sha256']
    binaries = {'baseline': Path(proof['baseline_path']), 'candidate': run / 'candidate'}
    for label, binary in binaries.items():
        assert sha(binary) == proof[label + '_sha256']
    manifest = {row['id']: row for row in json.loads(args.manifest.read_text())}
    selection = json.loads(args.selection.read_text())
    rng = random.Random(20261107)
    rows = []
    for selected in selection['selected']:
        row = manifest[selected['id']]
        assert row['split'] == 'tuning' and sha(row['path']) == row['sha256']
        for repeat in range(3):
            order = list(binaries)
            rng.shuffle(order)
            for label in order:
                process = subprocess.run(['/usr/bin/time', '-l', str(binaries[label]),
                                          'duke-pptx', row['path'], '-', '0'],
                                         capture_output=True, text=True, timeout=120)
                stem = output / f"{row['id']}.{repeat}.{label}"
                stem.with_suffix(stem.suffix + '.stderr').write_text(process.stderr)
                stem.with_suffix(stem.suffix + '.stdout').write_text(process.stdout)
                assert process.returncode == 0
                result = json.loads(process.stdout.splitlines()[-1])
                assert result['status'] == 'ok'
                match = re.search(r'^\s*(\d+)\s+maximum resident set size\s*$',
                                  process.stderr, re.MULTILINE)
                assert match
                rows.append(dict(id=row['id'], band=selected['band'], label=label,
                                 repeat=repeat, peak_rss_bytes=int(match.group(1))))
    summary = []
    for selected in selection['selected']:
        values = {label: statistics.median(row['peak_rss_bytes'] for row in rows
                                          if row['id'] == selected['id'] and row['label'] == label)
                  for label in binaries}
        summary.append(dict(id=selected['id'], band=selected['band'],
                            median_peak_rss_bytes=values,
                            ratio=values['candidate'] / values['baseline']))
    receipt = dict(summary=summary, records=rows, selection_sha256=sha(args.selection),
                   manifest_sha256=sha(args.manifest), native_gate_sha256=sha(run / 'parity-summary.json'),
                   binaries={label: sha(binary) for label, binary in binaries.items()},
                   harness_sha256=sha(__file__), platform=platform.platform(),
                   scope='Three cold-process peak-RSS samples per build on the preselected profiling inputs. Includes process and model memory. No output serialization. Resource diagnostics only; this is not a latency measurement or a corpus-wide memory bound.')
    (output / 'summary.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(summary))


if __name__ == '__main__':
    main()

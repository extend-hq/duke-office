import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path

from feedback import OUT

# Sibling checkout of github.com/extend-hq/react-pptx; override with REACT_PPTX.
REACT_PPTX = Path(os.environ.get('REACT_PPTX', Path(__file__).resolve().parents[3] / 'react-pptx'))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    report = OUT / 'report'
    digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
    proof = json.loads((run / 'picture-fallback-probes-summary.json').read_text())
    assert proof['passed'] == proof['total']
    assert proof['candidate_sha256'] == digest(run / 'candidate')
    cases = []
    for row in proof['receipts']:
        model = run / 'probes' / (row['id'] + '.full.json.gz')
        control = run / 'probes' / (row['id'] + '.control.json.gz')
        cases.append(dict(row, browser_primary_expected=row['primary_expected'] and row['case'] != 'declared-encoding',
                          model=json.load(gzip.open(model, 'rt')),
                          control=json.load(gzip.open(control, 'rt')),
                          model_sha256=digest(model), control_model_sha256=digest(control)))
    (report / 'picture-fallback-models.json').write_text(json.dumps(cases))
    script = Path(__file__).with_suffix('.mjs')
    (report / 'picture-fallback-probe.mjs').write_bytes(script.read_bytes())
    (report / 'picture-fallback-probe.html').write_text('''<!doctype html><html><meta charset="utf-8"><title>Picture fallback checks</title><body style="margin:0"><script type="module" src="./picture-fallback-probe.mjs"></script></body></html>''')
    direct = (report / 'image-fallback-probe.html').read_text().replace('./image-fallback-viewer.mjs', './picture-fallback-viewer.mjs')
    (report / 'picture-fallback-direct-probe.html').write_text(direct)
    sources = [REACT_PPTX / 'packages/react-viewer/src/normalized-viewer.ts',
               REACT_PPTX / 'packages/presentation-model/src/index.ts']
    artifacts = [report / name for name in ['picture-fallback-models.json', 'picture-fallback-probe.html',
                 'picture-fallback-probe.mjs', 'picture-fallback-direct-probe.html', 'picture-fallback-viewer.mjs']]
    receipt = dict(candidate_sha256=proof['candidate_sha256'], cases=len(cases),
                   source_sha256={str(p): digest(p) for p in sources},
                   artifact_sha256={str(p): digest(p) for p in artifacts},
                   harness_sha256={str(p): digest(p) for p in [Path(__file__), script]},
                   scope='Browser execution pending. Native models compare generated image pixels and layout styles against the frozen baseline compatibility render, or the native preferred artwork. A declared legacy encoding that the browser cannot decode must use the compatibility picture; the outer SVG alone decodes blank. Runtime-only mutations check bounded decode concurrency, resource rejection and mount lifetime. This is not native PowerPoint visual fidelity.')
    (run / 'browser-identity.json').write_text(json.dumps(receipt, indent=2))
    print('Generated', len(cases), 'native browser cases')


if __name__ == '__main__':
    main()

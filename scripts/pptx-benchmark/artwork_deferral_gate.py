"""Semantic gate for the deferred-artwork parse option.

For every manifest input the candidate runner parses the file twice: eagerly
(`duke-pptx`) and with `deferArtwork` (`duke-pptx-deferred`). After removing
artwork fields (`shapeSvg`, `pictureSvg`, `chartSvg`, `paintBounds`,
`fallbackPicture`, in their compacted `*Bytes` forms) and the warnings that only
rendering can raise, both compact models must be identical, and statuses and
error strings must agree. The `duke-pptx-artwork-check` engine then renders up
to `--check-limit` nodes per input on demand, both through `render_node_artwork`
(one parse per render) and through a retained `ParsedPresentation` handle, and
compares each result with the eagerly rendered artwork; the handle's model must
equal the deferred parse byte for byte.
"""
import argparse
import gzip
import hashlib
import json
from pathlib import Path

from feedback import OUT, invoke

ARTWORK_KEYS = {'shapeSvg', 'shapeSvgBytes', 'pictureSvg', 'pictureSvgBytes', 'chartSvg', 'chartSvgBytes', 'paintBounds', 'fallbackPicture'}
RENDERING_WARNINGS = {'unsupported-shape-appearance', 'unsupported-picture-appearance', 'unsupported-chart-rendering', 'ink-fallback'}


def strip(value):
    if isinstance(value, dict):
        result = {}
        for key, child in value.items():
            if key in ARTWORK_KEYS:
                continue
            if key == 'warnings' and isinstance(child, list):
                child = [w for w in child if not (isinstance(w, dict) and w.get('code') in RENDERING_WARNINGS)]
            result[key] = strip(child)
        return result
    if isinstance(value, list):
        return [strip(child) for child in value]
    return value


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--check-limit', type=int, default=6)
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    binary = run / 'candidate'
    identity = json.loads((run / 'identity.json').read_text())
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == identity['binary_sha256']
    rows = json.loads(args.manifest.read_text())
    assert len(rows) == len({row['id'] for row in rows})
    destination = run / 'artwork-deferral.jsonl'
    if destination.exists():
        raise SystemExit('Preserve existing audit receipts.')
    scratch = run / 'artwork-deferral-scratch'
    scratch.mkdir(exist_ok=True)
    records = []
    with destination.open('w', buffering=1) as stream:
        for index, row in enumerate(rows):
            assert hashlib.sha256(Path(row['path']).read_bytes()).hexdigest() == row['sha256']
            eager_path, deferred_path = scratch / 'eager.json.gz', scratch / 'deferred.json.gz'
            eager = invoke(binary, 'duke-pptx', row, eager_path, 0)
            deferred = invoke(binary, 'duke-pptx-deferred', row, deferred_path, 0)
            record = dict(id=row['id'], status=eager['status'], deferred_status=deferred['status'], error=eager.get('error'), deferred_error=deferred.get('error'))
            equal = eager['status'] == deferred['status'] and eager.get('error') == deferred.get('error')
            if equal and eager['status'] == 'ok':
                models = [json.load(gzip.open(path, 'rt')) for path in [eager_path, deferred_path]]
                equal = strip(models[0]) == strip(models[1])
                record['deferred_has_artwork'] = json.dumps(models[1]).find('"shapeSvgBytes"') >= 0 or json.dumps(models[1]).find('"pictureSvgBytes"') >= 0 or json.dumps(models[1]).find('"chartSvgBytes"') >= 0
                check = invoke(binary, 'duke-pptx-artwork-check', row, '-', args.check_limit)
                record.update(checked=check.get('checked', 0), matched=check.get('matched', 0), reparse_matched=check.get('reparse_matched', 0), retained_matched=check.get('retained_matched', 0),
                              retained_document_equal=check.get('retained_document_equal', False), check_status=check['status'], mismatches=check.get('mismatches', []))
                equal = (equal and check['status'] == 'ok' and record['checked'] == record['matched'] == record['reparse_matched'] == record['retained_matched']
                         and record['retained_document_equal'] is True and not record['deferred_has_artwork'])
            record['passed'] = equal
            records.append(record)
            stream.write(json.dumps(record) + '\n')
            if not equal:
                print('MISMATCH', row['id'], flush=True)
            if index % 25 == 0:
                print(index, len(rows), flush=True)
    for path in scratch.glob('*'):
        path.unlink()
    scratch.rmdir()
    summary = dict(total=len(records), passed=sum(r['passed'] for r in records), checked=sum(r.get('checked', 0) for r in records), matched=sum(r.get('matched', 0) for r in records),
                   reparse_matched=sum(r.get('reparse_matched', 0) for r in records), retained_matched=sum(r.get('retained_matched', 0) for r in records),
                   retained_documents_equal=sum(r.get('retained_document_equal') is True for r in records), successful_inputs=sum(r['status'] == 'ok' for r in records),
                   candidate_sha256=identity['binary_sha256'], manifest_path=str(args.manifest), manifest_sha256=hashlib.sha256(args.manifest.read_bytes()).hexdigest(), records_sha256=hashlib.sha256(destination.read_bytes()).hexdigest(), check_limit=args.check_limit, harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), failures=[r['id'] for r in records if not r['passed']],
                   scope='Deferred-artwork output equals eager output after removing artwork fields and rendering-only warnings; on-demand node renders equal eager artwork for the checked nodes, both through one parse per render and through the retained handle, whose model equals the deferred parse. This does not measure rendered fidelity.')
    (run / 'artwork-deferral-summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary))
    if summary['failures']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()

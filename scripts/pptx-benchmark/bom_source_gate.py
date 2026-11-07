import argparse
import hashlib
import json
from pathlib import Path
import zipfile

from artwork_math_gate import artwork_hashes
from feedback import OUT, invoke
from object_gate import invoke as full_model


def normalized_archive(path, destination):
    changed = []
    with zipfile.ZipFile(path) as original, zipfile.ZipFile(destination, 'w') as normalized:
        normalized.comment = original.comment
        for info in original.infolist():
            data = original.read(info)
            if info.filename.endswith('.xml') and data.startswith(b'\xef\xbb\xbf'):
                changed.append(dict(part=info.filename, before_sha256=hashlib.sha256(data).hexdigest(), after_sha256=hashlib.sha256(data[3:]).hexdigest()))
                data = data[3:]
            normalized.writestr(info, data)
    return changed


def compare(a, b, path='', prefixes=None):
    if a == b:
        return []
    if isinstance(a, str) and isinstance(b, str) and path.rsplit('/', 1)[-1] in {'chartXml', 'sourceXml', 'source_xml'} and a.removeprefix('\ufeff') == b.removeprefix('\ufeff'):
        prefixes.append(path)
        return []
    if type(a) != type(b):
        return [path + '/type']
    if isinstance(a, dict):
        return [issue for key in a.keys() | b.keys() for issue in ([path + '/' + key + '/key'] if key not in a or key not in b else compare(a[key], b[key], path + '/' + key, prefixes))]
    if isinstance(a, list):
        if len(a) != len(b):
            return [path + '/length']
        return [issue for i, (x, y) in enumerate(zip(a, b)) for issue in compare(x, y, path + '/' + str(i), prefixes)]
    return [path]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--baseline', default='xml-scan2')
    parser.add_argument('--manifest', type=Path, required=True)
    args = parser.parse_args()
    run, before = [OUT / 'experiments' / label for label in [args.label, args.baseline]]
    candidate, baseline = run / 'candidate', before / 'candidate'
    identity = json.loads((run / 'identity.json').read_text())
    assert hashlib.sha256(candidate.read_bytes()).hexdigest() == identity['binary_sha256']
    proof = json.loads((before / 'parity-summary.json').read_text())
    assert proof['equal'] == proof['total']
    assert hashlib.sha256(baseline.read_bytes()).hexdigest() == proof['candidate_sha256']
    assert hashlib.sha256((before / 'parity.jsonl').read_bytes()).hexdigest() == proof['records_sha256']
    previous = {r['id']: r for r in map(json.loads, (before / 'parity.jsonl').read_text().splitlines())}
    rows = json.loads(args.manifest.read_text())
    assert len(rows) == len({row['id'] for row in rows})
    destination = run / 'bom-source.jsonl'
    if destination.exists():
        raise SystemExit('Preserve previous receipts.')
    fields = lambda row: (row['status'], row.get('error'), row.get('full_sha256'))
    records = []
    with destination.open('w', buffering=1) as stream:
        for index, row in enumerate(rows):
            assert hashlib.sha256(Path(row['path']).read_bytes()).hexdigest() == row['sha256']
            current = invoke(candidate, 'rust-pptx', row, run / (row['id'] + '.compact.json.gz'), 0)
            old = previous.get(row['id'])
            if old is None:
                old = invoke(baseline, 'rust-pptx', row, run / 'baseline.compact.json.gz', 0)
            exact = fields(current) == fields(old)
            hashes = old.get('artwork_hashes', []) if exact else []
            issues, parts, prefixes = [], [], []
            if not exact:
                normalized = run / 'normalized.pptx'
                parts = normalized_archive(row['path'], normalized)
                assert parts, 'Changed output must have a source XML BOM.'
                reference_row = dict(row, path=str(normalized))
                expected, model = full_model(baseline, reference_row, run / 'reference.full.json.gz')
                replay, actual = full_model(candidate, row, run / 'candidate.full.json.gz')
                assert fields(replay) == fields(current)
                if expected['status'] != current['status']:
                    issues = ['normalized-baseline-status']
                elif current['status'] == 'ok':
                    issues = compare(model, actual, prefixes=prefixes)
                    hashes = artwork_hashes(actual)
                elif expected.get('error') != current.get('error'):
                    issues = ['normalized-baseline-error']
                del model, actual
                normalized.unlink()
            elif current['status'] == 'ok' and not hashes:
                replay, actual = full_model(candidate, row, run / 'candidate.full.json.gz')
                assert fields(replay) == fields(current)
                hashes = artwork_hashes(actual)
                del actual
            record = dict(id=row['id'], **current, exact=exact, passed=not issues, baseline_status=old['status'], recovered=old['status'] != 'ok' and current['status'] == 'ok', normalized_parts=parts, retained_source_bom_prefixes=prefixes, artwork_hashes=hashes, issues=issues)
            records.append(record)
            stream.write(json.dumps(record) + '\n')
            for filename in ['reference.full.json.gz', 'candidate.full.json.gz', 'baseline.compact.json.gz']:
                path = run / filename
                if issues and path.exists():
                    path.rename(run / (row['id'] + '.' + filename))
                else:
                    path.unlink(missing_ok=True)
            if issues:
                print('MISMATCH', row['id'], issues[:8], flush=True)
            if index % 25 == 0:
                print(index, len(rows), flush=True)
    summary = dict(total=len(records), passed=sum(r['passed'] for r in records), exact=sum(r['exact'] for r in records), recovered=sum(r['recovered'] for r in records), changed_existing_models=sum(not r['exact'] and r['baseline_status'] == r['status'] == 'ok' for r in records), normalized_parts=sum(len(r['normalized_parts']) for r in records), retained_source_bom_prefixes=sum(len(r['retained_source_bom_prefixes']) for r in records), failures=[r['id'] for r in records if not r['passed']], candidate_sha256=identity['binary_sha256'], baseline_sha256=proof['candidate_sha256'], baseline_label=args.baseline, manifest_sha256=hashlib.sha256(args.manifest.read_bytes()).hexdigest(), records_sha256=hashlib.sha256(destination.read_bytes()).hexdigest(), scope='Exact outputs/errors, or candidate original input versus frozen baseline on a copy with only leading UTF-8 BOM bytes removed from XML members. Every other model field and all binary payloads must match; raw source XML strings may retain their original leading BOM. Reserved validation inputs are excluded.')
    (run / 'bom-source-summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary))
    if summary['failures']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()

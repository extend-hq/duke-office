import argparse
import gzip
import hashlib
import json
from pathlib import Path
import zipfile

from bom_source_gate import compare
from feedback import OUT
from object_gate import changes


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def at_path(value, path):
    for key in path.split('/')[1:]:
        value = value[int(key)] if isinstance(value, list) else value[key]
    return value


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--manifest', type=Path, required=True)
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    summary_path = run / 'bom-source-summary.json'
    summary = json.loads(summary_path.read_text())
    assert digest(run / 'candidate') == summary['candidate_sha256']
    assert digest(args.manifest) == summary['manifest_sha256']
    assert digest(run / 'bom-source.jsonl') == summary['records_sha256']
    rows = {r['id']: r for r in json.loads(args.manifest.read_text())}
    records = [json.loads(line) for line in (run / 'bom-source.jsonl').read_text().splitlines()]
    assert len(records) == summary['total'] == len(rows)
    assert {r['id'] for r in records} == rows.keys()
    reviewed = []
    for record in records:
        if record['passed']:
            continue
        row = rows[record['id']]
        assert digest(Path(row['path'])) == row['sha256']
        reference_path = run / (row['id'] + '.reference.full.json.gz')
        candidate_path = run / (row['id'] + '.candidate.full.json.gz')
        reference = json.load(gzip.open(reference_path))
        candidate = json.load(gzip.open(candidate_path))
        prefixes = []
        assert set(compare(reference, candidate, prefixes=prefixes)) == set(record['issues'])
        source = {}
        with zipfile.ZipFile(row['path']) as archive:
            for part in record['normalized_parts']:
                data = archive.read(part['part'])
                assert data.startswith(b'\xef\xbb\xbf')
                assert hashlib.sha256(data).hexdigest() == part['before_sha256']
                assert hashlib.sha256(data[3:]).hexdigest() == part['after_sha256']
                source[data.decode('utf-8')] = part['part']
        fields = []
        for path in changes(reference, candidate):
            before, after = at_path(reference, path), at_path(candidate, path)
            assert path.rsplit('/', 1)[-1] in {'chartXml', 'chartStyleXml', 'chartColorsXml', 'sourceXml', 'source_xml'}
            assert isinstance(before, str) and isinstance(after, str)
            assert after.startswith('\ufeff') and before == after[1:]
            assert after in source
            fields.append(dict(path=path, source_part=source[after], retained_source_sha256=hashlib.sha256(after.encode()).hexdigest()))
        reviewed.append(dict(id=row['id'], raw_issues=record['issues'], fields=fields,
                             reference_artifact_sha256=digest(reference_path),
                             candidate_artifact_sha256=digest(candidate_path)))
    assert {r['id'] for r in reviewed} == set(summary['failures'])
    result = dict(**{k: v for k, v in summary.items() if k not in {'passed', 'failures', 'scope'}},
                  passed=summary['passed'] + len(reviewed), failures=[], raw_passed=summary['passed'],
                  raw_summary_sha256=digest(summary_path), reviews=reviewed,
                  scope=summary['scope'] + ' Separate review verifies remaining chart-style/color XML differences byte-for-byte against original source members; raw audit failures are preserved.')
    destination = run / 'bom-source-reviewed-summary.json'
    assert not destination.exists(), 'Preserve previous review receipts.'
    destination.write_text(json.dumps(result, indent=2))
    print(json.dumps({k: result[k] for k in ['total', 'passed', 'raw_passed', 'recovered', 'failures']}))


if __name__ == '__main__':
    main()

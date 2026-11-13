import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re

import olefile

from artwork_math_gate import artwork_hashes
from feedback import OUT, ROOT, invoke
from object_gate import invoke as full_model


def fields(record):
    return record['status'], record.get('full_sha256'), record.get('error')


def without_added_notes(document):
    result = dict(document)
    result['slides'] = [dict(slide) for slide in document['slides']]
    for slide in result['slides']:
        slide.pop('notes', None)
        slide['warnings'] = [w for w in slide['warnings'] if w.get('code') != 'unresolved-notes']
    result['warnings'] = [w for w in result['warnings'] if w.get('code') != 'unresolved-notes']
    return result


def notes_consistent(document):
    for slide in document['slides']:
        for note in slide.get('notes', []):
            actual = [''.join(run['text'] for run in paragraph['runs']) for paragraph in note['paragraphs']]
            if actual != re.split('[\r\n\x0b]', note['text']):
                return False
    return True


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--baseline', default='portable-math2')
    parser.add_argument('--resume', action='store_true')
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    baseline_run = OUT / 'experiments' / args.baseline
    binary, baseline = run / 'candidate', baseline_run / 'candidate'
    identity = json.loads((run / 'identity.json').read_text())
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == identity['binary_sha256']
    proof = json.loads((baseline_run / 'artwork-reviewed-summary.json').read_text())
    assert proof['passed'] == proof['total']
    assert hashlib.sha256(baseline.read_bytes()).hexdigest() == proof['candidate_sha256']
    old_path = baseline_run / 'artwork-math.jsonl'
    assert hashlib.sha256(old_path.read_bytes()).hexdigest() == proof['records_sha256']
    previous = {r['id']: r for r in map(json.loads, old_path.read_text().splitlines())}
    manifest_paths = [OUT / 'manifest.json', OUT / 'expanded-optimized/cohort.json', OUT / 'corpus-followup/manifest.json', OUT / 'poi-runtime/legacy-tuning-cohort.json']
    rows = {}
    for path in manifest_paths:
        for row in json.loads(path.read_text()):
            if path.name == 'manifest.json' and path.parent.name == 'corpus-followup' and row['split'] != 'tuning':
                continue
            if row['id'] in rows:
                assert row['sha256'] == rows[row['id']]['sha256']
            rows[row['id']] = row
    destination = run / 'legacy-features.jsonl'
    if destination.exists() and not args.resume:
        raise SystemExit('Preserve existing audit receipts.')
    if args.resume and (run / 'legacy-features-summary.json').exists():
        raise SystemExit('Do not resume a completed audit.')
    records = [json.loads(line) for line in destination.read_text().splitlines()] if args.resume else []
    assert [r['id'] for r in records] == list(rows)[:len(records)]
    for row in rows.values():
        assert hashlib.sha256(Path(row['path']).read_bytes()).hexdigest() == row['sha256']
    completed = len(records)
    with destination.open('a' if args.resume else 'w', buffering=1) as stream:
        for index, row in enumerate(rows.values()):
            if index < completed:
                continue
            assert hashlib.sha256(Path(row['path']).read_bytes()).hexdigest() == row['sha256']
            current = invoke(binary, 'duke-pptx', row, run / (row['id'] + '.compact.json.gz'), 0)
            old = previous.get(row['id'])
            if old is None:
                baseline_output = run / (row['id'] + '.baseline-compact.json.gz')
                old = invoke(baseline, 'duke-pptx', row, baseline_output, 0)
            exact = fields(current) == fields(old)
            hashes = old.get('artwork_hashes', []) if exact else []
            issues, changes = [], {}
            if not exact:
                if current['status'] != 'ok':
                    issues.append('acceptance or error changed')
                else:
                    after_path = run / (row['id'] + '.after.json.gz')
                    replay, after = full_model(binary, row, after_path)
                    assert fields(replay) == fields(current)
                    hashes = artwork_hashes(after)
                    changes['note_count'] = sum(len(s.get('notes', [])) for s in after['slides'])
                    changes['unresolved_notes'] = sum(w.get('code') == 'unresolved-notes' for w in after['warnings'])
                    if after['format'] != 'ppt' or not notes_consistent(after):
                        issues.append('unexpected format or inconsistent notes')
                    if old['status'] == 'ok':
                        before_path = run / (row['id'] + '.before.json.gz')
                        replay, before = full_model(baseline, row, before_path)
                        assert fields(replay) == fields(old)
                        if any(s.get('notes') for s in before['slides']):
                            issues.append('baseline already contained notes')
                        if before != without_added_notes(after):
                            issues.append('non-note model content changed')
                        if not issues:
                            before_path.unlink()
                    else:
                        with olefile.OleFileIO(row['path']) as ole:
                            names = {'/'.join(parts).casefold() for parts in ole.listdir()}
                        prefix = 'pp97_dualstorage/'
                        expected = {prefix + name for name in ['current user', 'powerpoint document']}
                        poi_path = OUT / 'poi-legacy-tuning' / (row['id'] + '.json')
                        if not expected <= names or not poi_path.exists():
                            issues.append('new acceptance has no dual-storage and independent reference')
                        else:
                            reference = json.loads(poi_path.read_text())
                            if len(after['slides']) != len(reference['slides']) or any(not s.get('sourcePart', '').casefold().startswith('/' + prefix) for s in after['slides']):
                                issues.append('dual-storage slide count or source identity differs')
                            changes['new_dual_storage_acceptance'] = True
                            changes['slides'] = len(after['slides'])
                    if not issues:
                        after_path.unlink()
            record = dict(id=row['id'], **current, exact=exact, passed=not issues, issues=issues, changes=changes, artwork_hashes=hashes)
            records.append(record)
            stream.write(json.dumps(record) + '\n')
            if issues:
                print('MISMATCH', row['id'], issues, flush=True)
            if index % 25 == 0:
                print(index, len(rows), flush=True)
    summary = dict(total=len(records), passed=sum(r['passed'] for r in records), exact=sum(r['exact'] for r in records), notes_added=sum(r['changes'].get('note_count', 0) for r in records), unresolved_notes=sum(r['changes'].get('unresolved_notes', 0) for r in records), dual_storage=[dict(id=r['id'], slides=r['changes']['slides']) for r in records if r['changes'].get('new_dual_storage_acceptance')], failures=[r['id'] for r in records if r['issues']], candidate_sha256=identity['binary_sha256'], baseline_sha256=proof['candidate_sha256'], records_sha256=hashlib.sha256(destination.read_bytes()).hexdigest(), manifest_sha256={str(p.relative_to(OUT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in manifest_paths}, scope='Regression scope: exact outputs/errors except legacy speaker notes, explicit unresolved-note diagnostics, and independently recognized dual-storage acceptance. Text accuracy is evaluated separately against POI; this does not establish rendering fidelity.')
    (run / 'legacy-features-summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary))
    if summary['failures']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()

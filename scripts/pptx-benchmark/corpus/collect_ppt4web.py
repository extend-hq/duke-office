import collections
import hashlib
import json
from pathlib import Path
import urllib.parse
import urllib.request
import zipfile

import olefile


OUT = Path(__file__).resolve().parents[3] / 'output/pptx-benchmark-20261106'
DESTINATION = OUT / 'corpus-followup/ppt4web'


def classify(path):
    with path.open('rb') as stream:
        prefix = stream.read(128)
    if prefix.startswith(bytes.fromhex('d0cf11e0a1b11ae1')):
        try:
            with olefile.OleFileIO(path) as archive:
                names = ['/'.join(parts) for parts in archive.listdir()]
            return dict(container='ole', kind='legacy-ppt' if 'PowerPoint Document' in names else 'other-ole', streams=names)
        except Exception as error:
            return dict(container='ole', kind='container-error', error=str(error))
    if prefix.startswith(b'PK'):
        try:
            with zipfile.ZipFile(path) as archive:
                names = archive.namelist()
            return dict(container='pptx' if 'ppt/presentation.xml' in names else 'zip',
                        kind='ooxml-presentation' if 'ppt/presentation.xml' in names else 'other-zip',
                        members=len(names))
        except Exception as error:
            return dict(container='zip', kind='container-error', error=str(error))
    return dict(container='other', kind='unclassified', header_hex=prefix[:16].hex())


def main():
    selection_path = DESTINATION / 'selection.json'
    plan = json.loads(selection_path.read_text())
    destination = DESTINATION / 'originals'
    destination.mkdir(exist_ok=True)
    receipt_path = DESTINATION / 'download-records.jsonl'
    if receipt_path.exists():
        raise SystemExit('Preserve existing download receipts.')
    manifests = ['manifest.json', 'expanded/manifest.json', 'corpus-followup/manifest.json',
                 'corpus-preservation/manifest.json', 'corpus-python-pptx/manifest.json',
                 'corpus-aspose/manifest.json', 'corpus-followup/embrace-ole-supplement/manifest.json',
                 'corpus-followup/keynote-supplement/manifest.json', 'corpus-followup/openxml-sdk/manifest.json']
    known, identities = {}, {}
    for name in manifests:
        path = OUT / name
        identities[name] = hashlib.sha256(path.read_bytes()).hexdigest()
        for row in json.loads(path.read_text()):
            known.setdefault(row['sha256'], row)
    previous_count = len(known)
    previous_reserved = sum(row.get('split') == 'validation' for row in known.values())
    rows, attempts, duplicates = [], [], []
    received = 0
    with receipt_path.open('w', buffering=1) as stream:
        for selected in plan['selected']:
            url = selected['download_url']
            parsed = urllib.parse.urlparse(url)
            assert parsed.scheme in ['https', 'http']
            assert parsed.hostname == 'ppt4web.ru' or parsed.hostname.endswith('.ppt4web.ru')
            record = dict(group=selected['group'], url=url, split=selected['split'])
            try:
                if received >= plan['maximum_total_bytes']:
                    raise ValueError('The total download budget is exhausted.')
                limit = min(plan['maximum_file_bytes'], plan['maximum_total_bytes'] - received)
                with urllib.request.urlopen(url, timeout=12) as response:
                    record['http_status'] = response.status
                    declared = int(response.headers.get('Content-Length', '0'))
                    if declared > limit:
                        raise ValueError('The declared file size exceeds the download budget.')
                    content = response.read(limit + 1)
                received += len(content)
                if len(content) > limit:
                    raise ValueError('The response exceeds the download budget.')
                digest = hashlib.sha256(content).hexdigest()
                path = destination / (selected['group'] + '.ppt')
                path.write_bytes(content)
                record.update(status='downloaded', bytes=len(content), sha256=digest,
                              path=str(path), classification=classify(path))
                if digest in known:
                    duplicates.append(dict(record, previous_path=known[digest]['path']))
                else:
                    row = dict(id=digest[:16], sha256=digest, path=str(path), bytes=len(content),
                               format=Path(parsed.path).suffix.lstrip('.').lower(),
                               source='ppt4web original', source_url=url,
                               source_page=selected['url'], source_metadata_revision=plan['revision'],
                               group=selected['group'], split=selected['split'],
                               **record['classification'])
                    known[digest] = row
                    rows.append(row)
            except Exception as error:
                record.update(status='download_error', error=str(error))
            attempts.append(record)
            stream.write(json.dumps(record) + '\n')
            print(len(attempts), len(plan['selected']), record['status'], flush=True)
    (DESTINATION / 'manifest.json').write_text(json.dumps(rows, indent=2) + '\n')
    (DESTINATION / 'duplicates.json').write_text(json.dumps(duplicates, indent=2) + '\n')
    summary = dict(attempted=len(attempts), downloaded=sum(r['status'] == 'downloaded' for r in attempts),
                   errors=sum(r['status'] == 'download_error' for r in attempts),
                   new_unique=len(rows), duplicates=len(duplicates), previous_unique=previous_count,
                   combined_unique=len(known), previous_reserved=previous_reserved,
                   combined_reserved=previous_reserved + sum(r['split'] == 'validation' for r in rows),
                   received_bytes=received, formats=dict(collections.Counter(r['format'] for r in rows)),
                   kinds=dict(collections.Counter(r['kind'] for r in rows)),
                   splits=dict(collections.Counter(r['split'] for r in rows)),
                   metadata_revision=plan['revision'], selection_sha256=hashlib.sha256(selection_path.read_bytes()).hexdigest(),
                   records_sha256=hashlib.sha256(receipt_path.read_bytes()).hexdigest(),
                   prior_manifest_sha256=identities, harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                   scope='Original download URLs from pinned metadata. Header, ZIP-directory and CFB-directory classification only; no presentation parser or reference extractor was run. The source provides no independent original-byte checksum. Converted copies remain in bulk archives and were not downloaded.')
    (DESTINATION / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps({key: value for key, value in summary.items() if key != 'prior_manifest_sha256'}))


if __name__ == '__main__':
    main()

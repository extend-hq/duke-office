import collections
import hashlib
import json
from pathlib import Path
import zipfile

import olefile

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'output/pptx-benchmark-20261106'


def classify(row):
    path = Path(row['path'])
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != row['sha256']:
        raise ValueError('Input checksum changed')
    record = {key: row[key] for key in ['id', 'sha256', 'bytes', 'source', 'split']}
    record['claimed_format'] = row['format']
    try:
        if data.startswith(b'\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1'):
            record['container'] = 'ole'
            with olefile.OleFileIO(path) as archive:
                names = ['/'.join(name) for name in archive.listdir()]
                if 'EncryptedPackage' in names:
                    kind = 'encrypted-ooxml'
                elif 'PowerPoint Document' in names:
                    kind = 'legacy-ppt'
                elif 'PP40' in names:
                    kind = 'legacy-ppt4'
                else:
                    kind = 'other-ole'
                record.update(container='ole', kind=kind, streams=names)
        elif data.startswith(b'PK'):
            with zipfile.ZipFile(path) as archive:
                names = archive.namelist()
                kind = 'ooxml-presentation' if 'ppt/presentation.xml' in names else 'other-zip'
                record.update(container='zip', kind=kind, members=len(names))
        else:
            prefix = data[:128].lstrip().lower()
            kind = 'empty' if not data else 'pdf' if data.startswith(b'%PDF') else 'html' if prefix.startswith((b'<!doctype html', b'<html')) else 'other'
            record.update(container='other', kind=kind, magic_hex=data[:16].hex())
    except Exception as error:
        record.update(kind='container-error', error=str(error))
    return record


def main():
    records = []
    known = {r['sha256'] for name in ['manifest.json', 'expanded/manifest.json'] for r in json.loads((OUT / name).read_text())}
    for source in ['loc', 'embrace']:
        rows = json.loads((OUT / 'corpus-followup' / source / 'manifest.json').read_text())
        records.extend(classify(row) for row in rows)
    if len({r['sha256'] for r in records}) != len(records) or any(r['sha256'] in known for r in records):
        raise ValueError('Followup corpus is not distinct from existing inputs')
    directory = OUT / 'corpus-followup'
    (directory / 'classification.json').write_text(json.dumps(records, indent=2))
    summary = dict(total=len(records), unique_new=len(records), bytes=sum(r['bytes'] for r in records), kinds=dict(collections.Counter(r['kind'] for r in records)), sources=dict(collections.Counter(r['source'] for r in records)), splits={split: dict(collections.Counter(r['kind'] for r in records if r['split'] == split)) for split in ['tuning', 'validation']}, scope='Container metadata only; no presentation parser or semantic oracle was run on these inputs.', olefile_version=olefile.__version__)
    (directory / 'classification-summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary))


if __name__ == '__main__':
    main()

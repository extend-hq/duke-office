import collections
import hashlib
import io
import json
from pathlib import Path
import zipfile

from expand import OUT, ROOT


def classify(data):
    if data.startswith(bytes.fromhex('d0cf11e0a1b11ae1')):
        return 'ole'
    if data.startswith(b'PK'):
        try:
            with zipfile.ZipFile(io.BytesIO(data)) as package:
                names = set(package.namelist())
                if 'ppt/presentation.xml' in names:
                    return 'pptx'
                if 'ppt/slides/slide1.xml' in names and '[Content_Types].xml' in names:
                    return 'ooxml-slide'
                if 'mimetype' in names:
                    mime = package.read('mimetype')
                    if b'opendocument.presentation' in mime:
                        return 'odp'
                if 'content.xml' in names:
                    return 'xml-presentation'
                return 'other-zip'
        except Exception:
            return 'invalid-zip'
    if data.lstrip().startswith(b'<?xml'):
        return 'flat-xml'
    return 'other'


def main():
    original = json.loads((OUT.parent / 'manifest.json').read_text())
    seen = {r['sha256'] for r in original}
    rows = []
    for path in OUT.glob('*-downloads.jsonl'):
        rows.extend(r for r in map(json.loads, path.read_text().splitlines()) if r['status'] == 'downloaded')
    poi = json.loads((OUT / 'poi/manifest.json').read_text())
    for row in poi['files']:
        data = Path(row['local_path']).read_bytes()
        rows.append(dict(path=row['local_path'], url=row['download_url'], repo=poi['repo'], commit=poi['ref'],
                         source='github', source_path=row['source_path'], sha256=hashlib.sha256(data).hexdigest(),
                         format=Path(row['source_path']).suffix.lower()[1:], bytes=len(data)))
    unique = {}
    for row in rows:
        sha = row['sha256']
        record = unique.setdefault(sha, dict(row, id=sha[:16], sources=[], seen_in_original=sha in seen))
        record['sources'].append({k: row[k] for k in ['url', 'repo', 'commit', 'source_path', 'source', 'name', 'collection', 'format'] if k in row})
    for sha, row in unique.items():
        data = Path(row['path']).read_bytes()
        if hashlib.sha256(data).hexdigest() != sha:
            raise ValueError('input identity changed')
        row['container'] = classify(data)
        row['split'] = 'validation' if int(hashlib.sha256(('ppt-expanded-20261107:' + sha).encode()).hexdigest()[:8], 16) % 5 == 0 else 'tuning'
    additions = sorted((r for r in unique.values() if not r['seen_in_original']), key=lambda r: r['sha256'])
    target = OUT / 'manifest.json'
    payload = json.dumps(additions, indent=2)
    if target.exists() and target.read_text() != payload:
        raise ValueError('frozen expansion changed; choose a new corpus revision')
    target.write_text(payload)
    summary = dict(downloaded=len(rows), unique_downloaded=len(unique), duplicates_of_original=sum(r['seen_in_original'] for r in unique.values()),
                   added=len(additions), combined=len(original) + len(additions), bytes=sum(r['bytes'] for r in additions),
                   by_format=dict(collections.Counter(r['format'] for r in additions)),
                   by_container=dict(collections.Counter(r['container'] for r in additions)),
                   by_split=dict(collections.Counter(r['split'] for r in additions)), manifest_sha256=hashlib.sha256(payload.encode()).hexdigest())
    (OUT / 'summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()

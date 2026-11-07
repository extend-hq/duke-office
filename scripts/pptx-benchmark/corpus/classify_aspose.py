import collections
import hashlib
import json
from pathlib import Path
import zipfile


OUT = Path(__file__).resolve().parents[3] / 'output/pptx-benchmark-20261106'
ROOT = OUT / 'corpus-aspose'


def main():
    metadata = json.loads((ROOT / 'source-metadata.json').read_text())
    expected = {entry['path']: entry for entry in metadata['files']}
    known = {row['sha256'] for name in ['manifest.json', 'expanded/manifest.json', 'corpus-followup/manifest.json', 'corpus-preservation/manifest.json', 'corpus-python-pptx/manifest.json'] for row in json.loads((OUT / name).read_text())}
    grouped, duplicates = {}, []
    for folder in ['examples', 'plugins']:
        manifest = json.loads((ROOT / folder / 'manifest.json').read_text())
        assert manifest['ref'] == metadata['commit']
        for entry in manifest['files']:
            path = Path(entry['local_path'])
            data = path.read_bytes()
            source = expected[entry['source_path']]
            assert len(data) == source['size']
            assert hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest() == source['sha']
            digest = hashlib.sha256(data).hexdigest()
            if digest in known:
                duplicates.append(dict(sha256=digest, source_path=entry['source_path']))
                continue
            known.add(digest)
            extension = Path(entry['source_path']).suffix[1:].lower()
            container = 'unrecognized'
            if data.startswith(bytes.fromhex('d0cf11e0a1b11ae1')):
                container = 'ole'
            elif zipfile.is_zipfile(path):
                with zipfile.ZipFile(path) as package:
                    names = set(package.namelist())
                    container = 'pptx' if 'ppt/presentation.xml' in names else 'odp' if extension == 'odp' and 'content.xml' in names else 'zip'
            elif extension == 'fodp' and data.lstrip().startswith(b'<?xml'):
                container = 'fodp'
            group = str(Path(entry['source_path']).parent)
            row = dict(id=digest[:16], sha256=digest, path=str(path), bytes=len(data), format=extension, container=container, group=group, source_commit=metadata['commit'], source_blob_sha1=source['sha'], source_path=entry['source_path'], source_url=entry['download_url'], header_hex=data[:16].hex())
            grouped.setdefault(group, []).append(row)
    ordered = sorted(grouped, key=lambda group: hashlib.sha256(('20261107:' + group).encode()).hexdigest())
    target = (sum(map(len, grouped.values())) + 4) // 5
    reserved, count = set(), 0
    for group in ordered:
        if count >= target:
            break
        reserved.add(group)
        count += len(grouped[group])
    rows = [dict(row, split='validation' if group in reserved else 'tuning') for group, members in grouped.items() for row in members]
    summary = dict(downloaded=len(expected), new_unique=len(rows), duplicates=len(duplicates), total_bytes=sum(row['bytes'] for row in rows), splits=dict(collections.Counter(row['split'] for row in rows)), formats=dict(collections.Counter(row['format'] for row in rows)), containers=dict(collections.Counter(row['container'] for row in rows)), groups=len(grouped), source_commit=metadata['commit'], split_policy='Source feature directories remain together. Seeded SHA-256 group ordering reserves at least one fifth of unique inputs before semantic inspection. Only headers and ZIP directory names are inspected before the split.', scope='Curated example inputs; not an independent production-distribution sample. Container identification does not establish acceptance or semantic accuracy.')
    (ROOT / 'manifest.json').write_text(json.dumps(rows, indent=2))
    (ROOT / 'duplicates.json').write_text(json.dumps(duplicates, indent=2))
    (ROOT / 'summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary))


if __name__ == '__main__':
    main()

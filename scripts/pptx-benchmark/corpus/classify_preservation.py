import collections
import hashlib
import json
from pathlib import Path
import zipfile


OUT = Path(__file__).resolve().parents[3] / 'output/pptx-benchmark-20261106'
ROOT = OUT / 'corpus-preservation'


def main():
    metadata = json.loads((ROOT / 'source-metadata.json').read_text())
    known = {r['sha256'] for name in ['manifest.json', 'expanded/manifest.json', 'corpus-followup/manifest.json'] for r in json.loads((OUT / name).read_text())}
    grouped = {}
    for folder in ['mac', 'odf13']:
        manifest = json.loads((ROOT / folder / 'manifest.json').read_text())
        assert manifest['ref'] == metadata['commit']
        expected = {entry['path']: entry for entry in metadata[folder]['tree'] if entry['type'] == 'blob'}
        for file in manifest['files']:
            path = Path(file['local_path'])
            data = path.read_bytes()
            source = expected[Path(file['source_path']).name]
            assert len(data) == source['size']
            assert hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest() == source['sha']
            digest = hashlib.sha256(data).hexdigest()
            if digest in known:
                continue
            known.add(digest)
            group = Path(file['source_path']).name.split(' ')[0].split('+')[0].split('-')[0] if folder == 'mac' else 'odf13'
            container = 'ole' if data.startswith(bytes.fromhex('d0cf11e0a1b11ae1')) else 'unrecognized'
            if zipfile.is_zipfile(path):
                with zipfile.ZipFile(path) as package:
                    container = 'odp' if package.read('mimetype').decode() == 'application/vnd.oasis.opendocument.presentation' else 'zip'
            generation = 'PowerPoint for Mac 2001 migration' if folder == 'mac' and file['source_path'].endswith('2001.ppt') else 'PowerPoint 4 for Mac' if folder == 'mac' else 'ODF 1.3 presentation'
            row = dict(id=digest[:16], sha256=digest, path=str(path), bytes=len(data), format=path.suffix[1:], container=container, source_generation=generation, source_commit=metadata['commit'], source_blob_sha1=source['sha'], source_path=file['source_path'], source_url=file['download_url'], license='CC0', group=group, header_hex=data[:16].hex())
            grouped.setdefault(group, []).append(row)
    pairs = sorted([name for name in grouped if name != 'odf13'], key=lambda name: hashlib.sha256(('20261107:' + name).encode()).hexdigest())
    reserved = set(pairs[:1])
    rows = [dict(row, split='validation' if group in reserved else 'tuning') for group, group_rows in grouped.items() for row in group_rows]
    summary = dict(new_unique=len(rows), total_bytes=sum(r['bytes'] for r in rows), splits=dict(collections.Counter(r['split'] for r in rows)), containers=dict(collections.Counter(r['container'] for r in rows)), generations=dict(collections.Counter(r['source_generation'] for r in rows)), source_commit=metadata['commit'], split_policy='Source/migrated pairs remain together. One of two Mac source groups is reserved using a seeded SHA-256 group ordering before parser evaluation; the standalone ODF input is tuning.', scope='Container metadata and source-provided format identification only. No semantic or visual accuracy claim.')
    (ROOT / 'manifest.json').write_text(json.dumps(rows, indent=2))
    (ROOT / 'summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary))


if __name__ == '__main__':
    main()

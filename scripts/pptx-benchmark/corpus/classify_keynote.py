import hashlib
import json
from pathlib import Path
import zipfile
from xml.etree import ElementTree as ET


ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'output/pptx-benchmark-20261106'
DESTINATION = OUT / 'corpus-followup/keynote-supplement'


def hashes(value):
    if isinstance(value, dict):
        for key, child in value.items():
            if key == 'sha256' and isinstance(child, str):
                yield child
            else:
                yield from hashes(child)
    elif isinstance(value, list):
        for child in value:
            yield from hashes(child)


def main():
    known = set()
    sources = []
    for path in OUT.rglob('*.json'):
        if DESTINATION in path.parents or not any(word in path.name for word in ('manifest', 'cohort')):
            continue
        value = json.loads(path.read_text())
        known.update(hashes(value))
        sources.append(dict(path=str(path.relative_to(OUT)), sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
    manifest = json.loads((DESTINATION / 'downloads/manifest.json').read_text())
    rows = []
    for source in manifest['files']:
        path = Path(source['local_path']); data = path.read_bytes(); digest = hashlib.sha256(data).hexdigest()
        assert digest not in known, 'Duplicate input; do not expand the corpus total.'
        with zipfile.ZipFile(path) as archive:
            assert archive.testzip() is None
            names = archive.namelist()
            if 'index.apxl' in names:
                root = ET.fromstring(archive.read('index.apxl'))
                assert root.tag == '{http://developer.apple.com/namespaces/keynote2}presentation', root.tag
                container = 'keynote-xml'
                detail = dict(root=root.tag, slides=sum(n.tag == '{http://developer.apple.com/namespaces/keynote2}slide' for n in root.iter()))
            else:
                assert 'Index/Document.iwa' in names
                container = 'keynote-iwa'
                detail = dict(iwa_parts=sum(n.endswith('.iwa') for n in names), metadata_parts=[n for n in names if n.startswith('Metadata/')])
        rows.append(dict(id=digest[:16], sha256=digest, bytes=len(data), path=str(path.resolve()), extension='key', format=container, split='validation', source=manifest['repo'], source_commit=manifest['ref'], source_path=source['source_path'], source_url=source['download_url'], zip_entries=len(names), crc_valid=True, inspection=detail, parser_evaluated=False))
        known.add(digest)
    output = DESTINATION / 'manifest.json'
    assert not output.exists(), 'Preserve existing receipts.'
    output.write_text(json.dumps(rows, indent=2))
    summary = dict(new_unique=len(rows), total_bytes=sum(r['bytes'] for r in rows), reserved=len(rows), formats={r['format']:sum(x['format']==r['format'] for x in rows) for r in rows}, source_commit=manifest['ref'], deduplication_manifests=sources, manifest_sha256=hashlib.sha256(output.read_bytes()).hexdigest(), harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), scope='Independent ZIP integrity, XML root and IWA part inspection; both inputs reserved before any Rust or peer parser evaluation. Existing PowerPoint benchmark denominators are unchanged.')
    (DESTINATION / 'classification.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps({k:v for k,v in summary.items() if k != 'deduplication_manifests'}))


if __name__ == '__main__':
    main()

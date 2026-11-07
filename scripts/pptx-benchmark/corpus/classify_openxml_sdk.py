import collections
import hashlib
import json
from pathlib import Path
import zipfile
from xml.etree import ElementTree as ET


OUT = Path(__file__).resolve().parents[3] / 'output/pptx-benchmark-20261106'
ROOT = OUT / 'corpus-followup/openxml-sdk'


def main():
    source = json.loads((ROOT / 'discovery.json').read_text())
    download = json.loads((ROOT / 'downloads/manifest.json').read_text())
    assert download['ref'] == source['commit']
    expected = {source['folder'] + '/' + r['path']: r for r in source['candidates']}
    known = set()
    manifests = ['manifest.json', 'expanded/manifest.json', 'corpus-followup/manifest.json', 'corpus-preservation/manifest.json', 'corpus-python-pptx/manifest.json', 'corpus-aspose/manifest.json', 'corpus-followup/embrace-ole-supplement/manifest.json', 'corpus-followup/keynote-supplement/manifest.json']
    identities = {}
    for name in manifests:
        path = OUT / name
        if path.exists():
            known.update(r['sha256'] for r in json.loads(path.read_text()))
            identities[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    prior_total = len(known)
    rows, duplicates = [], []
    for entry in download['files']:
        path = Path(entry['local_path']); data = path.read_bytes(); original = expected[entry['source_path']]
        assert len(data) == original['size']
        assert hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest() == original['sha']
        digest = hashlib.sha256(data).hexdigest()
        if digest in known:
            duplicates.append(dict(sha256=digest, source_path=entry['source_path']))
            continue
        known.add(digest)
        rows.append(dict(id=digest[:16], sha256=digest, path=str(path.resolve()), bytes=len(data), format=Path(entry['source_path']).suffix[1:].lower(), group=Path(entry['source_path']).name.casefold(), source='Open XML SDK', source_commit=source['commit'], source_blob_sha1=original['sha'], source_path=entry['source_path'], source_url=entry['download_url']))
    groups = collections.Counter(r['group'] for r in rows)
    ordered = sorted(groups, key=lambda group: hashlib.sha256(('20261107:openxml-sdk:' + group).encode()).hexdigest())
    reserved, count = set(), 0
    for group in ordered:
        if count >= (len(rows) + 4) // 5:
            break
        reserved.add(group); count += groups[group]
    for row in rows:
        row['split'] = 'validation' if row['group'] in reserved else 'tuning'
    assignments = ROOT / 'split-before-inspection.json'
    assert not assignments.exists(), 'Preserve existing receipts.'
    assignments.write_text(json.dumps(rows, indent=2))
    for row in rows:
        try:
            with zipfile.ZipFile(row['path']) as archive:
                row['crc_failure'] = archive.testzip()
                root = ET.fromstring(archive.read('ppt/presentation.xml'))
                namespace = root.tag.rsplit('}', 1)[0].lstrip('{')
                row.update(container='pptx', presentation_namespace=namespace, slide_references=sum(n.tag == '{' + namespace + '}sldId' for n in root.iter()), parts=len(archive.namelist()))
        except Exception as error:
            row.update(container='unclassified', classification_error=str(error))
    (ROOT / 'manifest.json').write_text(json.dumps(rows, indent=2))
    (ROOT / 'duplicates.json').write_text(json.dumps(duplicates, indent=2))
    summary = dict(downloaded=len(download['files']), new_unique=len(rows), duplicates=len(duplicates), previous_unique=prior_total, combined_unique=len(known), total_bytes=sum(r['bytes'] for r in rows), splits=dict(collections.Counter(r['split'] for r in rows)), formats=dict(collections.Counter(r['format'] for r in rows)), containers=dict(collections.Counter(r['container'] for r in rows)), namespaces=dict(collections.Counter(r.get('presentation_namespace', 'unclassified') for r in rows)), crc_failures=sum(r.get('crc_failure') is not None for r in rows), groups=len(groups), source_commit=source['commit'], deduplication_manifests=identities, split_assignment_sha256=hashlib.sha256(assignments.read_bytes()).hexdigest(), manifest_sha256=hashlib.sha256((ROOT / 'manifest.json').read_bytes()).hexdigest(), harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), split_policy='Case-folded source basenames keep named strict/transitional counterparts together within the new collection. Seeded SHA-256 group ordering reserves at least one fifth before ZIP/XML inspection or parser evaluation. Exact duplicates of earlier collections are excluded.', scope='Public regression corpus, not an independent production sample. ZIP CRCs, presentation root namespace and slide references are independent metadata checks, not parser acceptance or visual fidelity. Existing comparison denominators remain frozen.')
    (ROOT / 'summary.json').write_text(json.dumps(summary, indent=2)); print(json.dumps({k:v for k,v in summary.items() if k != 'deduplication_manifests'}))


if __name__ == '__main__':
    main()

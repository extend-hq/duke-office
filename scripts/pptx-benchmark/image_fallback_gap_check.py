import argparse
import gzip
import hashlib
import json
from pathlib import Path
import zipfile
from xml.etree import ElementTree as ET

from common import NS, rels, sha
from expanded_gap_audit import nodes, pictures
from feedback import OUT


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--label', required=True); parser.add_argument('--phase', default='expanded-fields'); args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    proof = json.loads((run / 'image-fallback-reviewed-summary.json').read_text())
    assert proof['passed'] == proof['total']
    assert hashlib.sha256((run / 'candidate').read_bytes()).hexdigest() == proof['candidate_sha256']
    assert hashlib.sha256((run / 'image-fallback-reviewed.jsonl').read_bytes()).hexdigest() == proof['records_sha256']
    gaps_path = OUT / args.phase / 'image-gap-audit.json'
    gaps = json.loads(gaps_path.read_text())
    assert gaps['binary_sha256'] == proof['baseline_sha256']
    manifest = {r['id']: r for r in json.loads((OUT / args.phase / 'cohort.json').read_text())}
    receipts = []
    models = {}
    for gap in gaps['occurrences']:
        ident = gap['id']
        if ident not in models:
            models[ident] = json.load(gzip.open(run / (ident + '.compact.json.gz'), 'rt'))
        document = models[ident]; slide = document['slides'][gap['slide']]; part = slide['sourcePart']
        image = next(n for n in nodes(slide['nodes']) if n.get('sourcePart') == part and n.get('sourceId') == gap['source_id'])
        path = Path(manifest[ident]['path']); assert sha(path.read_bytes()) == manifest[ident]['sha256']
        with zipfile.ZipFile(path) as archive:
            relationships = rels(archive, part)
            picture = next(p for p in pictures(ET.fromstring(archive.read(part)).find('./p:cSld/p:spTree', NS)) if p.find('.//p:cNvPr', NS).get('id') == gap['source_id'])
            blip = picture.find('./p:blipFill/a:blip', NS)
            original = relationships[blip.get('{' + NS['r'] + '}embed')]['target']
            assert image['fallbackAssetId'] == original
            assert document['assets'][original]['dataSha256'] == sha(archive.read(original))
            assert image['assetId'] != original
        receipts.append(dict(id=ident, slide=gap['slide'], source_id=gap['source_id'], explicit_fallback_matches=True))
    summary = dict(total=len(receipts), matched=len(receipts), inputs=len(models), candidate_sha256=proof['candidate_sha256'], baseline_sha256=proof['baseline_sha256'], gap_audit_sha256=sha(gaps_path.read_bytes()), harness_sha256=sha(Path(__file__).read_bytes()), receipts=receipts, scoring_changed=False, scope='Every historical selected-SVG/main-blip mismatch now has an explicit source-occurrence fallback association with the original byte-identical payload. Existing comparison scores are unchanged; this supplemental check is not visual accuracy.')
    (run / 'image-fallback-gap-summary.json').write_text(json.dumps(summary, indent=2)); print(json.dumps({k:v for k,v in summary.items() if k != 'receipts'}))


if __name__ == '__main__':
    main()

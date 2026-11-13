import collections
import gzip
import json
from pathlib import Path
import sys
import zipfile
from xml.etree import ElementTree as ET

from common import NS, core, local, rels, sha

OUT = Path(__file__).resolve().parents[2] / 'output/pptx-benchmark-20261106'


def nodes(items):
    for item in items:
        yield item
        yield from nodes(item.get('children', []))


def pictures(node):
    kind = local(node)
    if kind == 'pic':
        yield node
    elif kind == 'AlternateContent':
        for child in list(node)[:1]:
            for item in child:
                yield from pictures(item)
    elif kind in ('grpSp', 'spTree'):
        for child in node:
            yield from pictures(child)


def main(phase):
    run = OUT / phase
    assessment = json.loads((run / 'assessment.json').read_text())
    manifest = {x['id']: x for x in json.loads((run / 'cohort.json').read_text())}
    totals = collections.Counter()
    details = []
    for row in assessment['rows']:
        if row['engine'] != 'duke-pptx' or row['status'] != 'ok':
            continue
        ident = row['id']
        if row['metrics']['images']['recall'] == 1:
            continue
        model = json.load(gzip.open(run / 'native' / (ident + '.duke-pptx.json.gz'), 'rt'))
        actual = core(model)
        reference = json.loads((OUT / 'oracle' / (ident + '.json')).read_text())
        with zipfile.ZipFile(manifest[ident]['path']) as package:
            for index, (source, parsed) in enumerate(zip(reference['slides'], actual['slides'])):
                missing = collections.Counter(source['images']) - collections.Counter(parsed['images'])
                if not missing:
                    continue
                slide = model['slides'][index]
                part = slide['sourcePart']
                root = ET.fromstring(package.read(part))
                relationships = rels(package, part)
                by_id = {n.get('sourceId', n['id']): n for n in nodes(slide['nodes']) if n.get('sourcePart') == part}
                tree = root.find('./p:cSld/p:spTree', NS)
                for picture in pictures(tree):
                    blip = picture.find('./p:blipFill/a:blip', NS)
                    if blip is None:
                        continue
                    target = relationships.get(blip.get('{' + NS['r'] + '}embed'), {}).get('target')
                    if target not in package.namelist():
                        continue
                    digest = sha(package.read(target))
                    if not missing[digest]:
                        continue
                    properties = picture.find('.//p:cNvPr', NS)
                    node = by_id.get(properties.get('id') if properties is not None else None, {})
                    alternatives = [relationships.get(n.get('{' + NS['r'] + '}embed'), {}).get('target') for n in blip.iter() if local(n) == 'svgBlip']
                    selected = node.get('assetId')
                    alternative_match = selected in alternatives and selected in package.namelist()
                    if alternative_match:
                        alternative_match = model['assets'].get(selected, {}).get('dataSha256') == sha(package.read(selected))
                    original_retained = model['assets'].get(target, {}).get('dataSha256') == digest
                    category = 'svg_alternative_with_original_payload' if alternative_match and original_retained else 'unresolved'
                    details.append(dict(id=ident, slide=index, source_id=node.get('sourceId'), category=category))
                    totals[category] += 1
                    missing[digest] -= 1
                totals['unclassified'] += sum(missing.values())
    result = dict(phase=phase, binary_sha256=assessment['identity']['binary_sha256'], counts=dict(totals), occurrences=details, scoring_changed=False)
    (run / 'image-gap-audit.json').write_text(json.dumps(result, indent=2))
    print(json.dumps(dict(totals)))


if __name__ == '__main__':
    main(sys.argv[1])

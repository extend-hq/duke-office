import collections
import gzip
import json
from pathlib import Path
import zipfile
from xml.etree import ElementTree as ET

import common
from expanded_gap_audit import nodes, pictures
from feedback import OUT
from sdk_source_check import STRICT, TRANSITIONAL, digest


def main():
    run = OUT / 'corpus-followup/openxml-sdk/source-check1'
    assessment = json.loads((run / 'assessment.json').read_text())
    cohort = {r['id']: r for r in json.loads((run / 'cohort.json').read_text())}
    details = []
    unclassified = 0
    for row in assessment['rows']:
        if row['engine'] != 'duke-slides' or row['status'] != 'ok' or row['metrics']['images']['recall'] == 1:
            continue
        ident = row['id']
        path = run / 'native' / (ident + '.duke-slides.json.gz')
        model = json.load(gzip.open(path, 'rt'))
        reference = json.loads((run / 'reference' / (ident + '.json')).read_text())
        actual = common.core(model)
        ns = STRICT if reference['namespace_family'] == 'strict' else TRANSITIONAL
        source = cohort[ident]
        assert digest(Path(source['path'])) == source['sha256']
        with zipfile.ZipFile(source['path']) as archive:
            for index, (expected, found) in enumerate(zip(reference['slides'], actual['slides'])):
                missing = collections.Counter(expected['images']) - collections.Counter(found['images'])
                if not missing:
                    continue
                slide = model['slides'][index]
                part = slide['sourcePart']
                relationships = common.rels(archive, part)
                tree = ET.fromstring(archive.read(part)).find('./p:cSld/p:spTree', ns)
                for picture in pictures(tree):
                    blip = picture.find('./p:blipFill/a:blip', ns)
                    if blip is None:
                        continue
                    rid = blip.get('{' + ns['r'] + '}embed')
                    relation = relationships.get(rid, {})
                    target = relation.get('target')
                    marker = 'unresolved:' + str(target)
                    if not missing[marker] or target in archive.namelist() or relation.get('external'):
                        continue
                    source_id = picture.find('.//p:cNvPr', ns).get('id')
                    node = next(n for n in nodes(slide['nodes']) if n.get('sourcePart') == part and n.get('sourceId') == source_id)
                    assert node['type'] == 'image' and node['assetId'] == target and target not in model['assets']
                    details.append(dict(id=ident, slide=index, source_id=source_id, category='missing_package_target_retained', target=target, source_sha256=source['sha256'], model_sha256=digest(path)))
                    missing[marker] -= 1
                unclassified += sum(missing.values())
    result = dict(assessment_sha256=digest(run / 'assessment.json'), candidate_sha256=assessment['identity']['binary_sha256'], harness_sha256=digest(Path(__file__)), counts=dict(collections.Counter(r['category'] for r in details)), unclassified=unclassified, occurrences=details, scoring_changed=False, scope='Independent relationship inspection of unmatched image occurrences. Missing payloads remain absent; the frozen metric is unchanged.')
    (run / 'image-gap-audit.json').write_text(json.dumps(result, indent=2))
    print(json.dumps(result))


if __name__ == '__main__':
    main()

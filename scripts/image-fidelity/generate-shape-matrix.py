"""Select editable shape and connector probes for fixed native comparisons."""

import argparse
import hashlib
import json
from pathlib import Path
from xml.sax.saxutils import escape

import generate_office_art_atlas as atlas


SELECTED = [
    'geometry/roundRect', 'geometry/ellipse', 'geometry/hexagon', 'geometry/star5',
    'geometry/donut', 'geometry/blockArc', 'geometry/pie', 'geometry/flowChartDecision',
    'geometry/wedgeRoundRectCallout', 'geometry/custom-numeric-lines',
    'geometry/custom-cubic-outside-frame', 'geometry/custom-arc', 'geometry/custom-formula',
    'fills-outlines/arrow-triangle', 'geometry/bentConnector3', 'geometry/curvedConnector3',
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    if any(args.output.iterdir()):
        raise FileExistsError('Choose an empty output directory')
    catalog = {row['id']: row for row in atlas.all_cases()}
    rows = [catalog[identifier] for identifier in SELECTED]
    paragraphs = []
    for ordinal, row in enumerate(rows, 1):
        shape = atlas.shape(row['params'], width=240, height=144)
        drawing = f'<w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="251658240" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>{96 * atlas.EMU}</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>{120 * atlas.EMU}</wp:posOffset></wp:positionV><wp:extent cx="{240 * atlas.EMU}" cy="{144 * atlas.EMU}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapNone/><wp:docPr id="{ordinal}" name="{escape(row["id"])}" descr="{escape(row["label"])}"/><a:graphic><a:graphicData uri="{atlas.NAMESPACES["wps"]}">{shape}</a:graphicData></a:graphic></wp:anchor></w:drawing>'
        page_break = '<w:pageBreakBefore/>' if ordinal > 1 else ''
        paragraphs.append(f'<w:p><w:pPr>{page_break}<w:spacing w:before="0" w:after="0" w:line="20" w:lineRule="exact"/></w:pPr><w:r>{drawing}</w:r><w:r><w:rPr><w:sz w:val="2"/></w:rPr><w:t> </w:t></w:r></w:p>')
    section = '<w:sectPr><w:pgSz w:w="6480" w:h="6480"/><w:pgMar w:top="360" w:right="360" w:bottom="360" w:left="360" w:header="0" w:footer="0" w:gutter="0"/></w:sectPr>'
    document = f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {atlas.NS} mc:Ignorable="w14 wps wpg a14"><w:body>{"".join(paragraphs)}{section}</w:body></w:document>'
    target = args.output / 'shape-matrix.docx'
    validation = atlas.build_package(target, document, atlas.source_png())
    manifest = dict(schemaVersion=1, document=target.name,
                    documentSha256=hashlib.sha256(target.read_bytes()).hexdigest(), dpi=96,
                    pageSizePx=dict(width=432, height=432),
                    layoutFramePx=dict(x=96, y=120, width=240, height=144),
                    comparisonRegionPx=dict(x=48, y=48, width=336, height=336),
                    generator='scripts/image-fidelity/generate-shape-matrix.py',
                    atlasGeneratorSha256=hashlib.sha256(Path(atlas.__file__).read_bytes()).hexdigest(),
                    cases=[dict(page=index, **row) for index, row in enumerate(rows, 1)],
                    validation=validation,
                    limitations=['Bounded shape and connector selection; not exhaustive parameter coverage.',
                                 'Native appearance is unmeasured until an independent capture receipt exists.',
                                 'Atlas support labels are generation-time hypotheses, not native comparison results.'])
    (args.output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps(dict(document=str(target), cases=len(rows), sha256=manifest['documentSha256'])))


if __name__ == '__main__':
    main()

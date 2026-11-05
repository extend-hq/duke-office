"""Build bounded cached-drawing and semantic SmartArt probes for manual review."""

import argparse
import hashlib
import json
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo

import generate_office_art_atlas as atlas


DGM = 'http://schemas.openxmlformats.org/drawingml/2006/diagram'
DSP = 'http://schemas.microsoft.com/office/drawing/2008/diagram'
REL = atlas.NAMESPACES['r']
REL_NS = 'http://schemas.openxmlformats.org/package/2006/relationships'
WIDTH, HEIGHT = 480, 240
IDS = [f'{{00000000-0000-0000-0000-{value:012d}}}' for value in range(1, 5)]


def group_transform():
    width, height = WIDTH * atlas.EMU, HEIGHT * atlas.EMU
    return f'<a:xfrm><a:off x="0" y="0"/><a:ext cx="{width}" cy="{height}"/><a:chOff x="0" y="0"/><a:chExt cx="{width}" cy="{height}"/></a:xfrm>'


def text_body(text, inherited=False):
    properties = '<a:pPr algn="ctr"><a:defRPr sz="2400"><a:solidFill><a:srgbClr val="A52A65"/></a:solidFill><a:latin typeface="Arial"/></a:defRPr></a:pPr>' if inherited else '<a:pPr algn="ctr"/>'
    run = '<a:rPr b="1"/>' if inherited else '<a:rPr sz="1800"><a:solidFill><a:srgbClr val="17334F"/></a:solidFill><a:latin typeface="Arial"/></a:rPr>'
    return '<a:bodyPr anchor="ctr"/><a:lstStyle/><a:p>' + properties + '<a:r>' + run + '<a:t>' + text + '</a:t></a:r></a:p>'


def drawing(empty_transform=False, text_transform=False):
    properties = '' if empty_transform else group_transform()
    first = '<dsp:sp modelId="' + IDS[1] + '"><dsp:nvSpPr><dsp:cNvPr id="1" name="Styled cached text"/><dsp:cNvSpPr/></dsp:nvSpPr><dsp:spPr>' + atlas.transform('', 180, 90, 20 * atlas.EMU, 20 * atlas.EMU) + '<a:prstGeom prst="roundRect"><a:avLst/></a:prstGeom>' + atlas.solid('E7F0FC') + atlas.outline() + '</dsp:spPr><dsp:txBody>' + text_body('Styled label', True) + '</dsp:txBody></dsp:sp>'
    if text_transform:
        text_frame = atlas.transform('rot="5400000"', 180, 90, 20 * atlas.EMU, 20 * atlas.EMU).replace('a:xfrm', 'dsp:txXfrm')
        first = first.replace('</dsp:sp>', text_frame + '</dsp:sp>')
    second = '<dsp:sp modelId="' + IDS[3] + '"><dsp:nvSpPr><dsp:cNvPr id="2" name="Cached text"/><dsp:cNvSpPr/></dsp:nvSpPr><dsp:spPr>' + atlas.transform('rot="600000"', 180, 90, 280 * atlas.EMU, 20 * atlas.EMU) + '<a:prstGeom prst="hexagon"><a:avLst/></a:prstGeom>' + atlas.solid('DDF4EA') + atlas.outline() + '</dsp:spPr><dsp:txBody>' + text_body('Cached label') + '</dsp:txBody></dsp:sp>'
    connector = '<dsp:sp modelId="5"><dsp:nvSpPr><dsp:cNvPr id="3" name="Connector"/><dsp:cNvSpPr/></dsp:nvSpPr><dsp:spPr>' + atlas.transform('', 80, 0, 200 * atlas.EMU, 65 * atlas.EMU) + '<a:prstGeom prst="line"><a:avLst/></a:prstGeom>' + atlas.outline(tail='<a:tailEnd type="triangle"/>') + '</dsp:spPr></dsp:sp>'
    image = '<dsp:sp modelId="6"><dsp:nvSpPr><dsp:cNvPr id="4" name="Picture fill"/><dsp:cNvSpPr/></dsp:nvSpPr><dsp:spPr>' + atlas.transform('', 144, 80, 168 * atlas.EMU, 140 * atlas.EMU) + '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:blipFill><a:blip r:embed="rIdImage"/><a:stretch><a:fillRect/></a:stretch></a:blipFill><a:ln><a:noFill/></a:ln></dsp:spPr><dsp:txBody><a:bodyPr/><a:lstStyle/><a:p/></dsp:txBody></dsp:sp>'
    return f'<dsp:drawing xmlns:dsp="{DSP}" xmlns:a="{atlas.NAMESPACES["a"]}" xmlns:r="{REL}"><dsp:spTree><dsp:nvGrpSpPr><dsp:cNvPr id="0" name="Diagram"/><dsp:cNvGrpSpPr/></dsp:nvGrpSpPr><dsp:grpSpPr>{properties}</dsp:grpSpPr>{first}{second}{connector}{image}</dsp:spTree></dsp:drawing>'


def data_model(cached=True):
    points = f'<dgm:pt modelId="{IDS[0]}"><dgm:prSet/><dgm:spPr/><dgm:t>{text_body("Mapped", True)}</dgm:t></dgm:pt><dgm:pt modelId="{IDS[1]}" type="pres"><dgm:prSet presAssocID="{IDS[0]}"/><dgm:spPr/></dgm:pt><dgm:pt modelId="{IDS[2]}"><dgm:prSet/><dgm:spPr/><dgm:t>{text_body("Semantic fallback")}</dgm:t></dgm:pt><dgm:pt modelId="{IDS[3]}" type="pres"><dgm:prSet presAssocID="{IDS[2]}"/><dgm:spPr/></dgm:pt>'
    points += f'<dgm:pt modelId="5" type="pres"><dgm:prSet presAssocID="{IDS[0]}"/><dgm:spPr/></dgm:pt><dgm:pt modelId="6" type="pres"><dgm:prSet presAssocID="{IDS[2]}"/><dgm:spPr/></dgm:pt>'
    extension = '<dgm:extLst><a:ext uri="{C02D2F84-1781-4D44-A6E2-84DEAB8CEABF}"><dsp:dataModelExt relId="rIdDrawing"/></a:ext></dgm:extLst>' if cached else ''
    return f'<dgm:dataModel xmlns:dgm="{DGM}" xmlns:a="{atlas.NAMESPACES["a"]}" xmlns:dsp="{DSP}"><dgm:ptLst>{points}</dgm:ptLst><dgm:cxnLst/>{extension}</dgm:dataModel>'


def generate(output):
    output.mkdir(parents=True, exist_ok=True)
    rows = []
    variants = [('cached', False, False, True, False), ('cached-empty-group-transform', True, False, True, False), ('cached-namespace-alias', False, True, True, False), ('semantic-fallback', False, False, False, False), ('cached-text-transform', False, False, True, True)]
    for identifier, empty_transform, alias, cached, text_transform in variants:
        prefix = 'dg' if alias else 'dgm'
        inline = f'<w:drawing><wp:inline><wp:extent cx="{WIDTH * atlas.EMU}" cy="{HEIGHT * atlas.EMU}"/><wp:docPr id="1" name="{identifier}"/><a:graphic><a:graphicData uri="{DGM}"><{prefix}:relIds r:dm="rIdData" r:lo="rIdLayout" r:qs="rIdStyle" r:cs="rIdColors"/></a:graphicData></a:graphic></wp:inline></w:drawing>'
        body = atlas.paragraph('SmartArt adapter · ' + identifier, 28, True) + atlas.paragraph('Cached geometry, authoritative cached text, inherited style, connector, and embedded picture.', 18) + atlas.paragraph(raw=inline)
        document = f'<w:document {atlas.NS} xmlns:{prefix}="{DGM}"><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="0" w:footer="0" w:gutter="0"/></w:sectPr></w:body></w:document>'
        content_types = '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
        for name, kind in [('data', 'data'), ('layout', 'layout'), ('style', 'style'), ('colors', 'colors')]:
            content_types += f'<Override PartName="/word/diagrams/{name}.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagram{kind.capitalize()}+xml"/>'
        if cached:
            content_types += '<Override PartName="/word/diagrams/drawing.xml" ContentType="application/vnd.ms-office.drawingml.diagramDrawing+xml"/>'
        content_types += '</Types>'
        rels = ''.join(f'<Relationship Id="rId{label}" Type="{REL}/diagram{kind}" Target="diagrams/{filename}.xml"/>' for label, kind, filename in [('Data', 'Data', 'data'), ('Layout', 'Layout', 'layout'), ('Style', 'QuickStyle', 'style'), ('Colors', 'Colors', 'colors')])
        parts = {
            '[Content_Types].xml': content_types,
            '_rels/.rels': f'<Relationships xmlns="{REL_NS}"><Relationship Id="rIdDocument" Type="{REL}/officeDocument" Target="word/document.xml"/></Relationships>',
            'word/document.xml': document,
            'word/_rels/document.xml.rels': f'<Relationships xmlns="{REL_NS}">{rels}</Relationships>',
            'word/diagrams/data.xml': data_model(cached),
            'word/diagrams/layout.xml': f'<dgm:layoutDef xmlns:dgm="{DGM}" uniqueId="urn:office-art-atlas:layout" minVer="12.0"><dgm:title val="Linear layout"/><dgm:layoutNode name="root"><dgm:alg type="lin"/><dgm:shape type="rect"/></dgm:layoutNode></dgm:layoutDef>',
            'word/diagrams/style.xml': f'<dgm:styleDef xmlns:dgm="{DGM}" uniqueId="urn:office-art-atlas:style" minVer="12.0"><dgm:title val="Direct colors"/><dgm:styleLbl name="node0"/></dgm:styleDef>',
            'word/diagrams/colors.xml': f'<dgm:colorsDef xmlns:dgm="{DGM}" uniqueId="urn:office-art-atlas:colors" minVer="12.0"><dgm:title val="Direct colors"/><dgm:styleLbl name="node0"/></dgm:colorsDef>',
        }
        if cached:
            parts.update({
                'word/diagrams/drawing.xml': drawing(empty_transform, text_transform),
                'word/diagrams/_rels/data.xml.rels': f'<Relationships xmlns="{REL_NS}"><Relationship Id="rIdDrawing" Type="http://schemas.microsoft.com/office/2007/relationships/diagramDrawing" Target="drawing.xml"/></Relationships>',
                'word/diagrams/_rels/drawing.xml.rels': f'<Relationships xmlns="{REL_NS}"><Relationship Id="rIdImage" Type="{REL}/image" Target="../media/source.png"/></Relationships>',
                'word/media/source.png': atlas.source_png(),
            })
        path = output / ('smartart-' + identifier + '.docx')
        with ZipFile(path, 'w', compression=ZIP_DEFLATED) as archive:
            for name, contents in parts.items():
                info = ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
                info.compress_type = ZIP_DEFLATED
                archive.writestr(info, contents)
        rows.append(dict(document=path.name, id=identifier, sha256=hashlib.sha256(path.read_bytes()).hexdigest(), validation=atlas.validate_package(path), nativeVisual='unmeasured', expected={'diagramImages': 1, 'hasCachedGeometry': cached, 'semanticText': ['Mapped', 'Semantic fallback'] if not cached else [], 'cachedText': ['Styled label', 'Cached label'] if cached else [], 'inheritedTextSizePx': 32 if cached else None, 'inheritedTextColor': '#A52A65' if cached else None, 'separateTextRotationDeg': 90 if text_transform else 0}))
    (output / 'manifest.json').write_text(json.dumps({'documents': rows, 'notes': ['Synthetic cached drawing probes; no native Word opening or visual parity measured.', 'The empty group transform and namespace alias variants are valid structural edge probes.', 'The semantic-only document deliberately omits a cached drawing.']}, indent=2) + '\n')
    readme = '\n'.join([
        '# SmartArt DOCX adapter probes', '',
        'Five self-contained manual documents cover cached drawing geometry, authoritative saved text, inherited run formatting, namespace aliases, empty group transforms, picture fills, a line connector, and semantic-only fallback.', '',
        'The normal, empty-group-transform, and namespace-alias variants should render identically. The styled cached label inherits Arial, 24pt, bold, and color #A52A65. Cached text takes precedence over semantic associations. Missing and empty cached text bodies stay empty, including associated connector and picture shapes. The semantic-only document should display two labels without invented layout.', '',
        'The separate text-transform variant requests 90-degree independent text rotation. The shared reader preserves its offset, extent, rotation, and flips; the DOCX adapter renders the saved text frame independently of shape paint. Incomplete text frames still report a fallback warning.', '',
        'Native Word opening, editing, and visual results remain unmeasured. Parser smoke and schema-metadata receipts are stored separately when validation is run; they do not establish Word parity.', '',
        'Regenerate with `/usr/bin/python3 scripts/image-fidelity/generate_smartart_docx.py --output artifacts/office-art-fixtures-20261105/smartart`.', '',
    ])
    (output / 'README.md').write_text(readme)
    with ZipFile(output / 'smartart-manual-probes.zip', 'w', compression=ZIP_DEFLATED) as archive:
        for name in ['README.md', 'manifest.json'] + [row['document'] for row in rows]:
            info = ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = ZIP_DEFLATED
            archive.writestr(info, (output / name).read_bytes())
    print(json.dumps({'output': str(output), 'documents': len(rows)}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    generate(parser.parse_args().output)

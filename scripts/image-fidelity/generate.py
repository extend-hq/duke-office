"""Generate a deterministic picture-appearance matrix for native visual capture."""

import argparse
import hashlib
import io
import json
from pathlib import Path
from xml.sax.saxutils import escape
from zipfile import ZipFile, ZIP_DEFLATED, ZipInfo

from PIL import Image, ImageDraw


EMU = 9525
WIDTH, HEIGHT = 192, 128
LEFT, TOP = 120, 120
PAGE = 432
NS = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" xmlns:a14="http://schemas.microsoft.com/office/drawing/2010/main" xmlns:asvg="http://schemas.microsoft.com/office/drawing/2016/SVG/main" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:w10="urn:schemas-microsoft-com:office:word"'


def source_png():
    image = Image.new('RGBA', (WIDTH, HEIGHT), 'white')
    pixels = image.load()
    colors = [(235, 40, 35), (25, 185, 65), (35, 80, 235), (245, 195, 35), (30, 195, 200), (205, 45, 190)]
    for y in range(80):
        for x in range(WIDTH):
            pixels[x, y] = (*colors[x // 32], 255)
    for y in range(80, HEIGHT):
        for x in range(WIDTH):
            if x < 128:
                level = round(255 * x / 127)
                pixels[x, y] = (level, level, level, 255)
            else:
                pixels[x, y] = (235, 45, 45, round(255 * (x - 128) / 63))
    draw = ImageDraw.Draw(image)
    draw.rectangle((8, 8, 23, 23), fill=(255, 255, 255, 255))
    draw.polygon([(144, 13), (182, 50), (144, 65)], fill=(15, 20, 25, 255))
    draw.line([(5, 120), (120, 86)], fill=(0, 160, 245, 255), width=4)
    output = io.BytesIO()
    image.save(output, format='PNG', dpi=(96, 96))
    return output.getvalue()


def cases():
    return [
        dict(id='plain'),
        dict(id='rotate-30', transform='rot="1800000"'),
        dict(id='rotate-90', transform='rot="5400000"'),
        dict(id='flip-horizontal', transform='flipH="1"'),
        dict(id='flip-vertical', transform='flipV="1"'),
        dict(id='rotate-flip', transform='rot="1800000" flipH="1"'),
        dict(id='crop-positive', crop='l="20000" t="10000" r="10000" b="15000"'),
        dict(id='crop-negative', crop='l="-20000" t="-15000" r="10000" b="0"'),
        dict(id='fill-rectangle', fill_rect='l="10000" t="20000" r="15000" b="10000"'),
        dict(id='ellipse', geometry='ellipse'),
        dict(id='round-rectangle', geometry='roundRect'),
        dict(id='triangle', geometry='triangle'),
        dict(id='diamond', geometry='diamond'),
        dict(id='border', line='<a:ln w="38100"><a:solidFill><a:srgbClr val="283449"/></a:solidFill></a:ln>'),
        dict(id='border-dashed', line='<a:ln w="28575"><a:solidFill><a:srgbClr val="283449"/></a:solidFill><a:prstDash val="dash"/></a:ln>'),
        dict(id='grayscale', effect='<a:grayscl/>'),
        dict(id='bilevel', effect='<a:biLevel thresh="50000"/>'),
        dict(id='duotone', effect='<a:duotone><a:srgbClr val="102040"/><a:srgbClr val="FFE0A0"/></a:duotone>'),
        dict(id='opacity', effect='<a:alphaModFix amt="45000"/>'),
        dict(id='alpha-bilevel', effect='<a:alphaBiLevel thresh="50000"/>'),
        dict(id='alpha-inverse', effect='<a:alphaInv/>'),
        dict(id='alpha-replace', effect='<a:alphaRepl a="60000"/>'),
        dict(id='luminance-brightness', effect='<a:lum bright="20000" contrast="0"/>'),
        dict(id='luminance-contrast', effect='<a:lum bright="0" contrast="30000"/>'),
        dict(id='luminance-combined', effect='<a:lum bright="-20000" contrast="30000"/>'),
        dict(id='blur', effect='<a:blur rad="38100" grow="1"/>'),
        dict(id='hsl', effect='<a:hsl hue="3600000" sat="50000" lum="100000"/>'),
        dict(id='tint', effect='<a:tint hue="10800000" amt="50000"/>'),
        dict(id='outer-shadow', shape_effect='<a:outerShdw blurRad="50800" dist="76200" dir="2700000" algn="ctr" rotWithShape="0"><a:srgbClr val="202040"><a:alpha val="70000"/></a:srgbClr></a:outerShdw>'),
        dict(id='inner-shadow', shape_effect='<a:innerShdw blurRad="38100" dist="38100" dir="2700000"><a:srgbClr val="202040"><a:alpha val="70000"/></a:srgbClr></a:innerShdw>'),
        dict(id='glow', shape_effect='<a:glow rad="76200"><a:srgbClr val="20A0E0"><a:alpha val="70000"/></a:srgbClr></a:glow>'),
        dict(id='soft-edge', shape_effect='<a:softEdge rad="76200"/>'),
        dict(id='reflection', shape_effect='<a:reflection blurRad="12700" stA="50000" stPos="0" endA="0" endPos="60000" dist="38100" dir="5400000" sx="100000" sy="-100000" kx="0" ky="0" algn="bl" fadeDir="5400000" rotWithShape="0"/>'),
        dict(id='tile', tile='tx="0" ty="0" sx="50000" sy="50000" flip="none" algn="tl"'),
        dict(id='svg-alternative', svg='rIdSvg'),
        dict(id='svg-missing-alternative', svg='rIdMissing'),
        dict(id='vml-crop', vml=True, vml_image='cropleft="13107f" croptop="6554f" cropright="6554f" cropbottom="9830f"'),
        dict(id='vml-rotation-flip', vml=True, vml_style='rotation:30;flip:x;'),
        dict(id='vml-grayscale', vml=True, vml_image='grayscale="t"'),
        dict(id='vml-bilevel', vml=True, vml_image='bilevel="t"'),
    ]


def picture(case, ordinal):
    if case.get('vml'):
        return f'<w:pict><v:shape id="picture-{ordinal}" type="#_x0000_t75" style="position:absolute;margin-left:{LEFT}px;margin-top:{TOP}px;width:{WIDTH}px;height:{HEIGHT}px;mso-position-horizontal-relative:page;mso-position-vertical-relative:page;{case.get("vml_style", "")}" filled="f" stroked="f"><v:imagedata r:id="rIdImage" {case.get("vml_image", "")}/><w10:wrap type="none"/></v:shape></w:pict>'
    extension = ''
    if case.get('svg'):
        extension = f'<a:extLst><a:ext uri="{{96DAC541-7B7A-43D3-8B79-37D633B846F1}}"><asvg:svgBlip r:embed="{case["svg"]}"/></a:ext></a:extLst>'
    crop = f'<a:srcRect {case["crop"]}/>' if case.get('crop') else ''
    fill = f'<a:tile {case["tile"]}/>' if case.get('tile') else f'<a:stretch><a:fillRect {case.get("fill_rect", "")}/></a:stretch>'
    effect = f'<a:effectLst>{case["shape_effect"]}</a:effectLst>' if case.get('shape_effect') else ''
    return f'<w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="251658240" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>{LEFT * EMU}</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>{TOP * EMU}</wp:posOffset></wp:positionV><wp:extent cx="{WIDTH * EMU}" cy="{HEIGHT * EMU}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapNone/><wp:docPr id="{ordinal}" name="{escape(case["id"])}" descr="{escape(case["id"])}"/><wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect="1"/></wp:cNvGraphicFramePr><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="0" name="{escape(case["id"])}"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdImage">{case.get("effect", "")}{extension}</a:blip>{crop}{fill}</pic:blipFill><pic:spPr><a:xfrm {case.get("transform", "")}><a:off x="0" y="0"/><a:ext cx="{WIDTH * EMU}" cy="{HEIGHT * EMU}"/></a:xfrm><a:prstGeom prst="{case.get("geometry", "rect")}"><a:avLst/></a:prstGeom>{case.get("line", "")}{effect}</pic:spPr></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing>'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    if any((args.output / name).exists() for name in ('word-reference.pdf', 'word-reference.json')):
        raise FileExistsError('Native reference already exists; choose a new output directory')
    image = source_png()
    (args.output / 'source.png').write_bytes(image)
    rows = cases()
    paragraphs = []
    for ordinal, case in enumerate(rows, 1):
        page_break = '<w:pageBreakBefore/>' if ordinal > 1 else ''
        paragraphs.append(f'<w:p><w:pPr>{page_break}<w:spacing w:before="0" w:after="0" w:line="20" w:lineRule="exact"/></w:pPr><w:r>{picture(case, ordinal)}</w:r><w:r><w:rPr><w:sz w:val="2"/></w:rPr><w:t> </w:t></w:r></w:p>')
    document = f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NS}><w:body>{"".join(paragraphs)}<w:sectPr><w:pgSz w:w="{PAGE * 15}" w:h="{PAGE * 15}"/><w:pgMar w:top="360" w:right="360" w:bottom="360" w:left="360" w:header="0" w:footer="0" w:gutter="0"/></w:sectPr></w:body></w:document>'
    svg = '<svg xmlns="http://www.w3.org/2000/svg" width="192" height="128" viewBox="0 0 192 128"><rect width="192" height="128" fill="#E03030"/><path d="M10 10 L182 64 L10 118 Z" fill="#2080F0"/><circle cx="42" cy="64" r="18" fill="#F0E040"/></svg>'
    files = {
        '[Content_Types].xml': '<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Default Extension="svg" ContentType="image/svg+xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>',
        '_rels/.rels': '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>',
        'word/_rels/document.xml.rels': '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdImage" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/source.png"/><Relationship Id="rIdSvg" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/alternative.svg"/></Relationships>',
        'word/document.xml': document,
        'word/media/source.png': image,
        'word/media/alternative.svg': svg,
    }
    output = args.output / 'image-matrix.docx'
    with ZipFile(output, 'w', compression=ZIP_DEFLATED) as archive:
        for name, data in files.items():
            info = ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = ZIP_DEFLATED
            archive.writestr(info, data)
    manifest = {
        'schemaVersion': 1,
        'document': output.name,
        'documentSha256': hashlib.sha256(output.read_bytes()).hexdigest(),
        'sourceSha256': hashlib.sha256(image).hexdigest(),
        'dpi': 96,
        'pageSizePx': {'width': PAGE, 'height': PAGE},
        'layoutFramePx': {'x': LEFT, 'y': TOP, 'width': WIDTH, 'height': HEIGHT},
        'comparisonRegionPx': {'x': 48, 'y': 48, 'width': 336, 'height': 336},
        'cases': [dict(page=i, **row) for i, row in enumerate(rows, 1)],
        'limitations': ['Synthetic appearance coverage, not an exhaustive format corpus.', 'The missing SVG alternative intentionally exercises unavailable relationship handling.', 'Visual results depend on the installed Word build and PDF export pipeline.'],
    }
    (args.output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps({'document': str(output), 'cases': len(rows), 'sha256': manifest['documentSha256']}))


if __name__ == '__main__':
    main()

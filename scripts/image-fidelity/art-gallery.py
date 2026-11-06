"""Build fixed-coordinate Office art inputs and a measured comparison gallery."""

import argparse
import copy
import csv
import hashlib
import io
import json
import math
import re
import subprocess
from pathlib import Path
from xml.etree import ElementTree as ET
from zipfile import ZipFile, ZIP_DEFLATED, ZipInfo

from PIL import Image, ImageDraw, ImageFont

import generate_office_art_atlas as atlas
from compare import metrics

PAGE = (816, 1056)
W = atlas.NAMESPACES['w']
WP = atlas.NAMESPACES['wp']


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def record_reference(directory, version, export_method):
    manifest = json.loads((directory / 'manifest.json').read_text())
    target = directory / 'word-cairo-reference.json'
    if target.exists():
        raise FileExistsError(target)
    assert digest(directory / manifest['document']) == manifest['documentSha256']
    paths = sorted((directory / 'word-cairo-pages').glob('page-*.png'))
    assert len(paths) == len(manifest['cases'])
    for ordinal, path in enumerate(paths, 1):
        assert path.name == f'page-{ordinal:02d}.png'
        assert Image.open(path).size == PAGE
    write_json(target, dict(wordVersion=version, documentSha256=manifest['documentSha256'],
               pdfSha256=digest(directory / 'word-reference.pdf'), pageCount=len(paths), dpi=96,
               exportMethod=export_method,
               pages=[dict(path=str(path.relative_to(directory)), sha256=digest(path)) for path in paths]))


def fixed_drawing(node, ordinal):
    node = copy.deepcopy(node)
    frames = [n for n in node.iter() if n.tag in (f'{{{WP}}}anchor', f'{{{WP}}}inline')]
    extent = frames[0].find(f'{{{WP}}}extent')
    width, height = (int(extent.get(k)) / atlas.EMU for k in ('cx', 'cy'))
    x, y = (PAGE[0] - width) / 2, (PAGE[1] - height) / 2
    def position(frame, axis):
        value = frame.find(f'{{{WP}}}position{axis}/{{{WP}}}posOffset')
        return int(value.text) / atlas.EMU if value is not None else 0
    origin = [position(frames[0], axis) for axis in ['H', 'V']]
    def pixels(value):
        match = re.fullmatch(r'(-?[\d.]+)(pt|in|px)?', value)
        if not match:
            raise ValueError('Unsupported fallback positioning unit')
        return float(match[1]) * {'pt': 4 / 3, 'in': 96, 'px': 1, None: 1}[match[2]]
    for fallback in node.findall('.//{urn:schemas-microsoft-com:vml}shape'):
        style = dict(item.split(':', 1) for item in fallback.get('style', '').split(';') if ':' in item)
        for axis, key, value, old in [('horizontal', 'margin-left', x, origin[0]), ('vertical', 'margin-top', y, origin[1])]:
            if key in style:
                style[key] = f'{(pixels(style[key]) + value - old) * .75:.6f}pt'
                style[f'mso-position-{axis}-relative'] = 'page'
        fallback.set('style', ';'.join(f'{key}:{value}' for key, value in style.items()))
    for frame in frames:
        adjusted = [x + position(frame, 'H') - origin[0], y + position(frame, 'V') - origin[1]]
        frame.tag = f'{{{WP}}}anchor'
        frame.attrib = dict(distT='0', distB='0', distL='0', distR='0', simplePos='0',
                            relativeHeight='251658240', behindDoc='0', locked='0',
                            layoutInCell='1', allowOverlap='1')
        kept = [n for n in frame if n.tag.rsplit('}', 1)[-1] in
                ('extent', 'effectExtent', 'docPr', 'cNvGraphicFramePr', 'graphic')]
        frame[:] = []
        ET.SubElement(frame, f'{{{WP}}}simplePos', x='0', y='0')
        for axis, value in zip(['H', 'V'], adjusted):
            placement = ET.SubElement(frame, f'{{{WP}}}position{axis}', relativeFrom='page')
            ET.SubElement(placement, f'{{{WP}}}posOffset').text = str(round(value * atlas.EMU))
        for n in kept:
            local = n.tag.rsplit('}', 1)[-1]
            if local == 'docPr':
                ET.SubElement(frame, f'{{{WP}}}wrapNone')
                n.set('id', str(ordinal))
            frame.append(n)
    return node, dict(x=x, y=y, width=width, height=height)


def generate(source, output):
    output.mkdir(parents=True, exist_ok=True)
    target = output / 'office-art-gallery.docx'
    if target.exists():
        raise FileExistsError(target)
    with ZipFile(source) as archive:
        parts = {name: archive.read(name) for name in archive.namelist()}
    namespaces = dict(item for _, item in ET.iterparse(io.BytesIO(parts['word/document.xml']), events=['start-ns']))
    namespaces.update(atlas.NAMESPACES)
    for prefix, uri in namespaces.items():
        ET.register_namespace(prefix, uri)
    root = ET.fromstring(parts['word/document.xml'])
    body = root.find(f'{{{W}}}body')
    rows, objects = [], []
    for paragraph in body.findall(f'{{{W}}}p'):
        for run in paragraph.findall(f'{{{W}}}r'):
            for node in run:
                props = node.find('.//{*}docPr')
                if props is None:
                    continue
                name = props.get('descr') or props.get('name') or 'Drawing'
                if node.find('.//{*}model3d') is not None:
                    family, mode = '3D models', 'Saved Office raster preview'
                elif node.find('.//{*}relIds') is not None:
                    family, mode = 'SmartArt', 'Saved drawing geometry'
                elif node.find('.//{*}txbxContent') is not None:
                    family, mode = 'WordArt', 'Text fill, outline and shadow'
                elif 'Ink' in props.get('name', ''):
                    family, mode = 'Ink', 'Saved ink representation'
                elif node.find('.//{*}svgBlip') is not None:
                    family, mode = 'SVG icons', 'Embedded vector artwork'
                else:
                    family, mode = 'Shapes', 'DrawingML geometry and paint'
                objects.append(node)
                rows.append(dict(id=f'art-{len(rows) + 1:02d}', title=name, family=family, mode=mode))
    selected = ['wordart/textNoShape', 'wordart/solid-fill', 'wordart/outline-only',
                'wordart/shadow', 'wordart/textWave1', 'wordart/textArchUp',
                'wordart/linear-gradient', 'wordart/extrusion',
                'three-d/camera-isometricTopUp', 'three-d/material-metal',
                'three-d/bevel-circle', 'artistic/artisticPencilSketch']
    catalog = {row['id']: row for row in atlas.all_cases()}
    for key in selected:
        row = catalog[key]
        drawing = atlas.drawing(row, len(rows) + 1).replace('rIdImage', 'rIdGalleryImage').replace('rIdOriginal', 'rIdGalleryImage')
        node = ET.fromstring(f'<root {atlas.NS}>{drawing}</root>')[0]
        objects.append(node)
        family = {'wordart': 'WordArt', 'three-d': '3D shapes', 'artistic': 'Artistic effects'}[row['family']]
        rows.append(dict(id=key, title=row['label'], family=family,
                         mode='Unchanged cache control' if family == 'Artistic effects' else 'DrawingML rendering'))
    body[:] = []
    for ordinal, (node, row) in enumerate(zip(objects, rows), 1):
        node, frame = fixed_drawing(node, ordinal)
        row.update(page=ordinal, framePx=frame)
        row['cropPx'] = [max(0, math.floor(frame['x']) - 36), max(0, math.floor(frame['y']) - 36),
                         min(PAGE[0], math.ceil(frame['x'] + frame['width']) + 36),
                         min(PAGE[1], math.ceil(frame['y'] + frame['height']) + 36)]
        paragraph = ET.SubElement(body, f'{{{W}}}p')
        props = ET.SubElement(paragraph, f'{{{W}}}pPr')
        if ordinal > 1:
            ET.SubElement(props, f'{{{W}}}pageBreakBefore')
        ET.SubElement(props, f'{{{W}}}spacing', {f'{{{W}}}after': '0', f'{{{W}}}line': '20', f'{{{W}}}lineRule': 'exact'})
        ET.SubElement(paragraph, f'{{{W}}}r').append(node)
    section = ET.SubElement(body, f'{{{W}}}sectPr')
    ET.SubElement(section, f'{{{W}}}pgSz', {f'{{{W}}}w': str(PAGE[0] * 15), f'{{{W}}}h': str(PAGE[1] * 15)})
    ET.SubElement(section, f'{{{W}}}pgMar', {f'{{{W}}}{k}': '0' for k in ['top', 'right', 'bottom', 'left', 'header', 'footer', 'gutter']})
    document = ET.tostring(root, encoding='unicode')
    for prefix, uri in namespaces.items():
        if f'xmlns:{prefix}=' not in document:
            document = document.replace('<w:document ', f'<w:document xmlns:{prefix}="{uri}" ', 1)
    parts['word/document.xml'] = document.encode()
    rels = ET.fromstring(parts['word/_rels/document.xml.rels'])
    ET.SubElement(rels, '{http://schemas.openxmlformats.org/package/2006/relationships}Relationship',
                  Id='rIdGalleryImage', Type=atlas.NAMESPACES['r'] + '/image', Target='media/gallery-source.png')
    parts['word/_rels/document.xml.rels'] = ET.tostring(rels)
    parts['word/media/gallery-source.png'] = atlas.source_png()
    with ZipFile(target, 'w', ZIP_DEFLATED) as archive:
        for name, value in parts.items():
            info = ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = ZIP_DEFLATED
            archive.writestr(info, value)
    write_json(output / 'manifest.json', dict(document=target.name, documentSha256=digest(target),
               sourceSha256=digest(source), dpi=96, pageSizePx=dict(width=PAGE[0], height=PAGE[1]),
               layoutFramePx=dict(x=0, y=0, width=PAGE[0], height=PAGE[1]),
               comparisonRegionPx=dict(x=0, y=0, width=PAGE[0], height=PAGE[1]), cases=rows))
    print(json.dumps(dict(document=str(target), cases=len(rows))))


def font(size, bold=False):
    for path in ['/System/Library/Fonts/Helvetica.ttc', '/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf']:
        if Path(path).exists():
            return ImageFont.truetype(path, size, index=1 if bold and path.endswith('.ttc') else 0)
    return ImageFont.load_default(size=size)


def draw_gallery(rows, title, path, native_version):
    panel_w, panel_h, gap, margin = 276, 240, 16, 24
    card_w, card_h, header = panel_w * 2 + gap, 340, 110
    canvas = Image.new('RGB', (margin * 3 + card_w * 2, header + math.ceil(len(rows) / 2) * card_h + 58), '#edf2f5')
    draw = ImageDraw.Draw(canvas)
    dark, muted = '#244358', '#516d7e'
    draw.text((margin, 18), title, font=font(28, True), fill=dark)
    draw.text((margin, 56), f'Word {native_version} print PDF  /  Rust Office Art + DOCX adapter  /  November 6, 2026', font=font(16), fill=muted)
    draw.text((margin, 80), 'Same crop and scale; native print-PDF page offset retained. See comparison method.', font=font(15), fill=muted)
    for i, row in enumerate(rows):
        x, y = margin + i % 2 * (card_w + margin), header + i // 2 * card_h
        title = row['title']
        while draw.textlength(title, font=font(19, True)) > card_w:
            title = title[:-2].rstrip('…') + '…'
        draw.text((x, y), title, font=font(19, True), fill=dark)
        for offset, source, label in [(0, row['referencePath'], 'MICROSOFT WORD'), (panel_w + gap, row['candidatePath'], 'RUST OFFICE ART')]:
            draw.text((x + offset, y + 28), label, font=font(12, True), fill=muted)
            box = (x + offset, y + 48, x + offset + panel_w, y + 48 + panel_h)
            draw.rectangle(box, fill='white', outline='#d6e0e7')
            image = Image.open(source).convert('RGB').crop(row['cropPx'])
            image.thumbnail((panel_w - 2, panel_h - 2), Image.Resampling.LANCZOS)
            canvas.paste(image, (x + offset + (panel_w - image.width) // 2, y + 48 + (panel_h - image.height) // 2))
        color = row['colorAgreementWithin16OnInkUnion']
        score = f'Color match {color:.1%}' if color is not None else 'Blank on both sides'
        draw.text((x, y + 298), f'{row["family"]}  ·  {score}', font=font(14), fill=muted)
        draw.text((x, y + 318), row['mode'], font=font(13), fill='#885519' if row.get('limitation') else muted)
    draw.text((margin, canvas.height - 42), '3D models use saved previews. WordArt and shapes use generated geometry; pencil uses procedural grain.', font=font(15), fill=muted)
    draw.text((margin, canvas.height - 21), 'Scores use original 96 dpi pages and visible ink; thumbnails are scaled for display only.', font=font(14), fill=muted)
    canvas.save(path)


def gallery(directory, candidate, model, output):
    manifest = json.loads((directory / 'manifest.json').read_text())
    reference = json.loads((directory / 'word-cairo-reference.json').read_text())
    assert digest(directory / manifest['document']) == manifest['documentSha256'] == reference['documentSha256']
    assert digest(directory / 'word-reference.pdf') == reference['pdfSha256']
    references = {int(Path(p['path']).stem.rsplit('-', 1)[1]): p for p in reference['pages']}
    rows = []
    limitations = {
        'WordArt': 'Shaped glyph outlines; substitute font metrics',
        '3D shapes': 'Projected geometry, extrusion and material lighting',
        'Ink': 'InkML brush geometry and pressure',
        'Artistic effects': 'Unchanged cache; regeneration is unmeasured',
    }
    titles = {15: 'SmartArt · cycle matrix', 17: 'SmartArt · funnel', 18: 'WordArt · outline and shadow',
              19: 'WordArt · plain text', 20: 'WordArt · solid fill', 21: 'WordArt · outline only',
              22: 'WordArt · shadow', 23: 'WordArt · wave deformation', 24: 'WordArt · arch deformation',
              25: 'WordArt · gradient fill', 26: 'WordArt · 3D text', 27: '3D shape · isometric extrusion',
              28: '3D shape · metallic material', 29: '3D shape · rounded bevel', 30: 'Artistic effect · saved cache control',
              5: 'Ink · pencil', 6: 'Ink · highlighter', 7: 'Ink · pen'}
    for case in manifest['cases']:
        page = case['page']
        native = directory / references[page]['path']
        actual = candidate / f'page-{page:02d}.png'
        assert digest(native) == references[page]['sha256']
        expected_image, actual_image = (Image.open(p).convert('RGB') for p in [native, actual])
        assert expected_image.size == actual_image.size == PAGE
        row = dict(case, title=titles.get(page, case['title']), referencePath=str(native),
                   referenceSha256=digest(native), candidatePath=str(actual), candidateSha256=digest(actual),
                   **metrics(expected_image, actual_image))
        if row['family'] in limitations:
            row['limitation'] = limitations[row['family']]
            row['mode'] = row['limitation']
        if page in (23, 24):
            row['mode'] = 'Glyph outlines mapped through preset warp geometry'
        elif page == 26:
            row['mode'] = 'Glyph outlines with projected extrusion and lighting'
        baseline = directory / 'parser-before' / actual.name
        if baseline.exists():
            row['beforeColorAgreement'] = metrics(expected_image, Image.open(baseline).convert('RGB'))['colorAgreementWithin16OnInkUnion']
            row['beforeSha256'] = digest(baseline)
        row['cropPx'] = list(case['cropPx'])
        if page == 15:
            row['cropPx'][2] = min(PAGE[0], row['cropPx'][2] + 100)
            row['cropPx'][3] = min(PAGE[1], row['cropPx'][3] + 100)
        rows.append(row)
    groups = {
        'overview': ('Office Art · native Word comparisons', [1, 4, 8, 12, 13, 14, 15, 17, 18, 20, 21, 25, 23, 27, 7, 30]),
        'models-and-smartart': ('Office Art · 3D models and SmartArt', [4, 8, 12, 13, 14, 15, 17]),
        'wordart-and-3d': ('Office Art · WordArt and 3D shape fidelity', list(range(18, 30))),
        'icons-shapes-and-ink': ('Office Art · icons, shapes, ink and cached effects', [1, 2, 3, 5, 6, 7, 9, 10, 11, 16, 30]),
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    galleries = []
    for key, (title, pages) in groups.items():
        path = output.with_name(output.name + '-' + key + '.png')
        draw_gallery([rows[p - 1] for p in pages], title, path, reference['wordVersion'])
        galleries.append(dict(path=str(path), sha256=digest(path), pages=pages))
    columns = ['page', 'id', 'title', 'family', 'mode', 'beforeColorAgreement',
               'colorAgreementWithin16OnInkUnion', 'inkIntersectionOverUnion', 'meanAbsoluteChannelError',
               'referenceInkPixels', 'candidateInkPixels', 'referenceSha256', 'candidateSha256']
    with output.with_name(output.name + '-comparison.csv').open('w', newline='') as f:
        writer = csv.DictWriter(f, fieldnames=columns, extrasaction='ignore')
        writer.writeheader()
        writer.writerows(rows)
    root = Path(__file__).resolve().parents[2]
    sources = [p for package in ['rust-docx', 'rust-office-art'] for p in (root / 'crates' / package / 'src').rglob('*.rs')]
    sources += [p for p in (root / 'crates/rust-office-art/src').rglob('*') if p.is_file() and p.suffix != '.rs']
    sources += [root / 'Cargo.lock', root / 'crates/rust-office-art/Cargo.toml', root / 'crates/rust-docx/Cargo.toml']
    source_hashes = {str(p.relative_to(root)): digest(p) for p in sorted(sources)}
    provenance = dict(date='2026-11-06', pipeline='Native rust-docx model using rust-office-art; generated SVGs and saved raster assets displayed by Chromium',
                      documentSha256=manifest['documentSha256'], sourceSha256=manifest['sourceSha256'],
                      modelSha256=digest(model), nativeReference=reference, galleries=galleries,
                      baselineModelSha256=digest(directory / 'model-before.json') if (directory / 'model-before.json').exists() else None,
                      generatorSha256=digest(Path(__file__)), manifestSha256=digest(directory / 'manifest.json'),
                      browserReceipt=(directory / 'browser-version.log').read_text(),
                      gitRevision=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
                      workingTreeModified=bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip()),
                      sourceHashes=source_hashes, measurement='RGB within 16 levels over the union of visible ink on full 816 × 1056 pages; no per-image alignment',
                      candidatePageSizePx=PAGE, caseCount=len(rows), cases=rows)
    write_json(output.with_name(output.name + '-provenance.json'), provenance)
    print(json.dumps([dict(page=row['page'], family=row['family'], color=row['colorAgreementWithin16OnInkUnion'], before=row.get('beforeColorAgreement')) for row in rows], indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path)
    parser.add_argument('--directory', type=Path)
    parser.add_argument('--candidate', type=Path)
    parser.add_argument('--model', type=Path)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--record-reference', action='store_true')
    parser.add_argument('--word-version')
    parser.add_argument('--export-method')
    args = parser.parse_args()
    if args.record_reference and args.directory and args.word_version and args.export_method:
        record_reference(args.directory, args.word_version, args.export_method)
    elif args.source and args.output:
        generate(args.source, args.output)
    elif args.directory and args.candidate and args.model and args.output:
        gallery(args.directory, args.candidate, args.model, args.output)
    else:
        parser.error('Supply --source and --output to generate; --record-reference, --directory, --word-version and --export-method to record; or --directory, --candidate, --model and --output to compare')

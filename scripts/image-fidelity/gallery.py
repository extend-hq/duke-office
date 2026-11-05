"""Build a fixed-coordinate native and browser OfficeArt comparison gallery."""

import argparse
import hashlib
import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

from compare import metrics


CASES = [
    ('pictures', 'plain', 'Baseline image', ''),
    ('pictures', 'rotate-30', 'Rotation · 30°', ''),
    ('pictures', 'crop-positive', 'Positive crop', ''),
    ('pictures', 'crop-negative', 'Negative crop', ''),
    ('pictures', 'ellipse', 'Ellipse clipping', ''),
    ('pictures', 'border-dashed', 'Dashed outside border', 'Native dash spacing and outward border mask.'),
    ('pictures', 'tile', 'Tiled image', 'Embedded image density determines tile size.'),
    ('pictures', 'alpha-inverse', 'Alpha inversion', 'Default replacement color is black.'),
    ('pictures', 'hsl', 'HSL adjustment', ''),
    ('pictures', 'tint', 'Tint', ''),
    ('pictures', 'outer-shadow', 'Outer shadow', ''),
    ('pictures', 'glow', 'Glow', ''),
    ('pictures', 'soft-edge', 'Soft edge', 'Edge kernel remains an approximation.'),
    ('groups', 'nested-nonuniform-picture-rotation', 'Nested scale and rotation', 'Nonuniform parent scale; combined rotation.'),
    ('pictures', 'blur', 'Blur · native blank output', 'Native blank output remains in the comparison.'),
    ('pictures', 'svg-missing-alternative', 'Unavailable SVG alternative', 'Intentional missing relationship; fallback differs.'),
]

SHAPE_CASES = [
    ('shapes', 'geometry/roundRect', 'Rounded rectangle', ''),
    ('shapes', 'geometry/star5', 'Five-point star', ''),
    ('shapes', 'geometry/donut', 'Donut', ''),
    ('shapes', 'geometry/blockArc', 'Block arc', ''),
    ('shapes', 'geometry/wedgeRoundRectCallout', 'Rounded callout', ''),
    ('shapes', 'geometry/custom-numeric-lines', 'Custom line geometry', ''),
    ('shapes', 'geometry/custom-cubic-outside-frame', 'Cubic path beyond its frame', ''),
    ('shapes', 'geometry/custom-arc', 'Custom arc', ''),
    ('shapes', 'geometry/custom-formula', 'Custom formula and coordinate units', 'Thin-stroke disagreement remains visible.'),
    ('shapes', 'fills-outlines/arrow-triangle', 'Arrow ends', 'Thin-stroke antialiasing differs.'),
    ('shapes', 'geometry/bentConnector3', 'Bent connector', ''),
    ('shapes', 'geometry/curvedConnector3', 'Curved connector', 'Matching vector path; antialiasing differs.'),
]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def font(size, bold=False):
    candidates = [
        '/System/Library/Fonts/Supplemental/Arial Bold.ttf' if bold else '/System/Library/Fonts/Supplemental/Arial.ttf',
        '/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf' if bold else '/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf',
    ]
    for path in candidates:
        if Path(path).exists():
            return ImageFont.truetype(path, size)
    return ImageFont.load_default()


def source(directory, candidate_directory, capture_proof, runtime_version, runtime_source):
    manifest = json.loads((directory / 'manifest.json').read_text())
    receipt_path = directory / 'word-cairo-reference.json'
    receipt = json.loads(receipt_path.read_text())
    if manifest['documentSha256'] != receipt['documentSha256']:
        raise ValueError('Fixture and native reference identify different input bytes')
    if digest(directory / manifest['document']) != manifest['documentSha256']:
        raise ValueError('Fixture digest mismatch')
    if digest(directory / 'word-reference.pdf') != receipt['pdfSha256']:
        raise ValueError('Native PDF digest mismatch')
    references = {}
    for page in receipt['pages']:
        path = directory / page['path']
        if digest(path) != page['sha256']:
            raise ValueError('Native raster digest mismatch')
        references[int(path.stem.rsplit('-', 1)[1])] = path
    proof = json.loads(capture_proof.read_text())
    if proof['inputSha256'] != manifest['documentSha256']:
        raise ValueError('Browser capture identifies a different input')
    build = proof['buildInfo']
    if build['rustDocxWasmVersion'] != runtime_version or build['source'] != runtime_source:
        raise ValueError('Browser runtime does not match the gallery label')
    if proof['wasmReceipt']['sha256'] != build['wasmSha256']:
        raise ValueError('Served browser runtime does not match its build receipt')
    captured = {str((directory / row['path']).resolve()): row['sha256'] for row in proof['pages']}
    for case in manifest['cases']:
        candidate = (candidate_directory / f'page-{case["page"]:02d}.png').resolve()
        if captured.get(str(candidate)) != digest(candidate):
            raise ValueError('Candidate page does not match the browser capture receipt')
    return dict(directory=directory, candidate_directory=candidate_directory, manifest=manifest,
                receipt=receipt, receipt_path=receipt_path, references=references,
                capture_proof=capture_proof, proof=proof)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kind', choices=('pictures', 'shapes'), default='pictures')
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--group-directory', type=Path)
    parser.add_argument('--viewer-directory', type=Path, required=True)
    parser.add_argument('--group-viewer-directory', type=Path)
    parser.add_argument('--runtime-version', required=True)
    parser.add_argument('--runtime-source', choices=('registry', 'workspace'), default='registry')
    parser.add_argument('--capture-proof', type=Path, required=True)
    parser.add_argument('--group-capture-proof', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--provenance', type=Path, required=True)
    args = parser.parse_args()
    if args.kind == 'pictures':
        if not args.group_directory or not args.group_viewer_directory or not args.group_capture_proof:
            parser.error('Picture galleries require both group directories and a group capture proof')
        sources = {'pictures': source(args.directory, args.viewer_directory, args.capture_proof, args.runtime_version, args.runtime_source),
                   'groups': source(args.group_directory, args.group_viewer_directory, args.group_capture_proof, args.runtime_version, args.runtime_source)}
        selected = CASES
        title = 'OfficeArt picture rendering'
        denominator = 'Full reports retain all 40 picture cases and 5 group cases.'
    else:
        sources = {'shapes': source(args.directory, args.viewer_directory, args.capture_proof, args.runtime_version, args.runtime_source)}
        selected = SHAPE_CASES
        title = 'OfficeArt shapes and connectors'
        denominator = 'The full report retains all 16 shape and connector cases.'
    margin, gap, panel, panel_gap = 24, 24, 280, 12
    card_width, card_height, header, footer = panel * 2 + panel_gap, 380, 112, 76
    width = margin * 2 + card_width * 2 + gap
    rows = (len(selected) + 1) // 2
    canvas = Image.new('RGB', (width, header + rows * card_height + footer), '#edf2f5')
    draw = ImageDraw.Draw(canvas)
    dark, muted = '#244358', '#516d7e'
    draw.text((margin, 18), f'Word and react-docx  /  {title}', font=font(29, True), fill=dark)
    word_version = sources[args.kind]['receipt']['wordVersion']
    runtime = 'published parser' if args.runtime_source == 'registry' else 'workspace parser'
    draw.text((margin, 57), f'Word {word_version} PDF → Cairo 96 dpi  ·  React uses {runtime} {args.runtime_version}', font=font(17), fill=muted)
    draw.text((margin, 82), 'Same page coordinates and crop on both sides. Known outliers are retained.', font=font(16), fill=muted)
    measurements = []
    for index, (kind, identifier, title, note) in enumerate(selected):
        item = sources[kind]
        case = next(row for row in item['manifest']['cases'] if row['id'] == identifier)
        page = case['page']
        reference_path = item['references'][page]
        candidate_path = item['candidate_directory'] / f'page-{page:02d}.png'
        expected_size = tuple(item['manifest']['pageSizePx'][key] for key in ('width', 'height'))
        reference, candidate = (Image.open(path).convert('RGB') for path in (reference_path, candidate_path))
        if reference.size != expected_size or candidate.size != expected_size:
            raise ValueError('Page dimensions differ from the fixed native fixture')
        bounds = item['manifest']['comparisonRegionPx']
        region = (bounds['x'], bounds['y'], bounds['x'] + bounds['width'], bounds['y'] + bounds['height'])
        reference, candidate = reference.crop(region), candidate.crop(region)
        values = metrics(reference, candidate)
        x = margin + (index % 2) * (card_width + gap)
        y = header + (index // 2) * card_height
        draw.text((x, y), title, font=font(21, True), fill=dark)
        draw.text((x, y + 30), 'WORD · CAIRO', font=font(12, True), fill=muted)
        draw.text((x + panel + panel_gap, y + 30), 'REACT-DOCX', font=font(12, True), fill=muted)
        for offset, image in [(0, reference), (panel + panel_gap, candidate)]:
            left, top = x + offset, y + 50
            canvas.paste(image.resize((panel, panel), Image.Resampling.LANCZOS), (left, top))
            draw.rectangle((left, top, left + panel, top + panel), outline='#d6e0e7', width=1)
        color, ink = values['colorAgreementWithin16OnInkUnion'], values['inkIntersectionOverUnion']
        score = 'No native ink' if not values['referenceInkPixels'] else f'Color agreement {color:.1%}  ·  Ink overlap {ink:.1%}'
        draw.text((x, y + 337), score, font=font(14), fill=muted)
        if note:
            draw.text((x, y + 357), note, font=font(13), fill='#885519' if identifier in ('blur', 'svg-missing-alternative') else muted)
        measurements.append(dict(id=identifier, group=kind, page=page, title=title, note=note,
                                 reference=str(reference_path), referenceSha256=digest(reference_path),
                                 candidate=str(candidate_path), candidateSha256=digest(candidate_path), **values))
    end = header + rows * card_height + 8
    draw.text((margin, end), f'Representative selection: {len(selected)} cases. {denominator}', font=font(15), fill=muted)
    draw.text((margin, end + 24), 'Color agreement uses a 16-level RGB tolerance over visible ink. Gallery scaling is for display only.', font=font(15), fill=muted)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    canvas.save(args.output)
    provenance = dict(runtimeVersion=args.runtime_version, runtimeSource=args.runtime_source, kind=args.kind, captureProof=str(args.capture_proof),
                      captureProofSha256=digest(args.capture_proof), gallery=str(args.output), gallerySha256=digest(args.output),
                      referenceBackend='Poppler Cairo, 96 dpi', selectionCount=len(measurements),
                      sourceCaseCount={key: len(value['manifest']['cases']) for key, value in sources.items()},
                      referenceReceipts={key: dict(path=str(value['receipt_path']), sha256=digest(value['receipt_path'])) for key, value in sources.items()},
                      captureReceipts={key: dict(path=str(value['capture_proof']), sha256=digest(value['capture_proof']),
                                                 buildInfo=value['proof']['buildInfo'], viewerSources=value['proof']['viewerSources'],
                                                 browserReceipt=value['proof'].get('browserReceipt'),
                                                 viewerSourceTreeSha256=value['proof'].get('viewerSourceTreeSha256')) for key, value in sources.items()},
                      displayScale=panel / 336, perImageAlignment=False, cases=measurements)
    args.provenance.parent.mkdir(parents=True, exist_ok=True)
    args.provenance.write_text(json.dumps(provenance, indent=2) + '\n')
    print(json.dumps(dict(output=str(args.output), cases=len(measurements), dimensions=list(canvas.size))))


if __name__ == '__main__':
    main()

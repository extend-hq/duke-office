"""Compare full-page candidate renders against fixed native reference regions."""

import argparse
import hashlib
from html import escape
import json
from pathlib import Path

from PIL import Image, ImageChops, ImageEnhance


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def metrics(reference, candidate):
    sums = 0
    union = intersection = color = reference_ink = candidate_ink = 0
    maximum = 0
    expected_pixels = reference.get_flattened_data() if hasattr(reference, 'get_flattened_data') else reference.getdata()
    actual_pixels = candidate.get_flattened_data() if hasattr(candidate, 'get_flattened_data') else candidate.getdata()
    for expected, actual in zip(expected_pixels, actual_pixels):
        delta = [abs(a - b) for a, b in zip(expected, actual)]
        sums += sum(delta)
        maximum = max(maximum, *delta)
        expected_ink = min(expected) < 247
        actual_ink = min(actual) < 247
        reference_ink += expected_ink
        candidate_ink += actual_ink
        if expected_ink or actual_ink:
            union += 1
            intersection += expected_ink and actual_ink
            color += max(delta) <= 16
    return dict(
        meanAbsoluteChannelError=sums / (reference.width * reference.height * 3),
        maximumChannelError=maximum,
        inkIntersectionOverUnion=intersection / union if union else None,
        colorAgreementWithin16OnInkUnion=color / union if union else None,
        referenceInkPixels=reference_ink,
        candidateInkPixels=candidate_ink,
        inkUnionPixels=union,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference-directory', type=Path, required=True)
    parser.add_argument('--candidate-directory', type=Path, required=True)
    parser.add_argument('--candidate-pattern', default='page-{page:02d}.png')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--label', default='Candidate')
    args = parser.parse_args()
    reference_dir = args.reference_directory.resolve()
    candidate_dir = args.candidate_directory.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    manifest = json.loads((reference_dir / 'manifest.json').read_text())
    receipt = json.loads((reference_dir / 'word-reference.json').read_text())
    if manifest['documentSha256'] != receipt['documentSha256']:
        raise ValueError('Native receipt and fixture manifest identify different input bytes')
    bounds = manifest['comparisonRegionPx']
    region = (bounds['x'], bounds['y'], bounds['x'] + bounds['width'], bounds['y'] + bounds['height'])
    expected_size = (manifest['pageSizePx']['width'], manifest['pageSizePx']['height'])
    references = {row['path']: row['sha256'] for row in receipt['pages']}
    rows, cards = [], []
    for case in manifest['cases']:
        name, page = case['id'], case['page']
        ref_path = reference_dir / f'word-pages/page-{page:02d}.png'
        relative = str(ref_path.relative_to(reference_dir))
        if digest(ref_path) != references[relative]:
            raise ValueError(f'Native reference digest mismatch at page {page}')
        reference = Image.open(ref_path).convert('RGB')
        if reference.size != expected_size:
            raise ValueError(f'Unexpected native reference size at page {page}: {reference.size}')
        candidate_path = candidate_dir / args.candidate_pattern.format(page=page, id=name)
        row = dict(id=name, page=page, reference=relative, candidate=str(candidate_path), referenceSha256=references[relative])
        card = f'<article><h2>{page:02d} {escape(name)}</h2>'
        candidate = None
        if not candidate_path.is_file():
            row['status'] = 'missing-candidate'
        else:
            candidate = Image.open(candidate_path).convert('RGB')
            row['candidateSha256'] = digest(candidate_path)
            if candidate.size != expected_size:
                row.update(status='size-mismatch', candidateSize=list(candidate.size))
                candidate = None
            else:
                row.update(metrics(reference.crop(region), candidate.crop(region)))
                row['status'] = 'blank-native-reference' if not row['referenceInkPixels'] else ('blank-candidate' if not row['candidateInkPixels'] else 'compared')
        ref_crop = output / f'{page:02d}-word.png'
        reference.crop(region).save(ref_crop)
        card += f'<div class="images"><figure><img src="{ref_crop.name}" alt="Word"/><figcaption>Word</figcaption></figure>'
        if candidate is not None:
            candidate_crop = output / f'{page:02d}-candidate.png'
            candidate.crop(region).save(candidate_crop)
            diff = ImageChops.difference(reference.crop(region), candidate.crop(region))
            diff_path = output / f'{page:02d}-diff.png'
            ImageEnhance.Brightness(diff).enhance(4).save(diff_path)
            card += f'<figure><img src="{candidate_crop.name}" alt="{escape(args.label)}"/><figcaption>{escape(args.label)}</figcaption></figure><figure><img src="{diff_path.name}" alt="Difference"/><figcaption>Difference ×4</figcaption></figure>'
        card += f'</div><p>{escape(row["status"])}'
        if 'meanAbsoluteChannelError' in row:
            ink = row['inkIntersectionOverUnion']
            color = row['colorAgreementWithin16OnInkUnion']
            card += f' · MAE {row["meanAbsoluteChannelError"]:.3f}'
            if ink is not None:
                card += f' · ink overlap {ink:.2%} · color agreement {color:.2%}'
        cards.append(card + '</p></article>')
        rows.append(row)
    result = dict(label=args.label, inputSha256=manifest['documentSha256'], nativePdfSha256=receipt['pdfSha256'], wordVersion=receipt['wordVersion'], dpi=manifest['dpi'], regionPx=bounds, caseCount=len(rows), statuses={status: sum(row['status'] == status for row in rows) for status in sorted({row['status'] for row in rows})}, cases=rows)
    (output / 'comparison.json').write_text(json.dumps(result, indent=2) + '\n')
    html = '<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Word picture appearance comparison</title><style>body{font:14px system-ui;margin:24px;background:#f5f5f3;color:#19212b}h1{font-size:28px}article{background:white;padding:18px;margin:16px 0;border:1px solid #ddd;border-radius:8px}h2{font-size:17px;margin:0 0 12px}.images{display:flex;gap:12px;flex-wrap:wrap}figure{margin:0}img{width:336px;height:336px;border:1px solid #eee}figcaption{font-size:12px;color:#59616a;margin-top:5px}p{max-width:1000px;line-height:1.5}</style>'
    html += f'<h1>Word vs {escape(args.label)}</h1><p>Word {escape(receipt["wordVersion"])} · {len(rows)} cases · 96 dpi. Fixed page coordinates are preserved. No per-image resizing or alignment correction is applied. Ink overlap uses non-white pixels; color agreement uses the same union with a 16-level channel tolerance. Blank native output is reported separately and is not evidence of successful image support. The unavailable SVG alternative is a deliberate robustness case.</p>'
    html += ''.join(cards) + '</html>'
    (output / 'report.html').write_text(html)
    print(json.dumps({key: result[key] for key in ('label', 'caseCount', 'statuses')}))


if __name__ == '__main__':
    main()

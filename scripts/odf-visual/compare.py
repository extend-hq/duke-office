"""Measure fixed full-page captures using the Office Art ink-union metric."""
import argparse
import hashlib
import importlib.util
import json
from html import escape
from pathlib import Path
import statistics

from PIL import Image, ImageChops


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('root', type=Path)
    p.add_argument('--baseline', type=Path, required=True)
    p.add_argument('--candidate', type=Path, required=True)
    p.add_argument('--baseline-model', type=Path, required=True)
    p.add_argument('--candidate-model', type=Path, required=True)
    p.add_argument('--browser-version', required=True)
    p.add_argument('--reference-version', required=True)
    args = p.parse_args()
    root = args.root.resolve()
    manifest = json.loads((root / 'manifest.json').read_text())
    for name, digest in manifest['inputs'].items():
        if sha(root / name) != digest:
            raise ValueError(f'Input changed: {name}')
    size = tuple(manifest['pageSizePx'][k] for k in ('width', 'height'))
    receipt = dict(inputSha256=manifest['inputs']['visual.odp'], referenceVersion=args.reference_version,
                   pdfSha256=sha(root / 'reference/visual.pdf'), dpi=manifest['dpi'], pages=[])
    for case in manifest['cases']:
        path = root / f'reference/page-{case["page"]:02d}.png'
        if Image.open(path).size != size:
            raise ValueError(f'Reference dimensions do not match the manifest: {path}')
        receipt['pages'].append(dict(path=str(path.relative_to(root)), sha256=sha(path)))
    receipt_path = root / 'reference-receipt.json'
    if receipt_path.exists() and json.loads(receipt_path.read_text()) != receipt:
        raise ValueError('Sealed native reference changed')
    receipt_path.write_text(json.dumps(receipt, indent=2))
    spec = importlib.util.spec_from_file_location('office_metrics', Path(__file__).resolve().parents[1] / 'image-fidelity/compare.py')
    metrics = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(metrics)
    report_dir = root / 'report'
    report_dir.mkdir(exist_ok=True)
    rows, cards = [], []
    for case in manifest['cases']:
        page = case['page']
        ref = Image.open(root / f'reference/page-{page:02d}.png').convert('RGB')
        ref.save(report_dir / f'{page:02d}-reference.png')
        row = dict(case)
        cards.append(f'<article><h2>{page:02d} {escape(case["id"])}</h2><div class="images"><figure><img src="{page:02d}-reference.png"><figcaption>LibreOffice</figcaption></figure>')
        for version, directory in [('baseline', args.baseline), ('candidate', args.candidate)]:
            path = directory / f'page-{page:02d}.png'
            result = dict(status='missing', colorAgreementWithin16OnInkUnion=0.0, inkIntersectionOverUnion=0.0)
            if path.exists():
                im = Image.open(path).convert('RGB')
                result['sha256'] = sha(path)
                if im.size != size:
                    result.update(status='size-mismatch', actualSize=list(im.size))
                else:
                    result.update(metrics.metrics(ref, im))
                    result['status'] = 'compared' if result['candidateInkPixels'] else 'blank'
                    if not result['referenceInkPixels']:
                        result['status'] = 'blank-reference'
                    im.save(report_dir / f'{page:02d}-{version}.png')
                    if version == 'candidate':
                        ImageChops.difference(ref, im).save(report_dir / f'{page:02d}-difference.png')
                    score = result['colorAgreementWithin16OnInkUnion']
                    label = f'{score:.1%}' if score is not None else 'unscored'
                    cards.append(f'<figure><img src="{page:02d}-{version}.png"><figcaption>{version}: {label}</figcaption></figure>')
            row[version] = result
        rows.append(row)
        cards.append(f'<figure><img src="{page:02d}-difference.png"><figcaption>Absolute difference</figcaption></figure></div></article>')
    summary = {}
    for family in ('art', 'chart'):
        subset = [r for r in rows if r['family'] == family]
        if not subset:
            continue
        summary[family] = dict(cases=len(subset))
        for version in ('baseline', 'candidate'):
            values = [r[version]['colorAgreementWithin16OnInkUnion'] for r in subset]
            values = [v for v in values if v is not None]
            summary[family][version] = dict(median=statistics.median(values), mean=statistics.mean(values))
    result = dict(summary=summary, cases=rows, reference=receipt, browserVersion=args.browser_version,
                  deviceScale=1, baselineModelSha256=sha(args.baseline_model), candidateModelSha256=sha(args.candidate_model),
                  metric='RGB agreement within 16 over union of ink below 247, complete fixed pages; no alignment or resizing')
    (report_dir / 'report.json').write_text(json.dumps(result, indent=2))
    header = '<h1>OpenDocument visual comparison</h1><p>LibreOffice PDF versus native Rust models in the existing presentation viewer. Full 432 × 432 pages at 96 dpi; no per-object alignment or resizing. Color agreement uses the union of visible ink, so white margins cannot improve scores. This matrix is bounded feature coverage, not complete ODF parity.</p>'
    header += '<table><tr><th>Family<th>Cases<th>Before median<th>After median'
    for family, value in summary.items():
        header += f'<tr><td>{family}<td>{value["cases"]}<td>{value["baseline"]["median"]:.1%}<td>{value["candidate"]["median"]:.1%}'
    header += '</table>'
    (report_dir / 'index.html').write_text('<!doctype html><meta charset="utf-8"><title>OpenDocument visual comparison</title><style>body{font:15px system-ui;margin:32px;background:#f3f4f6;color:#15202b}p{max-width:1000px;line-height:1.5}table{border-collapse:collapse}td,th{padding:8px 24px;border-bottom:1px solid #ccd1d7;text-align:left}article{padding:16px;background:white;margin:20px 0;border:1px solid #d8dde3}.images{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:8px}figure{margin:0}img{width:100%;background:white;border:1px solid #e6e8eb}figcaption{padding:8px 0}h2{font-size:16px}</style>'+header+''.join(cards))
    print(json.dumps(summary, indent=2))

if __name__ == '__main__':
    main()

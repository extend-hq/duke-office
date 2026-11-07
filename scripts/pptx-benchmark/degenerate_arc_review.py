import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re
from xml.etree import ElementTree as ET

from artwork_math_gate import NUMBER
from artwork_bounds_review import at
from feedback import OUT


def reference_brackets(root):
    result = []
    for path in root.iter('{http://www.w3.org/2000/svg}path'):
        value = path.get('d', '')
        if not value.startswith('M') or re.search('[A-KN-Za-kn-z]', NUMBER.sub('', value)):
            continue
        values = list(map(float, NUMBER.findall(value)))
        if len(values) % 2:
            continue
        points = []
        for point in zip(values[::2], values[1::2]):
            if points and point == points[-1]:
                continue
            while len(points) > 1:
                a, b = points[-2:]
                if (b[0] - a[0]) * (point[1] - b[1]) != (b[1] - a[1]) * (point[0] - b[0]):
                    break
                if (b[0] - a[0]) * (point[0] - b[0]) + (b[1] - a[1]) * (point[1] - b[1]) < 0:
                    break
                points.pop()
            points.append(point)
        if len(points) != 4:
            continue
        a, b, c, d = points
        if a[1] == b[1] and b[0] == c[0] and c[1] == d[1] and d[0] == a[0] and b[0] > a[0] and c[1] > b[1]:
            result.append(dict(width=b[0] - a[0], height=c[1] - b[1], points=points, path=value))
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--reference-svg', type=Path, required=True)
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    rows = [json.loads(line) for line in (run / 'artwork-math.jsonl').read_text().splitlines()]
    reference = reference_brackets(ET.parse(args.reference_svg).getroot())
    results = []
    for row in rows:
        if not row['issues']:
            continue
        model = json.load(gzip.open(run / (row['id'] + '.after.json.gz'), 'rt'))
        for issue in row['issues']:
            node_path, sep, attribute_path = issue.partition('/shapeSvg/')
            record = dict(id=row['id'], issue=issue, passed=False)
            if sep and attribute_path.endswith('/@d'):
                node = at(model, node_path)
                source = ET.fromstring('<root xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">' + node['sourceXml'] + '</root>')[0]
                geometry = next((n for n in source.iter() if n.tag.rsplit('}', 1)[-1] == 'prstGeom'), None)
                guide = next((n for n in geometry.iter() if n.tag.rsplit('}', 1)[-1] == 'gd' and n.get('name') == 'adj'), None) if geometry is not None else None
                tokens = guide.get('fmla', '').split() if guide is not None else []
                svg = ET.fromstring(node['shapeSvg'])
                path = svg
                for index in attribute_path.split('/')[:-1]:
                    path = path[int(index)]
                value = path.get('d', '')
                numbers = list(map(float, NUMBER.findall(value)))
                width = node['paintBounds']['layoutWidthPx']
                height = node['paintBounds']['layoutHeightPx']
                expected = [0, 0, width, 0, 0, 0, 1, width, 0, width, height, width, 0, 0, 0, 1, 0, height]
                match = next((r for r in reference if abs(r['width'] - width * 2540 / 96) <= 1 and abs(r['height'] - height * 2540 / 96) <= 1), None)
                valid_source = geometry is not None and geometry.get('prst') == 'rightBracket' and len(tokens) == 2 and tokens[0] == 'val' and float(tokens[1]) == 0
                valid_path = re.sub(r'[^A-Za-z]', '', NUMBER.sub('', value)) == 'MALA' and len(numbers) == len(expected) and all(abs(a - b) <= 1e-10 for a, b in zip(numbers, expected))
                record.update(passed=valid_source and valid_path and match is not None, source_adjustment=tokens, dimensions_px=[width, height], candidate_path=value, reference=match)
            results.append(record)
    summary = dict(reviewed=len(results), passed=sum(r['passed'] for r in results), reference_svg_sha256=hashlib.sha256(args.reference_svg.read_bytes()).hexdigest(), reference_svg=str(args.reference_svg), method='Source declares a right bracket with zero corner adjustment. Candidate endpoints form a horizontal/vertical/horizontal polyline; independently exported LibreOffice SVG has the same topology and dimensions within its one-unit coordinate quantization. This is not Microsoft PowerPoint pixel fidelity.')
    (run / 'degenerate-arc-review.json').write_text(json.dumps(dict(summary=summary, rows=results), indent=2))
    print(json.dumps(summary))
    if summary['passed'] != summary['reviewed']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()

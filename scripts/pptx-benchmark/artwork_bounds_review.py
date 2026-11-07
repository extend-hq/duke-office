import argparse
import collections
import gzip
import hashlib
import json
import math

from feedback import OUT


def at(document, path):
    for key in path.split('/')[1:]:
        document = document[int(key)] if isinstance(document, list) else document[key]
    return document


def rectangle(bounds, layout):
    return [bounds['xPx'], bounds['yPx'], bounds['xPx'] + bounds['widthPx'], bounds['yPx'] + bounds['heightPx']] if bounds else [0., 0., *layout]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    source = run / 'artwork-math.jsonl'
    records = [json.loads(line) for line in source.read_text().splitlines()]
    counts = collections.Counter()
    results = []
    for record in records:
        changed = [r for r in record['changes']['paint_bounds'] if r['before'] != r['after']]
        if not changed:
            continue
        model = json.load(gzip.open(run / (record['id'] + '.compact.json.gz'), 'rt'))
        for change in changed:
            before, after = change['before'], change['after']
            node = at(model, change['path'].rsplit('/', 1)[0])
            exemplar = after or before
            layout = [exemplar['layoutWidthPx'], exemplar['layoutHeightPx']]
            a, b = rectangle(before, layout), rectangle(after, layout)
            finite = all(math.isfinite(v) for v in a + b + layout)
            same_layout = before is None or after is None or all(before[k] == after[k] for k in ['layoutWidthPx', 'layoutHeightPx'])
            tiny = all(abs(x - y) <= 1e-7 for x, y in zip(a, b))
            rounded = all(v.is_integer() for v in map(float, a + b)) and all(abs(x - y) <= 1 for x, y in zip(a, b))
            kind = 'subpixel' if tiny else 'integer_edge' if rounded else 'unclassified'
            passed = finite and same_layout and node.get('type') in {'shape', 'image'} and kind != 'unclassified' and b[2] > b[0] and b[3] > b[1]
            counts[kind] += 1
            results.append(dict(id=record['id'], path=change['path'], kind=kind, passed=passed, before=a, after=b, layout=layout, geometry=node.get('geometry'), node_type=node.get('type')))
    summary = dict(inputs=len(records), changed_bounds=len(results), passed=sum(r['passed'] for r in results), kinds=dict(counts), failures=[r for r in results if not r['passed']], records_sha256=hashlib.sha256(source.read_bytes()).hexdigest(), scope='Bound metadata review only: unchanged layout dimensions, finite positive bounds, subpixel differences or integer edges moving by at most one pixel. SVG numeric and browser regression receipts are separate; this is not native rendering fidelity.')
    (run / 'bounds-review.json').write_text(json.dumps(dict(summary=summary, rows=results), indent=2))
    print(json.dumps(summary))
    if summary['failures']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()

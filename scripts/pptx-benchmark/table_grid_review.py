import argparse
from collections import Counter
import copy
import hashlib
import json
from pathlib import Path
import re
import zipfile
from xml.etree import ElementTree as ET
from xml.parsers import expat

from feedback import OUT, invoke as compact
from object_gate import invoke

D = {'http://schemas.openxmlformats.org/drawingml/2006/main', 'http://purl.oclc.org/ooxml/drawingml/main'}
P = {'http://schemas.openxmlformats.org/presentationml/2006/main', 'http://purl.oclc.org/ooxml/presentationml/main'}
sha = lambda p: hashlib.sha256(Path(p).read_bytes()).hexdigest()
local = lambda n: n.tag.rsplit('}', 1)[-1]
yes = lambda v: v in ('true', '1')


def unmerge_xml(data):
    edits = []
    parser = expat.ParserCreate(namespace_separator='}')

    def start(name, attributes):
        if name not in {ns + '}tc' for ns in D}:
            return
        offset = parser.CurrentByteIndex
        end, quote = offset, None
        while end < len(data):
            char = data[end]
            if quote is not None:
                if char == quote:
                    quote = None
            elif char in (34, 39):
                quote = char
            elif char == 62:
                break
            end += 1
        tag = data[offset:end + 1]
        replacement = re.sub(rb'\s+(?:gridSpan|rowSpan|hMerge|vMerge)\s*=\s*(?:"[^"]*"|\x27[^\x27]*\x27)', b'', tag)
        if replacement != tag:
            edits.append((offset, end + 1, replacement))

    parser.StartElementHandler = start
    parser.Parse(data, True)
    for start, end, replacement in reversed(edits):
        data = data[:start] + replacement + data[end:]
    return data


def unmerge(source, destination):
    with zipfile.ZipFile(source) as original, zipfile.ZipFile(destination, 'w') as result:
        for info in original.infolist():
            data = original.read(info)
            if info.filename.endswith('.xml') and any(k in data for k in [b'gridSpan', b'rowSpan', b'hMerge', b'vMerge']):
                data = unmerge_xml(data)
            result.writestr(info, data)


def tables(document):
    result = []

    def walk(value):
        if isinstance(value, dict):
            if value.get('type') == 'table':
                result.append(value)
            for child in value.values():
                if isinstance(child, (dict, list)):
                    walk(child)
        elif isinstance(value, list):
            for child in value:
                if isinstance(child, (dict, list)):
                    walk(child)

    walk(document)
    return result


def without_rows(document):
    result = copy.deepcopy(document)
    for table in tables(result):
        table.pop('rows')
    return result


def verify(source, actual, control):
    counts = Counter()
    a, b = tables(actual), tables(control)
    assert len(a) == len(b)
    with zipfile.ZipFile(source) as archive:
        cache = {}
        for table, reference in zip(a, b):
            part = table['sourcePart']
            if part not in cache:
                cache[part] = ET.fromstring(archive.read(part))
            frame = next(n for n in cache[part].iter() if local(n) == 'graphicFrame' and any(local(p) == 'cNvPr' and p.get('id') == table['sourceId'] for p in n.iter()))
            source_table = next(n for n in frame.iter() if n.tag in {'{' + ns + '}tbl' for ns in D})
            grid = [[c for c in r if local(c) == 'tc'] for r in source_table if local(r) == 'tr']
            assert len(table['rows']) == len(reference['rows']) == len(grid)
            anchors = [(r, c, int(cell.get('rowSpan', '1')), int(cell.get('gridSpan', '1'))) for r, row in enumerate(grid) for c, cell in enumerate(row) if not (yes(cell.get('hMerge')) or yes(cell.get('vMerge')))]
            for r, row in enumerate(grid):
                assert len(row) == len(table['rows'][r]) == len(reference['rows'][r])
                for c, cell in enumerate(row):
                    observed = dict(table['rows'][r][c])
                    unmerged = dict(reference['rows'][r][c])
                    for key in ['rowSpan', 'colSpan']:
                        source_key = 'gridSpan' if key == 'colSpan' else key
                        span = int(cell.get(source_key, '1'))
                        assert observed.get(key) == (span if span > 1 else None)
                        observed.pop(key, None)
                        unmerged.pop(key, None)
                    h, v = yes(cell.get('hMerge')), yes(cell.get('vMerge'))
                    merge = observed.pop('merge', None)
                    if h or v:
                        assert merge and merge['horizontal'] == h and merge['vertical'] == v
                        candidates = [(ar, ac) for ar, ac, height, width in anchors if ar <= r < ar + height and ac <= c < ac + width and h == (c > ac) and v == (r > ar)]
                        if len(candidates) == 1:
                            ar, ac = candidates[0]
                            assert merge.get('origin') == dict(row=ar, column=ac), (part, r, c, merge, candidates)
                        elif not candidates:
                            assert 'origin' not in merge
                        else:
                            raise AssertionError('Ambiguous source merge region')
                        counts['covered_cells'] += 1
                        counts['covered_nonempty_cells'] += bool(''.join(n.text or '' for n in cell.iter() if local(n) == 't').strip())
                        counts['unresolved_cells'] += 'origin' not in merge
                    else:
                        assert merge is None
                    assert observed == unmerged, (part, r, c)
                    counts['cells'] += 1
            counts['tables'] += 1
    return counts


def review(run, manifest, records_path, name):
    baseline = OUT / 'experiments/modern-comments2/candidate'
    rows = json.loads(manifest.read_text())
    old = {r['id']: r for r in map(json.loads, records_path.read_text().splitlines())}
    destination = run / (name + '.jsonl')
    assert not destination.exists()
    records, totals, changed = [], Counter(), []
    fields = lambda r: (r['status'], r.get('full_sha256'), r.get('error'))
    with destination.open('w', buffering=1) as log:
        for index, row in enumerate(rows):
            assert row.get('split') != 'validation' and sha(row['path']) == row['sha256']
            record = compact(run / 'candidate', 'duke-pptx', row, run / (name + '.compact.json.gz'), 0)
            equal = fields(record) == fields(old[row['id']])
            if not equal:
                prior, before = invoke(baseline, row, run / (name + '.before.json.gz'))
                current, after = invoke(run / 'candidate', row, run / (name + '.after.json.gz'))
                assert fields(prior) == fields(old[row['id']]) and fields(current) == fields(record)
                assert prior['status'] == current['status'] == 'ok'
                assert without_rows(before) == without_rows(after)
                package = run / (name + '.unmerged.pptx')
                unmerge(row['path'], package)
                control_record, control = invoke(baseline, {**row, 'path': str(package)}, run / (name + '.control.json.gz'))
                assert control_record['status'] == 'ok'
                counts = verify(row['path'], after, control)
                assert counts['tables'] > 0
                record['source_counts'] = dict(counts)
                totals.update(counts)
                changed.append(row)
                print('reviewed', row['id'], dict(counts), flush=True)
            record.update(id=row['id'], equal=equal, passed=True)
            records.append(record)
            log.write(json.dumps(record) + '\n')
            if index % 50 == 0:
                print(index, len(rows), flush=True)
    summary = dict(total=len(records), passed=len(records), exact=sum(r['equal'] for r in records), changed=len(changed), source_counts=dict(totals), candidate_sha256=sha(run/'candidate'), baseline_sha256=sha(baseline), records_sha256=sha(destination), manifest_sha256=sha(manifest), reference_records_sha256=sha(records_path), harness_sha256=sha(__file__), scope='All fields and payloads outside table rows are exact. Changed table grids, source spans, merge flags and origins independently checked against source XML. Rich cell content and column styling compared with baseline on byte-preserving unmerged packages; this metamorphic reference does not establish native rendering parity.')
    (run / (name + '-summary.json')).write_text(json.dumps(summary, indent=2) + '\n')
    (run / (name + '-changed.json')).write_text(json.dumps(changed, indent=2) + '\n')
    for suffix in ['compact.json.gz', 'before.json.gz', 'after.json.gz', 'control.json.gz', 'unmerged.pptx']:
        (run / (name + '.' + suffix)).unlink(missing_ok=True)
    print(json.dumps(summary))


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--label', required=True)
    p.add_argument('--manifest', required=True, type=Path)
    p.add_argument('--reference-records', required=True, type=Path)
    p.add_argument('--name', default='table-reviewed')
    a = p.parse_args()
    run = OUT / 'experiments' / a.label
    assert sha(run / 'candidate') == json.loads((run / 'identity.json').read_text())['binary_sha256']
    review(run, a.manifest, a.reference_records, a.name)

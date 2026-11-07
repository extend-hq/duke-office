import argparse
import collections
import hashlib
import json
from pathlib import Path
import zipfile
from xml.parsers import expat

from artwork_math_gate import artwork_hashes
from feedback import OUT
from object_gate import changes, invoke


SVG = 'http://schemas.microsoft.com/office/drawing/2016/SVG/main}svgBlip'
DRAWING = ['http://schemas.openxmlformats.org/drawingml/2006/main}', 'http://purl.oclc.org/ooxml/drawingml/main}']
OLD_WARNING = 'Picture appearance is retained in source XML; the available asset and basic effects are used as fallback.'
NEW_WARNING = 'Preferred image appearance could not be generated; compatibility-image artwork is available.'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def without_svg(data):
    if b'svgBlip' not in data and b's\x00v\x00g\x00B\x00l\x00i\x00p' not in data and b'\x00s\x00v\x00g\x00B\x00l\x00i\x00p' not in data:
        return data, 0
    step = 2 if data.startswith((b'\xff\xfe', b'\xfe\xff', b'<\x00', b'\x00<')) else 1
    offset = 1 if data.startswith((b'\xfe\xff', b'\x00<')) else 0
    def tag_end(start):
        quote = None
        for index in range(start, len(data), step):
            char = data[index + offset]
            if quote is not None:
                if char == quote: quote = None
            elif char in (34, 39): quote = char
            elif char == 62: return index + step
        raise ValueError('Unterminated source tag')
    parser = expat.ParserCreate(namespace_separator='}')
    stack, removals = [], []
    def start(name, attributes):
        parent_names = [frame[0] for frame in stack[-3:]]
        selected = name == SVG and any(parent_names == [ns + 'blip', ns + 'extLst', ns + 'ext'] for ns in DRAWING)
        index = parser.CurrentByteIndex
        stack.append((name, index, tag_end(index), selected))
    def end(name):
        _, start_index, start_end, selected = stack.pop()
        if selected:
            end_index = parser.CurrentByteIndex
            removals.append((start_index, end_index if end_index == start_end else tag_end(end_index)))
    parser.StartElementHandler = start; parser.EndElementHandler = end
    parser.Parse(data, True)
    result = data
    for start_index, end_index in sorted(removals, reverse=True):
        result = result[:start_index] + result[end_index:]
    return result, len(removals)


def compatibility_package(source, output):
    counts = {}
    with zipfile.ZipFile(source) as before, zipfile.ZipFile(output, 'w') as after:
        for info in before.infolist():
            data = before.read(info.filename)
            if info.filename.endswith('.xml'):
                data, removed = without_svg(data)
                if removed: counts[info.filename] = removed
            after.writestr(info, data)
    return counts


def fallback_nodes(value, path=(), inherited_part=None):
    if isinstance(value, dict):
        part = value.get('sourcePart', inherited_part)
        if value.get('type') == 'image' and 'fallbackPicture' in value:
            yield path, value, part
        for key, child in value.items():
            if key not in {'data', 'bytes'}:
                yield from fallback_nodes(child, path + (key,), part)
    elif isinstance(value, list):
        for index, child in enumerate(value):
            yield from fallback_nodes(child, path + (index,), inherited_part)


def at(value, path):
    for part in path: value = value[part]
    return value


def restore_warning_messages(value, recovered_parts):
    changed = 0
    if isinstance(value, dict):
        if value.get('code') == 'unsupported-picture-appearance' and value.get('message') == NEW_WARNING:
            assert value.get('partName') in recovered_parts, value
            value['message'] = OLD_WARNING
            changed += 1
        for key, child in value.items():
            if key not in {'data', 'bytes'}:
                changed += restore_warning_messages(child, recovered_parts)
    elif isinstance(value, list):
        changed += sum(restore_warning_messages(child, recovered_parts) for child in value)
    return changed


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--label', required=True); parser.add_argument('--manifest', required=True, type=Path); args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    raw_proof = json.loads((run / 'parity-summary.json').read_text())
    assert digest(run / 'parity.jsonl') == raw_proof['records_sha256'] and digest(args.manifest) == raw_proof['manifest_sha256']
    candidate, baseline = run / 'candidate', Path(raw_proof['baseline_path'])
    assert digest(candidate) == raw_proof['candidate_sha256'] and digest(baseline) == raw_proof['baseline_sha256']
    manifest = {row['id']: row for row in json.loads(args.manifest.read_text())}
    raw = [json.loads(line) for line in (run / 'parity.jsonl').read_text().splitlines()]
    output = run / 'picture-fallback-reviewed.jsonl'; assert not output.exists()
    controls = run / 'compatibility-controls'; controls.mkdir()
    totals, reviewed = collections.Counter(), []
    with output.open('w', buffering=1) as stream:
        for index, record in enumerate(raw):
            row = manifest[record['id']]; assert digest(Path(row['path'])) == row['sha256']
            result = dict(record, passed=record['equal'])
            if not record['equal']:
                old, before = invoke(baseline, row, run / 'before.json.gz')
                current, after = invoke(candidate, row, run / 'after.json.gz')
                assert old['status'] == current['status'] == 'ok' and current['full_sha256'] == record['full_sha256']
                target = controls / (row['id'] + '.pptx')
                removed = compatibility_package(row['path'], target); assert removed
                control_row = dict(row, path=str(target), sha256=digest(target))
                control_record, control = invoke(baseline, control_row, run / 'control.json.gz')
                assert control_record['status'] == 'ok', control_record
                result['artwork_hashes'] = artwork_hashes(after)
                nodes = list(fallback_nodes(after)); assert nodes
                counts = collections.Counter(); recovered_parts = set()
                for path, node, part in nodes:
                    original = at(before, path); rendered = at(control, path)
                    assert node['fallbackAssetId'] == original['fallbackAssetId'] == rendered['assetId']
                    assert node['fallbackPicture']['svg'] == rendered['pictureSvg'], (row['id'], path)
                    assert node['fallbackPicture'].get('paintBounds') == rendered.get('paintBounds'), (row['id'], path)
                    counts['pictures'] += 1
                    counts['svg_bytes'] += len(node['fallbackPicture']['svg'].encode())
                    if not node.get('pictureSvg'):
                        recovered_parts.add(part); counts['primary_appearance_unavailable'] += 1
                    del node['fallbackPicture']
                counts['updated_warnings'] = restore_warning_messages(after, recovered_parts)
                result.update(source_counts=dict(counts), compatibility_sha256=control_row['sha256'], compatibility_output_sha256=control_record['full_sha256'], removed_extensions=removed, differences=changes(before, after))
                result['passed'] = not result['differences']
                totals.update(counts)
            reviewed.append(result); stream.write(json.dumps(result) + '\n')
            if index % 25 == 0 or not result['passed']:
                print(index, len(raw), result['passed'], flush=True)
    summary = dict(total=len(raw), passed=sum(r['passed'] for r in reviewed), exact=sum(r['equal'] for r in reviewed), source_counts=dict(totals), candidate_sha256=raw_proof['candidate_sha256'], baseline_sha256=raw_proof['baseline_sha256'], baseline_path=raw_proof['baseline_path'], records_sha256=digest(output), raw_records_sha256=raw_proof['records_sha256'], manifest_sha256=raw_proof['manifest_sha256'], harness_sha256=digest(Path(__file__)), scope='Each generated compatibility picture exactly matches the frozen baseline rendering of the same source package with only qualified SVG-extension elements removed. The original compatibility asset is selected through the unchanged main-blip relationship. Removing only fallbackPicture and restoring explicitly qualified recovery-warning wording leaves complete prior models, artwork, source XML and payload bytes exact. No native PowerPoint pixel-fidelity claim.')
    (run / 'picture-fallback-reviewed-summary.json').write_text(json.dumps(summary, indent=2)); print(json.dumps(summary)); assert summary['passed'] == summary['total']


if __name__ == '__main__':
    main()

import argparse
import hashlib
import io
import json
import zipfile

from artwork_math_gate import artwork_hashes
from feedback import OUT, invoke as compact
from fields_probes import package
from fields_review import verify_sources, visit
from object_gate import invoke


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--label', required=True); args = parser.parse_args()
    run = OUT / 'experiments' / args.label; target = run / 'boundary-probes'; target.mkdir()
    manifest, records, receipts = [], [], []
    for name in ['empty-note', 'missing-note', 'default-namespace', 'shadowed-namespace', 'depth-255', 'depth-256', 'depth-257']:
        path = (target / (name + '.pptx')).resolve(); package(path)
        with zipfile.ZipFile(io.BytesIO(path.read_bytes())) as archive:
            parts = {info.filename: archive.read(info) for info in archive.infolist()}
        part = 'ppt/notesSlides/notes.xml' if name.endswith('-note') else 'ppt/slides/slide1.xml'
        content = parts[part].decode()
        if name.endswith('-note'):
            content = content.replace('<a:t>retained</a:t>', '<a:t/>' if name == 'empty-note' else '')
        elif name == 'default-namespace':
            content = content.replace('<a:fld id="number" type="slidenum"><a:t>7</a:t></a:fld>', '<fld xmlns="http://schemas.openxmlformats.org/drawingml/2006/main" id="number" type="slidenum"><t>7</t></fld>')
        elif name == 'shadowed-namespace':
            content = content.replace('<a:fld id="number"', '<a:fld xmlns:a="urn:extension" id="number"')
        else:
            depth = int(name.rsplit('-', 1)[-1]); nested = depth - 8
            content = content.replace('<a:t>7</a:t>', '<e:item>' * nested + '<a:t>7</a:t>' + '</e:item>' * nested)
        parts[part] = content.encode()
        with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
            for part, content in parts.items():
                info = zipfile.ZipInfo(part, (2000, 1, 1, 0, 0, 0)); info.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(info, content)
        row = dict(id=name, path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest()); manifest.append(row)
        record, document = invoke(run / 'candidate', row, target / (name + '.full.json.gz'))
        counts = {}
        if name == 'depth-257':
            assert record['status'] == 'error' and record['error'] == 'resource limit exceeded: XML nesting depth', record
        else:
            assert record['status'] == 'ok', record
            counts = dict(verify_sources(path, document))
            fields = {run['field']['id']: run for _, run in visit(document)}
            if name.endswith('-note'):
                assert document['slides'][0]['notes'] and fields['note']['text'] == ''
                assert fields['note']['field']['hasCachedText'] == (name == 'empty-note')
            elif name == 'shadowed-namespace':
                assert 'number' not in fields
            elif name.startswith('depth-'):
                assert fields['number']['text'] == '' and fields['number']['field']['hasCachedText'] is False
            else:
                assert fields['number']['text'] == '7'
        result = compact(run / 'candidate', 'rust-pptx', row, target / (name + '.compact.json.gz'), 0)
        result.update(id=name)
        if document:
            result['artwork_hashes'] = artwork_hashes(document)
        records.append(result); receipts.append(dict(id=name, input_sha256=row['sha256'], expected_status=record['status'], counts=counts, passed=True))
        print(name, record['status'], flush=True)
    (target / 'manifest.json').write_text(json.dumps(manifest, indent=2))
    destination = target / 'records.jsonl'; destination.write_text(''.join(json.dumps(r) + '\n' for r in records))
    summary = dict(total=len(receipts), passed=len(receipts), receipts=receipts, records_sha256=hashlib.sha256(destination.read_bytes()).hexdigest(), candidate_sha256=hashlib.sha256((run / 'candidate').read_bytes()).hexdigest(), harness_sha256=hashlib.sha256(__import__('pathlib').Path(__file__).read_bytes()).hexdigest(), scope='Field-only notes with absent/empty cached values, default and shadowed namespaces, and XML nesting boundary before retained-source processing.')
    (run / 'fields-boundary-summary.json').write_text(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()

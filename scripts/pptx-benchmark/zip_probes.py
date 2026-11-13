import argparse
import hashlib
import io
import json
from pathlib import Path
import struct
import zipfile

from feedback import OUT, invoke


class StreamingBuffer(io.BytesIO):
    def seekable(self): return False
    def seek(self, *args): raise io.UnsupportedOperation('not seekable')


def package(compression, streamed=False, force_zip64=False, padding=0, empty=False):
    destination = StreamingBuffer() if streamed else io.BytesIO()
    content = {
        '[Content_Types].xml': '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>',
        'ppt/presentation.xml': '<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst></p:presentation>',
        'ppt/_rels/presentation.xml.rels': '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>',
        'ppt/slides/slide1.xml': '<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id="1" name="Text"/></p:nvSpPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>Archive integrity</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>' + ' ' * padding,
    }
    if empty:
        content['ppt/slides/_rels/slide1.xml.rels'] = ''
    with zipfile.ZipFile(destination, 'w', compression=compression) as archive:
        for name, text in content.items():
            info = zipfile.ZipInfo(name, date_time=(2020, 1, 1, 0, 0, 0))
            info.compress_type = compression
            with archive.open(info, 'w', force_zip64=force_zip64) as stream:
                stream.write(text.encode())
    return destination.getvalue()


def mutate(data, field, change):
    result = bytearray(data)
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        offset = archive.getinfo('ppt/slides/slide1.xml').header_offset
        central = archive.start_dir
    while result[central:central + 4] == b'PK\x01\x02':
        name_len, extra_len, comment_len = struct.unpack_from('<HHH', result, central + 28)
        if struct.unpack_from('<I', result, central + 42)[0] == offset:
            locations = {'crc': (14, 16, 'I'), 'size': (22, 24, 'I'), 'compressed_size': (18, 20, 'I'), 'flags': (6, 8, 'H'), 'compression': (8, 10, 'H')}
            if field == 'local_signature':
                result[offset:offset + 4] = b'BAD!'
                break
            local_field, central_field, kind = locations[field]
            old = struct.unpack_from('<' + kind, result, central + central_field)[0]
            value = change(old)
            struct.pack_into('<' + kind, result, offset + local_field, value)
            struct.pack_into('<' + kind, result, central + central_field, value)
            break
        central += 46 + name_len + extra_len + comment_len
    return bytes(result)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--baseline-label', default='xml-memory3')
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    directory = run / 'zip-probes'
    directory.mkdir(exist_ok=True)
    base = package(zipfile.ZIP_DEFLATED)
    cases = dict(deflated=base, stored=package(zipfile.ZIP_STORED), streamed=package(zipfile.ZIP_DEFLATED, streamed=True), zip64=package(zipfile.ZIP_DEFLATED, force_zip64=True), prefixed=b'archive-prefix\0' + base, empty=package(zipfile.ZIP_DEFLATED, empty=True), high_ratio=package(zipfile.ZIP_DEFLATED, padding=2 * 1024 * 1024))
    for name, field, change in [('crc', 'crc', lambda x: x ^ 1), ('size_small', 'size', lambda x: x - 1), ('size_large', 'size', lambda x: x + 1), ('payload_short', 'compressed_size', lambda x: x - 2), ('encrypted', 'flags', lambda x: x | 1), ('unknown_method', 'compression', lambda x: 222), ('bad_header', 'local_signature', lambda x: x)]:
        cases[name] = mutate(base, field, change)
    records = []
    binaries = dict(baseline=OUT / 'experiments' / args.baseline_label / 'candidate', candidate=run / 'candidate')
    for name, data in cases.items():
        path = directory / (name + '.pptx')
        path.write_bytes(data)
        outputs = {engine: invoke(binary, 'duke-slides', dict(path=str(path)), directory / (name + '.' + engine + '.json.gz'), 0) for engine, binary in binaries.items()}
        fields = lambda record: (record['status'], record.get('full_sha256'), record.get('error'))
        records.append(dict(case=name, sha256=hashlib.sha256(data).hexdigest(), equal=fields(outputs['baseline']) == fields(outputs['candidate']), outputs=outputs))
    (directory / 'records.json').write_text(json.dumps(records, indent=2))
    summary = dict(total=len(records), equal=sum(r['equal'] for r in records), binaries={k: hashlib.sha256(v.read_bytes()).hexdigest() for k, v in binaries.items()}, failures=[r['case'] for r in records if not r['equal']], outcomes={r['case']: r['outputs']['candidate']['status'] for r in records})
    (directory / 'summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary))
    if summary['equal'] != summary['total']: raise SystemExit(1)


if __name__ == '__main__':
    main()

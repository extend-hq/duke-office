import argparse
import hashlib
import io
import json
from pathlib import Path
import struct
import zipfile
import zlib

from artwork_math_gate import artwork_hashes
from feedback import OUT, invoke as compact
from fields_probes import package
from image_fallback_review import verify_sources, visit
from object_gate import invoke


def make_package(path, name):
    package(path, strict=name == 'strict')
    with zipfile.ZipFile(io.BytesIO(path.read_bytes())) as archive:
        parts = {n: archive.read(n) for n in archive.namelist()}
    presentation = 'http://purl.oclc.org/ooxml/presentationml/main' if name == 'strict' else 'http://schemas.openxmlformats.org/presentationml/2006/main'
    drawing = 'http://purl.oclc.org/ooxml/drawingml/main' if name == 'strict' else 'http://schemas.openxmlformats.org/drawingml/2006/main'
    relationship = 'http://purl.oclc.org/ooxml/officeDocument/relationships' if name == 'strict' else 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'
    ns = f'xmlns:p="{presentation}" xmlns:a="{drawing}" xmlns:r="{relationship}" xmlns:s="http://schemas.microsoft.com/office/drawing/2016/SVG/main"'
    alternative = '<s:svgBlip r:embed="vector"/>'
    if name == 'same-target':
        alternative = '<s:svgBlip r:embed="raster"/>'
    elif name == 'linked-svg':
        alternative = '<s:svgBlip r:link="vector"/>'
    elif name == 'empty-svg-id':
        alternative = '<s:svgBlip r:embed=""/>'
    elif name == 'raster-only':
        alternative = ''
    fill = '<p:blipFill><a:blip r:embed="raster"><a:extLst><a:ext uri="{96DAC541-7B7A-43D3-8B79-37D633B846F1}">' + alternative + '</a:ext></a:extLst></a:blip><a:srcRect l="10000" t="15000"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>'
    def picture(content, ident='2', nv=''):
        return '<p:pic><p:nvPicPr><p:cNvPr id="' + ident + '" name="Picture"/><p:cNvPicPr/><p:nvPr><p:ph idx="1"/>' + nv + '</p:nvPr></p:nvPicPr>' + content + '<p:spPr><a:xfrm><a:off x="95250" y="95250"/><a:ext cx="1905000" cy="1428750"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>'
    slide = picture('' if name in ('layout', 'master') else fill, nv='<a:videoFile r:link="video"/>' if name == 'media' else '')
    if name == 'group':
        slide = '<p:grpSp><p:nvGrpSpPr><p:cNvPr id="9" name="Group"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>' + slide + '</p:grpSp>'
    parts['ppt/slides/slide1.xml'] = f'<p:sld {ns}><p:cSld><p:spTree>{slide}</p:spTree></p:cSld></p:sld>'
    parts['ppt/slideLayouts/layout.xml'] = f'<p:sldLayout {ns}><p:cSld><p:spTree>{picture(fill if name == "layout" else "", "4") if name in ("layout", "master") else ""}</p:spTree></p:cSld></p:sldLayout>'
    parts['ppt/slideMasters/master.xml'] = f'<p:sldMaster {ns}><p:cSld><p:spTree>{picture(fill, "5") if name == "master" else ""}</p:spTree></p:cSld></p:sldMaster>'
    def rel(ident, target, external=False, role='image'):
        mode = ' TargetMode="External"' if external else ''
        return f'<Relationship Id="{ident}" Type="{relationship}/{role}" Target="{target}"{mode}/>'
    image_rels = rel('raster', '../media/raster.png', name == 'external-raster') + rel('vector', '../media/vector.svg', name == 'external-svg')
    for part in ['ppt/slides/_rels/slide1.xml.rels', 'ppt/slideLayouts/_rels/layout.xml.rels', 'ppt/slideMasters/_rels/master.xml.rels']:
        value = parts[part].decode().replace('</Relationships>', image_rels + rel('video', 'https://invalid.example/media', True, 'video') + '</Relationships>')
        parts[part] = value
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    parts['ppt/media/raster.png'] = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 1, 1, 8, 6, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(b'\x00\xff\x00\x00\xff')) + chunk(b'IEND', b'')
    parts['ppt/media/vector.svg'] = '<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30"><rect width="40" height="30" fill="#287FA8"/></svg>' if name != 'bad-svg' else '<svg'
    parts['[Content_Types].xml'] = '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="svg" ContentType="image/svg+xml"/><Default Extension="png" ContentType="image/png"/></Types>'
    if name == 'missing-svg':
        del parts['ppt/media/vector.svg']
    if name == 'missing-raster':
        del parts['ppt/media/raster.png']
    with zipfile.ZipFile(path, 'w') as archive:
        for part, value in parts.items():
            if isinstance(value, str):
                if name == 'alias':
                    value = value.replace('xmlns:s=', 'xmlns:vector=').replace('<s:', '<vector:')
                value = value.encode()
                if name == 'bom' and part.endswith('.xml'):
                    value = b'\xef\xbb\xbf' + value
            info = zipfile.ZipInfo(part, (2000, 1, 1, 0, 0, 0)); info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, value)


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--label', required=True); args = parser.parse_args()
    run = OUT / 'experiments' / args.label; target = run / 'probes'; target.mkdir()
    manifest, records, receipts = [], [], []
    names = ['transitional', 'strict', 'alias', 'bom', 'same-target', 'missing-svg', 'missing-raster', 'external-svg', 'external-raster', 'linked-svg', 'raster-only', 'empty-svg-id', 'layout', 'master', 'group', 'media', 'bad-svg']
    for name in names:
        path = (target / (name + '.pptx')).resolve(); make_package(path, name)
        row = dict(id=name, path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest()); manifest.append(row)
        record, document = invoke(run / 'candidate', row, target / (name + '.full.json.gz'))
        assert record['status'] == 'ok', record
        counts = dict(verify_sources(path, document))
        expected = name not in {'same-target', 'missing-svg', 'missing-raster', 'external-svg', 'external-raster', 'linked-svg', 'raster-only', 'empty-svg-id'}
        assert bool(counts.get('associations')) == expected, (name, counts)
        images = []
        def walk(value):
            if isinstance(value, dict):
                if value.get('type') == 'image':
                    images.append(value)
                for k, v in value.items():
                    if k not in {'data', 'bytes'}: walk(v)
            elif isinstance(value, list):
                for v in value: walk(v)
        walk(document['slides'][0]['nodes'])
        assert len(images) == 1, name
        image = images[0]
        assert image['assetId'] == ('ppt/media/raster.png' if name in {'same-target', 'missing-svg', 'external-svg', 'linked-svg', 'raster-only', 'empty-svg-id'} else 'ppt/media/vector.svg'), (name, image['assetId'])
        assert image.get('crop') == dict(left=.1, top=.15, right=0, bottom=0), (name, image.get('crop'))
        result = compact(run / 'candidate', 'duke-slides', row, target / (name + '.compact.json.gz'), 0)
        result.update(id=name, artwork_hashes=artwork_hashes(document)); records.append(result)
        receipts.append(dict(id=name, passed=True, counts=counts, selected=image['assetId'], fallback=image.get('fallbackAssetId')))
        print(name, counts, flush=True)
    (target / 'manifest.json').write_text(json.dumps(manifest, indent=2))
    destination = target / 'records.jsonl'; destination.write_text(''.join(json.dumps(r) + '\n' for r in records))
    summary = dict(total=len(receipts), passed=len(receipts), receipts=receipts, records_sha256=hashlib.sha256(destination.read_bytes()).hexdigest(), candidate_sha256=hashlib.sha256((run / 'candidate').read_bytes()).hexdigest(), harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), scope='Image representation fallback associations, selected payload, inheritance, crop, external or missing assets, nested groups, media posters and undecodable SVG bytes. Linked SVG selection remains unsupported and uses the saved raster.')
    (run / 'image-fallback-probes-summary.json').write_text(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()

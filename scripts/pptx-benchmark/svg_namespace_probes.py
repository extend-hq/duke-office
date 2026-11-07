import argparse
import base64
import gzip
import hashlib
import io
import json
import zipfile
from pathlib import Path
from xml.etree import ElementTree as ET

from artwork_math_gate import artwork_hashes
from common import rels
from feedback import OUT, invoke as compact
from image_fallback_probes import make_package
from object_gate import invoke
from sdk_source_check import STRICT, TRANSITIONAL


SVG = 'http://schemas.microsoft.com/office/drawing/2016/SVG/main'
RASTER_CASES = {'foreign-element', 'unqualified-element', 'element-shadowed', 'foreign-attribute', 'unqualified-attribute', 'attribute-shadowed', 'nested', 'direct', 'foreign-list', 'foreign-extension', 'foreign-blip'}
CASES = ['valid', 'alias', 'default-element', 'foreign-element', 'unqualified-element', 'element-shadowed', 'foreign-attribute', 'unqualified-attribute', 'attribute-shadowed', 'foreign-attribute-first', 'foreign-attribute-last', 'relationship-alias', 'foreign-first', 'missing-first', 'empty-first', 'external-first', 'nested', 'direct', 'foreign-list', 'foreign-extension', 'foreign-blip', 'nonempty', 'scope-restored']


def package(path, case, strict, owner='slide'):
    make_package(path, 'strict' if strict else owner if owner != 'slide' else 'transitional')
    with zipfile.ZipFile(io.BytesIO(path.read_bytes())) as archive:
        parts = {name: archive.read(name) for name in archive.namelist()}
    rns = STRICT['r'] if strict else TRANSITIONAL['r']
    valid = '<s:svgBlip r:embed="vector"/>'
    variants = {
        'alias': '<v:svgBlip xmlns:v="' + SVG + '" r:embed="vector"/>',
        'default-element': '<svgBlip xmlns="' + SVG + '" r:embed="vector"/>',
        'foreign-element': '<f:svgBlip xmlns:f="urn:foreign" r:embed="vector"/>',
        'unqualified-element': '<svgBlip r:embed="vector"/>',
        'element-shadowed': '<s:svgBlip xmlns:s="urn:foreign" r:embed="vector"/>',
        'foreign-attribute': '<s:svgBlip xmlns:f="urn:foreign" f:embed="vector"/>',
        'unqualified-attribute': '<s:svgBlip embed="vector"/>',
        'attribute-shadowed': '<s:svgBlip xmlns:r="urn:foreign" r:embed="vector"/>',
        'foreign-attribute-first': '<s:svgBlip xmlns:f="urn:foreign" f:embed="decoy" r:embed="vector"/>',
        'foreign-attribute-last': '<s:svgBlip xmlns:f="urn:foreign" r:embed="vector" f:embed="decoy"/>',
        'relationship-alias': '<s:svgBlip xmlns:q="' + rns + '" q:embed="vector"/>',
        'foreign-first': '<f:svgBlip xmlns:f="urn:foreign" r:embed="decoy"/>' + valid,
        'missing-first': '<s:svgBlip r:embed="missing"/>' + valid,
        'empty-first': '<s:svgBlip r:embed=""/>' + valid,
        'external-first': '<s:svgBlip r:embed="external"/>' + valid,
        'nested': '<f:wrapper xmlns:f="urn:foreign">' + valid + '</f:wrapper>',
        'nonempty': '<s:svgBlip r:embed="vector"></s:svgBlip>',
        'scope-restored': '<f:wrapper xmlns:f="urn:foreign" xmlns:s="urn:foreign"/>' + valid,
    }
    owner_part = {'slide':'ppt/slides/slide1.xml','layout':'ppt/slideLayouts/layout.xml','master':'ppt/slideMasters/master.xml','group':'ppt/slides/slide1.xml','media':'ppt/slides/slide1.xml'}[owner]
    value = parts[owner_part].decode().replace(valid, variants.get(case, valid))
    if case == 'direct':
        value = value.replace('<a:extLst><a:ext uri="{96DAC541-7B7A-43D3-8B79-37D633B846F1}">', '').replace('</a:ext></a:extLst>', '')
    elif case in {'foreign-list', 'foreign-extension', 'foreign-blip'}:
        tag = {'foreign-list':'extLst','foreign-extension':'ext','foreign-blip':'blip'}[case]
        value = value.replace('<a:' + tag + ('>' if tag == 'extLst' else ' '), '<f:' + tag + ' xmlns:f="urn:foreign"' + ('>' if tag == 'extLst' else ' ')).replace('</a:' + tag + '>', '</f:' + tag + '>')
    parts[owner_part] = value.encode()
    for name in list(parts):
        if name.endswith('.rels'):
            extra = f'<Relationship Id="decoy" Type="{rns}/image" Target="../media/decoy.svg"/><Relationship Id="missing" Type="{rns}/image" Target="../media/missing.svg"/><Relationship Id="external" Type="{rns}/image" Target="https://invalid.example/asset" TargetMode="External"/>'
            parts[name] = parts[name].replace(b'</Relationships>', extra.encode() + b'</Relationships>')
    parts['ppt/media/decoy.svg'] = b'<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30"><rect width="40" height="30" fill="#00ff00"/></svg>'
    for name in list(parts):
        if name.endswith('.xml'):
            parts[name] = parts[name].replace(b'<a:prstGeom prst="rect">', b'<a:prstGeom prst="ellipse">')
    with zipfile.ZipFile(path, 'w') as archive:
        for name, data in parts.items():
            info = zipfile.ZipInfo(name, (2000, 1, 1, 0, 0, 0)); info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data)
    return owner_part


def images(value):
    if isinstance(value, dict):
        if value.get('type') == 'image':
            yield value
        for key, part in value.items():
            if key not in {'data', 'bytes'}:
                yield from images(part)
    elif isinstance(value, list):
        for part in value:
            yield from images(part)


def source_selection(path, part, strict):
    ns = STRICT if strict else TRANSITIONAL
    with zipfile.ZipFile(path) as archive:
        root = ET.fromstring(archive.read(part)); relationships = rels(archive, part)
        picture = root.find('.//p:pic', ns)
        selected = None
        for node in picture.findall('./p:blipFill/a:blip/a:extLst/a:ext/{' + SVG + '}svgBlip', ns):
            ident = node.get('{' + ns['r'] + '}embed')
            relationship = relationships.get(ident, {})
            if not relationship.get('external', True) and relationship.get('target') in archive.namelist():
                selected = relationship['target']; break
        return selected or 'ppt/media/raster.png'


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--label', required=True); args = parser.parse_args()
    run = OUT / 'experiments' / args.label; target = run / 'namespace-probes'; target.mkdir()
    manifest, records, receipts = [], [], []
    configurations = [(case, strict, 'slide') for strict in [False, True] for case in CASES] + [('foreign-first', False, owner) for owner in ['layout','master','group','media']]
    baseline = OUT / 'experiments/image-fallback1/candidate'
    for case, strict, owner in configurations:
        ident = ('strict' if strict else 'transitional') + '-' + owner + '-' + case
        path = (target / (ident + '.pptx')).resolve(); part = package(path, case, strict, owner)
        expected = source_selection(path, part, strict)
        assert expected == ('ppt/media/raster.png' if case in RASTER_CASES else 'ppt/media/vector.svg'), ident
        row = dict(id=ident, path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest()); manifest.append(row)
        result, document = invoke(run / 'candidate', row, target / (ident + '.full.json.gz'))
        assert result['status'] == 'ok', (ident, result)
        image = list(images(document['slides'][0]['nodes'])); assert len(image) == 1, ident
        image = image[0]; assert image['assetId'] == expected, (ident, image['assetId'], expected)
        assert image.get('fallbackAssetId') == ('ppt/media/raster.png' if expected.endswith('.svg') else None), ident
        with zipfile.ZipFile(path) as archive:
            assert bytes(document['assets'][expected]['data']) == archive.read(expected)
            for fallback in [image.get('fallbackAssetId')]:
                if fallback: assert bytes(document['assets'][fallback]['data']) == archive.read(fallback)
            if case == 'direct':
                assert 'pictureSvg' not in image, ident
                assert any(w['code'] == 'unsupported-picture-appearance' for w in document['slides'][0]['warnings']), ident
            else:
                assert base64.b64encode(archive.read(expected)).decode() in image['pictureSvg'], ident
        assert image.get('crop') == dict(left=.1, top=.15, right=0, bottom=0), ident
        old = compact(baseline, 'rust-pptx', row, target / (ident + '.baseline.json.gz'), 0)
        before = json.load(gzip.open(target / (ident + '.baseline.json.gz'), 'rt'))
        old_image = list(images(before['slides'][0]['nodes']))[0]
        current = compact(run / 'candidate', 'rust-pptx', row, target / (ident + '.compact.json.gz'), 0)
        current.update(id=ident, artwork_hashes=artwork_hashes(document)); records.append(current)
        receipts.append(dict(id=ident, selected=expected, baseline_selected=old_image['assetId'], selection_corrected=old_image['assetId'] != expected, full_equal=current['full_sha256'] == old['full_sha256'], passed=True))
        print(ident, expected, flush=True)
    (target / 'manifest.json').write_text(json.dumps(manifest, indent=2))
    destination = target / 'records.jsonl'; destination.write_text(''.join(json.dumps(r) + '\n' for r in records))
    summary = dict(total=len(receipts), passed=len(receipts), selection_corrections=sum(r['selection_corrected'] for r in receipts), receipts=receipts, candidate_sha256=hashlib.sha256((run / 'candidate').read_bytes()).hexdigest(), records_sha256=hashlib.sha256(destination.read_bytes()).hexdigest(), harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), scope='Qualified SVG element, qualified relationship attribute and qualified extension-container path. Independent ElementTree selection and exact retained payload bytes. Generated artwork contains the selected payload in all variants except the mislocated direct child, which retains the raster with an explicit unsupported-appearance warning. Includes Strict/Transitional, namespace shadowing, aliases, multiple alternatives, inheritance, groups and media posters. No native PowerPoint pixel-fidelity claim.')
    (run / 'svg-namespace-probes-summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps({k:v for k,v in summary.items() if k!='receipts'}))


if __name__ == '__main__':
    main()

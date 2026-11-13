import argparse
import base64
import io
import json
from pathlib import Path
import zipfile

from artwork_math_gate import artwork_hashes
from feedback import OUT, invoke as compact
from object_gate import invoke
from picture_fallback_review import compatibility_package, digest, NEW_WARNING
from svg_namespace_probes import package as base_package, images


CASES = ['primary', 'bad-svg', 'nested-good', 'nested-bad', 'utf16-le', 'utf16-be', 'declared-encoding', 'grayscale', 'shadow', 'rotation', 'tile', 'raster-effect', 'both-bad']


def package(path, case, strict=False, owner='slide'):
    part = base_package(path, 'valid', strict, owner)
    with zipfile.ZipFile(io.BytesIO(path.read_bytes())) as archive:
        parts = {name: archive.read(name) for name in archive.namelist()}
    vector = parts['ppt/media/vector.svg']
    if case in {'bad-svg','grayscale','shadow','rotation','both-bad'}:
        vector = b'<svg'
    elif case in {'nested-good','nested-bad'}:
        payload = parts['ppt/media/raster.png'] if case == 'nested-good' else b'invalid'
        vector = ('<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30"><image width="40" height="30" href="data:image/png;base64,' + base64.b64encode(payload).decode() + '"/></svg>').encode()
    elif case in {'utf16-le','utf16-be'}:
        encoding = 'utf-16le' if case.endswith('le') else 'utf-16be'
        vector = ('<?xml version="1.0" encoding="UTF-16"?>' + vector.decode()).encode(encoding)
        vector = (b'\xff\xfe' if encoding.endswith('le') else b'\xfe\xff') + vector
    elif case == 'declared-encoding':
        vector = ('<?xml version="1.0" encoding="windows-1252"?>' + vector.decode().replace('<rect ', '<desc>\u00e9</desc><rect ')).encode('cp1252')
    parts['ppt/media/vector.svg'] = vector
    if case == 'both-bad': parts['ppt/media/raster.png'] = b'invalid'
    for name in list(parts):
        if not name.endswith('.xml'): continue
        source = parts[name].decode()
        if case == 'grayscale': source = source.replace('</a:blip>', '<a:grayscl/></a:blip>')
        if case == 'raster-effect': source = source.replace('</a:blip>', '<a:alphaCeiling/></a:blip>')
        if case == 'shadow': source = source.replace('</p:spPr>', '<a:effectLst><a:outerShdw blurRad="63500" dist="63500" dir="2700000" algn="ctr" rotWithShape="1"><a:srgbClr val="111111"><a:alpha val="65000"/></a:srgbClr></a:outerShdw></a:effectLst></p:spPr>')
        if case == 'rotation': source = source.replace('<a:xfrm>', '<a:xfrm rot="1800000" flipH="1">')
        if case == 'tile': source = source.replace('<a:stretch><a:fillRect/></a:stretch>', '<a:tile tx="19050" ty="28575" sx="400000" sy="300000" flip="xy" algn="ctr"/>')
        parts[name] = source.encode()
    with zipfile.ZipFile(path, 'w') as archive:
        for name, data in parts.items():
            info = zipfile.ZipInfo(name, (2000, 1, 1, 0, 0, 0)); info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data)
    return part


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--label', required=True); args = parser.parse_args()
    run = OUT / 'experiments' / args.label; target = run / 'probes'; target.mkdir()
    baseline = OUT / 'experiments/svg-namespace2/candidate'
    configurations = [(case, strict, 'slide') for strict in [False, True] for case in CASES] + [('bad-svg', False, owner) for owner in ['layout','master','group','media']]
    manifest, records, receipts = [], [], []
    for case, strict, owner in configurations:
        ident = ('strict' if strict else 'transitional') + '-' + owner + '-' + case
        path = (target / (ident + '.pptx')).resolve(); package(path, case, strict, owner)
        row = dict(id=ident, path=str(path), sha256=digest(path)); manifest.append(row)
        record, model = invoke(run / 'candidate', row, target / (ident + '.full.json.gz')); assert record['status'] == 'ok', record
        node = list(images(model['slides'][0]['nodes'])); assert len(node) == 1; node = node[0]
        assert node['assetId'] == 'ppt/media/vector.svg' and node['fallbackAssetId'] == 'ppt/media/raster.png'
        assert 'fallbackPicture' in node, ident
        control_path = target / (ident + '.compatibility.pptx'); removed = compatibility_package(path, control_path); assert removed
        control_row = dict(row, path=str(control_path), sha256=digest(control_path))
        control_record, control = invoke(baseline, control_row, target / (ident + '.control.json.gz')); assert control_record['status'] == 'ok'
        control_node = list(images(control['slides'][0]['nodes']))[0]
        assert control_node['assetId'] == node['fallbackAssetId']
        assert node['fallbackPicture']['svg'] == control_node['pictureSvg'], ident
        assert node['fallbackPicture'].get('paintBounds') == control_node.get('paintBounds'), ident
        if case in {'tile','raster-effect'}:
            assert 'pictureSvg' not in node, ident
            assert any(w['message'] == NEW_WARNING for w in model['slides'][0]['warnings']), ident
        else:
            assert 'pictureSvg' in node, ident
        current = compact(run / 'candidate', 'duke-pptx', row, target / (ident + '.compact.json.gz'), 0)
        current.update(id=ident, artwork_hashes=artwork_hashes(model)); records.append(current)
        receipts.append(dict(id=ident, case=case, strict=strict, owner=owner, passed=True, primary_artwork='pictureSvg' in node, fallback_bytes=len(node['fallbackPicture']['svg'].encode()), control_sha256=control_record['full_sha256'], primary_expected=case in {'primary','nested-good','utf16-le','utf16-be','declared-encoding'}, error_expected=case=='both-bad'))
        print(ident, receipts[-1]['primary_artwork'], flush=True)
    (target / 'manifest.json').write_text(json.dumps(manifest, indent=2))
    destination = target / 'records.jsonl'; destination.write_text(''.join(json.dumps(row) + '\n' for row in records))
    summary = dict(total=len(receipts), passed=len(receipts), receipts=receipts, candidate_sha256=digest(run / 'candidate'), records_sha256=digest(destination), harness_sha256=digest(Path(__file__)), scope='Generated compatibility artwork and bounds exactly equal the frozen baseline rendering after qualified SVG alternatives are removed. Cases cover embedded decode failures, nested SVG images, UTF-16 and declared encoding, crop, ellipse geometry, grayscale, shadows, rotation, tiling, native raster effects, inheritance, groups and media posters. Browser execution is separate.')
    (run / 'picture-fallback-probes-summary.json').write_text(json.dumps(summary, indent=2)); print(json.dumps({k:v for k,v in summary.items() if k!='receipts'}))


if __name__ == '__main__':
    main()

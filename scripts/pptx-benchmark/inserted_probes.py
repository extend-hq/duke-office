import base64
import gzip
import json
from pathlib import Path
import subprocess
import sys
import zipfile
from xml.etree import ElementTree as ET

from common import NS
from inserted_gate import walk_objects

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'output/pptx-benchmark-20261106/inserted-probes'
M = 'http://schemas.openxmlformats.org/officeDocument/2006/math'
STRICT_M = 'http://purl.oclc.org/ooxml/officeDocument/math'
P14 = 'http://schemas.microsoft.com/office/powerpoint/2010/main'


def shape(ident, math, namespace=M):
    return f'<p:sp><p:nvSpPr><p:cNvPr id="{ident}" name="Equation"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="3000000" cy="1000000"/></a:xfrm></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a14:m xmlns:z="{namespace}">{math}</a14:m></a:p></p:txBody></p:sp>'


def run(value):
    return f'<z:r><z:t>{value}</z:t></z:r>'


def main():
    OUT.mkdir(exist_ok=True)
    equations = [
        shape(1, '<z:oMathPara><z:oMathParaPr><z:jc z:val="centerGroup"/></z:oMathParaPr><z:oMath><z:rad><z:radPr><z:degHide z:val="on"/></z:radPr><z:deg>' + run('3') + '</z:deg><z:e>' + run('x+1') + '</z:e></z:rad></z:oMath></z:oMathPara>'),
        shape(2, '<z:oMath><z:f><z:fPr><z:type z:val="noBar"/></z:fPr><z:num>' + run('a') + '</z:num><z:den>' + run('b') + '</z:den></z:f></z:oMath>', STRICT_M),
        shape(3, '<z:oMathPara><z:oMath><z:nary><z:sub>' + run('0') + '</z:sub><z:sup>' + run('1') + '</z:sup><z:e>' + run('x') + '</z:e></z:nary></z:oMath></z:oMathPara>'),
        shape(4, '<z:oMath><z:nary><z:sub>' + run('0') + '</z:sub><z:sup>' + run('1') + '</z:sup><z:e>' + run('x') + '</z:e></z:nary></z:oMath>'),
        shape(5, '<z:oMath><z:groupChr><z:e>' + run('x') + '</z:e></z:groupChr><z:phant><z:phantPr><z:show/><z:zeroWid/></z:phantPr><z:e>' + run('&lt;&amp;') + '</z:e></z:phant></z:oMath>'),
        shape(6, '<z:oMath>' + run('retained') + '</z:oMath>', 'urn:unsupported-math'),
        shape(7, '<z:oMath>' + '<z:e>' * 66 + run('bounded') + '</z:e>' * 66 + '</z:oMath>'),
    ]
    ink_xml = '<ink xmlns="http://www.w3.org/2003/InkML"><trace>!10 !20,\'5 \'5,&quot;1 &quot;1</trace></ink>'
    zero_ink = '<ink xmlns="http://www.w3.org/2003/InkML"><trace>10 20</trace></ink>'
    unsupported_ink = '<ink xmlns="http://www.w3.org/2003/InkML"><canvasTransform/><trace>10 20</trace></ink>'
    def ink_node(ident, rid, width, height):
        return f'<p:contentPart r:id="{rid}"><p14:nvContentPartPr><p14:cNvPr id="{ident}" name="Ink"/><p14:cNvContentPartPr/><p14:nvPr/></p14:nvContentPartPr><p14:xfrm><a:off x="100" y="200"/><a:ext cx="{width}" cy="{height}"/></p14:xfrm></p:contentPart>'
    fallback = '<p:pic><p:nvPicPr><p:cNvPr id="10" name="Ink preview"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="image"/></p:blipFill><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="95250" cy="95250"/></a:xfrm></p:spPr></p:pic>'
    tree = ''.join(equations) + ink_node(8, 'ink', 95250, 95250) + ink_node(9, 'zero', 0, 0) + '<mc:AlternateContent><mc:Choice Requires="p14">' + ink_node(10, 'unsupported', 95250, 95250) + '</mc:Choice><mc:Fallback>' + fallback + '</mc:Fallback></mc:AlternateContent>'
    controls = '<p:controls><p:control spid="11" name="Control" r:id="control" imgW="100" imgH="200"/></p:controls>'
    slide = f'<p:sld xmlns:p="{NS["p"]}" xmlns:a="{NS["a"]}" xmlns:r="{NS["r"]}" xmlns:p14="{P14}" xmlns:a14="http://schemas.microsoft.com/office/drawing/2010/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><p:cSld><p:spTree>{tree}</p:spTree>{controls}</p:cSld></p:sld>'
    relation_ns = 'http://schemas.openxmlformats.org/package/2006/relationships'
    def relationships(entries):
        return '<Relationships xmlns="' + relation_ns + '">' + ''.join(f'<Relationship Id="{ident}" Type="{NS["r"]}/{kind}" Target="{target}"/>' for ident, kind, target in entries) + '</Relationships>'
    binary = b'\x00\x01\xffretained-control'
    png = base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aZ1sAAAAASUVORK5CYII=')
    parts = {
        'ppt/presentation.xml': f'<p:presentation xmlns:p="{NS["p"]}" xmlns:r="{NS["r"]}"><p:sldIdLst><p:sldId id="256" r:id="slide"/></p:sldIdLst><p:sldSz cx="9144000" cy="6858000"/></p:presentation>',
        'ppt/_rels/presentation.xml.rels': relationships([('slide', 'slide', 'slides/slide1.xml')]),
        'ppt/slides/slide1.xml': slide,
        'ppt/slides/_rels/slide1.xml.rels': relationships([('ink', 'customXml', '/payloads/ink.xml'), ('zero', 'customXml', '/payloads/zero.xml'), ('unsupported', 'customXml', '/payloads/unsupported.xml'), ('image', 'image', '/payloads/preview.png'), ('control', 'control', '/payloads/control.xml'), ('vml', 'vmlDrawing', '/payloads/preview.vml')]),
        'payloads/ink.xml': ink_xml, 'payloads/zero.xml': zero_ink, 'payloads/unsupported.xml': unsupported_ink,
        'payloads/preview.png': png,
        'payloads/control.xml': '<ax:ocx xmlns:ax="http://schemas.microsoft.com/office/2006/activeX" xmlns:r="' + NS['r'] + '" ax:classid="retained-class" ax:persistence="persistStreamInit" r:id="binary"><ax:ocxPr ax:name="Caption" ax:value="A &amp; B"/></ax:ocx>',
        'payloads/_rels/control.xml.rels': relationships([('binary', 'activeXControlBinary', 'control.bin')]),
        'payloads/control.bin': binary,
        'payloads/preview.vml': '<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office"><v:shape o:spid="_x0000_s11" style="left:1in;top:2cm;width:30mm;height:40pt"><![if gte mso 9]><v:imagedata o:relid="preview"/><![endif]></v:shape></xml>',
        'payloads/_rels/preview.vml.rels': relationships([('preview', 'image', 'preview.png')]),
    }
    path = OUT / 'structured-content.pptx'
    with zipfile.ZipFile(path, 'w', zipfile.ZIP_DEFLATED) as package:
        for name, data in parts.items(): package.writestr(name, data)
    target = OUT / 'structured-content.json.gz'
    result = subprocess.run([sys.argv[1] if len(sys.argv) > 1 else str(ROOT / 'scripts/pptx-benchmark/target/release/pptx-benchmark'), 'duke-slides', str(path), str(target), '0', 'full'], capture_output=True, text=True, check=True)
    record = json.loads(result.stdout.splitlines()[-1]); assert record['status'] == 'ok', record
    model = json.load(gzip.open(target, 'rt')); nodes = {n['id']: n for n in model['slides'][0]['nodes']}
    equations = {k: next(n['math'] for n in walk_objects(nodes[str(k)]) if n.get('math')) for k in range(1, 8)}
    control = nodes['11']; definition = control['embeddedObject']['control']
    checks = {
        'inherited_namespace_and_radical': '<msqrt>' in equations[1]['mathMl'] and equations[1]['root']['namespace'] == M,
        'strict_math_namespace': 'linethickness="0"' in equations[2]['mathMl'] and equations[2]['root']['namespace'] == STRICT_M,
        'display_limit_default': '<munderover>' in equations[3]['mathMl'],
        'inline_limit_default': '<msubsup>' in equations[4]['mathMl'],
        'group_and_visible_phantom': '<munder accentunder="true">' in equations[5]['mathMl'] and '<mpadded width="0px">' in equations[5]['mathMl'] and '<mphantom>' not in equations[5]['mathMl'],
        'invalid_namespace_fallback': equations[6]['text'] == 'retained' and 'root' not in equations[6] and bool(equations[6]['sourceXml']) and bool(equations[6]['unsupported']),
        'depth_limit_retains_content': equations[7]['text'] == 'bounded' and 'root' not in equations[7] and bool(equations[7]['sourceXml']),
        'ink_delta_coordinates': [p['x'] for p in nodes['8']['ink']['drawing']['strokes'][0]['points']] == [10, 15, 21],
        'zero_extent_ink': nodes['9']['transform']['width'] == 0 and nodes['9']['paintBounds']['widthPx'] > 0 and bool(nodes['9']['pictureSvg']),
        'unsupported_ink_keeps_preview': nodes['10']['assetId'] == 'payloads/preview.png' and nodes['10']['ink']['sourceXml'] == unsupported_ink and 'drawing' not in nodes['10']['ink'],
        'control_properties_and_binary': definition['classId'] == 'retained-class' and definition['properties'] == [dict(name='Caption', value='A & B')] and definition['binaryAssetIds'] == ['payloads/control.bin'] and bytes(model['assets']['payloads/control.bin']['data']) == binary,
        'control_vml_geometry_and_preview': control['transform'] == dict(x=914400, y=720000, width=1080000, height=508000) and control['posterAssetId'] == 'payloads/preview.png',
    }
    for equation in equations.values():
        if equation.get('mathMl'): ET.fromstring(equation['mathMl'])
    (OUT / 'results.json').write_text(json.dumps(checks, indent=2)); print(json.dumps(checks, indent=2))
    assert all(checks.values())


if __name__ == '__main__':
    main()

"""Create fixed-coordinate chart and drawing probes for native visual comparison."""
import hashlib
import json
from pathlib import Path
import sys
import zipfile

ROOT = Path(sys.argv[1])
ROOT.mkdir(parents=True, exist_ok=True)
NS = ' '.join(f'xmlns:{k}="{v}"' for k, v in {
    'office': 'urn:oasis:names:tc:opendocument:xmlns:office:1.0',
    'style': 'urn:oasis:names:tc:opendocument:xmlns:style:1.0',
    'draw': 'urn:oasis:names:tc:opendocument:xmlns:drawing:1.0',
    'text': 'urn:oasis:names:tc:opendocument:xmlns:text:1.0',
    'table': 'urn:oasis:names:tc:opendocument:xmlns:table:1.0',
    'chart': 'urn:oasis:names:tc:opendocument:xmlns:chart:1.0',
    'svg': 'urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0',
    'fo': 'urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0',
    'xlink': 'http://www.w3.org/1999/xlink',
    'presentation': 'urn:oasis:names:tc:opendocument:xmlns:presentation:1.0',
}.items())
STYLES = '''<office:styles><draw:gradient draw:name="Gradient" draw:style="linear" draw:start-color="#0055aa" draw:end-color="#ee9944" draw:start-intensity="100%" draw:end-intensity="100%" draw:angle="450" draw:border="0%"/></office:styles>
<office:automatic-styles>
<style:page-layout style:name="Page"><style:page-layout-properties fo:page-width="4.5in" fo:page-height="4.5in" fo:margin="0in" style:print-orientation="portrait"/></style:page-layout>
<style:style style:name="PageStyle" style:family="drawing-page"><style:drawing-page-properties draw:fill="none"/></style:style>
<style:style style:name="Shape" style:family="graphic"><style:graphic-properties draw:fill="solid" draw:fill-color="#4472c4" draw:stroke="solid" svg:stroke-color="#18335a" svg:stroke-width="2px"/></style:style>
<style:style style:name="GradientShape" style:family="graphic" style:parent-style-name="Shape"><style:graphic-properties draw:fill="gradient" draw:fill-gradient-name="Gradient"/></style:style>
<style:style style:name="Break" style:family="paragraph"><style:paragraph-properties fo:break-before="page" fo:margin="0in"/><style:text-properties fo:font-size="1pt"/></style:style>
</office:automatic-styles><office:master-styles><style:master-page style:name="Master" style:page-layout-name="Page"/></office:master-styles>'''

def custom(path, equations='', attrs='', style='Shape'):
    return f'<draw:custom-shape draw:style-name="{style}" svg:x="120px" svg:y="120px" svg:width="192px" svg:height="128px"><draw:enhanced-geometry draw:type="non-primitive" svg:viewBox="0 0 21600 21600" draw:enhanced-path="{path}" {attrs}>{equations}</draw:enhanced-geometry></draw:custom-shape>'

shapes = [
    ('rectangle', '<draw:rect draw:style-name="Shape" svg:x="120px" svg:y="120px" svg:width="192px" svg:height="128px"/>'),
    ('ellipse', '<draw:ellipse draw:style-name="Shape" svg:x="120px" svg:y="120px" svg:width="192px" svg:height="128px"/>'),
    ('adjusted-geometry', custom('M 0 0 L ?tip 10800 0 21600 6000 10800 Z N', '<draw:equation draw:name="tip" draw:formula="$0"/>', 'draw:modifiers="15000"')),
    ('forward-equations', custom('M 0 0 L ?first 10800 0 21600 Z N', '<draw:equation draw:name="first" draw:formula="?second*2"/><draw:equation draw:name="second" draw:formula="max(5000, $0)"/>', 'draw:modifiers="9000"')),
    ('conditional-equations', custom('M 0 10800 L ?a 0 21600 10800 ?a 21600 Z N', '<draw:equation draw:name="a" draw:formula="if($0, width/3, width*2/3)"/>', 'draw:modifiers="1"')),
    ('quadratic-curve', custom('M 0 21600 Q 10800 -8000 21600 21600 Z N')),
    ('quadrant-arcs', custom('M 0 10800 X 10800 0 21600 10800 10800 21600 0 10800 Z N')),
    ('clockwise-arc', custom('M 10800 10800 W 0 0 21600 21600 21600 10800 10800 21600 Z N')),
    ('counterclockwise-arc', custom('M 10800 10800 A 0 0 21600 21600 21600 10800 10800 21600 Z N')),
    ('angle-ellipse', custom('U 10800 10800 10800 7000 25 295 Z N')),
    ('full-ellipse', custom('U 10800 10800 10800 10800 0 360 Z N')),
    ('separate-paints', custom('M 0 0 L 21600 0 21600 21600 0 21600 Z S N M 0 0 L 21600 21600 F N')),
    ('gradient', custom('M 0 0 L 21600 0 10800 21600 Z N', style='GradientShape')),
    ('mirrored-geometry', custom('M 0 0 L ?tip 10800 0 21600 Z N', '<draw:equation draw:name="tip" draw:formula="$0"/>', 'draw:modifiers="15000" draw:mirror-horizontal="true"')),
    ('rotated-geometry', custom('M 0 0 L 21600 10800 0 21600 Z N').replace('svg:x="120px" svg:y="120px"', 'draw:transform="rotate (0.35) translate (3.175cm 4.233333cm)"')),
]

def chart_xml(kind, extra='', rows=False, repeat=False):
    categories = ['North', 'South', 'West']
    series = [('Alpha', [2, 5, 3]), ('Beta', [4, 1, 6])]
    def cell(value):
        if isinstance(value, (float, int)):
            return f'<table:table-cell office:value-type="float" office:value="{value}"><text:p>{value}</text:p></table:table-cell>'
        return f'<table:table-cell office:value-type="string"><text:p>{value}</text:p></table:table-cell>'
    data = [[''] + [s[0] for s in series]] + [[category] + [s[1][i] for s in series] for i, category in enumerate(categories)]
    if rows: data = list(map(list, zip(*data)))
    table = '<table:table table:name="local-table"><table:table-header-columns><table:table-column/></table:table-header-columns><table:table-columns><table:table-column table:number-columns-repeated="'+str(len(data[0])-1)+'"/></table:table-columns><table:table-header-rows><table:table-row>' + ''.join(cell(c) for c in data[0]) + '</table:table-row></table:table-header-rows><table:table-rows>' + ''.join('<table:table-row>' + ''.join(cell(c) for c in row) + '</table:table-row>' for row in data[1:]) + '</table:table-rows></table:table>'
    if repeat: table = table.replace(cell(2), '<table:table-cell table:number-columns-repeated="1" office:value-type="float" office:value="2"><text:p>2</text:p></table:table-cell>')
    cat = 'local-table.B1:local-table.D1' if rows else 'local-table.A2:local-table.A4'
    entries = []
    for i, (name, _) in enumerate(series[:1] if kind in ('circle', 'ring', 'bubble', 'scatter') else series):
        values = f'local-table.B{i+2}:local-table.D{i+2}' if rows else f'local-table.{chr(66+i)}2:local-table.{chr(66+i)}4'
        label = f'local-table.A{i+2}' if rows else f'local-table.{chr(66+i)}1'
        domains = ''
        if kind in ('scatter', 'bubble'):
            domains = '<chart:domain table:cell-range-address="local-table.C2:local-table.C4"/>'
            if kind == 'bubble': domains += '<chart:domain table:cell-range-address="local-table.B2:local-table.B4"/>'
        entries.append(f'<chart:series chart:class="chart:{'circle' if kind == 'ring' else kind}" chart:style-name="Series{i}" chart:values-cell-range-address="{values}" chart:label-cell-address="{label}">{domains}<chart:data-point chart:repeated="3"/></chart:series>')
    styles = f'''<office:automatic-styles>
<style:style style:name="Chart" style:family="chart"><style:graphic-properties draw:fill="solid" draw:fill-color="#ffffff" draw:stroke="none"/><style:text-properties fo:font-size="10pt" fo:font-family="Liberation Sans"/></style:style>
<style:style style:name="Plot" style:family="chart"><style:chart-properties chart:series-source="{'rows' if rows else 'columns'}" {extra}/><style:graphic-properties draw:fill="none" draw:stroke="none"/></style:style>
<style:style style:name="Category" style:family="chart"><style:chart-properties chart:display-label="true"/></style:style>
<style:style style:name="Axis" style:family="chart"><style:chart-properties chart:display-label="true" chart:minimum="0" chart:maximum="8" chart:interval-major="2"/></style:style>
<style:style style:name="Series0" style:family="chart"><style:graphic-properties draw:fill="solid" draw:fill-color="#004586" draw:stroke="solid" svg:stroke-color="#004586" svg:stroke-width="1pt"/></style:style>
<style:style style:name="Series1" style:family="chart"><style:graphic-properties draw:fill="solid" draw:fill-color="#ff420e" draw:stroke="solid" svg:stroke-color="#ff420e" svg:stroke-width="1pt"/></style:style>
</office:automatic-styles>'''
    body = f'<office:body><office:chart><chart:chart xlink:href="." xlink:type="simple" chart:class="chart:{kind}" chart:style-name="Chart" svg:width="360px" svg:height="330px"><chart:legend chart:legend-position="end"/><chart:plot-area chart:style-name="Plot"><chart:axis chart:dimension="x" chart:name="primary-x" chart:style-name="Category"><chart:categories table:cell-range-address="{cat}"/></chart:axis><chart:axis chart:dimension="y" chart:name="primary-y" chart:style-name="Axis"><chart:grid chart:class="major"/></chart:axis>{"".join(entries)}</chart:plot-area>{table}</chart:chart></office:chart></office:body>'
    if kind in ('circle', 'ring'):
        import re
        body = re.sub(r'<chart:axis.*?</chart:axis>', '', body)
    return f'<office:document {NS} office:version="1.3">{styles}{body}</office:document>'

charts = [('columns', 'bar', '', False), ('bars', 'bar', 'chart:vertical="true"', False), ('stacked', 'bar', 'chart:stacked="true"', False), ('row-oriented', 'bar', '', True), ('lines', 'line', '', False), ('area', 'area', '', False), ('pie', 'circle', '', False), ('ring', 'ring', '', False), ('scatter', 'scatter', '', False), ('bubble', 'bubble', '', False), ('radar', 'radar', '', False)]
objects = {}
cases = []
for name, xml in shapes:
    cases.append(dict(id=name, family='art', shape=xml))
for name, kind, extra, rows in charts:
    key = f'Object{len(objects)+1}'
    objects[key] = chart_xml(kind, extra, rows)
    xml = f'<draw:frame svg:x="36px" svg:y="36px" svg:width="360px" svg:height="330px"><draw:object xlink:href="./{key}" xlink:type="simple"/></draw:frame>'
    cases.append(dict(id=name, family='chart', shape=xml))

def document(family, flat=False):
    blocks = []
    for i, case in enumerate(cases):
        shape = case['shape']
        if flat:
            for key, content in objects.items():
                shape = shape.replace(f'<draw:object xlink:href="./{key}" xlink:type="simple"/>', f'<draw:object>{content}</draw:object>')
        if family in ('presentation', 'drawing'):
            blocks.append(f'<draw:page draw:name="{case["id"]}" draw:style-name="PageStyle" draw:master-page-name="Master">{shape}</draw:page>')
        elif family == 'text':
            shape = shape.replace('svg:x=', 'text:anchor-type="paragraph" svg:x=', 1)
            blocks.append(f'<text:p'+ (' text:style-name="Break"' if i else '') + f'>{shape}</text:p>')
        else:
            blocks.append(f'<table:table table:name="{case["id"]}"><table:shapes>{shape}</table:shapes><table:table-column/><table:table-row><table:table-cell/></table:table-row></table:table>')
    return f'<office:document {NS} office:version="1.3" office:mimetype="application/vnd.oasis.opendocument.{"graphics" if family == "drawing" else family}">{STYLES}<office:body><office:{family}>{"".join(blocks)}</office:{family}></office:body></office:document>'

for ext, family in [('odp','presentation'), ('odg','drawing'), ('odt','text'), ('ods','spreadsheet')]:
    path = ROOT / f'visual.{ext}'
    with zipfile.ZipFile(path, 'w', zipfile.ZIP_DEFLATED) as package:
        mime = f'application/vnd.oasis.opendocument.{"graphics" if family == "drawing" else family}'
        package.writestr('mimetype', mime, compress_type=zipfile.ZIP_STORED)
        package.writestr('content.xml', document(family).replace('<office:document ', '<office:document-content ', 1).replace('</office:document>', '</office:document-content>'))
        package.writestr('styles.xml', f'<office:document-styles {NS} office:version="1.3">{STYLES}</office:document-styles>')
        entries = [('/', mime), ('content.xml', 'text/xml'), ('styles.xml', 'text/xml')]
        for key, content in objects.items():
            package.writestr(f'{key}/content.xml', content.replace('<office:document ', '<office:document-content ',1).replace('</office:document>', '</office:document-content>'))
            entries.extend([(f'{key}/', 'application/vnd.oasis.opendocument.chart'), (f'{key}/content.xml','text/xml')])
        manifest = '<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">' + ''.join(f'<manifest:file-entry manifest:full-path="{p}" manifest:media-type="{m}"/>' for p,m in entries) + '</manifest:manifest>'
        package.writestr('META-INF/manifest.xml', manifest)
    (ROOT / f'visual.f{ext}').write_text(document(family, True))
for index, case in enumerate(cases):
    case.pop('shape'); case['page'] = index + 1
manifest = dict(pageSizePx=dict(width=432,height=432), dpi=96, cases=cases, inputs={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in ROOT.glob('visual.*')})
(ROOT / 'manifest.json').write_text(json.dumps(manifest, indent=2))
print(f'Generated {len(cases)} cases across 8 packaged/flat inputs')

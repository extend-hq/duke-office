"""Expand an automatic-layout presentation with a font and canvas-size matrix."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET
import zipfile

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('source', type=Path)
p.add_argument('output', type=Path)
p.add_argument('--titles-only', action='store_true')
a = p.parse_args()
a.output.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(a.source) as z:
    parts = {name: z.read(name) for name in z.namelist()}
ns = dict(re.findall(r'xmlns:(\w+)="([^"]+)"', '\n'.join(
    data.decode() for name, data in parts.items() if name.endswith('.xml'))))
for prefix, uri in ns.items():
    ET.register_namespace(prefix, uri)
def tag(name):
    prefix, local = name.split(':')
    return '{' + ns[prefix] + '}' + local
base = ET.fromstring(parts['content.xml'])
body = base.find('.//' + tag('office:presentation'))
pages = list(body)
for page in pages:
    body.remove(page)
variants = []
for family, sizes in [('Liberation Sans', [8, 10, 14]), ('Calibri', [8, 10, 14]),
                      ('Times New Roman', [10, 14]), ('Courier New', [10, 14])]:
    for size in sizes:
        variants.append(dict(id=f'{family}-{size}', family=family, size=size, source=2, width=360, height=330))
for style in ['bold', 'italic']:
    variants.append(dict(id=style, family='Calibri', size=10, style=style, source=2, width=360, height=330))
for width, height in [(200, 220), (384, 220), (200, 350), (384, 350)]:
    variants.append(dict(id=f'long-{width}-{height}', family='Liberation Sans', size=10, source=3, width=width, height=height))
if a.titles_only:
    variants = [dict(id=f'title-{height}-{size}', family='Liberation Sans', size=10,
                     source=2, width=360, height=height, titleSize=size,
                     title='Comparative results' if size is None else 'Detailed comparison across reporting periods')
                for height, size in [(220, None), (350, None), (330, 12), (330, 18)]]
output = {name: data for name, data in parts.items() if '/' not in name or name.startswith('META-INF/')}
M = '{urn:oasis:names:tc:opendocument:xmlns:manifest:1.0}'
ET.register_namespace('manifest', M[1:-1])
manifest = ET.Element(M+'manifest', {M+'version':'1.3'})
for name, media in [('/', 'application/vnd.oasis.opendocument.presentation'), ('content.xml', 'text/xml'), ('styles.xml', 'text/xml')]:
    ET.SubElement(manifest, M+'file-entry', {M+'full-path':name, M+'media-type':media})
for i, case in enumerate(variants, 1):
    page = copy.deepcopy(pages[case['source']])
    obj = page.find('.//' + tag('draw:object'))
    old = obj.get(tag('xlink:href')).removeprefix('./')
    root = ET.fromstring(parts[old+'/content.xml'])
    for n in root.iter(tag('style:text-properties')):
        n.set(tag('fo:font-size'), str(case['size'])+'pt')
        n.set(tag('fo:font-family'), case['family'])
        if case.get('style') == 'bold': n.set(tag('fo:font-weight'), 'bold')
        if case.get('style') == 'italic': n.set(tag('fo:font-style'), 'italic')
    chart = root.find('.//' + tag('chart:chart'))
    if case.get('title'):
        heading = ET.Element(tag('chart:title'))
        if case['titleSize'] is not None:
            styles = root.find(tag('office:automatic-styles'))
            style = ET.SubElement(styles, tag('style:style'), {tag('style:name'):'Heading', tag('style:family'):'chart'})
            ET.SubElement(style, tag('style:text-properties'), {
                tag('fo:font-size'):str(case['titleSize'])+'pt', tag('fo:font-family'):case['family']})
            heading.set(tag('chart:style-name'), 'Heading')
        ET.SubElement(heading, tag('text:p')).text = case['title']
        chart.insert(0, heading)
    frame = page.find('.//' + tag('draw:frame'))
    for n in [chart, frame]:
        n.set(tag('svg:width'), str(case['width'])+'px')
        n.set(tag('svg:height'), str(case['height'])+'px')
    frame.set(tag('svg:x'), str((432-case['width'])/2)+'px')
    frame.set(tag('svg:y'), str((432-case['height'])/2)+'px')
    directory = f'Object{i}'
    output[directory+'/content.xml'] = ET.tostring(root)
    obj.set(tag('xlink:href'), './'+directory)
    page.set(tag('draw:name'), case['id'])
    body.append(page)
    for suffix, media in [('/', 'application/vnd.oasis.opendocument.chart'), ('/content.xml', 'text/xml')]:
        ET.SubElement(manifest, M+'file-entry', {M+'full-path':directory+suffix, M+'media-type':media})
output['content.xml'] = ET.tostring(base)
output['META-INF/manifest.xml'] = ET.tostring(manifest)
with zipfile.ZipFile(a.output/'visual.odp', 'w', zipfile.ZIP_DEFLATED) as z:
    for name, data in output.items():
        z.writestr(name, data, compress_type=zipfile.ZIP_STORED if name == 'mimetype' else zipfile.ZIP_DEFLATED)
result = dict(pageSizePx=dict(width=432, height=432), dpi=96,
              cases=[dict(id=v['id'], page=i, family='chart', parameters=v) for i, v in enumerate(variants, 1)],
              inputs={'visual.odp':hashlib.sha256((a.output/'visual.odp').read_bytes()).hexdigest()},
              sourceSha256=hashlib.sha256(a.source.read_bytes()).hexdigest())
(a.output/'manifest.json').write_text(json.dumps(result, indent=2)+'\n')

"""Generate a bounded automatic radar-layout matrix from the standard visual probes."""
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
a = p.parse_args()
a.output.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(a.source) as z:
    parts = {name: z.read(name) for name in z.namelist()}
ns = dict(re.findall(r'xmlns:(\w+)="([^"]+)"', parts['content.xml'].decode()))
for prefix, uri in ns.items():
    ET.register_namespace(prefix, uri)
def tag(name):
    prefix, name = name.split(':')
    return '{' + ns[prefix] + '}' + name
objects = [(name, ET.fromstring(data)) for name, data in parts.items() if name.endswith('/content.xml')]
name, chart_source = next((name, root) for name, root in objects if any(n.get(tag('chart:class')) == 'chart:radar' for n in root.iter()))
base = ET.fromstring(parts['content.xml'])
body = base.find('.//' + tag('office:presentation'))
source_page = next(page for page in body if any(n.get(tag('xlink:href'), '').removeprefix('./') == name.removesuffix('/content.xml') for n in page.iter()))
for page in list(body):
    body.remove(page)
variants = [('small-font', 3, 8, 'end', False, False), ('large-font', 3, 14, 'end', False, False),
            ('five-categories', 5, 10, 'end', False, False), ('long-labels', 7, 10, 'end', True, False),
            ('top-legend', 5, 10, 'top', False, False), ('no-legend', 5, 10, None, False, False),
            ('left-legend', 5, 10, 'start', False, False), ('with-title', 5, 10, 'end', False, True)]
output_parts = {name: data for name, data in parts.items() if '/' not in name or name.startswith('META-INF/')}
manifest = ET.Element('{urn:oasis:names:tc:opendocument:xmlns:manifest:1.0}manifest')
M = '{urn:oasis:names:tc:opendocument:xmlns:manifest:1.0}'
ET.register_namespace('manifest', M[1:-1])
manifest.set(M+'version', '1.3')
ET.SubElement(manifest, M+'file-entry', {M+'full-path':'/', M+'media-type':'application/vnd.oasis.opendocument.presentation'})
for name in ['content.xml', 'styles.xml']:
    ET.SubElement(manifest, M+'file-entry', {M+'full-path':name, M+'media-type':'text/xml'})
for i, (label, count, size, position, long_labels, title) in enumerate(variants, 1):
    root = copy.deepcopy(chart_source)
    for n in root.iter(tag('style:text-properties')):
        n.set(tag('fo:font-size'), f'{size}pt')
    styles = root.find(tag('office:automatic-styles'))
    for style in styles:
        if style.get(tag('style:name')) in ('Category', 'Axis'):
            text = ET.SubElement(style, tag('style:text-properties'))
            text.set(tag('fo:font-size'), f'{size}pt')
            text.set(tag('fo:font-family'), 'Liberation Sans')
    legend_style = ET.SubElement(styles, tag('style:style'), {tag('style:name'):'Legend', tag('style:family'):'chart'})
    ET.SubElement(legend_style, tag('style:text-properties'), {tag('fo:font-size'):f'{size}pt', tag('fo:font-family'):'Liberation Sans'})
    chart = root.find('.//' + tag('chart:chart'))
    legend = chart.find(tag('chart:legend'))
    legend.set(tag('chart:style-name'), 'Legend')
    if position:
        legend.set(tag('chart:legend-position'), position)
    else:
        chart.remove(legend)
    if title:
        heading = ET.Element(tag('chart:title'))
        ET.SubElement(heading, tag('text:p')).text = 'Comparative results'
        chart.insert(0, heading)
    for n in root.iter():
        for key, value in list(n.attrib.items()):
            if key in [tag('table:cell-range-address'), tag('chart:values-cell-range-address')]:
                n.set(key, re.sub(r'([ABC])4$', lambda m: m[1]+str(count+1), value))
            if key == tag('chart:repeated'):
                n.set(key, str(count))
    rows = root.find('.//' + tag('table:table-rows'))
    rows.clear()
    for index in range(count):
        row = ET.SubElement(rows, tag('table:table-row'))
        category = ('Long category label ' if long_labels else 'Category ') + str(index+1)
        for value in [category, 1 + index % 6, 6 - index % 5]:
            attrs = {tag('office:value-type'): 'string' if isinstance(value, str) else 'float'}
            if not isinstance(value, str): attrs[tag('office:value')] = str(value)
            cell = ET.SubElement(row, tag('table:table-cell'), attrs)
            ET.SubElement(cell, tag('text:p')).text = str(value)
    directory = f'Object{i}'
    output_parts[directory+'/content.xml'] = ET.tostring(root)
    ET.SubElement(manifest, M+'file-entry', {M+'full-path':directory+'/', M+'media-type':'application/vnd.oasis.opendocument.chart'})
    ET.SubElement(manifest, M+'file-entry', {M+'full-path':directory+'/content.xml', M+'media-type':'text/xml'})
    page = copy.deepcopy(source_page)
    page.set(tag('draw:name'), label)
    for n in page.iter(tag('draw:object')):
        n.set(tag('xlink:href'), './'+directory)
    body.append(page)
output_parts['content.xml'] = ET.tostring(base)
output_parts['META-INF/manifest.xml'] = ET.tostring(manifest)
with zipfile.ZipFile(a.output/'visual.odp', 'w', zipfile.ZIP_DEFLATED) as z:
    for name, data in output_parts.items():
        z.writestr(name, data, compress_type=zipfile.ZIP_STORED if name == 'mimetype' else zipfile.ZIP_DEFLATED)
result = {'pageSizePx': {'width':432,'height':432}, 'dpi':96,
          'cases':[{'id':v[0],'page':i,'family':'chart'} for i,v in enumerate(variants,1)],
          'inputs':{'visual.odp':hashlib.sha256((a.output/'visual.odp').read_bytes()).hexdigest()},
          'sourceSha256':hashlib.sha256(a.source.read_bytes()).hexdigest()}
(a.output/'manifest.json').write_text(json.dumps(result, indent=2)+'\n')

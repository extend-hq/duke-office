"""Build a local SmartArt layout inventory and cache-free native capture inputs."""

import argparse
import copy
import hashlib
import json
import re
import uuid
from collections import Counter
from pathlib import Path
from zipfile import ZipFile, ZIP_DEFLATED
import xml.etree.ElementTree as ET

DGM = 'http://schemas.openxmlformats.org/drawingml/2006/diagram'
DRAW = 'http://schemas.openxmlformats.org/drawingml/2006/main'
REL = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'
PKG = 'http://schemas.openxmlformats.org/package/2006/relationships'
CT = 'http://schemas.openxmlformats.org/package/2006/content-types'
NS = {'d': DGM, 'a': DRAW}
for prefix, namespace in [('dgm', DGM), ('a', DRAW), ('r', REL)]:
    ET.register_namespace(prefix, namespace)


def serialize(node):
    return ET.tostring(node, encoding='utf-8', xml_declaration=True)


def identifier(layout, value):
    return '{' + str(uuid.uuid5(uuid.NAMESPACE_URL, layout + '/' + value)).upper() + '}'


def semantic_data(layout, root, style_id, colors_id):
    source = next((root.find(path, NS) for path in [
        'd:sampData/d:dataModel', 'd:clrData/d:dataModel', 'd:styleData/d:dataModel'
    ] if root.find(path, NS) is not None), None)
    if source is None:
        source = ET.Element('{' + DGM + '}dataModel')
        points = ET.SubElement(source, '{' + DGM + '}ptLst')
        connections = ET.SubElement(source, '{' + DGM + '}cxnLst')
        ET.SubElement(points, '{' + DGM + '}pt', {'modelId': '0', 'type': 'doc'})
        for index in range(1, 4):
            ET.SubElement(points, '{' + DGM + '}pt', {'modelId': str(index), 'type': 'node'})
            ET.SubElement(connections, '{' + DGM + '}cxn', {'modelId': str(index + 3), 'type': 'parOf', 'srcId': '0', 'destId': str(index), 'srcOrd': str(index - 1), 'destOrd': '0'})
    data = copy.deepcopy(source)
    points = data.findall('d:ptLst/d:pt', NS)
    mapping = {point.get('modelId'): identifier(layout, 'point/' + point.get('modelId')) for point in points}
    for index, point in enumerate(points):
        point.set('modelId', mapping[point.get('modelId')])
        props = point.find('d:prSet', NS)
        if props is None:
            props = ET.Element('{' + DGM + '}prSet')
            point.insert(0, props)
        props.set('phldr', '0')
        if point.get('type') == 'doc':
            props.attrib.update({'loTypeId': layout, 'qsTypeId': style_id, 'csTypeId': colors_id})
        if point.find('d:spPr', NS) is None:
            ET.SubElement(point, '{' + DGM + '}spPr')
        if point.find('d:t', NS) is None:
            body = ET.SubElement(point, '{' + DGM + '}t')
            ET.SubElement(body, '{' + DRAW + '}bodyPr')
            ET.SubElement(body, '{' + DRAW + '}lstStyle')
            paragraph = ET.SubElement(body, '{' + DRAW + '}p')
            if point.get('type') != 'doc':
                run = ET.SubElement(paragraph, '{' + DRAW + '}r')
                ET.SubElement(run, '{' + DRAW + '}rPr', {'lang': 'en-US', 'sz': '1800'})
                ET.SubElement(run, '{' + DRAW + '}t').text = 'Node ' + str(index)
            ET.SubElement(paragraph, '{' + DRAW + '}endParaRPr', {'lang': 'en-US'})
    for index, connection in enumerate(data.findall('d:cxnLst/d:cxn', NS)):
        connection.set('modelId', identifier(layout, 'connection/' + str(index)))
        for name in ['srcId', 'destId']:
            connection.set(name, mapping[connection.get(name)])
        connection.attrib.pop('presId', None)
    for name in ['bg', 'whole']:
        if data.find('d:' + name, NS) is None:
            ET.SubElement(data, '{' + DGM + '}' + name)
    return serialize(data)


def package(template, layouts, style, colors, output):
    parts = {name: data for name, data in template.items() if not name.startswith('word/diagrams/')}
    content_types = ET.fromstring(parts['[Content_Types].xml'])
    for node in list(content_types):
        if '/diagrams/' in node.get('PartName', ''):
            content_types.remove(node)
    relationships = ET.fromstring(parts['word/_rels/document.xml.rels'])
    for node in list(relationships):
        if 'diagram' in node.get('Type', '').lower():
            relationships.remove(node)
    document = parts['word/document.xml'].decode()
    opening = document[:document.index('<w:body>') + len('<w:body>')]
    for prefix, namespace in [('dgm', DGM), ('a', DRAW), ('wp', 'http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing')]:
        if 'xmlns:' + prefix + '=' not in opening:
            opening = opening.replace('<w:document ', '<w:document xmlns:' + prefix + '="' + namespace + '" ', 1)
    section = re.findall(r'<w:sectPr\b[\s\S]*?</w:sectPr>', document)[-1]
    body = []
    for index, item in enumerate(layouts, 1):
        layout, root = item['file'], item['root']
        suffix = str(index)
        part_names = {'Data': 'data', 'Layout': 'layout', 'QuickStyle': 'quickStyle', 'Colors': 'colors'}
        rid = {}
        for kind, prefix in part_names.items():
            name = prefix + suffix + '.xml'
            rid[kind] = 'rIdCatalog' + kind + suffix
            ET.SubElement(relationships, '{' + PKG + '}Relationship', {'Id': rid[kind], 'Type': REL + '/diagram' + kind, 'Target': 'diagrams/' + name})
            ckind = {'Data': 'Data', 'Layout': 'Layout', 'QuickStyle': 'Style', 'Colors': 'Colors'}[kind]
            ET.SubElement(content_types, '{' + CT + '}Override', {'PartName': '/word/diagrams/' + name, 'ContentType': 'application/vnd.openxmlformats-officedocument.drawingml.diagram' + ckind + '+xml'})
        parts['word/diagrams/data' + suffix + '.xml'] = semantic_data(root.get('uniqueId'), root, ET.fromstring(style).get('uniqueId'), ET.fromstring(colors).get('uniqueId'))
        parts['word/diagrams/layout' + suffix + '.xml'] = layout.read_bytes()
        parts['word/diagrams/quickStyle' + suffix + '.xml'] = style
        parts['word/diagrams/colors' + suffix + '.xml'] = colors
        if index > 1:
            body.append('<w:p><w:r><w:br w:type="page"/></w:r></w:p>')
        body.append('<w:p><w:r><w:rPr><w:b/><w:sz w:val="28"/></w:rPr><w:t>' + layout.stem + '</w:t></w:r></w:p>')
        body.append('<w:p><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="5486400" cy="3657600"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="' + str(index) + '" name="Diagram ' + str(index) + '"/><wp:cNvGraphicFramePr/><a:graphic><a:graphicData uri="' + DGM + '"><dgm:relIds r:dm="' + rid['Data'] + '" r:lo="' + rid['Layout'] + '" r:qs="' + rid['QuickStyle'] + '" r:cs="' + rid['Colors'] + '"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>')
    parts['word/document.xml'] = (opening + ''.join(body) + section + '</w:body></w:document>').encode()
    parts['[Content_Types].xml'] = serialize(content_types)
    parts['word/_rels/document.xml.rels'] = serialize(relationships)
    for name, value in parts.items():
        if name.endswith(('.xml', '.rels')):
            ET.fromstring(value)
    with ZipFile(output, 'w', ZIP_DEFLATED) as archive:
        for name, value in parts.items():
            archive.writestr(name, value)
    return {'document': output.name, 'sha256': hashlib.sha256(output.read_bytes()).hexdigest(), 'diagrams': len(layouts), 'nativeStatus': 'unmeasured', 'cachedDrawings': 0}


def generate(resources, template_path, output):
    output.mkdir(parents=True, exist_ok=True)
    raw = []
    algorithms, categories, shapes = Counter(), Counter(), Counter()
    for path in sorted((resources / 'lo').glob('*.glo')):
        root = ET.parse(path).getroot()
        kinds = sorted({node.get('type') for node in root.findall('d:catLst/d:cat', NS)})
        algs = sorted({node.get('type') for node in root.findall('.//d:alg', NS)})
        presets = sorted({node.get('type') for node in root.findall('.//d:shape', NS) if node.get('type')})
        raw.append({'file': path, 'root': root, 'id': root.get('uniqueId'), 'categories': kinds, 'algorithms': algs, 'shapePresets': presets, 'sampleSource': next((tag for tag in ['sampData', 'clrData', 'styleData'] if root.find('d:' + tag + '/d:dataModel', NS) is not None), 'generated-three-node-graph'), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
    selected = list({item['id']: item for item in raw}.values())
    for item in selected:
        algorithms.update(item['algorithms']); categories.update(value.lower() for value in item['categories']); shapes.update(item['shapePresets'])
    inventory = {'resourceDirectory': str(resources), 'rawLayoutFiles': len(raw), 'uniqueLayoutIds': len(selected), 'categoryCounts': categories, 'algorithmCounts': algorithms, 'shapePresetCounts': shapes, 'layouts': [{key: str(value) if key == 'file' else value for key, value in item.items() if key != 'root'} for item in selected]}
    headers = ET.parse(resources / 'lo' / 'layoutdefhdrlst.xml').getroot()
    header_ids = {node.get('uniqueId', '').strip().lower() for node in headers}
    file_ids = {item['id'].strip().lower() for item in selected}
    inventory.update({'galleryHeaderLayoutCount': len(headers), 'fileIdsNotInGalleryHeaders': sorted(file_ids - header_ids), 'galleryHeaderIdsWithoutLayoutFile': sorted(header_ids - file_ids)})
    (output / 'installed-layout-inventory.json').write_text(json.dumps(inventory, indent=2) + '\n')
    with ZipFile(template_path) as archive:
        template = {name: archive.read(name) for name in archive.namelist()}
    style = template['word/diagrams/quickStyle1.xml']
    colors = template['word/diagrams/colors1.xml']
    representatives = ['list1', 'process1', 'cycle1', 'orgchart1', 'venn1', 'matrix1', 'pyramid1', 'picturegrid']
    family_rows = [next(item for item in selected if item['file'].stem == name) for name in representatives]
    manifests = [package(template, family_rows, style, colors, output / 'smartart-eight-families-cache-free.docx'), package(template, selected, style, colors, output / 'smartart-all-installed-layouts-cache-free.docx')]
    manifests.append(package(template, [family_rows[0]], style, colors, output / 'smartart-list-control-cache-free.docx'))
    (output / 'manifest.json').write_text(json.dumps({'documents': manifests, 'sourceTemplateSha256': hashlib.sha256(template_path.read_bytes()).hexdigest(), 'scope': 'Local native-capture inputs using installed layout templates. No saved drawings are invented. Parser semantic fallback is expected until Office materializes the layouts.', 'redistribution': 'Keep installed template XML and these derived capture inputs in local artifacts; do not add them to the public fixture bundle.'}, indent=2) + '\n')
    print(json.dumps({'output': str(output), 'uniqueLayouts': len(selected), 'documents': manifests}, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--office-resources', type=Path, required=True)
    parser.add_argument('--template-docx', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    generate(args.office_resources, args.template_docx, args.output)

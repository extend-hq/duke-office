import base64
from xml.etree import ElementTree as ET

SVG = '{http://www.w3.org/2000/svg}'
DATA_SVG = 'data:image/svg+xml;base64,'


def normalized(svg, depth=0):
    if depth > 16:
        raise ValueError('Embedded SVG nesting exceeds comparison budget')
    root = ET.fromstring(svg)
    definitions = root.find(SVG + 'defs')
    removed = 0
    if definitions is not None:
        candidates = [n for n in definitions if n.tag == SVG + 'g' and n.attrib == {'id': 'picture-body'}]
        for candidate in candidates:
            subtree = {id(n) for n in candidate.iter()}
            referenced = any('#picture-body' in value for node in root.iter() if id(node) not in subtree for value in node.attrib.values())
            if not referenced:
                definitions.remove(candidate)
                removed += 1
    for node in root.iter():
        for key, value in node.attrib.items():
            if value.startswith(DATA_SVG):
                nested, nested_removed = normalized(base64.b64decode(value[len(DATA_SVG):], validate=True), depth + 1)
                node.set(key, DATA_SVG + base64.b64encode(nested).decode())
                removed += nested_removed
    return ET.tostring(root), removed


def equivalent_svg(before, after):
    old, old_removed = normalized(before)
    new, new_removed = normalized(after)
    return old == new and old_removed > new_removed

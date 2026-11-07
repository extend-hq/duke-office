import argparse
import json
import re
from xml.sax.saxutils import quoteattr
import zipfile

from artwork_math_gate import artwork_hashes
from comments_probes import package
from feedback import OUT, invoke as compact
from object_gate import invoke
from object_editing_review import ALLOWED, LOCKS, nodes, sha, verify, without_editing


def probe(path, variant):
    strict = variant == 'strict'
    package(path, strict=strict)
    with zipfile.ZipFile(path) as archive:
        parts = {n: archive.read(n) for n in archive.namelist()}
    p = 'http://purl.oclc.org/ooxml/presentationml/main' if strict else 'http://schemas.openxmlformats.org/presentationml/2006/main'
    d = 'http://purl.oclc.org/ooxml/drawingml/main' if strict else 'http://schemas.openxmlformats.org/drawingml/2006/main'
    p14 = 'http://schemas.microsoft.com/office/powerpoint/2010/main'
    a14 = 'http://schemas.microsoft.com/office/drawing/2010/main'
    roots = [('shape', 'sp', 'nvSpPr', 'cNvSpPr', 'spLocks'), ('connector', 'cxnSp', 'nvCxnSpPr', 'cNvCxnSpPr', 'cxnSpLocks'), ('picture', 'pic', 'nvPicPr', 'cNvPicPr', 'picLocks'), ('group', 'grpSp', 'nvGrpSpPr', 'cNvGrpSpPr', 'grpSpLocks'), ('graphicFrame', 'graphicFrame', 'nvGraphicFramePr', 'cNvGraphicFramePr', 'graphicFrameLocks'), ('contentPart', 'contentPart', 'nvContentPartPr', 'cNvContentPartPr', 'cpLocks')]
    values = {'true': '1', 'false': 'false', 'invalid': 'TRUE', 'whitespace': ' 0 ', 'empty': None}
    objects = []
    for index, (kind, tag, nv, prop, lock) in enumerate(roots, 2):
        prefix = 'q' if kind == 'contentPart' else 'p'
        lock_prefix = 'b' if kind == 'contentPart' else 'a'
        value = values.get(variant, 'true')
        attributes = '' if value is None else ' '.join(k + '=' + quoteattr(value) for k in LOCKS)
        if variant == 'qualified-attributes':
            attributes = ' '.join('e:' + k + '="true"' for k in LOCKS)
        lock_xml = f'<{lock_prefix}:{lock} {attributes}/>'
        if variant == 'foreign-locks':
            lock_xml = f'<e:{lock} {attributes}/>'
        if variant == 'nested-lookalikes':
            lock_xml = f'<a:extLst><a:ext uri="urn:editing"><{lock_prefix}:{lock} {attributes}/></a:ext></a:extLst>'
        if variant == 'shadowed-prefix':
            lock_xml = f'<{lock_prefix}:{lock} xmlns:{lock_prefix}="urn:foreign" {attributes}/>'
        if variant == 'default-drawing':
            uri = a14 if kind == 'contentPart' else d
            lock_xml = f'<{lock} xmlns="{uri}" {attributes}/>'
        if variant == 'extension-only':
            lock_xml = '<a:extLst><a:ext uri="urn:editing"><e:data e:flag="yes"> preserved &amp; <![CDATA[raw]]></e:data></a:ext></a:extLst>'
        if variant == 'absent':
            lock_xml = ''
        attributes = {'shape': 'txBox', 'picture': 'preferRelativeResize', 'contentPart': 'isComment'}
        attr = attributes.get(kind)
        prop_attributes = (' ' + attr + '=' + quoteattr(value)) if attr and value is not None else ''
        if variant == 'qualified-attributes' and attr:
            prop_attributes = ' e:' + attr + '="true"'
        if variant in {'extension-only', 'absent'}:
            prop_attributes = ''
        endpoints = ''
        if kind == 'connector':
            ident, site = ('0002', '+4294967295') if variant == 'uint-limits' else ('2', '0')
            ident, site = ('-000', ' +000 ') if variant == 'negative-zero' else (ident, site)
            ident, site = ('4294967296', '-1') if variant == 'invalid-uint' else (ident, site)
            endpoints = f'<a:stCxn id={quoteattr(ident)} idx={quoteattr(site)}/><a:endCxn id="99" idx="1"/>'
            if variant == 'qualified-attributes':
                endpoints = '<a:stCxn e:id="2" idx="0"/><a:endCxn id="2" e:idx="0"/>'
            if variant == 'missing-uint':
                endpoints = '<a:stCxn id="2"/><a:endCxn idx="0"/>'
            if variant == 'foreign-endpoints':
                endpoints = '<e:stCxn id="2" idx="0"/><a:extLst><a:ext uri="urn:nested"><a:endCxn id="2" idx="0"/></a:ext></a:extLst>'
            if variant == 'absent':
                endpoints = ''
        properties = f'<{prefix}:{prop}{prop_attributes}>{lock_xml}{endpoints}</{prefix}:{prop}>'
        if variant == 'foreign-properties':
            properties = properties.replace(f'{prefix}:{prop}', f'e:{prop}')
        geometry = '<p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>' if kind in {'shape', 'connector', 'picture'} else ''
        nested = ''
        if kind == 'group':
            nested = '<p:grpSpPr/>' + objects[0].replace('id="2"', 'id="12"')
        objects.append(f'<{prefix}:{tag}><{prefix}:{nv}><{prefix}:cNvPr id="{index}" name="Object {index}"/>{properties}<{prefix}:nvPr/></{prefix}:{nv}>{geometry}{nested}</{prefix}:{tag}>')
        if variant == 'foreign-nonvisual':
            objects[-1] = objects[-1].replace(f'{prefix}:{nv}', f'e:{nv}')
        if variant == 'foreign-object':
            objects[-1] = re.sub(r'(<\/?)' + prefix + ':' + tag + r'(?=[ >])', r'\1e:' + tag, objects[-1])
    source = f'<p:sld xmlns:p="{p}" xmlns:a="{d}" xmlns:q="{p14}" xmlns:b="{a14}" xmlns:e="urn:foreign"><p:cSld><p:spTree>{"".join(objects)}</p:spTree></p:cSld></p:sld>'
    if variant == 'renamed-prefixes':
        source = re.sub(r'(<\/?)p:', r'\1pres:', source.replace('xmlns:p=', 'xmlns:pres='))
        source = re.sub(r'(<\/?)a:', r'\1draw:', source.replace('xmlns:a=', 'xmlns:draw='))
    if variant == 'default-host':
        source = re.sub(r'(<\/?)p:', r'\1', source.replace('xmlns:p=', 'xmlns='))
    parts['ppt/slides/slide1.xml'] = source.encode()
    with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
        for name, data in parts.items():
            archive.writestr(name, data)


def main():
    p = argparse.ArgumentParser(); p.add_argument('--label', required=True); a = p.parse_args()
    run = OUT/'experiments'/a.label
    destination = run/'editing-probes'; destination.mkdir(exist_ok=False)
    variants = ['true', 'false', 'invalid', 'whitespace', 'empty', 'strict', 'renamed-prefixes', 'default-host', 'default-drawing', 'qualified-attributes', 'foreign-locks', 'foreign-properties', 'nested-lookalikes', 'shadowed-prefix', 'extension-only', 'uint-limits', 'negative-zero', 'invalid-uint', 'missing-uint', 'foreign-endpoints', 'absent', 'foreign-nonvisual', 'foreign-object']
    receipts, manifest, browser = [], [], []
    for variant in variants:
        path = destination/(variant+'.pptx'); probe(path, variant)
        row = dict(id=variant, path=str(path.resolve()), sha256=sha(path)); manifest.append(row)
        old, before = invoke(OUT/'experiments/xml-indexed1/candidate', row, destination/(variant+'.before.json.gz'))
        new, after = invoke(run/'candidate', row, destination/(variant+'.json.gz'))
        assert old['status'] == new['status'] == 'ok', (variant, old, new)
        assert without_editing(after) == before, variant
        counts = verify(path, after)
        assert counts['nodes_checked'] == 7, (variant, counts)
        if variant in {'true', 'false', 'whitespace', 'strict', 'renamed-prefixes', 'default-host', 'default-drawing'}:
            assert counts['editing_nodes'] == 7
            for node in nodes(after):
                editing = node['editing']
                assert set(editing['locks']) == {LOCKS[k] for k in ALLOWED[editing['kind']]}
                assert all(value == (variant not in {'false', 'whitespace'}) for value in editing['locks'].values())
        if variant in {'foreign-properties', 'foreign-nonvisual', 'foreign-object'}:
            assert counts['editing_nodes'] == 0
        if variant == 'absent':
            assert counts['editing_nodes'] == 1 and counts['endpoints'] == 0
        if variant in {'qualified-attributes', 'foreign-endpoints', 'invalid-uint', 'missing-uint'}:
            assert counts['endpoints'] == (1 if variant == 'invalid-uint' else 0)
        record = compact(run/'candidate', 'rust-pptx', row, destination/(variant+'.compact.json.gz'), 0)
        assert record['full_sha256'] == new['full_sha256']
        receipts.append(dict(id=variant, input_sha256=row['sha256'], passed=True, counts=dict(counts), full_sha256=new['full_sha256'], artwork_hashes=artwork_hashes(after)))
        browser.append(dict(id=variant, model=after))
        print(variant, dict(counts), flush=True)
    (destination/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    (destination/'browser-models.json').write_text(json.dumps(browser)+'\n')
    (run/'editing-probes-summary.json').write_text(json.dumps(dict(total=len(receipts), passed=len(receipts), candidate_sha256=sha(run/'candidate'), harness_sha256=sha(__file__), review_harness_sha256=sha(__file__.replace('_probes.py','_review.py')), receipts=receipts), indent=2)+'\n')


if __name__ == '__main__':
    main()

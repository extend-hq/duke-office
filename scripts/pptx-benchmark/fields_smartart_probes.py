import argparse
import hashlib
import io
import json
import zipfile

from artwork_math_gate import artwork_hashes
from feedback import OUT, invoke as compact
from fields_probes import package
from fields_review import verify_sources, visit
from object_gate import invoke


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--label', required=True); args = parser.parse_args()
    run = OUT / 'experiments' / args.label; target = run / 'smartart-probes'; target.mkdir()
    rows, records, receipts = [], [], []
    variants = [('references', '<a:t>A&#13;B&#10;C&#9;D</a:t>', 'A\rB\nC\tD', 'custom&#9;type', 'custom\ttype'), ('literal-lines', '<a:t>A\r\nB\rC</a:t>', 'A\nB\nC', 'custom\r\ntype', 'custom type'), ('missing', '', '', 'custom', 'custom'), ('empty', '<a:t/>', '', 'custom', 'custom')]
    for materialized in [False, True]:
        for variant, content, expected_text, source_type, expected_type in variants:
            name = ('cached-' if materialized else 'semantic-') + variant
            path = (target / (name + '.pptx')).resolve(); package(path)
            with zipfile.ZipFile(io.BytesIO(path.read_bytes())) as archive:
                parts = {n: archive.read(n) for n in archive.namelist()}
            frame = '<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="9" name="Diagram"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/diagram"><dgm:relIds xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" r:dm="diagram"/></a:graphicData></a:graphic></p:graphicFrame>'
            parts['ppt/slides/slide1.xml'] = parts['ppt/slides/slide1.xml'].replace(b'</p:spTree>', frame.encode() + b'</p:spTree>')
            parts['ppt/slides/_rels/slide1.xml.rels'] = parts['ppt/slides/_rels/slide1.xml.rels'].replace(b'</Relationships>', b'<Relationship Id="diagram" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/diagramData" Target="../diagrams/data.xml"/></Relationships>')
            field = '<a:fld id="smartart" type="' + source_type + '">' + content + '</a:fld>'
            body = '<a:bodyPr/><a:lstStyle/><a:p>' + field + '</a:p>'
            extension = '<dgm:extLst><a:ext uri="diagram"><dsp:dataModelExt relId="drawing"/></a:ext></dgm:extLst>' if materialized else ''
            namespaces = 'xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:dsp="http://schemas.microsoft.com/office/drawing/2008/diagram"'
            parts['ppt/diagrams/data.xml'] = ('<dgm:dataModel ' + namespaces + '><dgm:ptLst><dgm:pt modelId="node" type="node"><dgm:t>' + body + '</dgm:t></dgm:pt></dgm:ptLst>' + extension + '</dgm:dataModel>').encode()
            if materialized:
                parts['ppt/diagrams/_rels/data.xml.rels'] = b'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="drawing" Type="http://schemas.microsoft.com/office/2007/relationships/diagramDrawing" Target="drawing.xml"/></Relationships>'
                parts['ppt/diagrams/drawing.xml'] = ('<dsp:drawing ' + namespaces + '><dsp:spTree><dsp:grpSpPr/><dsp:sp modelId="node"><dsp:nvSpPr><dsp:cNvPr id="1" name="Node"/><dsp:cNvSpPr/></dsp:nvSpPr><dsp:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"/></dsp:spPr><dsp:txBody>' + body + '</dsp:txBody></dsp:sp></dsp:spTree></dsp:drawing>').encode()
            with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
                for part, value in parts.items():
                    info = zipfile.ZipInfo(part, (2000, 1, 1, 0, 0, 0)); info.compress_type = zipfile.ZIP_DEFLATED
                    archive.writestr(info, value)
            row = dict(id=name, path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest()); rows.append(row)
            record, document = invoke(run / 'candidate', row, target / (name + '.full.json.gz'))
            assert record['status'] == 'ok', record
            counts = dict(verify_sources(path, document))
            fields = [(location, value) for location, value in visit(document) if value['field']['id'] == 'smartart']
            assert len(fields) == 1, fields
            location, value = fields[0]
            assert value['text'] == expected_text and value['field']['type'] == expected_type, value
            assert value['field']['hasCachedText'] == (variant != 'missing')
            assert value['field']['sourcePart'] == ('ppt/diagrams/drawing.xml' if materialized else 'ppt/diagrams/data.xml')
            result = compact(run / 'candidate', 'duke-slides', row, target / (name + '.compact.json.gz'), 0)
            result.update(id=name, artwork_hashes=artwork_hashes(document)); records.append(result)
            receipts.append(dict(id=name, input_sha256=row['sha256'], field_location=location, expected_text=expected_text, expected_type=expected_type, counts=counts, passed=True)); print(name, 'passed', flush=True)
    (target / 'manifest.json').write_text(json.dumps(rows, indent=2))
    destination = target / 'records.jsonl'; destination.write_text(''.join(json.dumps(r) + '\n' for r in records))
    summary = dict(total=len(receipts), passed=len(receipts), receipts=receipts, records_sha256=hashlib.sha256(destination.read_bytes()).hexdigest(), candidate_sha256=hashlib.sha256((run / 'candidate').read_bytes()).hexdigest(), harness_sha256=hashlib.sha256(__import__('pathlib').Path(__file__).read_bytes()).hexdigest(), scope='Materialized and semantic-fallback SmartArt fields: original XML character-reference and literal-line semantics, custom types, source part identity, absent and empty caches. Semantic fallback geometry remains explicitly degraded.')
    (run / 'fields-smartart-summary.json').write_text(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()

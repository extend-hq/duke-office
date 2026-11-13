import argparse
import hashlib
import json
from pathlib import Path
import zipfile

from artwork_math_gate import artwork_hashes
from feedback import OUT, invoke as compact
from fields_review import verify_sources, visit
from object_gate import invoke


def package(path, strict=False, alias="a", bom=False, external=False):
    drawing = "http://purl.oclc.org/ooxml/drawingml/main" if strict else "http://schemas.openxmlformats.org/drawingml/2006/main"
    presentation = "http://purl.oclc.org/ooxml/presentationml/main" if strict else "http://schemas.openxmlformats.org/presentationml/2006/main"
    relationship = "http://purl.oclc.org/ooxml/officeDocument/relationships" if strict else "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
    relns = "http://schemas.openxmlformats.org/package/2006/relationships"
    ns = f'xmlns:p="{presentation}" xmlns:a="{drawing}" xmlns:r="{relationship}" xmlns:e="urn:extension"'
    rpr = '<a:rPr sz="2400" b="0" lang="en-US"><a:solidFill><a:srgbClr val="2468AC"/></a:solidFill></a:rPr>'
    ppr = '<a:pPr algn="ctr" lvl="2" rtl="1" marL="1200" marR="1800" indent="-600" defTabSz="914400" fontAlgn="base" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:lnSpc><a:spcPct val="120000"/></a:lnSpc><a:spcBef><a:spcPts val="250"/></a:spcBef><a:spcAft><a:spcPts val="0"/></a:spcAft><a:buClr><a:srgbClr val="112233"/></a:buClr><a:buSzPct val="75000"/><a:buFont typeface="Arial"/><a:buChar char="•"/><a:tabLst><a:tab pos="914400" algn="ctr"/><a:tab pos="1828800" algn="dec"/></a:tabLst><a:defRPr sz="1800" b="1" i="1" spc="75" kern="1200" baseline="5000" lang="fr-FR"><a:latin typeface="Arial"/><a:solidFill><a:srgbClr val="ABCDEF"/></a:solidFill></a:defRPr></a:pPr>'
    values = [
        '<a:fld id="number" type="slidenum"><a:t>7</a:t></a:fld>',
        '<a:fld id="custom" type="custom:extension"><a:t> saved value </a:t></a:fld>',
        '<a:fld id="absent"/>',
        '<a:fld id="empty" type=""><a:t/></a:fld>',
        '<a:fld id="whitespace" type=" A\tB&#9;C\r\nD "><a:t> leading\r\nline\rtab\t&amp;<![CDATA[&lt;\r\n]]>&#13; tail </a:t></a:fld>',
        '<a:fld id="styled" type="datetimeFigureOut">' + rpr + ppr + '<a:t>saved date</a:t></a:fld>',
        '<a:fld id="defaults"><a:pPr><a:defRPr sz="1900" b="1"><a:latin typeface="Arial"/></a:defRPr></a:pPr><a:t>defaults</a:t></a:fld>',
        '<e:fld id="foreign"><a:t>must be ignored</a:t></e:fld>',
        '<a:fld id="collision" e:id="foreign" type="custom" e:type="foreign"><e:rPr sz="9900"/><a:rPr sz="1700"/><e:pPr algn="r"/><a:pPr><a:buNone/><a:buClrTx/><a:buFontTx/><a:buSzTx/></a:pPr><e:t>must be ignored</e:t><a:t>retained</a:t></a:fld>',
        '<a:fld id="picture"><a:pPr><a:buBlip><a:blip r:embed="image"/></a:buBlip></a:pPr><a:t>image bullet</a:t></a:fld>',
    ]
    body = '<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz="1100" b="0"/></a:pPr>' + ''.join(values) + '</a:p></p:txBody>'
    def shape(ident, content, placeholder=""):
        return f'<p:sp><p:nvSpPr><p:cNvPr id="{ident}" name="Text"/><p:cNvSpPr/><p:nvPr>{placeholder}</p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>{content}</p:sp>'
    small = lambda ident: '<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:fld id="' + ident + '" type="custom"><a:t>retained</a:t></a:fld></a:p></p:txBody>'
    table = '<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="3" name="Table"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table"><a:tbl><a:tblPr/><a:tblGrid><a:gridCol w="914400"/></a:tblGrid><a:tr h="914400"><a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:fld id="table" type="custom"><a:t>cell value</a:t></a:fld></a:p></a:txBody><a:tcPr/></a:tc></a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame>'
    def rel(ident, kind, target, mode=""):
        return f'<Relationship Id="{ident}" Type="{relationship}/{kind}" Target="{target}"{mode}/>'
    def relationships(contents):
        return '<Relationships xmlns="' + relns + '">' + contents + '</Relationships>'
    parts = {
        '[Content_Types].xml': '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>',
        '_rels/.rels': relationships(rel('root', 'officeDocument', 'ppt/presentation.xml')),
        'ppt/presentation.xml': f'<p:presentation {ns}><p:sldMasterIdLst><p:sldMasterId id="1" r:id="master"/></p:sldMasterIdLst><p:sldIdLst><p:sldId id="256" r:id="slide"/></p:sldIdLst><p:sldSz cx="9144000" cy="6858000"/></p:presentation>',
        'ppt/_rels/presentation.xml.rels': relationships(rel('slide', 'slide', 'slides/slide1.xml') + rel('master', 'slideMaster', 'slideMasters/master.xml')),
        'ppt/slides/slide1.xml': f'<p:sld {ns}><p:cSld><p:spTree>{shape(2, body)}{table}</p:spTree></p:cSld></p:sld>',
        'ppt/slides/_rels/slide1.xml.rels': relationships(rel('layout', 'slideLayout', '../slideLayouts/layout.xml') + rel('notes', 'notesSlide', '../notesSlides/notes.xml') + rel('image', 'image', '../media/bullet.png', ' TargetMode="External"' if external else '')),
        'ppt/slideLayouts/layout.xml': f'<p:sldLayout {ns}><p:cSld><p:spTree>{shape(4, small("layout"))}</p:spTree></p:cSld></p:sldLayout>',
        'ppt/slideLayouts/_rels/layout.xml.rels': relationships(rel('master', 'slideMaster', '../slideMasters/master.xml')),
        'ppt/slideMasters/master.xml': f'<p:sldMaster {ns}><p:cSld><p:spTree>{shape(5, small("master"))}</p:spTree></p:cSld><p:sldLayoutIdLst><p:sldLayoutId id="1" r:id="layout"/></p:sldLayoutIdLst></p:sldMaster>',
        'ppt/slideMasters/_rels/master.xml.rels': relationships(rel('layout', 'slideLayout', '../slideLayouts/layout.xml')),
        'ppt/notesSlides/notes.xml': f'<p:notes {ns}><p:cSld><p:spTree>{shape(6, small("note"), '<p:ph type="body"/>')}</p:spTree></p:cSld></p:notes>',
        'ppt/media/bullet.png': bytes.fromhex('89504e470d0a1a0a'),
    }
    with zipfile.ZipFile(path, 'w') as archive:
        for name, value in parts.items():
            if isinstance(value, str):
                value = value.replace('xmlns:a=', 'xmlns:' + alias + '=').replace('<a:', '<' + alias + ':').replace('</a:', '</' + alias + ':').encode()
                if bom:
                    value = b'\xef\xbb\xbf' + value
            info = zipfile.ZipInfo(name, (2000, 1, 1, 0, 0, 0)); info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, value)


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--label', required=True); args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    destination = run / 'probes'; destination.mkdir()
    binary = run / 'candidate'
    cases = [('transitional', {}), ('strict', dict(strict=True)), ('alias', dict(alias='drawing')), ('bom', dict(bom=True)), ('external', dict(external=True))]
    rows, records, receipts = [], [], []
    for name, options in cases:
        path = (destination / (name + '.pptx')).resolve(); package(path, **options)
        row = dict(id=name, path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest()); rows.append(row)
        result, document = invoke(binary, row, destination / (name + '.full.json.gz'))
        assert result['status'] == 'ok', result
        counts = verify_sources(path, document)
        fields = {run['field']['id']: run for _, run in visit(document)}
        assert set(fields) == {'number', 'custom', 'absent', 'empty', 'whitespace', 'styled', 'defaults', 'collision', 'picture', 'table', 'layout', 'master', 'note'}, fields.keys()
        assert fields['number']['text'] == '7' and fields['custom']['text'] == ' saved value '
        assert fields['absent']['field']['hasCachedText'] is False and fields['empty']['field']['hasCachedText'] is True
        assert fields['whitespace']['text'] == ' leading\nline\ntab\t&&lt;\n\r tail '
        assert fields['whitespace']['field']['type'] == ' A B\tC D '
        styled = fields['styled']; assert styled['fontSizePt'] == 24 and styled['bold'] is False and styled['italic'] is True and styled['fontFamily'] == 'Arial' and styled['language'] == 'en-US'
        assert fields['defaults']['fontSizePt'] == 19 and fields['defaults']['bold'] is True
        assert fields['collision']['text'] == 'retained' and fields['collision']['fontSizePt'] == 17 and fields['collision']['field']['type'] == 'custom'
        picture = fields['picture']['field']['paragraphProperties']
        assert picture.get('bulletImageAssetId') == (None if options.get('external') else 'ppt/media/bullet.png')
        record = compact(binary, 'duke-pptx', row, destination / (name + '.compact.json.gz'), 0)
        record.update(id=name, artwork_hashes=artwork_hashes(document)); records.append(record)
        receipts.append(dict(id=name, input_sha256=row['sha256'], counts=dict(counts), full_sha256=result['full_sha256'], passed=True))
        print(name, dict(counts), flush=True)
    (destination / 'manifest.json').write_text(json.dumps(rows, indent=2))
    records_path = destination / 'records.jsonl'; records_path.write_text(''.join(json.dumps(r) + '\n' for r in records))
    summary = dict(total=len(receipts), passed=len(receipts), receipts=receipts, candidate_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), records_sha256=hashlib.sha256(records_path.read_bytes()).hexdigest(), harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), scope='Independent synthetic source checks across namespaces, prefix aliases, BOMs, foreign elements/attributes, custom field types, absent and empty caches, XML whitespace, paragraph/run properties and override precedence, internal/external bullet relationships, slide/table/notes/layout/master fields.')
    (run / 'fields-probes-summary.json').write_text(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()

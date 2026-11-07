import argparse
import json
from pathlib import Path
import zipfile

from artwork_math_gate import artwork_hashes
from comments_probes import package
from feedback import OUT
from object_gate import invoke
from table_grid_review import sha, tables, unmerge, verify, without_rows, yes


def probe(path, variant):
    strict = variant == 'strict'
    package(path, strict=strict)
    with zipfile.ZipFile(path) as archive:
        parts = {n: archive.read(n) for n in archive.namelist()}
    p = 'http://purl.oclc.org/ooxml/presentationml/main' if strict else 'http://schemas.openxmlformats.org/presentationml/2006/main'
    d = 'http://purl.oclc.org/ooxml/drawingml/main' if strict else 'http://schemas.openxmlformats.org/drawingml/2006/main'
    rel = 'http://purl.oclc.org/ooxml/officeDocument/relationships' if strict else 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'
    attrs = [[{} for _ in range(4)] for _ in range(4)]

    def merge(r, c, height, width):
        for y in range(r, r + height):
            for x in range(c, c + width):
                a = attrs[y][x]
                if y == r and height > 1:
                    a['rowSpan'] = str(height)
                if x == c and width > 1:
                    a['gridSpan'] = str(width)
                if x > c:
                    a['hMerge'] = '1'
                if y > r:
                    a['vMerge'] = 'true'

    if variant == 'horizontal':
        merge(0, 0, 1, 2)
    elif variant == 'vertical':
        merge(0, 1, 3, 1)
    elif variant == 'adjacent':
        merge(0, 0, 2, 2); merge(0, 2, 3, 2)
    elif variant == 'orphan':
        attrs[0][0] = dict(hMerge='1')
        attrs[0][1] = dict(vMerge='true')
    elif variant == 'inconsistent':
        merge(0, 0, 1, 2)
        attrs[0][1]['vMerge'] = '1'
    elif variant == 'huge-span':
        attrs[0][0] = dict(rowSpan='2147483647', gridSpan='2147483647')
        attrs[0][1] = dict(hMerge='1')
    elif variant == 'false-flags':
        attrs[0][0] = dict(hMerge='false', vMerge='0', rowSpan='1', gridSpan='1')
    else:
        merge(0, 0, 3, 3)
    if variant == 'ragged':
        attrs[3] = attrs[3][:2]
    rows, visible, covered = [], [], []
    for r, row in enumerate(attrs):
        cells = []
        for c, a in enumerate(row):
            hidden = yes(a.get('hMerge')) or yes(a.get('vMerge'))
            text = ('Covered' if hidden else 'Visible') + f' {r} {c}'
            (covered if hidden else visible).append(text)
            attributes = ' '.join(f'{k}="{v}"' for k, v in a.items())
            cells.append(f'<a:tc {attributes}><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr b="1" sz="1400"><a:hlinkClick r:id="link"/></a:rPr><a:t>{text}</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc>')
        rows.append('<a:tr h="400000">' + ''.join(cells) + '</a:tr>')
    grid = ''.join(f'<a:gridCol w="{width}"/>' for width in [600000, 900000, 1200000, 1500000])
    parts['ppt/slides/slide1.xml'] = f'<p:sld xmlns:p="{p}" xmlns:a="{d}" xmlns:r="{rel}"><p:cSld><p:spTree><p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="2" name="Grid"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="0" y="0"/><a:ext cx="4200000" cy="1600000"/></p:xfrm><a:graphic><a:graphicData uri="{d}/table"><a:tbl><a:tblPr bandCol="1" firstCol="1" lastCol="1"><a:tableStyleId>grid</a:tableStyleId></a:tblPr><a:tblGrid>{grid}</a:tblGrid>{"".join(rows)}</a:tbl></a:graphicData></a:graphic></p:graphicFrame></p:spTree></p:cSld></p:sld>'.encode()
    styles = ''.join(f'<a:{name}><a:tcStyle><a:fill><a:solidFill><a:srgbClr val="{color}"/></a:solidFill></a:fill></a:tcStyle></a:{name}>' for name, color in [('wholeTbl', 'FFFFFF'), ('band1V', 'FF0000'), ('band2V', '0000FF'), ('firstCol', '00FF00'), ('lastCol', 'FFFF00')])
    parts['ppt/tableStyles.xml'] = f'<a:tblStyleLst xmlns:a="{d}" def="grid"><a:tblStyle styleId="grid" styleName="Grid">{styles}</a:tblStyle></a:tblStyleLst>'.encode()
    parts['ppt/_rels/presentation.xml.rels'] = parts['ppt/_rels/presentation.xml.rels'].replace(b'</Relationships>', f'<Relationship Id="styles" Type="{rel}/tableStyles" Target="tableStyles.xml"/></Relationships>'.encode())
    parts['ppt/slides/_rels/slide1.xml.rels'] = f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="link" Type="{rel}/hyperlink" Target="https://example.invalid/" TargetMode="External"/></Relationships>'.encode()
    with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
        for name, data in parts.items():
            archive.writestr(name, data)
    return dict(visible=visible, covered=covered)


def main():
    p = argparse.ArgumentParser(); p.add_argument('--label', required=True); a = p.parse_args()
    run = OUT / 'experiments' / a.label
    destination = run / 'table-probes'; destination.mkdir(exist_ok=False)
    baseline = OUT / 'experiments/modern-comments2/candidate'
    rows, receipts, browser = [], [], []
    for variant in ['horizontal', 'vertical', 'rectangular', 'adjacent', 'strict', 'orphan', 'inconsistent', 'huge-span', 'false-flags', 'ragged']:
        path = destination / (variant + '.pptx')
        expectation = probe(path, variant)
        row = dict(id=variant, path=str(path.resolve()), sha256=sha(path)); rows.append(row)
        old, before = invoke(baseline, row, destination / (variant + '.before.json.gz'))
        new, after = invoke(run/'candidate', row, destination / (variant + '.json.gz'))
        assert old['status'] == new['status'] == 'ok'
        assert without_rows(before) == without_rows(after)
        unmerged = destination / (variant + '.unmerged.pptx'); unmerge(path, unmerged)
        _, control = invoke(baseline, {**row, 'path': str(unmerged)}, destination/(variant+'.control.json.gz'))
        counts = verify(path, after, control)
        table = tables(after)[0]
        for r in table['rows']:
            for c, cell in enumerate(r):
                assert cell['fill']['color']['value'] == ['#00FF00', '#FF0000', '#0000FF', '#FFFF00'][c], cell['fill']
                run_value = cell['paragraphs'][0]['runs'][0]
                assert run_value['bold'] and run_value['hyperlink'] == 'https://example.invalid/'
        receipts.append(dict(id=variant, passed=True, full_sha256=new['full_sha256'], source_counts=dict(counts), artwork_hashes=artwork_hashes(after)))
        browser.append(dict(id=variant, model=after, **expectation))
        print(variant, dict(counts), flush=True)
    (destination/'manifest.json').write_text(json.dumps(rows, indent=2)+'\n')
    (destination/'browser-models.json').write_text(json.dumps(browser)+'\n')
    summary = dict(total=len(receipts), passed=len(receipts), candidate_sha256=sha(run/'candidate'), harness_sha256=sha(__file__), review_harness_sha256=sha(Path(__file__).with_name('table_grid_review.py')), receipts=receipts)
    (run/'table-probes-summary.json').write_text(json.dumps(summary, indent=2)+'\n')


if __name__ == '__main__':
    main()

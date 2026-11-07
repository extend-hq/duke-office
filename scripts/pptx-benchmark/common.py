import base64
import collections
import hashlib
import json
import posixpath
import re
import unicodedata
import zipfile
from xml.etree import ElementTree as ET

NS = {'p':'http://schemas.openxmlformats.org/presentationml/2006/main', 'a':'http://schemas.openxmlformats.org/drawingml/2006/main', 'r':'http://schemas.openxmlformats.org/officeDocument/2006/relationships', 'c':'http://schemas.openxmlformats.org/drawingml/2006/chart'}

def norm(s):
    return re.sub(r'\s+', ' ', unicodedata.normalize('NFKC', s or '')).strip()

def local(node): return node.tag.rsplit('}',1)[-1]
def sha(data): return hashlib.sha256(data).hexdigest()
def paragraph(runs): return {'text':''.join(r['text'] for r in runs),'runs':runs}
def plain(text): return paragraph([{'text':text}])
def slide(): return dict(paragraphs=[], tables=[], images=[], charts=[], shapes=[], notes=[], comments=[], hidden=False)
def rels(z, part):
    path=posixpath.join(posixpath.dirname(part),'_rels',posixpath.basename(part)+'.rels')
    if path not in z.namelist(): return {}
    return {n.get('Id'):dict(target=n.get('Target') if n.get('TargetMode')=='External' else posixpath.normpath(posixpath.join(posixpath.dirname(part),n.get('Target',''))).lstrip('/'),type=n.get('Type','').rsplit('/',1)[-1],external=n.get('TargetMode')=='External') for n in ET.fromstring(z.read(path))}

def xml_paragraphs(root, relationships):
    out=[]
    for p in root.findall('./a:p',NS):
        runs=[]
        for r in p:
            kind=local(r)
            if kind not in ('r','fld','br'): continue
            text='\n' if kind=='br' else ''.join(n.text or '' for n in r.findall('./a:t',NS))
            props=r.find('./a:rPr',NS)
            item={'text':text}
            if props is not None:
                item.update(bold=props.get('b') in ('1','true'),italic=props.get('i') in ('1','true'))
                link=props.find('./a:hlinkClick',NS)
                if link is not None: item['link']=relationships.get(link.get('{'+NS['r']+'}id'),{}).get('target')
            runs.append(item)
        out.append(paragraph(runs))
    return out

def oracle(path):
    with zipfile.ZipFile(path) as z:
        damaged=z.testzip()
        if damaged: raise ValueError('CRC failure in '+damaged)
        root=ET.fromstring(z.read('ppt/presentation.xml'))
        relationships=rels(z,'ppt/presentation.xml')
        out={'slides':[], 'reference':'literal slide XML; drawing-tree order', 'uncompressed_bytes':sum(i.file_size for i in z.infolist())}
        size=root.find('./p:sldSz',NS)
        out['size']=[int(size.get(k,0)) for k in ('cx','cy')] if size is not None else None
        for sid in root.findall('./p:sldIdLst/p:sldId',NS):
            part=relationships[sid.get('{'+NS['r']+'}id')]['target']
            sr=ET.fromstring(z.read(part)); rr=rels(z,part); s=slide(); s['hidden']=sr.get('show') in ('0','false')
            def walk(node, grouped=False):
                kind=local(node)
                if kind=='AlternateContent':
                    branches=list(node)
                    if branches:
                        for child in branches[0]: walk(child,grouped)
                    return
                if kind=='grpSp':
                    for child in node: walk(child,True)
                    return
                if kind not in ('sp','pic','cxnSp','graphicFrame'): return
                body=node.find('./p:txBody',NS)
                ps=xml_paragraphs(body,rr) if body is not None else []
                s['paragraphs'].extend(ps)
                props=node.find('.//p:cNvPr',NS)
                transform=node.find('./p:spPr/a:xfrm',NS)
                if transform is None: transform=node.find('./p:xfrm',NS)
                if transform is not None and not grouped:
                    off=transform.find('./a:off',NS); ext=transform.find('./a:ext',NS)
                    if off is not None and ext is not None:
                        s['shapes'].append({'id':props.get('id') if props is not None else '', 'transform':[int(off.get('x',0)),int(off.get('y',0)),int(ext.get('cx',0)),int(ext.get('cy',0))]})
                if kind=='pic':
                    b=node.find('./p:blipFill/a:blip',NS)
                    if b is not None:
                        rid=b.get('{'+NS['r']+'}embed')
                        if rid:
                            target=rr.get(rid,{}).get('target'); s['images'].append(sha(z.read(target)) if target in z.namelist() else 'unresolved:'+str(target))
                for table in node.findall('./a:graphic/a:graphicData/a:tbl',NS):
                    cells=[]
                    for row in table.findall('./a:tr',NS):
                        for cell in row.findall('./a:tc',NS):
                            body=cell.find('./a:txBody',NS); cp=xml_paragraphs(body,rr) if body is not None else []
                            s['paragraphs'].extend(cp)
                            cells.append(dict(text=norm(''.join(p['text'] for p in cp)),col_span=int(cell.get('gridSpan','1')),row_span=int(cell.get('rowSpan','1'))))
                    s['tables'].append(cells)
                for chart in node.findall('.//c:chart',NS):
                    target=rr.get(chart.get('{'+NS['r']+'}id'),{}).get('target')
                    if target not in z.namelist(): continue
                    cr=ET.fromstring(z.read(target)); series=[]
                    for ser in cr.findall('.//c:ser',NS):
                        vals=ser.find('./c:val',NS)
                        if vals is None: vals=ser.find('./c:yVal',NS)
                        if vals is not None:
                            pts={int(p.get('idx')):float(p.find('./c:v',NS).text) for p in vals.findall('.//c:pt',NS) if p.find('./c:v',NS) is not None and p.find('./c:v',NS).text}
                            count=vals.find('.//c:ptCount',NS)
                            n=int(count.get('val')) if count is not None else (max(pts)+1 if pts else 0)
                            series.append([pts.get(i) for i in range(n)])
                    s['charts'].append(series)
            tree=sr.find('./p:cSld/p:spTree',NS)
            if tree is None: raise ValueError('missing slide shape tree')
            for node in tree: walk(node)
            for r in rr.values():
                target=r['target']
                if r['type']=='notesSlide' and target in z.namelist():
                    nr=ET.fromstring(z.read(target))
                    for shape in nr.findall('.//p:sp',NS):
                        ph=shape.find('./p:nvSpPr/p:nvPr/p:ph',NS)
                        if ph is not None and ph.get('type') in ('sldImg','sldNum','dt','hdr','ftr'): continue
                        body=shape.find('./p:txBody',NS)
                        if body is not None: s['notes'].extend(xml_paragraphs(body,rels(z,target)))
                if r['type']=='comments' and target in z.namelist():
                    s['comments'].extend(n.text or '' for n in ET.fromstring(z.read(target)).findall('.//p:text',NS))
            out['slides'].append(s)
        return out

def core(data):
    out={'slides':[], 'size':[data['size']['widthEmu'],data['size']['heightEmu']]}
    assets=data.get('assets',{})
    for src in data.get('slides',[]):
        s=slide();s['hidden']=src.get('hidden',False)
        def ps(items):
            return [paragraph([dict(r,link=r.get('hyperlink')) for r in p.get('runs',[])]) for p in items]
        def walk(nodes,grouped=False):
            for n in nodes:
                typ=n['type']
                t=n.get('transform',{})
                if not grouped: s['shapes'].append({'id':n['id'],'transform':[t.get(k,0) for k in ('x','y','width','height')]})
                if typ=='shape': s['paragraphs'].extend(ps(n.get('paragraphs',[])))
                if typ=='group': walk(n.get('children',[]),True)
                if typ=='table':
                    cells=[]
                    for row in n['rows']:
                        for c in row:
                            cp=ps(c.get('paragraphs',[]));s['paragraphs'].extend(cp)
                            cells.append(dict(text=norm(''.join(p['text'] for p in cp)),col_span=c.get('colSpan',1),row_span=c.get('rowSpan',1)))
                    s['tables'].append(cells)
                if typ=='image': s['images'].append(assets.get(n.get('assetId'),{}).get('dataSha256','missing'))
                if typ=='media' and n.get('poster'): s['images'].append(assets.get(n['poster'].get('assetId'),{}).get('dataSha256','missing'))
                if typ=='chart': s['charts'].append([ser.get('values',[]) for ser in n.get('series',[])])
        walk(src.get('nodes',[]))
        s['notes']=[p for n in src.get('notes',[]) for p in (ps(n['paragraphs']) if n.get('paragraphs') else [plain(n['text'])])]
        def comment_text(items):
            for item in items:
                yield item['text']
                yield from comment_text(item.get('replies', []))
        s['comments']=list(comment_text(src.get('comments', [])))
        out['slides'].append(s)
    return out

def ir(engine,data):
    out={'slides':[]}
    for section in data.get('sections',[]):
        s=slide();s['hidden']=section.get('hidden',False)
        def image(n):
            if engine=='undoc': value=data.get('resource_payload_sha256',{}).get(n.get('resource_id'),'missing')
            elif n.get('dataSha256'): value=n['dataSha256']
            elif isinstance(n.get('data'),str): value=sha(base64.b64decode(n['data']))
            else:value='missing'
            s['images'].append(value)
        def runs(items):
            out=[]
            for r in items:
                typ=r.get('type','text')
                if typ=='image': image(r)
                elif typ in ('line_break','tab'):out.append({'text':' '})
                elif typ=='hyperlink':
                    out.extend(dict(a,link=r.get('url')) for a in runs(r.get('content',r.get('runs',[]))))
                else: out.append(dict(r.get('style',{}),text=r.get('text',''),bold=r.get('bold',r.get('style',{}).get('bold')),italic=r.get('italic',r.get('style',{}).get('italic')),link=r.get('hyperlink')))
            return out
        def para(n):
            for im in n.get('images',[]):image(im)
            return paragraph(runs(n.get('runs',n.get('content',[]))))
        def walk(nodes, destination):
            for n in nodes or []:
                typ=n.get('type','').lower()
                if typ in ('paragraph','heading'):destination.append(para(n))
                elif typ=='image':image(n)
                elif typ=='table':
                    cells=[]
                    for row in n.get('rows',[]):
                        for cell in row.get('cells',[]):
                            cp=[]
                            content=cell.get('content',[])
                            if engine=='undoc':cp=[para(p) for p in content]
                            else:walk(content,cp)
                            destination.extend(cp)
                            cells.append(dict(text=norm(''.join(p['text'] for p in cp)),col_span=cell.get('col_span',1),row_span=cell.get('row_span',1)))
                    s['tables'].append(cells)
                elif typ=='list':
                    for item in n.get('items',[]):
                        walk(item.get('content'),destination)
                        if item.get('nested'):walk([dict(item['nested'],type='list')],destination)
                elif typ in ('text_box','shape'):walk(n.get('content'),destination)
        walk(section.get('content',section.get('elements',[])),s['paragraphs'])
        if engine=='undoc':s['notes']=[para(p) for p in section.get('notes',[]) or []]
        else:walk(section.get('speaker_notes'),s['notes'])
        out['slides'].append(s)
    return out

def python_model(presentation):
    out={'slides':[],'size':[presentation.slide_width,presentation.slide_height]}
    def link(run):
        try:return run.hyperlink.address
        except (KeyError,ValueError,AttributeError):return None
    def ps(frame):
        return [dict(paragraph([{'text':r.text,'bold':r.font.bold,'italic':r.font.italic,'link':link(r)} for r in p.runs]),text=p.text) for p in frame.paragraphs]
    for src in presentation.slides:
        s=slide()
        def walk(shapes, grouped=False):
            for shape in shapes:
                if not grouped and shape.shape_type!=6:
                    s['shapes'].append(dict(id=str(shape.shape_id),transform=[shape.left,shape.top,shape.width,shape.height]))
                if shape.shape_type==6:walk(shape.shapes,True)
                if shape.has_text_frame:s['paragraphs'].extend(ps(shape.text_frame))
                if shape.has_table:
                    cells=[]
                    for row in shape.table.rows:
                        for cell in row.cells:
                            cp=ps(cell.text_frame);s['paragraphs'].extend(cp)
                            cells.append(dict(text=norm(''.join(p['text'] for p in cp)),col_span=cell.span_width,row_span=cell.span_height))
                    s['tables'].append(cells)
                if shape.shape_type==13 or hasattr(type(shape),'image'):
                    try:s['images'].append(shape.image.blob)
                    except (ValueError,AttributeError):s['images'].append('missing')
                if shape.has_chart:
                    try:s['charts'].append([list(ser.values) for ser in shape.chart.series])
                    except (ValueError,AttributeError,NotImplementedError):s['charts'].append([])
        walk(src.shapes)
        if src.has_notes_slide and src.notes_slide.notes_text_frame is not None:s['notes']=ps(src.notes_slide.notes_text_frame)
        out['slides'].append(s)
    return out

def markdown_model(markdown):
    out={'slides':[]}
    for body in re.split(r'<!-- Slide number: \d+ -->',markdown)[1:]:
        s=slide();note=False;chart=False;chart_rows=False;table=None
        for line in body.split('\n'):
            line=line.strip()
            if line=='### Notes:':note=True;chart=False;table=None;continue
            if line.startswith('### Chart'):chart=True;chart_rows=False;s['charts'].append([]);table=None;continue
            if not line:
                table=None
                if chart_rows:chart=False
                continue
            if chart and line.startswith('|'):
                chart_rows=True
                continue
            if chart:chart=False
            if line.startswith('!['):continue
            if line.startswith('|') and line.endswith('|'):
                vals=[norm(v) for v in line[1:-1].split('|')]
                if all(re.fullmatch(r'[:\- ]+',v) for v in vals):continue
                if table is None:table=[];s['tables'].append(table)
                for v in vals:
                    table.append(dict(text=v,col_span=1,row_span=1));s['paragraphs'].append(plain(v))
            else:
                text=re.sub(r'^#{1,6}\s+','',line)
                s['notes' if note else 'paragraphs'].append(plain(text))
        out['slides'].append(s)
    return out

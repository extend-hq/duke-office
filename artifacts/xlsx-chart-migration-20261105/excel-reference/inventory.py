import hashlib, json, posixpath, re, zipfile
from pathlib import Path
from collections import Counter
from xml.etree import ElementTree as ET

base = Path('/Users/andrewluo/Documents/xlsx-testing')
out = Path(__file__).parent
ns = {'s':'http://schemas.openxmlformats.org/spreadsheetml/2006/main', 'r':'http://schemas.openxmlformats.org/officeDocument/2006/relationships', 'c':'http://schemas.openxmlformats.org/drawingml/2006/chart', 'a':'http://schemas.openxmlformats.org/drawingml/2006/main'}
def local(tag): return tag.split('}')[-1]
def rels(z, part):
    p = posixpath.join(posixpath.dirname(part),'_rels',posixpath.basename(part)+'.rels')
    if p not in z.namelist(): return {}
    return {r.get('Id'): posixpath.normpath(posixpath.join(posixpath.dirname(part),r.get('Target',''))) for r in ET.fromstring(z.read(p)) if r.get('TargetMode') != 'External'}
records=[]; skipped=[]; errors=[]
for p in sorted(base.rglob('*')):
    if not p.is_file() or p.name.startswith('~$') or p.suffix.lower() not in ('.xlsx','.xlsm','.xls'): continue
    if p.suffix.lower()=='.xls':
        skipped.append({'path':str(p),'reason':'legacy-binary-format'}); continue
    try:
        with zipfile.ZipFile(p) as z:
            parts=[n for n in z.namelist() if re.fullmatch(r'xl/charts/(chart|chartEx)\d+\.xml',n)]
            if not parts: continue
            workbook=ET.fromstring(z.read('xl/workbook.xml'))
            wr=rels(z,'xl/workbook.xml'); owners={}
            for s in workbook.findall('s:sheets/s:sheet',ns):
                part=wr.get(s.get('{'+ns['r']+'}id'))
                if not part or part not in z.namelist(): continue
                for dp in rels(z,part).values():
                    if '/drawings/' not in dp or dp not in z.namelist() or not dp.endswith('.xml'): continue
                    dr=rels(z,dp)
                    for anchor in ET.fromstring(z.read(dp)):
                        for el in anchor.iter():
                            if local(el.tag) != 'chart': continue
                            cp=dr.get(el.get('{'+ns['r']+'}id'))
                            if not cp: continue
                            ext=next((x for x in anchor.iter() if local(x.tag)=='ext' and x.get('cx')),None)
                            name=next((x.get('name') for x in anchor.iter() if local(x.tag)=='cNvPr'),None)
                            owners.setdefault(cp,[]).append({'sheet':s.get('name'),'sheetPart':part,'drawingPart':dp,'name':name,'widthEmu':ext.get('cx') if ext is not None else None,'heightEmu':ext.get('cy') if ext is not None else None})
            charts=[]
            for part in parts:
                raw=z.read(part); root=ET.fromstring(raw)
                types=sorted(set(local(e.tag) for e in root.iter() if local(e.tag).endswith('Chart') and local(e.tag)!='pivotChart'))
                layouts=sorted(set(e.get('layoutId') for e in root.iter() if e.get('layoutId')))
                title=' '.join(''.join(t.itertext()) for t in root.findall('.//c:title//a:t',ns))
                charts.append({'part':part,'sha256':hashlib.sha256(raw).hexdigest(),'types':types,'layouts':layouts,'title':title,'owners':owners.get(part,[]),'seriesCount':sum(local(e.tag) in ('ser','series') for e in root.iter())})
            records.append({'path':str(p),'relative':str(p.relative_to(base)),'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'charts':charts})
    except Exception as e: errors.append({'path':str(p),'error':str(e)})
manifest={'root':str(base),'workbooks':records,'skipped':skipped,'errors':errors}
(out/'inventory.json').write_text(json.dumps(manifest,indent=2))
summary={'chartWorkbooks':len(records),'chartParts':sum(len(w['charts']) for w in records),'uniqueChartXml':len(set(c['sha256'] for w in records for c in w['charts'])),'legacyWorkbooks':len(skipped),'errors':len(errors),'families':dict(Counter(t for w in records for c in w['charts'] for t in c['types']+c['layouts']))}
(out/'inventory-summary.json').write_text(json.dumps(summary,indent=2)); print(json.dumps(summary,indent=2))
print('\n'.join(f"{len(w['charts']):4} {w['relative']}" for w in records if not w['relative'].startswith('extend/')))

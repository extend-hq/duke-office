import collections
import csv
import json
from pathlib import Path
from xml.etree import ElementTree as ET
import zipfile

ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'output/pptx-benchmark-20261106'
X='{http://www.w3.org/2001/XMLSchema}'

def main():
    rows=[]
    for name in ['pml','dml-main','dml-picture','dml-chart','dml-chartDrawing','dml-diagram','dml-lockedCanvas','shared-math']:
        root=ET.parse(OUT/'spec'/(name+'.xsd')).getroot()
        def visit(node,owner='schema'):
            if node.tag in [X+'complexType',X+'simpleType',X+'group',X+'attributeGroup']:owner=node.get('name',owner)
            if node.tag in [X+'element',X+'attribute']:
                rows.append(dict(schema=name,owner=owner,kind=node.tag.split('}')[-1],name=node.get('name',node.get('ref','')),type=node.get('type',''),min_occurs=node.get('minOccurs','1'),max_occurs=node.get('maxOccurs','1'),use=node.get('use',''),default=node.get('default',''),audit_status='requires semantic audit'))
            for child in node:visit(child,owner)
        visit(root)
    with (OUT/'spec/schema-inventory.csv').open('w') as f:
        w=csv.DictWriter(f,fieldnames=rows[0].keys());w.writeheader();w.writerows(rows)
    manifest=json.loads((OUT/'manifest.json').read_text());counts=collections.defaultdict(set)
    for row in manifest:
        if row['format']!='pptx':continue
        try:
            with zipfile.ZipFile(row['path']) as z:
                for part in z.namelist():
                    if not part.startswith('ppt/') or not part.endswith('.xml'):continue
                    try:root=ET.fromstring(z.read(part))
                    except Exception:continue
                    for n in root.iter():
                        tag=n.tag.rsplit('}',1)[-1]
                        if tag in ['timing','transition','videoFile','audioFile','wavAudioFile','media','oleObj','control','contentPart','oMath','oMathPara','sectionLst','custShowLst','showPr','model3d','hlinkClick','hlinkHover','fld','bar3DChart','pie3DChart','ofPieChart','area3DChart','line3DChart','surface3DChart','chartSpace','relIds']:
                            counts[tag].add(row['id'])
                        if tag=='cNvPr':
                            if n.get('descr'):counts['shape_alt_text'].add(row['id'])
                            if n.get('hidden') in ('1','true'):counts['hidden_object'].add(row['id'])
        except Exception:pass
    result={'schema_declarations':len(rows),'schemas':dict(collections.Counter(r['schema'] for r in rows)),'corpus_feature_documents':{k:len(v) for k,v in sorted(counts.items())},'feature_document_ids':{k:sorted(v) for k,v in sorted(counts.items())}}
    (OUT/'spec/inventory.json').write_text(json.dumps(result,indent=2))
    print(json.dumps({k:v for k,v in result.items() if k!='feature_document_ids'},indent=2))

if __name__=='__main__':main()

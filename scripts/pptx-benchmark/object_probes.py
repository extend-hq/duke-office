import gzip
import json
from pathlib import Path
import subprocess
import sys
import zipfile
from xml.etree import ElementTree as ET
from common import NS, local, rels

ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'output/pptx-benchmark-20261106'

def main():
    run=OUT/'object-probes';run.mkdir(exist_ok=True)
    manifest=json.loads((OUT/'manifest.json').read_text())
    source=next(r['path'] for r in manifest if r['id']=='7940e3b1a339db11')
    with zipfile.ZipFile(source) as z: contents={name:z.read(name) for name in z.namelist()}
    slide='ppt/slides/slide1.xml';tree=ET.fromstring(contents[slide]);picture=tree.find('.//p:pic',NS)
    properties=picture.find('./p:nvPicPr/p:cNvPr',NS)
    properties.set('descr','Media & accessibility');properties.set('title','Accessible title');properties.set('hidden','false')
    click=properties.find('./a:hlinkClick',NS)
    click.set('{'+NS['r']+'}id','probeHyperlink');click.set('action','');click.set('tooltip','Go & inspect')
    relation_part='ppt/slides/_rels/slide1.xml.rels';relationships=ET.fromstring(contents[relation_part]);namespace=relationships.tag.split('}')[0]+'}'
    ET.SubElement(relationships,namespace+'Relationship',Id='probeHyperlink',Type=NS['r']+'/hyperlink',Target='../linked?first=1&second=2',TargetMode='External')
    media=next(n for n in picture.iter() if local(n)=='media');media.set('{'+NS['r']+'}link','probeMedia')
    ET.SubElement(relationships,namespace+'Relationship',Id='probeMedia',Type=NS['r']+'/video',Target='/payloads/clip.bin')
    contents['payloads/clip.bin']=b'\x00\xffmedia-payload\x01'
    p14='http://schemas.microsoft.com/office/powerpoint/2010/main'
    ET.SubElement(media,'{'+p14+'}trim',st='1.5s',end='250ms')
    ET.SubElement(media,'{'+p14+'}fade',attrib={'in':'1000000ns','out':'500µs'})
    bookmarks=ET.SubElement(media,'{'+p14+'}bmkLst');ET.SubElement(bookmarks,'{'+p14+'}bmk',name='Start',time='2min')
    contents[slide]=ET.tostring(tree);contents[relation_part]=ET.tostring(relationships)
    package=run/'relationships-and-media.pptx'
    with zipfile.ZipFile(package,'w',zipfile.ZIP_DEFLATED) as z:
        for name,data in contents.items():z.writestr(name,data)
    result=subprocess.run([sys.argv[1] if len(sys.argv)>1 else str(ROOT/'scripts/pptx-benchmark/target/release/pptx-benchmark'),'rust-pptx',str(package),str(run/'model.json.gz'),'0','full'],capture_output=True,text=True,check=True)
    record=json.loads(result.stdout.splitlines()[-1]);assert record['status']=='ok',record
    document=json.load(gzip.open(run/'model.json.gz','rt'));node=next(n for n in document['slides'][0]['nodes'] if n['type']=='media')
    checks={
        'external_relative_target_and_entities':node['clickAction']['target']=='../linked?first=1&second=2' and node['clickAction']['external'] is True,
        'accessibility':node['hidden'] is False and node['altText']=='Media & accessibility' and node['accessibilityTitle']=='Accessible title',
        'media_link_precedence':node['assetId']=='payloads/clip.bin',
        'nonstandard_asset_path':bytes(document['assets']['payloads/clip.bin']['data'])==contents['payloads/clip.bin'],
        'time_units':node['playback']==dict(trimStartMs=1500.0,trimEndMs=250.0,fadeInMs=1.0,fadeOutMs=0.5,bookmarks=[dict(name='Start',timeMs=120000.0)]),
        'poster_retained':node.get('poster',{}).get('type')=='image' and bool(node.get('posterAssetId')),
    }
    (run/'results.json').write_text(json.dumps(checks,indent=2));print(checks)
    assert all(checks.values())

if __name__=='__main__':main()

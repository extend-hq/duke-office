"""Isolate stored charts from an ODF corpus into uniformly fitted comparison frames."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile
from xml.etree import ElementTree as ET


def digest(data):
    return hashlib.sha256(data).hexdigest()


def length(value):
    import re
    match = re.fullmatch(r'([+-]?[\d.]+)(cm|mm|in|pt|px)', value or '')
    return float(match[1]) * dict(cm=96/2.54,mm=96/25.4,**{'in':96},pt=96/72,px=1)[match[2]] if match else None


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('corpus',type=Path)
    p.add_argument('template',type=Path)
    p.add_argument('output',type=Path)
    args=p.parse_args();args.output.mkdir(parents=True,exist_ok=True)
    with zipfile.ZipFile(args.template) as z:
        template=z.read('content.xml').decode();styles=z.read('styles.xml')
    prefix=template.split('<office:body>')[0]
    objects=[];seen=set()
    for path in sorted(args.corpus.rglob('*')):
        if not path.is_file():continue
        try:
            with zipfile.ZipFile(path) as z:
                for name in z.namelist():
                    if not name.endswith('/content.xml'):continue
                    content=z.read(name)
                    tree=ET.fromstring(content)
                    chart=tree.find('.//{urn:oasis:names:tc:opendocument:xmlns:chart:1.0}chart')
                    if chart is None:continue
                    directory=name.rsplit('/',1)[0]
                    part_styles=z.read(directory+'/styles.xml') if directory+'/styles.xml' in z.namelist() else b''
                    identity=digest(content+part_styles)
                    if identity in seen:continue
                    seen.add(identity)
                    svg='{urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0}'
                    w,h=length(chart.get(svg+'width')) or 360,length(chart.get(svg+'height')) or 330
                    scale=min(360/w,330/h)
                    objects.append(dict(content=content,styles=part_styles,width=w*scale,height=h*scale,sourceSha256=digest(path.read_bytes()),source=str(path),part=name,identity=identity))
        except (zipfile.BadZipFile,ET.ParseError,KeyError):pass
    cases=[];pages=[]
    output=args.output/'visual.odp'
    with zipfile.ZipFile(output,'w',zipfile.ZIP_DEFLATED) as z:
        z.writestr('mimetype','application/vnd.oasis.opendocument.presentation',compress_type=zipfile.ZIP_STORED)
        z.writestr('styles.xml',styles)
        entries=[('/','application/vnd.oasis.opendocument.presentation'),('content.xml','text/xml'),('styles.xml','text/xml')]
        for i,obj in enumerate(objects,1):
            key=f'Object{i}'
            pages.append(f'<draw:page draw:name="Chart{i}" draw:master-page-name="Master" draw:style-name="PageStyle"><draw:frame svg:x="36px" svg:y="36px" svg:width="{obj["width"]}px" svg:height="{obj["height"]}px"><draw:object xlink:href="./{key}" xlink:type="simple"/></draw:frame></draw:page>')
            z.writestr(key+'/content.xml',obj['content']);entries.extend([(key+'/','application/vnd.oasis.opendocument.chart'),(key+'/content.xml','text/xml')])
            if obj['styles']:
                z.writestr(key+'/styles.xml',obj['styles']);entries.append((key+'/styles.xml','text/xml'))
            cases.append(dict(id=f'chart-{i:02d}',page=i,family='chart',source=obj['source'],sourceSha256=obj['sourceSha256'],part=obj['part'],chartSha256=obj['identity'],frame=[36,36,obj['width'],obj['height']]))
        z.writestr('content.xml',prefix+'<office:body><office:presentation>'+''.join(pages)+'</office:presentation></office:body></office:document-content>')
        z.writestr('META-INF/manifest.xml','<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">'+''.join(f'<manifest:file-entry manifest:full-path="{name}" manifest:media-type="{mime}"/>' for name,mime in entries)+'</manifest:manifest>')
    (args.output/'manifest.json').write_text(json.dumps(dict(pageSizePx=dict(width=432,height=432),dpi=96,cases=cases,inputs={'visual.odp':digest(output.read_bytes())},derivation='Unchanged embedded content and styles in uniformly fitted frames, no saved previews'),indent=2))
    print(f'Collected {len(cases)} distinct charts')

if __name__=='__main__':main()

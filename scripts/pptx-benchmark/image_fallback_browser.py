import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path

from feedback import OUT

# Sibling checkout of github.com/extend-hq/react-pptx; override with REACT_PPTX.
REACT_PPTX = Path(os.environ.get('REACT_PPTX', Path(__file__).resolve().parents[3] / 'react-pptx'))


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--label', required=True); args = parser.parse_args()
    run = OUT / 'experiments' / args.label; report = OUT / 'report'
    model = json.load(gzip.open(run / 'probes/transitional.full.json.gz', 'rt'))
    (report / 'image-fallback-model.json').write_text(json.dumps(model))
    html = '''<!doctype html><html><meta charset="utf-8"><title>Image fallback viewer check</title><body style="margin:0"><div id="host" style="width:900px;height:700px"></div><script type="module">
import {NormalizedPresentationViewer} from './image-fallback-viewer.mjs';
const original=await (await fetch('./image-fallback-model.json')).json();
window.runImageFallbackProbe=async()=>{
  const results=[];
  for(const name of ['primary','decode-error','missing-asset','missing-data','url-error','both-errors','destroyed']){
    const model=structuredClone(original), node=model.slides[0].nodes[0], primary=model.assets[node.assetId], fallback=model.assets[node.fallbackAssetId];
    delete node.pictureSvg;delete node.paintBounds;
    const errors=[];
    if(['decode-error','both-errors','destroyed'].includes(name)) primary.data=[60,115,118,103];
    if(name==='missing-asset') delete model.assets[node.assetId];
    if(name==='missing-data') delete primary.data;
    if(name==='url-error') primary.url='data:image/svg+xml,%3Csvg';
    if(name==='both-errors') fallback.data=[0,1,2];
    for(const asset of Object.values(model.assets))if(Array.isArray(asset.data))asset.data=new Uint8Array(asset.data);
    const host=document.getElementById('host');host.replaceChildren();
    const viewer=new NormalizedPresentationViewer(host,model,{onNodeError:(id,error)=>errors.push({id,error:String(error)})});
    await viewer.setZoom(100);await viewer.renderSlide(0);
    if(name==='destroyed')viewer.destroy();
    const started=performance.now();
    let image=host.querySelector('img');
    while(performance.now()-started<3000 && !errors.length && (!image?.complete || !image?.naturalWidth)){
      if(name==='destroyed'){await new Promise(r=>setTimeout(r,100));break;}
      await new Promise(r=>setTimeout(r,10));image=host.querySelector('img');
    }
    const row={name,errors,naturalWidth:image?.naturalWidth??0,naturalHeight:image?.naturalHeight??0,srcKind:image?.src.split(':',1)[0]??null};
    if(image?.naturalWidth){const canvas=document.createElement('canvas');canvas.width=canvas.height=1;const ctx=canvas.getContext('2d');ctx.drawImage(image,0,0,1,1);row.pixel=Array.from(ctx.getImageData(0,0,1,1).data);row.crop={width:image.style.width,height:image.style.height,left:image.style.left,top:image.style.top};}
    row.passed=name==='destroyed'?errors.length===0:name==='both-errors'?errors.length===1&&row.naturalWidth===0:errors.length===0&&row.naturalWidth===(name==='primary'?40:1)&&JSON.stringify(row.pixel)===JSON.stringify(name==='primary'?[40,127,168,255]:[255,0,0,255]);
    results.push(row);viewer.destroy();
  }
  window.probeResults=results;return results;
};
window.ready=true;
</script></body></html>'''
    (report / 'image-fallback-probe.html').write_text(html)
    sources = [REACT_PPTX / 'packages/react-viewer/src/normalized-viewer.ts', REACT_PPTX / 'packages/presentation-model/src/index.ts']
    artifacts = [report / name for name in ['image-fallback-probe.html', 'image-fallback-model.json', 'image-fallback-viewer.mjs']]
    receipt = dict(candidate_sha256=hashlib.sha256((run / 'candidate').read_bytes()).hexdigest(), source_sha256={str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}, artifact_sha256={str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in artifacts}, scope='Browser execution pending; direct-image branch of the public model contract, with pictureSvg and paintBounds omitted from the native probe model. Generated pictureSvg rendering remains separate and is not claimed to recover from embedded image decode errors.')
    (run / 'browser-identity.json').write_text(json.dumps(receipt, indent=2))


if __name__ == '__main__':
    main()

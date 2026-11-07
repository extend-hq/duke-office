import argparse
import gzip
import hashlib
import json
from pathlib import Path

from feedback import OUT
from svg_namespace_probes import images


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--label', required=True); args = parser.parse_args()
    run = OUT / 'experiments' / args.label; report = OUT / 'report'
    proof = json.loads((run / 'svg-namespace-probes-summary.json').read_text())
    digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
    bundle_identity = json.loads((OUT / 'experiments/image-fallback1/browser-identity.json').read_text())
    bundle = report / 'image-fallback-viewer.mjs'
    assert digest(bundle) == bundle_identity['artifact_sha256'][str(bundle)]
    assert all(digest(Path(path)) == expected for path, expected in bundle_identity['source_sha256'].items())
    cases = []
    for row in proof['receipts']:
        source = run / 'namespace-probes' / (row['id'] + '.full.json.gz')
        document = json.load(gzip.open(source, 'rt')); image = list(images(document['slides'][0]['nodes']))[0]
        if 'pictureSvg' not in image:
            continue
        cases.append(dict(id=row['id'], model=document, expected=[255, 0, 0, 255] if row['selected'].endswith('.png') else [40, 127, 168, 255], model_sha256=digest(source)))
    (report / 'svg-namespace-models.json').write_text(json.dumps(cases))
    page = '''<!doctype html><html><meta charset="utf-8"><title>Qualified image viewer check</title><body style="margin:0"><div id="host" style="width:900px;height:700px"></div><script type="module">
import {NormalizedPresentationViewer} from './image-fallback-viewer.mjs';
const cases=await (await fetch('./svg-namespace-models.json')).json();
window.runSvgNamespaceProbe=async()=>{
  const results=[];
  for(const entry of cases){
    const model=structuredClone(entry.model),errors=[];
    for(const asset of Object.values(model.assets))if(Array.isArray(asset.data))asset.data=new Uint8Array(asset.data);
    const host=document.getElementById('host');host.replaceChildren();
    const viewer=new NormalizedPresentationViewer(host,model,{onNodeError:(id,error)=>errors.push({id,error:String(error)})});
    await viewer.setZoom(100);await viewer.renderSlide(0);
    const started=performance.now();let image=host.querySelector('img');
    while(performance.now()-started<3000&&(!image?.complete||!image?.naturalWidth)){
      await new Promise(resolve=>setTimeout(resolve,10));image=host.querySelector('img');
    }
    const row={id:entry.id,errors,width:image?.naturalWidth??0,height:image?.naturalHeight??0,generated:image?.src.startsWith('data:image/svg+xml')??false};
    if(image?.naturalWidth){
      const canvas=document.createElement('canvas');canvas.width=200;canvas.height=150;const context=canvas.getContext('2d');
      context.drawImage(image,0,0,200,150);row.center=Array.from(context.getImageData(100,75,1,1).data);row.corner=Array.from(context.getImageData(0,0,1,1).data);
    }
    row.passed=errors.length===0&&row.generated&&JSON.stringify(row.center)===JSON.stringify(entry.expected)&&row.corner?.[3]===0;
    results.push(row);viewer.destroy();
  }
  window.svgNamespaceResults=results;return results;
};window.ready=true;
</script></body></html>'''
    (report / 'svg-namespace-probe.html').write_text(page)
    artifacts = [report / name for name in ['svg-namespace-models.json','svg-namespace-probe.html','image-fallback-viewer.mjs']]
    receipt = dict(candidate_sha256=proof['candidate_sha256'], cases=len(cases), artifacts_sha256={str(p):digest(p) for p in artifacts}, consumer_source_sha256=bundle_identity['source_sha256'], scope='Unmodified native models passed through the React viewer generated-picture path. Browser checks await execution: selected representation center color and ellipse corner transparency. Two malformed direct-child variants with no generated artwork are covered separately by native warning assertions. This is a browser regression check, not native PowerPoint fidelity.')
    (run / 'browser-identity.json').write_text(json.dumps(receipt, indent=2)); print('Generated', len(cases), 'browser cases')


if __name__ == '__main__':
    main()

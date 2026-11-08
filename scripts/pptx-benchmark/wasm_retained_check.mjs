// Checks the WASM `ParsedPresentation` class against the WASM functions on a
// manifest: its `document()` must equal `parse_presentation(bytes, { deferArtwork: true })`
// after compacting, and for the first LIMIT nodes with eager artwork, its
// `render_node_artwork(target)` must equal both the eagerly stored artwork and
// `render_presentation_node_artwork(bytes, undefined, target)`.
//
//   node --expose-gc scripts/pptx-benchmark/wasm_retained_check.mjs WASM_DIR OUT_DIR MANIFEST [LIMIT]
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const output=path.join(root,'output/pptx-benchmark-20261106');
const require=createRequire(import.meta.url);
const wasmDirectory=process.argv[2] ?? 'wasm-candidate';
const wasm=require(path.join(output,wasmDirectory,'pptx_wasm.js'));
const directory=path.join(output,process.argv[3] ?? 'wasm-retained-check');
const manifestPath=path.resolve(process.argv[4] ?? path.join(output,'manifest.json'));
const limit=Number(process.argv[5] ?? 6);
const manifestBytes=fs.readFileSync(manifestPath);
const rows=JSON.parse(manifestBytes);
fs.mkdirSync(directory,{recursive:true});
const destination=path.join(directory,'records.json');
if (fs.existsSync(destination)) { console.error('Preserve existing receipts.'); process.exit(2); }

function canonical(value) {
  if (value instanceof Uint8Array) return {bytesSha256:createHash('sha256').update(value).digest('hex')};
  if (Array.isArray(value)) return value.map(canonical);
  if (!value || typeof value!=='object') return value;
  const result={};
  for (const key of Object.keys(value).sort()) result[key]=canonical(value[key]);
  return result;
}
const digest=(value)=>createHash('sha256').update(JSON.stringify(canonical(value)??null)).digest('hex');

function artwork(node) {
  if (node.type==='shape' && typeof node.shapeSvg==='string') return {svg:node.shapeSvg, paintBounds:node.paintBounds};
  if (node.type==='image' && typeof node.pictureSvg==='string') return {svg:node.pictureSvg, paintBounds:node.paintBounds, fallbackPicture:node.fallbackPicture};
  if (node.type==='chart' && typeof node.chartSvg==='string') return {svg:node.chartSvg};
  return undefined;
}
function collect(nodes, make, seen, out) {
  for (const node of nodes) {
    if (!seen.has(node.id)) {
      seen.add(node.id);
      const expected=artwork(node);
      if (expected && out.length<limit) out.push([make(node.id), expected]);
    }
    if (node.type==='group') collect(node.children, make, seen, out);
  }
}
const strip=(value)=>{ // undefined-valued keys do not survive JSON; align both sides
  return JSON.parse(JSON.stringify(value===undefined?null:value, (key, item)=>item instanceof Uint8Array?Array.from(item):item));
};

const records=[];
for (const [index,row] of rows.entries()) {
  const record={id:row.id};
  try {
    const bytes=fs.readFileSync(row.path);
    const eager=wasm.parse_presentation(bytes);
    const deferred=wasm.parse_presentation(bytes,{deferArtwork:true});
    const handle=new wasm.ParsedPresentation(bytes,undefined);
    try {
      record.document_equal=digest(handle.document())===digest(deferred);
      const targets=[];
      eager.slides.forEach((slide,slideIndex)=>collect(slide.nodes,(nodeId)=>({slideIndex,nodeId}),new Set(),targets));
      for (const layout of eager.layouts) collect(layout.nodes,(nodeId)=>({layoutId:layout.id,nodeId}),new Set(),targets);
      for (const master of eager.masters) collect(master.nodes,(nodeId)=>({masterId:master.id,nodeId}),new Set(),targets);
      let matched=0, functionMatched=0; const mismatches=[];
      for (const [target,expected] of targets) {
        const throughHandle=handle.render_node_artwork(target);
        const throughFunction=wasm.render_presentation_node_artwork(bytes,undefined,target);
        const handleOk=digest(strip(throughHandle))===digest(strip(expected));
        const functionOk=digest(strip(throughFunction))===digest(strip(expected));
        matched+=handleOk; functionMatched+=functionOk;
        if (!handleOk||!functionOk) mismatches.push({target, handleOk, functionOk});
      }
      record.checked=targets.length; record.matched=matched; record.function_matched=functionMatched; record.mismatches=mismatches;
      record.status='ok';
      record.passed=record.document_equal && matched===targets.length && functionMatched===targets.length;
    } finally { handle.free(); }
  } catch(error) {
    record.status='error'; record.error=String(error);
    // A presentation the bridge rejects must be rejected by the class too.
    try { new wasm.ParsedPresentation(fs.readFileSync(row.path),undefined).free(); record.class_error=null; } catch(classError) { record.class_error=String(classError); }
    record.passed=record.class_error===record.error;
  }
  records.push(record);
  if (index%50===0) console.log(index,rows.length);
  global.gc?.();
}
const wasmBytes=fs.readFileSync(path.join(output,wasmDirectory,'pptx_wasm_bg.wasm'));
const summary={node:process.version, wasm_sha256:createHash('sha256').update(wasmBytes).digest('hex'), manifest_sha256:createHash('sha256').update(manifestBytes).digest('hex'),
  harness_sha256:createHash('sha256').update(fs.readFileSync(fileURLToPath(import.meta.url))).digest('hex'), limit, inputs:records.length,
  passed:records.filter(r=>r.passed).length, successful:records.filter(r=>r.status==='ok').length, documents_equal:records.filter(r=>r.document_equal).length,
  checked:records.reduce((n,r)=>n+(r.checked??0),0), matched:records.reduce((n,r)=>n+(r.matched??0),0), function_matched:records.reduce((n,r)=>n+(r.function_matched??0),0),
  failures:records.filter(r=>!r.passed).map(r=>r.id),
  scope:'WASM ParsedPresentation class: document() equals the deferred parse_presentation model, and on-demand renders through the class equal eager artwork and render_presentation_node_artwork for the checked nodes. Not a rendered-fidelity check.'};
fs.writeFileSync(destination,JSON.stringify(records,null,2));
fs.writeFileSync(path.join(directory,'summary.json'),JSON.stringify(summary,null,2));
console.log(JSON.stringify(summary));
if (summary.failures.length) process.exit(1);

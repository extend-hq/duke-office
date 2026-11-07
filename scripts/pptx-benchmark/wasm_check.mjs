import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { createHash } from 'node:crypto';
import { gzipSync } from 'node:zlib';
import { fileURLToPath } from 'node:url';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const output=path.join(root,'output/pptx-benchmark-20261106');
const require=createRequire(import.meta.url);
const wasmDirectory=process.argv[2] ?? 'wasm-candidate';
const wasm=require(path.join(output,wasmDirectory,'pptx_wasm.js'));
const manifestPath=process.argv[4] ? path.resolve(process.argv[4]) : path.join(output,'manifest.json');
const manifestBytes=fs.readFileSync(manifestPath);
const rows=JSON.parse(manifestBytes);
const directory=path.join(output,process.argv[3] ?? 'wasm-check');
fs.mkdirSync(directory,{recursive:true});
const hashArtwork=process.argv.includes('--hash-artwork');
const hashEditing=process.argv.includes('--hash-editing');

function editingHashes(value, location='', result=[]) {
  if (!value || typeof value !== 'object' || value instanceof Uint8Array) return result;
  for (const [key, child] of Object.entries(value)) {
    if (key === 'data' || key === 'bytes') continue;
    const next=location+'/'+key;
    if (key === 'editing' && typeof child?.sourceXml === 'string') {
      result.push([next+'/sourceXml', createHash('sha256').update(child.sourceXml).digest('hex')]);
    } else if (child && typeof child === 'object') editingHashes(child,next,result);
  }
  return result;
}

function artworkHashes(value, location='', result=[]) {
  if (!value || typeof value !== 'object' || value instanceof Uint8Array) return result;
  for (const [key, child] of Object.entries(value)) {
    if (key === 'data' || key === 'bytes') continue;
    const next=location+'/'+key;
    if (['shapeSvg','pictureSvg','chartSvg','svg'].includes(key) && typeof child === 'string') {
      result.push([next, createHash('sha256').update(child).digest('hex')]);
    } else if (child && typeof child === 'object') artworkHashes(child,next,result);
  }
  return result;
}

function compact(value) {
  if (Array.isArray(value)) return value.map(compact);
  if (!value || typeof value!=='object') return value;
  const result={...value};
  for (const key of ['sourceXml','shapeSvg','pictureSvg','chartXml','chartSvg','svg','source_xml']) {
    if (key in result) {
      result[`${key}Bytes`]=typeof result[key]==='string'?Buffer.byteLength(result[key]):0;
      delete result[key];
    }
  }
  for (const key of ['data','bytes']) {
    const bytes=result[key];
    if (bytes instanceof Uint8Array || Array.isArray(bytes) && bytes.every(x=>Number.isInteger(x)&&x>=0&&x<256)) {
      result[`${key}Sha256`]=createHash('sha256').update(Buffer.from(bytes)).digest('hex');
      delete result[key];
    }
  }
  for (const key of Object.keys(result)) result[key]=compact(result[key]);
  return result;
}

const records=[];
for (const [index,row] of rows.entries()) {
  let record={id:row.id};
  try {
    const document=wasm.parse_presentation(fs.readFileSync(row.path));
    if (hashArtwork) record.artwork_hashes=artworkHashes(document);
    if (hashEditing) record.editing_source_hashes=editingHashes(document);
    fs.writeFileSync(path.join(directory,`${row.id}.json.gz`),gzipSync(JSON.stringify(compact(document))));
    record.status='ok';
  } catch(error) {
    record.status='error';record.error=String(error);
  }
  records.push(record);
  if (index%50===0) console.log(index,rows.length);
  global.gc?.();
}
const bytes=fs.readFileSync(path.join(output,wasmDirectory,'pptx_wasm_bg.wasm'));
fs.writeFileSync(path.join(directory,'records.json'),JSON.stringify(records,null,2));
fs.writeFileSync(path.join(directory,'identity.json'),JSON.stringify({node:process.version,wasm_sha256:createHash('sha256').update(bytes).digest('hex'),manifest_sha256:createHash('sha256').update(manifestBytes).digest('hex'),inputs:records.length,success:records.filter(x=>x.status==='ok').length},null,2));
console.log(records.reduce((counts,r)=>(counts[r.status]=(counts[r.status]||0)+1,counts),{}));

import { NormalizedPresentationViewer } from './picture-fallback-viewer.mjs';

const cases = await (await fetch('./picture-fallback-models.json')).json();
const wait = (ms) => new Promise(resolve => setTimeout(resolve, ms));
const encode = svg => `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
function imageNode(model) {
  const queue = [...model.slides[0].nodes];
  while (queue.length) {
    const node = queue.shift();
    if (node.type === 'image') return node;
    queue.push(...(node.children ?? []));
    if (node.poster) queue.push(node.poster);
  }
  throw new Error('No image node');
}
function setup(source) {
  const model = structuredClone(source), errors = [];
  for (const asset of Object.values(model.assets)) {
    if (Array.isArray(asset.data)) asset.data = new Uint8Array(asset.data);
  }
  const host = document.createElement('div');
  host.style.cssText = 'width:900px;height:700px;position:relative';
  document.body.append(host);
  const viewer = new NormalizedPresentationViewer(host, model, {
    onNodeError: (id, error) => errors.push({ id, error: String(error) }),
  });
  return { model, errors, host, viewer, dispose() { viewer.destroy(); host.remove(); } };
}
async function rendered(state) {
  await state.viewer.setZoom(100);
  await state.viewer.renderSlide(0);
  const image = state.host.querySelector('img');
  if (image?.src) await image.decode().catch(() => {});
  return image;
}
function styles(image) {
  if (!image) return null;
  return Object.fromEntries(['width', 'height', 'left', 'top', 'position', 'transform', 'objectFit']
    .map(key => [key, image.style[key]])
    .concat([['parentTransform', image.parentElement.style.transform]]));
}
function pixels(image) {
  const canvas = document.createElement('canvas');
  canvas.width = image.naturalWidth; canvas.height = image.naturalHeight;
  const context = canvas.getContext('2d');
  context.drawImage(image, 0, 0);
  return context.getImageData(0, 0, canvas.width, canvas.height).data;
}
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

window.runPictureFallbackProbe = async () => {
  const results = [];
  for (const entry of cases) {
    const state = setup(entry.model);
    let reference;
    try {
      const image = await rendered(state);
      const row = { id: entry.id, errors: state.errors, width: image?.naturalWidth ?? 0,
        height: image?.naturalHeight ?? 0, styles: styles(image) };
      if (entry.error_expected) {
        row.passed = state.errors.length === 1 && !image?.naturalWidth;
      } else {
        const referenceModel = structuredClone(entry.browser_primary_expected ? entry.model : entry.control);
        const node = imageNode(referenceModel);
        delete node.fallbackPicture;
        reference = setup(referenceModel);
        const expected = await rendered(reference);
        row.selectedExpectedSvg = image?.src === encode(node.pictureSvg);
        row.sameStyles = same(styles(image), styles(expected));
        row.sameSize = image?.naturalWidth === expected?.naturalWidth && image?.naturalHeight === expected?.naturalHeight;
        if (row.width && row.sameSize) {
          const actualPixels = pixels(image), expectedPixels = pixels(expected);
          row.differentChannels = actualPixels.reduce((total, byte, index) => total + (byte !== expectedPixels[index]), 0);
        }
        row.passed = !row.errors.length && !reference.errors.length && row.selectedExpectedSvg && row.sameStyles && row.sameSize && row.differentChannels === 0;
      }
      results.push(row);
    } catch (error) {
      results.push({ id: entry.id, passed: false, error: String(error) });
    } finally { state.dispose(); reference?.dispose(); }
  }
  window.pictureFallbackResults = results;
  return results;
};

window.runPictureFallbackRuntimeProbe = async () => {
  const results = [];
  const source = cases.find(entry => entry.id === 'transitional-slide-primary').model;
  for (const name of ['outer-invalid', 'self-contained', 'external-resource', 'nesting-limit', 'decode-queue', 'destroyed', 'unmounted', 'superseded']) {
    const model = structuredClone(source), node = imageNode(model);
    if (name === 'outer-invalid') node.pictureSvg = '<svg';
    if (name === 'self-contained') model.assets = {};
    if (name === 'external-resource') node.pictureSvg = '<svg xmlns="http://www.w3.org/2000/svg"><image href="https://invalid.example/image.png"/></svg>';
    if (name === 'nesting-limit') {
      let nested = '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>';
      for (let depth = 0; depth < 34; depth++) nested = `<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><image href="${encode(nested)}"/></svg>`;
      node.pictureSvg = nested;
    }
    if (name === 'decode-queue') {
      const defs = Array.from({ length: 12 }, (_, index) => `<image href="${encode(`<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><rect width="1" height="1" fill="rgb(${index},0,0)"/></svg>`)}"/>`).join('');
      node.pictureSvg = node.pictureSvg.replace('</svg>', `<defs>${defs}</defs></svg>`);
    }
    if (['destroyed', 'unmounted', 'superseded'].includes(name)) {
      node.pictureSvg = '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><image href="data:image/png,invalid"/></svg>';
      node.fallbackPicture.svg = node.pictureSvg;
    }
    const state = setup(model);
    const originalDecode = HTMLImageElement.prototype.decode;
    let active = 0, peak = 0, calls = 0;
    HTMLImageElement.prototype.decode = async function () {
      active++; calls++; peak = Math.max(peak, active);
      try { await wait(8); return await originalDecode.call(this); }
      finally { active--; }
    };
    try {
      if (['destroyed', 'unmounted', 'superseded'].includes(name)) {
        const handle = state.viewer.mount(0, state.host, 1), previous = handle.element.querySelector('img');
        if (name === 'destroyed') state.viewer.destroy();
        if (name === 'unmounted') handle.dispose();
        if (name === 'superseded') {
          const next = state.viewer.mount(0, state.host, 1);
          await next.ready;
        }
        await handle.ready;
        await wait(30);
        results.push({ name, passed: state.errors.length === (name === 'superseded' ? 1 : 0) && !previous?.getAttribute('src'),
          errors: state.errors, staleSource: previous?.getAttribute('src'), peak, calls });
      } else {
        const image = await rendered(state);
        const preferred = ['self-contained', 'decode-queue'].includes(name);
        const expected = preferred ? node.pictureSvg : node.fallbackPicture.svg;
        results.push({ name, errors: state.errors, peak, calls,
          passed: !state.errors.length && image?.src === encode(expected) && image.naturalWidth > 0 && peak <= 4 && (name !== 'decode-queue' || peak === 4) });
      }
    } catch (error) { results.push({ name, passed: false, error: String(error), peak, calls }); }
    finally { HTMLImageElement.prototype.decode = originalDecode; state.dispose(); }
  }
  window.pictureFallbackRuntimeResults = results;
  return results;
};
window.runPictureDataUriProbe = async () => {
  const source = cases.find(entry => entry.id === 'transitional-slide-primary').model;
  const svg = '<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><view id="v" viewBox="0 0 2 2"/><rect width="2" height="2" fill="#287fa8"/></svg>';
  const utf16 = '<?xml version="1.0" encoding="UTF-16"?>' + svg;
  const byteEscape = value => '%' + value.toString(16).padStart(2, '0');
  const urls = {
    'base64-fragment': 'data:image/svg+xml;base64,' + btoa(svg) + '#v',
    'escaped-base64': 'data:image/svg+xml;base64,' + [...btoa(svg)].map(char => byteEscape(char.charCodeAt(0))).join(''),
    'percent-utf16': 'data:image/svg+xml,%ff%fe' + [...utf16].map(char => byteEscape(char.charCodeAt(0)) + '%00').join(''),
    'percent-unicode': encode(svg.replace('<rect', '<desc>é</desc><rect')),
  };
  const rows = [];
  for (const [name, url] of Object.entries(urls)) {
    const state = setup(source);
    try {
      const artwork = `<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><image href="${url}" width="2" height="2"/></svg>`;
      const resolved = await state.viewer.assetCache.pictureUrl(artwork);
      const image = new Image(); image.src = resolved; await image.decode();
      const rgba = Array.from(pixels(image).slice(0, 4));
      rows.push({ name, rgba, passed: resolved === encode(artwork) && same(rgba, [40, 127, 168, 255]) });
    } catch (error) { rows.push({ name, passed: false, error: String(error) }); }
    finally { state.dispose(); }
  }
  return rows;
};
window.ready = true;

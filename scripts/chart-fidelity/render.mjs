import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const [root, run, modulePath] = process.argv.slice(2);
if (!root || !run) throw Error("Usage: node render.mjs <corpus directory> <output directory> [renderer entry]");
const sha = (data) => createHash("sha256").update(data).digest("hex");
const read = (path) => JSON.parse(readFileSync(path, "utf8"));
const entry = modulePath ? pathToFileURL(resolve(modulePath)) : new URL("../../browser/full.js", import.meta.url);
const { loadOfficeCharts } = await import(entry);
const binary = readFileSync(new URL("./generated/full/office_charts_bg.wasm", entry));
const renderer = await loadOfficeCharts(binary);
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.CHART_FIDELITY_PLAYWRIGHT || "playwright-core");
const browser = await chromium.launch({ executablePath: process.env.CHART_FIDELITY_CHROMIUM || undefined });
const jobs = readFileSync(resolve(root, "jobs.json"));
const results = [];
const fonts = process.env.CHART_FIDELITY_FONTS ? read(process.env.CHART_FIDELITY_FONTS) : [];
const faces = fonts.map(f => ({ ...f, data: readFileSync(f.path).toString("base64") }));
const fontManifest = faces.map(({family, weight, style, data}) => ({
  family, weight, style, sha256: sha(Buffer.from(data, "base64")),
}));
const engineSha256 = sha(binary);
const environment = sha(JSON.stringify([engineSha256, fontManifest, browser.version(), sha(readFileSync(new URL(import.meta.url)))]));
let cache = {};
try {
  const prior = read(resolve(run, "renders.json"));
  if (prior.environment === environment) cache = Object.fromEntries(prior.rows.filter(x => x.rendered).map(x => [x.id, x]));
} catch (error) { if (error.code !== "ENOENT") throw error; }
mkdirSync(run, { recursive: true });
const page = await browser.newPage({ deviceScaleFactor: 1 });
await page.route(/^https?:/, route => route.abort());
await page.setContent('<style>*{margin:0}iframe{display:block;border:0;width:100vw;height:100vh}</style><iframe sandbox="allow-same-origin"></iframe>');
await page.evaluate(async faces => {
  const doc = document.querySelector("iframe").contentDocument;
  doc.head.innerHTML = `<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:"><style>*{margin:0}svg{display:block}</style>`;
  for (const {family, weight, style, data} of faces) {
    const face = new FontFace(family, 'url(data:font/ttf;base64,' + data + ')',
      { weight: weight || "400", style: style || "normal" });
    doc.fonts.add(await face.load());
  }
}, faces);
try {
  for (const r of JSON.parse(jobs)) {
    const input = readFileSync(resolve(root, "cases", r.id + ".json"));
    const {xml, options} = JSON.parse(input);
    const cacheKey = sha(JSON.stringify([sha(input), r.cssWidth, r.cssHeight, r.dpi, r.pixels]));
    if (cache[r.id]?.cacheKey === cacheKey) { results.push(cache[r.id]); continue; }
    const result = { id: r.id, cacheKey };
    try {
      const width = r.cssWidth ?? options.width ?? 640;
      const height = r.cssHeight ?? options.height ?? 360;
      const svg = renderer.renderChart(xml, { ...options, width, height });
      if (!svg) throw Error("unsupported");
      result.svgSha256 = createHash("sha256").update(svg).digest("hex");
      const out = resolve(run, r.id);
      writeFileSync(out + ".svg", svg);
      const pixels = r.pixels ?? [Math.ceil(width), Math.ceil(height)];
      const scale = (r.dpi || 96) / 96;
      await page.setViewportSize({ width: pixels[0], height: pixels[1] });
      const geometry = await page.evaluate(async ({ svg, renderedWidth, renderedHeight }) => {
        const doc = new DOMParser().parseFromString(svg, "image/svg+xml");
        if (doc.querySelector("parsererror")) throw Error("invalid SVG");
        const host = document.querySelector("iframe").contentDocument;
        const root = doc.documentElement;
        root.style.width = renderedWidth + "px";
        root.style.height = renderedHeight + "px";
        host.body.replaceChildren(host.importNode(root, true));
        await host.fonts.ready;
        return {
          seriesTargets: [...new Set(Array.from(doc.querySelectorAll("[data-chart-series]"),
            n => Number(n.getAttribute("data-chart-series"))))].sort((a, b) => a - b),
          pointTargets: doc.querySelectorAll("[data-chart-point]").length,
          labels: Array.from(doc.querySelectorAll("text"), n => n.textContent),
          notices: Array.from(doc.querySelectorAll("desc"), n => n.textContent),
        };
      }, { svg, renderedWidth: width * scale, renderedHeight: height * scale });
      await page.screenshot({ path: out + ".png", animations: "disabled" });
      Object.assign(result, geometry, { rendered: true, size: [width, height], pixels });
    } catch (error) {
      Object.assign(result, { rendered: false, error: String(error) });
    }
    results.push(result);
  }
  writeFileSync(resolve(run, "renders.json"), JSON.stringify({
    completedAt: new Date().toISOString(),
    jobsSha256: createHash("sha256").update(jobs).digest("hex"),
    environment, engineSha256, fontManifest,
    browser: browser.version(), rows: results,
  }, null, 2));
} finally {
  await browser.close();
}
console.log(results.filter(r => r.rendered).length, "/", results.length, "produced SVG and PNG");

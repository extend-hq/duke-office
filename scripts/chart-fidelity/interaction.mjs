import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { join } from "node:path";
import { loadOfficeCharts } from "../../browser/full.js";

const directory = process.argv[2];
if (!directory) throw new Error("Pass the extracted chart corpus directory.");
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.CHART_FIDELITY_PLAYWRIGHT ?? "playwright-core");
const browser = await chromium.launch({
  headless: true,
  executablePath: process.env.CHART_FIDELITY_CHROMIUM || undefined,
});
const binary = await readFile(new URL("../../browser/generated/full/office_charts_bg.wasm", import.meta.url));
const renderer = await loadOfficeCharts(binary);
const manifestSource = await readFile(join(directory, "inputs.json"), "utf8");
const manifest = JSON.parse(manifestSource);
const sourceHash = createHash("sha256").update(manifestSource);

function inspectChart({ svg, xml, data }) {
  const parse = (markup) => new DOMParser().parseFromString(markup, "application/xml");
  const chart = parse(xml);
  const kids = (node, name) => Array.from(node?.children ?? []).filter(n => n.localName === name);
  const pointIndices = (node) => Array.from(node?.getElementsByTagNameNS("*", "pt") ?? [])
    .map(pt => pt.getAttribute("idx"));
  const plot = chart.getElementsByTagNameNS("*", "plotArea")[0];
  let series;
  let seriesKinds;
  if (data) {
    series = data.series.map(s => s.points.map(p => String(p.index)));
    seriesKinds = data.series.map(s => s.chartType);
  } else if (kids(plot, "plotAreaRegion").length) {
    const datasets = Array.from(chart.getElementsByTagNameNS("*", "data"));
    const nodes = kids(kids(plot, "plotAreaRegion")[0], "series");
    seriesKinds = nodes.map(s => s.getAttribute("layoutId"));
    series = nodes.map(s => {
      const id = kids(s, "dataId")[0]?.getAttribute("val");
      return pointIndices(datasets.find(d => d.getAttribute("id") === id));
    });
  } else {
    const groups = Array.from(plot?.children ?? []).filter(n => n.localName.endsWith("Chart"));
    seriesKinds = groups.flatMap(group => kids(group, "ser").map(() => group.localName.slice(0, -5)));
    series = groups.flatMap(group => kids(group, "ser")).map(s => Array.from(s.children)
        .filter(n => ["val", "cat", "yVal", "xVal", "bubbleSize"].includes(n.localName))
        .flatMap(pointIndices));
  }

  const root = document.importNode(parse(svg).documentElement, true);
  document.body.replaceChildren(root);
  root.style.maxWidth = "100vw";
  root.style.maxHeight = "100vh";
  const targets = [...root.querySelectorAll("[data-chart-series]")];
  const invalid = [];
  const plans = new Map();
  const markedSeries = new Set();
  for (const node of targets) {
    const { chartKind: kind, chartSeries: ordinal, chartPoint: point } = node.dataset;
    if (seriesKinds.includes("doughnut") && seriesKinds.includes("scatter")) {
      const expected = kind === "slice" ? "doughnut" : kind === "line" ? "scatter" : null;
      if (expected && seriesKinds[ordinal] !== expected) {
        invalid.push({ kind, ordinal, point, expected });
        continue;
      }
    }
    if (!/^\d+$/.test(ordinal) || !series[ordinal] || (point !== undefined && !series[ordinal].includes(point))) {
      invalid.push({ kind, ordinal, point });
      continue;
    }
    if (kind !== "legendEntry") markedSeries.add(ordinal);
    const key = [ordinal, kind].join("/");
    if (plans.has(key)) continue;
    const rect = node.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) continue;
    const candidates = [];
    for (const q of [0.5, 0.25, 0.75, 0.1, 0.9]) {
      for (const t of [0.5, 0.25, 0.75, 0.1, 0.9]) {
        candidates.push([rect.left + rect.width * q, rect.top + rect.height * t]);
      }
    }
    const shape = node instanceof SVGGeometryElement ? node : node.querySelector("path, polyline, line");
    if (shape) {
      const length = shape.getTotalLength();
      for (const fraction of [0.5, 0.25, 0.75]) {
        const p = shape.getPointAtLength(length * fraction).matrixTransform(shape.getScreenCTM());
        candidates.push([p.x, p.y]);
      }
    }
    for (const candidate of candidates) {
      const [x, y] = candidate.map(Math.round);
      const hit = document.elementFromPoint(x, y)?.closest("[data-chart-series]");
      if (hit !== node) continue;
      plans.set(key, { x, y, kind, ordinal, point });
      break;
    }
  }
  window.chartHit = null;
  root.addEventListener("pointerdown", e => {
    const hit = e.target.closest("[data-chart-series]");
    window.chartHit = hit && { kind: hit.dataset.chartKind, ordinal: hit.dataset.chartSeries,
      point: hit.dataset.chartPoint, trusted: e.isTrusted };
  });
  return { invalid, seriesCount: series.length, seriesKinds, marks: targets.length,
    markedSeries: [...markedSeries], plans: [...plans.values()] };
}

const rows = [];
try {
  const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
  await page.route("**/*", route => route.abort());
  await page.setContent('<style>body{margin:0}</style>');
  for (const { id, kinds } of manifest) {
    const caseSource = await readFile(join(directory, "cases", id + ".json"), "utf8");
    sourceHash.update(id).update("\0").update(caseSource).update("\0");
    const { xml, options } = JSON.parse(caseSource);
    const width = options.width ?? 640;
    const height = options.height ?? 360;
    const svg = renderer.renderChart(xml, { ...options, interactive: true });
    if (!svg) {
      rows.push({ id, kinds, rendered: false });
      continue;
    }
    const { plans, ...inspected } = await page.evaluate(inspectChart, { svg, xml, data: options.data });
    const staticSvg = renderer.renderChart(xml, { ...options, interactive: false });
    const paintIdentical = await page.evaluate(async ({ first, second, width, height }) => {
      async function raster(markup) {
        const image = new Image();
        const url = URL.createObjectURL(new Blob([markup], { type: "image/svg+xml" }));
        try {
          image.src = url;
          await image.decode();
          const canvas = document.createElement("canvas");
          canvas.width = Math.ceil(width);
          canvas.height = Math.ceil(height);
          const ctx = canvas.getContext("2d");
          ctx.drawImage(image, 0, 0, width, height);
          return ctx.getImageData(0, 0, canvas.width, canvas.height).data;
        } finally {
          URL.revokeObjectURL(url);
        }
      }
      const [a, b] = await Promise.all([raster(first), raster(second)]);
      return a.length === b.length && a.every((value, i) => value === b[i]);
    }, { first: svg, second: staticSvg, width, height });
    const clicks = [];
    for (const plan of plans) {
      await page.evaluate(() => { window.chartHit = null; });
      await page.mouse.click(plan.x, plan.y);
      const hit = await page.evaluate(() => window.chartHit);
      const passed = Boolean(hit?.trusted && hit.kind === plan.kind && hit.ordinal === plan.ordinal && hit.point === plan.point);
      clicks.push({ ...plan, hit, passed });
    }
    const clickedSeries = new Set(clicks.filter(c => c.passed && c.kind !== "legendEntry").map(c => c.ordinal));
    rows.push({ id, kinds, rendered: true, ...inspected, paintIdentical, clicks,
      unreachedMarkedSeries: inspected.markedSeries.filter(s => !clickedSeries.has(s)) });
  }
} finally {
  await browser.close();
}
const summary = {
  charts: rows.length,
  rendered: rows.filter(r => r.rendered).length,
  chartsWithPlotTargets: rows.filter(r => r.markedSeries?.length).length,
  legendOnlyCharts: rows.filter(r => r.marks && !r.markedSeries?.length).length,
  plotClicked: rows.filter(r => r.clicks?.some(c => c.passed && c.kind !== "legendEntry")).length,
  clicks: rows.flatMap(r => r.clicks ?? []).length,
  failedClicks: rows.flatMap(r => r.clicks ?? []).filter(c => !c.passed).length,
  invalidTargets: rows.flatMap(r => r.invalid ?? []).length,
  alteredPaint: rows.filter(r => r.paintIdentical === false).length,
};
await writeFile(join(directory, "interaction.json"), JSON.stringify({
  engineSha256: createHash("sha256").update(binary).digest("hex"),
  corpusSha256: sourceHash.digest("hex"),
  browser: browser.version(), summary, rows,
}, null, 2));
console.log(summary);
if (summary.failedClicks || summary.invalidTargets || summary.alteredPaint || !summary.plotClicked) process.exitCode = 1;

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { gunzipSync } from "node:zlib";
import { loadOfficeCharts } from "@extend-ai/rust-office-charts";
import { sourceDigest } from "./source-digest.mjs";
import payload from "./generated/payload.js";

const storedDigest = readFileSync(new URL("./generated/sources.sha256", import.meta.url), "utf8").trim();
assert.equal(sourceDigest(), storedDigest, "Renderer inputs changed. Run npm run build:browser.");
const binary = readFileSync(new URL("./generated/office_charts_bg.wasm", import.meta.url));
assert.deepEqual(gunzipSync(Buffer.from(payload, "base64")), binary);

const xml = `<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart">
  <c:chart><c:plotArea><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/>
    <c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>Series</c:v></c:tx>
      <c:cat><c:strLit><c:pt idx="0"><c:v>A</c:v></c:pt><c:pt idx="1"><c:v>B</c:v></c:pt></c:strLit></c:cat>
      <c:val><c:numLit><c:pt idx="0"><c:v>1</c:v></c:pt><c:pt idx="1"><c:v>3</c:v></c:pt></c:numLit></c:val>
    </c:ser><c:axId val="1"/><c:axId val="2"/></c:barChart>
    <c:catAx><c:axId val="1"/><c:axPos val="b"/><c:crossAx val="2"/></c:catAx>
    <c:valAx><c:axId val="2"/><c:axPos val="l"/><c:crossAx val="1"/></c:valAx>
  </c:plotArea></c:chart></c:chartSpace>`;

const renderer = await loadOfficeCharts();
const require = createRequire(import.meta.url);
assert.equal(await require("@extend-ai/rust-office-charts").loadOfficeCharts(), renderer);
const options = { width: 320, height: 200, interactive: true, spreadsheetDefaults: true };
const cached = renderer.renderChart(xml, options);
assert.match(cached, /<svg\b/);
assert.match(cached, /data-chart-series="0" data-chart-point="1"/);

const updated = renderer.renderChart(xml, {
  ...options,
  data: {
    series: [{
      chartType: "bar",
      name: "Series",
      points: [
        { index: 0, value: "8", categories: [{ level: 0, label: "Updated category" }] },
        { index: 1, value: "3", categories: [{ level: 0, label: "B" }] },
      ],
    }],
  },
});
assert.match(updated, /Updated category/);
assert.notEqual(updated, cached);
assert.equal(renderer.renderChart("<invalid/>"), null);

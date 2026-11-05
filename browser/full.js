import init, { renderChartSvg } from "./generated/full/office_charts.js";
import { createLoader } from "./loader.js";

export const loadOfficeCharts = createLoader(init, renderChartSvg, () => import("./generated/full/payload.js"));

import init, { renderChartSvg } from "./generated/office_charts.js";
import { createLoader } from "./loader.js";

export const loadOfficeCharts = createLoader(init, renderChartSvg, () => import("./generated/payload.js"));

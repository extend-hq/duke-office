import * as React from 'react';
import { createRoot } from 'react-dom/client';
import { getSheetsWasmModule } from '@xlsx/wasm';
import { safeCalculate } from '@xlsx/safe-calculate';
import { parseWorkbookChartStyleAssets } from '@xlsx/images';
import { loadWorkbookChartAssets } from '@xlsx/charts';
import { SharedChartSvg } from '@xlsx/shared-chart';
import { loadSpreadsheetFonts } from '@xlsx/spreadsheet-fonts';
import { loadOfficeCharts } from '@extend-ai/rust-office-charts/full';

const container = document.getElementById('chart')!;
const root = createRoot(container);
let workbook: any;
let assets: any;
let charts: any[] = [];
let currentSelection: any = null;
let renderNumber = 0;
const [sheets, renderer] = await Promise.all([getSheetsWasmModule(), loadOfficeCharts(), loadSpreadsheetFonts()]);

function Chart({ chart, width, height }: any) {
  const [selected, setSelected] = React.useState(null);
  currentSelection = selected;
  const fallback = <div data-verification-empty="true">No rendered chart</div>;
  if (!chart.renderSource) return <div data-verification-no-source="true">No chart source</div>;
  return <SharedChartSvg chart={chart} source={chart.renderSource} renderer={renderer}
    rect={{left:0,top:0,width,height}} palette={{border:'#ddd',mutedText:'#666',surface:'#fff',text:'#000'}}
    selectedChartElement={selected} fallback={fallback}
    onChartElementPointerDown={(selection: any) => setSelected(selection)} />;
}

const frame = () => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
const metadata = (chart: any, index: number) => ({
  index, id: chart.id, name: chart.name, title: chart.title, chartType: chart.chartType,
  is3d: chart.is3d, sheetIndex: chart.workbookSheetIndex,
  part: assets.chartOriginsById.get(chart.id)?.chartPath ?? chart.chartPath,
  anchor: chart.anchor, hasRenderSource: Boolean(chart.renderSource),
  series: chart.series.map((s: any) => ({id:s.id,name:s.name,points:s.values.length,categories:s.categories.length})),
});

(window as any).chartHarness = {
  async load(path: string) {
    root.render(null);
    workbook?.free?.();
    const bytes = new Uint8Array(await (await fetch('/workbook?path=' + encodeURIComponent(path))).arrayBuffer());
    const parsed = sheets.Workbook.fromBytes(bytes);
    const formulas = Array.from({length:parsed.sheetCount}, (_, i) => parsed.getSheet(i).formulaCount).reduce((a,b)=>a+b,0);
    workbook = formulas <= 1000 ? safeCalculate(parsed, {reparse:()=>sheets.Workbook.fromBytes(bytes)}).workbook : parsed;
    const styles = parseWorkbookChartStyleAssets(bytes);
    assets = loadWorkbookChartAssets(workbook, styles, new Map(Array.from({length:workbook.sheetCount},(_,i)=>[i,i])), true);
    charts = assets.chartsByWorkbookSheetIndex.flat();
    return {sheets:workbook.sheetNames, charts:charts.map(metadata), formulas};
  },
  async render(index: number, width = 640, height = 400) {
    const chart = charts[index];
    container.style.width = width + 'px'; container.style.height = height + 'px';
    currentSelection = null;
    root.render(<Chart key={++renderNumber} chart={chart} width={width} height={height} />);
    await frame();
    await document.fonts.ready;
    document.getElementById('status')!.textContent = '';
    return this.inspect();
  },
  inspect() {
    const svg = container.querySelector('svg');
    return {
      empty: Boolean(container.querySelector('[data-verification-empty],[data-verification-no-source]')) || !svg,
      svg: svg?.outerHTML,
      selection: currentSelection,
      shapes: Array.from(container.querySelectorAll('[data-xlsx-chart-series-index]')).map((node: any, i) => ({
        ordinal:i, tag:node.localName, series:Number(node.dataset.xlsxChartSeriesIndex),
        point:node.dataset.xlsxChartPointIndex == null ? null : Number(node.dataset.xlsxChartPointIndex),
        kind:node.getAttribute('data-chart-kind'), path:node.getAttribute('d'),
        bounds:JSON.parse(JSON.stringify(node.getBoundingClientRect())),
      })),
      geometryCount: svg?.querySelectorAll('path,rect,circle,ellipse,polygon,polyline,line').length ?? 0,
      text: svg?.textContent ?? container.textContent,
    };
  },
  async resetSelection() {
    const index = charts.findIndex(c=>c.id === currentSelection?.chartId);
    if (index >= 0) await this.render(index, container.clientWidth, container.clientHeight);
  },
};
document.getElementById('status')!.textContent = 'Ready';

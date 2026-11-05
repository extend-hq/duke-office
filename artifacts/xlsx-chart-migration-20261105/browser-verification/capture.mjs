import { chromium } from '/Users/andrewluo/.npm/_npx/31e32ef8478fbf80/node_modules/playwright/index.mjs';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(fileURLToPath(import.meta.url));
const inventory = JSON.parse(await readFile(resolve(root, '../excel-reference/inventory.json'), 'utf8'));
const output = resolve(root, process.env.CAPTURE_RUN || 'captures');
await mkdir(output, {recursive:true});
const browser = await chromium.launch({channel:'chrome',headless:true});
const page = await browser.newPage({viewport:{width:1600,height:1200},deviceScaleFactor:1});
const errors = [];
page.on('pageerror',error=>errors.push(String(error)));
await page.goto('http://127.0.0.1:4189');
await page.waitForFunction(()=>Boolean(window.chartHarness), {timeout:120000});
const results = [];
const filter = process.env.WORKBOOK_FILTER;
for (const book of inventory.workbooks.filter(book=>!filter || book.relative.includes(filter))) {
  const workbook = {path:book.path,relative:book.relative,sha256:book.sha256,expected:book.charts.length,charts:[],errors:[]};
  try {
    const loaded = await page.evaluate(path=>window.chartHarness.load(path),book.path);
    workbook.loaded = loaded.charts.length;
    for (const chart of loaded.charts) {
      const declared = book.charts.find(candidate=>candidate.part === chart.part);
      const owner = declared?.owners.find(owner=>owner.widthEmu && Number(owner.widthEmu)>0);
      const width = Math.max(80,Math.min(2400,Math.round(Number(owner?.widthEmu || chart.anchor?.sizeEmu?.cx || 6096000)/9525)));
      const height = Math.max(60,Math.min(1800,Math.round(Number(owner?.heightEmu || chart.anchor?.sizeEmu?.cy || 3810000)/9525)));
      await page.setViewportSize({width:Math.max(width,1000),height:Math.max(height,800)});
      const key = book.sha256.slice(0,12)+'-'+(chart.part?.split('/').pop()?.replace('.xml','') || chart.id);
      try {
        const rendered = await page.evaluate(({index,width,height})=>window.chartHarness.render(index,width,height),{index:chart.index,width,height});
        if(rendered.svg) await writeFile(resolve(output,key+'.svg'), rendered.svg);
        await page.locator('#chart').screenshot({path:resolve(output,key+'.png'),timeout:30000});
        const {svg,...details}=rendered;
        const result={...chart,key,width,height,png:resolve(output,key+'.png'),svg:rendered.svg?resolve(output,key+'.svg'):null,...details};
        workbook.charts.push(result);
        console.log(JSON.stringify({workbook:book.relative,chart:chart.part,type:chart.chartType,empty:details.empty,shapes:details.shapes.length}));
      } catch(error) { workbook.charts.push({...chart,key,width,height,error:String(error)}); }
    }
    workbook.missing = book.charts.filter(expected=>!loaded.charts.some(chart=>chart.part === expected.part)).map(chart=>({part:chart.part,types:chart.types,title:chart.title}));
  } catch(error) {workbook.errors.push(String(error));}
  results.push(workbook);
  await writeFile(resolve(output,'results.json'),JSON.stringify({generatedAt:new Date().toISOString(),errors,workbooks:results},null,2));
}
await browser.close();
console.log(JSON.stringify({workbooks:results.length,charts:results.reduce((n,b)=>n+b.charts.length,0),empty:results.flatMap(b=>b.charts.filter(c=>c.empty||c.error).map(c=>({workbook:b.relative,part:c.part,error:c.error}))),missing:results.flatMap(b=>(b.missing||[]).map(c=>({workbook:b.relative,...c}))),errors:results.flatMap(b=>b.errors.map(error=>({workbook:b.relative,error})))}));

async (page) => {
  const base = process.env.ODF_VISUAL_URL;
  const output = process.env.ODF_VISUAL_CAPTURES;
  if (!base || !output) throw new Error('Set ODF_VISUAL_URL and ODF_VISUAL_CAPTURES');
  await page.setViewportSize({width:432,height:432});
  await page.goto(base);
  await page.waitForFunction(() => typeof globalThis.renderCase === 'function');
  const count = await page.evaluate(async () => (await fetch('manifest.json').then(r=>r.json())).cases.length);
  for (const version of ['baseline', 'candidate']) {
    for (let index=0; index<count; index++) {
      await page.evaluate(async ({version,index}) => { await globalThis.renderCase(`${version}.json`,index); }, {version,index});
      await page.screenshot({path:`${output}/${version}/page-${String(index+1).padStart(2,'0')}.png`});
    }
  }
  return {browser:page.context().browser().version(),deviceScale:await page.evaluate(()=>devicePixelRatio)};
}

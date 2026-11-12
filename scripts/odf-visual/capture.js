async (page) => {
  const {url, output, width, height, count} = __ODF_CAPTURE_CONFIG__;
  await page.setViewportSize({width,height});
  await page.goto(url);
  await page.waitForFunction(() => typeof globalThis.renderCase === 'function');
  for (const version of ['baseline', 'candidate']) {
    for (let index=0; index<count; index++) {
      await page.evaluate(async ({version,index}) => { await globalThis.renderCase(`${version}.json`,index); }, {version,index});
      await page.screenshot({path:`${output}/${version}/page-${String(index+1).padStart(2,'0')}.png`});
    }
  }
  return {browser:page.context().browser().version(),deviceScale:await page.evaluate(()=>devicePixelRatio)};
}

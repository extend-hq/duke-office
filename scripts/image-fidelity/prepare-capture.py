"""Generate a Playwright CLI capture function for a picture comparison."""

import argparse
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--target', choices=['parser', 'viewer'], required=True)
    parser.add_argument('--base-url', required=True)
    args = parser.parse_args()
    directory = args.directory.resolve()
    count = len(json.loads((directory / 'manifest.json').read_text())['cases'])
    output = directory / (args.target + '-pages')
    output.mkdir(exist_ok=True)
    config = json.dumps(dict(url=args.base_url, directory=str(directory), output=str(output), count=count, target=args.target))
    code = r'''async (page) => {
  const config = CONFIG;
  await page.setViewportSize({ width: 1440, height: 1200 });
  await page.goto(config.url);
  if (config.target === 'viewer') {
    await page.locator('input[type="file"][accept*=".doc"]').setInputFiles(config.directory + '/image-matrix.docx');
    await page.locator('[data-docx-page-surface="true"]').first().waitFor({ timeout: 120000 });
  }
  for (let index = 0; index < config.count; index++) {
    if (config.target === 'viewer') {
      await page.locator('[data-docx-editor-root="true"]').evaluate((root, index) => {
        const frame = root.querySelector('[data-docx-page-wrapper="true"]');
        if (!frame) throw new Error('Missing page wrapper');
        const style = getComputedStyle(root);
        const stride = frame.getBoundingClientRect().height + parseFloat(style.rowGap || '0');
        let parent = root.parentElement;
        while (parent && !(parent.scrollHeight > parent.clientHeight && /auto|scroll/.test(getComputedStyle(parent).overflowY))) parent = parent.parentElement;
        if (!parent) throw new Error('Missing viewer scroll container');
        parent.scrollTop = index * stride;
      }, index);
    }
    const selector = config.target === 'viewer'
      ? `[data-docx-page-index="${index}"] [data-docx-page-surface="true"]`
      : `#page-${String(index + 1).padStart(2, '0')}`;
    const surface = page.locator(selector);
    await surface.waitFor({ timeout: 30000 });
    await surface.scrollIntoViewIfNeeded();
    await surface.evaluate(async element => {
      await Promise.all([...element.querySelectorAll('img')].map(image => image.decode()));
      await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    });
    await surface.screenshot({ path: `${config.output}/page-${String(index + 1).padStart(2, '0')}.png`, animations: 'disabled', caret: 'hide', scale: 'css' });
  }
  return { pages: config.count, target: config.target };
}
'''.replace('CONFIG', config)
    path = directory / ('capture-' + args.target + '.js')
    path.write_text(code)
    print(path)


if __name__ == '__main__':
    main()

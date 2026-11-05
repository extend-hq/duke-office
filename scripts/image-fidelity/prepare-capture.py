"""Generate a Playwright CLI capture function for a picture comparison."""

import argparse
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--target', choices=['parser', 'viewer'], required=True)
    parser.add_argument('--base-url', required=True)
    parser.add_argument('--output-directory', type=Path)
    parser.add_argument('--script-output', type=Path)
    args = parser.parse_args()
    directory = args.directory.resolve()
    manifest = json.loads((directory / 'manifest.json').read_text())
    count = len(manifest['cases'])
    output = args.output_directory.resolve() if args.output_directory else directory / (args.target + '-pages')
    output.mkdir(parents=True, exist_ok=True)
    config = json.dumps(dict(url=args.base_url, directory=str(directory), document=manifest['document'], output=str(output), count=count, target=args.target))
    code = r'''async (page) => {
  const config = CONFIG;
  await page.setViewportSize({ width: 1440, height: 1600 });
  await page.goto(config.url);
  if (config.target === 'viewer') {
    await page.locator('input[type="file"][accept*=".doc"]').setInputFiles(config.directory + '/' + config.document);
    await page.locator('[data-docx-page-surface="true"]').first().waitFor({ timeout: 120000 });
    await page.locator('[data-docx-page-surface="true"] img').first().waitFor({ timeout: 120000 });
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
    path = args.script_output.resolve() if args.script_output else directory / ('capture-' + args.target + '.js')
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(code)
    print(path)


if __name__ == '__main__':
    main()

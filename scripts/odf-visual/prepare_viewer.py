"""Bundle the existing presentation viewer and prepare fixed-page browser capture."""
import argparse
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('--viewer-repo', type=Path, required=True)
    parser.add_argument('--esbuild', type=Path, required=True)
    parser.add_argument('--url', required=True)
    parser.add_argument('--captures', type=Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve()
    manifest = json.loads((root / 'manifest.json').read_text())
    width, height = [manifest['pageSizePx'][k] for k in ('width', 'height')]
    module = args.viewer_repo.resolve() / 'packages/react-viewer/src/normalized-viewer.ts'
    entry = root / 'viewer-entry.ts'
    entry.write_text('import { NormalizedPresentationViewer } from ' + json.dumps(str(module)) + ';\n' + '''
let viewer;
globalThis.renderCase = async (filename, index) => {
  if (viewer) viewer.destroy();
  const model = await fetch(filename).then(r => r.json());
  viewer = new NormalizedPresentationViewer(document.querySelector('#viewer'), model);
  await viewer.setZoom(100);
  await viewer.renderSlide(index);
  await document.fonts.ready;
  await Promise.all([...document.images].map(i => i.decode().catch(() => {})));
};
''')
    subprocess.run([str(args.esbuild.resolve()), str(entry), '--bundle', '--format=esm',
                    '--platform=browser', '--loader:.wasm=file', f'--outfile={root / "viewer.js"}'], check=True)
    (root / 'index.html').write_text(f'''<!doctype html><html><head><style>
html,body{{margin:0;background:white}}#viewer{{width:{width}px;height:{height}px;overflow:hidden}}
.rpv-normalized-slide{{box-shadow:none!important;border:0!important;margin:0!important}}
</style></head><body><div id="viewer"></div><script type="module" src="viewer.js"></script></body></html>''')
    config = dict(url=args.url, output=str(args.captures.resolve()), width=width, height=height,
                  count=len(manifest['cases']))
    for version in ('baseline', 'candidate'):
        (args.captures / version).mkdir(parents=True, exist_ok=True)
    template = Path(__file__).with_name('capture.js').read_text()
    (root / 'capture.js').write_text(template.replace('__ODF_CAPTURE_CONFIG__', json.dumps(config)))
    print(root / 'capture.js')


if __name__ == '__main__':
    main()

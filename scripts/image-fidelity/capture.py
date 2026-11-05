"""Export the generated matrix through Word and retain a native reference receipt."""

import argparse
import hashlib
import json
from pathlib import Path
import plistlib
import re
import subprocess
from datetime import datetime, timezone


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', type=Path, required=True)
    args = parser.parse_args()
    directory = args.directory.resolve()
    manifest = json.loads((directory / 'manifest.json').read_text())
    document = directory / manifest['document']
    if digest(document) != manifest['documentSha256']:
        raise ValueError('Input digest does not match the manifest')
    pdf = directory / 'word-reference.pdf'
    receipt = directory / 'word-reference.json'
    if pdf.exists() or receipt.exists():
        raise FileExistsError('Native reference already exists; choose a new output directory')
    captured = subprocess.run(['osascript', str(Path(__file__).with_suffix('.applescript')), str(document), str(pdf)], text=True, capture_output=True, timeout=210)
    if captured.returncode:
        raise RuntimeError(captured.stderr)
    metadata = json.loads(captured.stdout)
    if metadata.get('status') != 'ok' or not pdf.exists():
        raise RuntimeError(metadata)
    info = subprocess.check_output(['pdfinfo', str(pdf)], text=True)
    count = int(re.search(r'^Pages:\s+(\d+)', info, re.MULTILINE)[1])
    if count != len(manifest['cases']):
        raise ValueError(f'Expected {len(manifest["cases"])} pages; native export contains {count}')
    png_dir = directory / 'word-pages'
    png_dir.mkdir()
    subprocess.run(['pdftoppm', '-png', '-r', '96', str(pdf), str(png_dir / 'page')], check=True)
    app = Path('/Applications/Microsoft Word.app/Contents/Info.plist')
    plist = plistlib.loads(app.read_bytes())
    metadata.update({
        'capturedAt': datetime.now(timezone.utc).isoformat(),
        'wordBundleVersion': plist.get('CFBundleVersion'),
        'wordShortVersion': plist.get('CFBundleShortVersionString'),
        'documentSha256': digest(document),
        'pdfSha256': digest(pdf),
        'pageCount': count,
        'dpi': 96,
        'exportMethod': 'Microsoft Word save as PDF; Poppler rasterized at 96 dpi',
        'pages': [dict(path=str(path.relative_to(directory)), sha256=digest(path)) for path in sorted(png_dir.glob('*.png'))],
    })
    receipt.write_text(json.dumps(metadata, indent=2) + '\n')
    print(json.dumps(metadata))


if __name__ == '__main__':
    main()

"""Export chart pictures from disposable XLSX copies in desktop Excel for Mac."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
from zipfile import ZipFile

HERE = Path(__file__).resolve().parent
SCRIPT = str(HERE / 'collect.applescript')


def capture(inputs, output, include_sheets=None):
    output.mkdir(parents=True, exist_ok=True)
    manifest = output / 'references.json'
    refs = json.loads(manifest.read_text()) if manifest.exists() else []
    pictures = output / 'excel'
    pictures.mkdir(exist_ok=True)
    version = subprocess.check_output(['osascript', '-e', 'tell application "Microsoft Excel" to get version'], text=True).strip()
    with tempfile.TemporaryDirectory(prefix='office-chart-oracle-') as tmp:
        tmp = Path(tmp)
        helper = tmp / 'picture'
        subprocess.run(['swiftc', str(HERE / 'picture.swift'), '-o', str(helper)], check=True)
        seen = set()
        for path in inputs:
            with ZipFile(path) as z:
                if any('vbaproject' in n.lower() or '/externallinks/' in n.lower() for n in z.namelist()):
                    print('Skipping workbook with macros or external links:', path.name, flush=True)
                    continue
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            if digest in seen:
                continue
            seen.add(digest)
            book = tmp / f'{tmp.name}-{digest[:12]}.xlsx'
            shutil.copyfile(path, book)
            book.chmod(0o444)
            subprocess.run(['open', '-g', '-a', 'Microsoft Excel', str(book)], check=True)
            try:
                time.sleep(1)
                for attempt in range(12):
                    inventory = subprocess.run(['osascript', SCRIPT, book.name, 'inventory'], text=True, capture_output=True, timeout=40)
                    if inventory.returncode == 0:
                        break
                    time.sleep(0.4)
                else:
                    print('Could not inspect', path.name, inventory.stderr.strip(), flush=True)
                    continue
                (pictures / f'{digest[:16]}-inventory.txt').write_text(inventory.stdout)
                known = {(r['sheet'], r.get('chartOrdinal', r['name'])) for r in refs if r['workbookSha256'] == digest and (output/r['png']).is_file()}
                for i, row in enumerate(inventory.stdout.splitlines()):
                    if not row.strip():
                        continue
                    kind, sheet, index, name, width, height = row.split('\t')
                    if include_sheets and sheet not in include_sheets:
                        continue
                    if (sheet, int(index.split(',')[-1])) in known:
                        continue
                    base = pictures / f'{digest[:16]}-{i+1}'
                    pdf = base.with_suffix('.pdf')
                    result = subprocess.run([str(helper), str(pdf), SCRIPT, book.name, kind, sheet, index], capture_output=True, text=True, timeout=40)
                    if result.returncode or not (pdf.is_file() or base.with_suffix('.png').is_file()):
                        print('No picture:', path.name, sheet, index, 'dimensions', width, height, result.stderr.strip(), flush=True)
                        continue
                    vector = pdf.is_file()
                    if vector:
                        subprocess.run(['pdftoppm', '-singlefile', '-scale-dimension-before-rotation', '-r', '96', '-png', str(pdf), str(base)], check=True, capture_output=True)
                    ref = dict(workbookSha256=digest, sheet=sheet, name=name, png=str(base.with_suffix('.png').relative_to(output)), excelVersion=version, dpi=96 if vector else 72, exportMethod='PDF rasterized at 96 dpi' if vector else 'native pasteboard raster', chartOrdinal=int(index.split(',')[-1]), worksheetOrdinal=int(index.split(',')[0]), referenceKind=kind)
                    if float(width) > 0 and float(height) > 0:
                        ref.update(widthPt=float(width), heightPt=float(height))
                    refs.append(ref)
                    tmp_manifest = manifest.with_suffix('.tmp')
                    tmp_manifest.write_text(json.dumps(refs, indent=2)+'\n')
                    tmp_manifest.replace(manifest)
                print(path.name, '—', sum(r['workbookSha256'] == digest for r in refs), 'native pictures', flush=True)
            finally:
                closed = subprocess.run(['osascript', SCRIPT, book.name, 'close'], capture_output=True, text=True, timeout=40)
                if closed.returncode:
                    raise RuntimeError(f'Could not close temporary workbook {book.name}: {closed.stderr.strip()}')


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('inputs', type=Path, nargs='*')
    p.add_argument('--workbooks', type=Path)
    p.add_argument('--sheet', action='append')
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    inputs = a.inputs
    if a.workbooks:
        inputs += [Path(b['aliases'][0]) for b in json.loads(a.workbooks.read_text()) if b['charts']]
    if not inputs:
        p.error('Provide inputs or --workbooks')
    capture(list(dict.fromkeys(inputs)), a.output.resolve(), a.sheet)

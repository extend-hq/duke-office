"""Build LibreOffice reference text for the corpus.

    references.py            (run from the work directory's copy, or set WORK)

Text documents are exported as UTF-8 text. Presentations and drawings are
exported to PDF and read back with pdftotext. Output goes to `refs/<family>/`.
"""
import os, subprocess, sys, shutil, json, time
B = os.path.abspath(os.environ.get('WORK') or (sys.argv[1] if len(sys.argv) > 1 else os.getcwd()))
PROFILE = 'file://' + os.path.join(B, 'libreoffice-profile')
def soffice(args, timeout):
    try:
        subprocess.run(['soffice', f'-env:UserInstallation={PROFILE}', '--headless'] + args,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=timeout)
        return True
    except subprocess.TimeoutExpired:
        subprocess.run(['pkill', '-f', 'soffice.*libreoffice-profile'], stderr=subprocess.DEVNULL)
        time.sleep(2)
        return False
status = {}
for family, target, suffix in (('ODT', 'txt:Text (encoded):UTF8', '.txt'), ('ODP', 'pdf', '.pdf'), ('ODG', 'pdf', '.pdf')):
    src = os.path.join(B, 'corpus', family)
    groups = {}
    for name in sorted(os.listdir(src)):
        groups.setdefault(os.path.splitext(name)[1].lower(), []).append(name)
    for ext, names in groups.items():
        raw = os.path.join(B, 'refs_raw', family, ext.strip('.'))
        os.makedirs(raw, exist_ok=True)
        def produced(name):
            return os.path.exists(os.path.join(raw, os.path.splitext(name)[0] + suffix))
        for start in range(0, len(names), 25):
            batch = [n for n in names[start:start + 25] if not produced(n)]
            if batch:
                soffice(['--convert-to', target, '--outdir', raw] + [os.path.join(src, n) for n in batch], 420)
            for n in names[start:start + 25]:
                if not produced(n):
                    soffice(['--convert-to', target, '--outdir', raw, os.path.join(src, n)], 90)
        out = os.path.join(B, 'refs', family)
        os.makedirs(out, exist_ok=True)
        for n in names:
            produced_path = os.path.join(raw, os.path.splitext(n)[0] + suffix)
            ref = os.path.join(out, n + '.txt')
            if not os.path.exists(produced_path):
                status[f'{family}/{n}'] = 'no reference'
                continue
            if suffix == '.pdf':
                r = subprocess.run(['pdftotext', '-enc', 'UTF-8', produced_path, ref], capture_output=True)
                status[f'{family}/{n}'] = 'ok' if r.returncode == 0 and os.path.exists(ref) else 'pdftotext failed'
            else:
                shutil.copyfile(produced_path, ref)
                status[f'{family}/{n}'] = 'ok'
        print(family, ext, len(names), sum(1 for n in names if status.get(f'{family}/{n}') == 'ok'), flush=True)
json.dump(status, open(os.path.join(B, 'refs_status.json'), 'w'), indent=0)
print('done', flush=True)

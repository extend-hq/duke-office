import concurrent.futures
import hashlib
import json
from pathlib import Path
import urllib.parse
import urllib.request

OUT = Path('output/pptx-benchmark-20261106')

def download(item):
    repo, commit, path = item
    url = f'https://raw.githubusercontent.com/{repo}/{commit}/{urllib.parse.quote(path)}'
    data = urllib.request.urlopen(url, timeout=90).read()
    digest = hashlib.sha256(data).hexdigest()
    target = OUT / 'corpus' / (digest + Path(path).suffix.lower())
    target.write_bytes(data)
    return dict(id=digest[:16], sha256=digest, path=str(target.resolve()), bytes=len(data), repo=repo, commit=commit, source_path=path, url=url, format=target.suffix[1:])

def safe(job):
    try: return download(job)
    except Exception as exc: return str(exc)

if __name__ == '__main__':
    (OUT / 'corpus').mkdir(parents=True, exist_ok=True)
    (OUT / 'sources').mkdir(exist_ok=True)
    for repo, commit in [('apache/poi','39edce935ef8ed746b4457062ecfa2670eabe49f'),('scanny/python-pptx','278b47b1dedd5b46ee84c286e77cdfb0bf4594be')]:
        tree=json.load(urllib.request.urlopen(f'https://api.github.com/repos/{repo}/git/trees/{commit}?recursive=1'))
        paths=[n['path'] for n in tree['tree'] if n['path'].lower().endswith(('.pptx','.ppt'))]
        (OUT/'sources'/(repo.replace('/','_')+'.json')).write_text(json.dumps(dict(repo=repo,sha=commit,paths=paths),indent=2))
    jobs = []
    for source in sorted((OUT / 'sources').glob('*.json')):
        data = json.loads(source.read_text())
        jobs.extend((data['repo'], data['sha'], p) for p in data['paths'])
    rows, errors = [], []
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        for job, result in zip(jobs, pool.map(lambda j: safe(j), jobs)):
            if isinstance(result, dict): rows.append(result)
            else: errors.append(dict(job=job, error=result))
    unique = {r['sha256']:r for r in rows}
    (OUT / 'manifest.json').write_text(json.dumps(sorted(unique.values(), key=lambda r:r['sha256']), indent=2))
    (OUT / 'download-errors.json').write_text(json.dumps(errors, indent=2))
    print(json.dumps(dict(downloads=len(rows), unique=len(unique), errors=errors)))

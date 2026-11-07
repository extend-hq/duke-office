import concurrent.futures
import hashlib
import json
from pathlib import Path
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'output/pptx-benchmark-20261106/expanded'
MAX_BYTES = 80 * 1024 * 1024


def fetch(row):
    row = dict(row)
    try:
        with urllib.request.urlopen(row.pop('download_url', row['url']), timeout=90) as response:
            data = response.read(MAX_BYTES + 1)
        if len(data) > MAX_BYTES:
            raise ValueError('download exceeds size limit')
        digest = hashlib.sha256(data).hexdigest()
        if row.get('sha256') and row['sha256'] != digest:
            raise ValueError('source digest mismatch')
        path = OUT / 'corpus' / (digest + '.' + row['format'])
        path.write_bytes(data)
        row.update(id=digest[:16], sha256=digest, path=str(path), bytes=len(data), status='downloaded')
    except Exception as exc:
        row.update(status='download_error', error=type(exc).__name__)
    return row


def download(rows, name):
    (OUT / 'corpus').mkdir(parents=True, exist_ok=True)
    output = OUT / (name + '-downloads.jsonl')
    done = {r['url'] for r in map(json.loads, output.read_text().splitlines()) if r['status'] == 'downloaded'} if output.exists() else set()
    rows = [r for r in rows if r['url'] not in done]
    with output.open('a', buffering=1) as stream, concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        for index, result in enumerate(pool.map(fetch, rows)):
            stream.write(json.dumps(result) + '\n')
            if (index + 1) % 50 == 0:
                print(name, index + 1, '/', len(rows), flush=True)


def main():
    rows = []
    for source in sorted(OUT.glob('*-inventory.json')):
        record = json.loads(source.read_text())
        if 'repo' not in record:
            continue
        if record.get('truncated'):
            raise ValueError('incomplete repository inventory')
        for item in record['files']:
            path = item['path']
            rows.append(dict(repo=record['repo'], commit=record['commit'], source_path=path,
                format=Path(path).suffix.lower()[1:], source='github',
                url=f"https://raw.githubusercontent.com/{record['repo']}/{record['commit']}/{urllib.parse.quote(path)}"))
    download(rows, 'github')


if __name__ == '__main__':
    main()

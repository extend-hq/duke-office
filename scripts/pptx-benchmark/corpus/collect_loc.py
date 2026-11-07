import argparse
import binascii
import collections
from concurrent.futures import ThreadPoolExecutor
import csv
import hashlib
import io
import json
from pathlib import Path
import random
import struct
import urllib.parse
import urllib.request
import zipfile
import zlib

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'output/pptx-benchmark-20261106'
URL = 'https://lclabspublicdata.s3.us-east-2.amazonaws.com/lcwa_gov_powerpoint_data.zip'
METADATA_URL = 'https://lclabspublicdata.s3.us-east-2.amazonaws.com/lcwa_gov_powerpoint_metadata.csv'
MAX_PART = 64 * 1024 * 1024


class RangeReader(io.RawIOBase):
    def __init__(self):
        super().__init__()
        with urllib.request.urlopen(urllib.request.Request(URL, method='HEAD'), timeout=60) as response:
            self.size = int(response.headers['Content-Length'])
            self.etag = response.headers['ETag']
        self.position = 0
        self.transferred = 0

    def readable(self): return True
    def seekable(self): return True
    def tell(self): return self.position

    def seek(self, offset, whence=0):
        self.position = offset if whence == 0 else self.position + offset if whence == 1 else self.size + offset
        if self.position < 0:
            raise ValueError('Negative ZIP offset')
        return self.position

    def read(self, size=-1):
        size = self.size - self.position if size < 0 else min(size, self.size - self.position)
        if size <= 0: return b''
        data = self.fetch(self.position, size)
        self.position += len(data)
        return data

    def fetch(self, start, size):
        if size > MAX_PART or start < 0 or start + size > self.size:
            raise ValueError('Requested range exceeds download budget')
        if size == 0:
            return b''
        end = start + size - 1
        request = urllib.request.Request(URL, headers={'Range': f'bytes={start}-{end}', 'If-Match': self.etag})
        with urllib.request.urlopen(request, timeout=60) as response:
            if response.status != 206 or response.headers.get('Content-Range') != f'bytes {start}-{end}/{self.size}':
                raise ValueError('Server did not honor the bounded byte range')
            data = response.read(size + 1)
            if len(data) != size: raise ValueError('Truncated or oversized range response')
        self.transferred += len(data)
        return data


def content(reader, entry):
    header = struct.unpack('<4s5H3I2H', reader.fetch(entry.header_offset, 30))
    if header[0] != b'PK\x03\x04' or header[2] & 1:
        raise ValueError('Unsupported local ZIP header')
    if entry.file_size > MAX_PART or entry.compress_size > MAX_PART:
        raise ValueError('Member exceeds download budget')
    payload = reader.fetch(entry.header_offset + 30 + header[-2] + header[-1], entry.compress_size)
    if entry.compress_type == zipfile.ZIP_STORED:
        data = payload
    elif entry.compress_type == zipfile.ZIP_DEFLATED:
        decoder = zlib.decompressobj(-15)
        data = decoder.decompress(payload, MAX_PART + 1)
        if not decoder.eof or decoder.unused_data or decoder.unconsumed_tail:
            raise ValueError('Incomplete or oversized DEFLATE stream')
    else:
        raise ValueError('Unsupported ZIP compression')
    if len(data) != entry.file_size or binascii.crc32(data) != entry.CRC:
        raise ValueError('ZIP size or CRC mismatch')
    return data


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--count', type=int, default=150)
    parser.add_argument('--max-total-bytes', type=int, default=500_000_000)
    parser.add_argument('--plan-only', action='store_true')
    args = parser.parse_args()
    directory = OUT / 'corpus-followup/loc'
    directory.mkdir(parents=True, exist_ok=True)
    metadata_path = directory.parent / 'loc-metadata.csv'
    if not metadata_path.exists():
        with urllib.request.urlopen(METADATA_URL, timeout=60) as response:
            metadata = response.read(4 * 1024 * 1024 + 1)
        if len(metadata) > 4 * 1024 * 1024:
            raise ValueError('Metadata exceeds download budget')
        metadata_path.write_bytes(metadata)
    metadata = metadata_path.read_bytes()
    rows = list(csv.DictReader(io.StringIO(metadata.decode('utf-16')), delimiter='\t'))
    known = {r['sha256'] for name in ['manifest.json', 'expanded/manifest.json'] for r in json.loads((OUT / name).read_text())}
    reader = RangeReader()
    with zipfile.ZipFile(reader) as archive:
        infos = {entry.filename: entry for entry in archive.infolist()}
        manifest_name = next(name for name in infos if name.endswith('/manifest-sha256.txt') or name == 'manifest-sha256.txt')
        prefix = manifest_name[:-len('manifest-sha256.txt')]
        bag = {line.split(None, 1)[0]: prefix + line.split(None, 1)[1] for line in archive.read(manifest_name).decode().splitlines() if line.strip()}
    rng = random.Random(20261108)
    eligible = []
    for row in rows:
        sha = row['sha256'].strip()
        if sha in known or sha not in bag: continue
        entry = infos[bag[sha]]
        if entry.file_size > MAX_PART or entry.compress_size > MAX_PART: continue
        extension = Path(urllib.parse.urlsplit(row['original']).path).suffix.lower().lstrip('.')
        eligible.append(dict(row, sha256=sha, archive_path=bag[sha], bytes=entry.file_size, format=extension, split='validation' if int(sha[:8], 16) % 5 == 0 else 'tuning'))
    rng.shuffle(eligible)
    eligible.sort(key=lambda row: row['format'] not in {'pptx', 'pptm', 'pps', 'ppsx', 'pot', 'potx', 'potm'})
    chosen = []
    total = 0
    for row in eligible:
        if len(chosen) >= args.count: break
        if total + row['bytes'] > args.max_total_bytes: continue
        chosen.append(row)
        total += row['bytes']
    plan = dict(archive_url=URL, archive_etag=reader.etag, archive_bytes=reader.size, metadata_sha256=hashlib.sha256(metadata).hexdigest(), seed=20261108, selection='Seeded shuffle, rare filename extensions first; excludes existing hashes and members above 64 MiB; bounded total bytes.', eligible=len(eligible), selected=len(chosen), selected_bytes=total, extensions=dict(collections.Counter(row['format'] for row in chosen)), split=dict(collections.Counter(row['split'] for row in chosen)), rows=chosen)
    (directory / 'plan.json').write_text(json.dumps(plan, indent=2))
    print(json.dumps({k: v for k, v in plan.items() if k != 'rows'}), flush=True)
    if args.plan_only: return
    (directory / 'corpus').mkdir(exist_ok=True)

    def fetch(row):
        result = dict(row)
        sha = row['sha256']
        path = directory / 'corpus' / sha
        try:
            data = path.read_bytes() if path.exists() else content(reader, infos[row['archive_path']])
            if hashlib.sha256(data).hexdigest() != sha or hashlib.sha512(data).hexdigest() != row['sha512'].strip():
                raise ValueError('Published checksum mismatch')
            path.write_bytes(data)
            container = 'ole' if data.startswith(b'\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1') else 'zip' if data.startswith(b'PK') else 'other'
            result.update(id=sha[:16], path=str(path), source='library-of-congress', container=container, status='downloaded')
        except Exception as error:
            result.update(status='download_error', error=str(error))
        return result

    results = []
    with ThreadPoolExecutor(max_workers=4) as pool, (directory / 'downloads.jsonl').open('w', buffering=1) as stream:
        for i, result in enumerate(pool.map(fetch, chosen)):
            results.append(result)
            stream.write(json.dumps(result) + '\n')
            if i % 20 == 0: print(i + 1, len(chosen), flush=True)
    (directory / 'manifest.json').write_text(json.dumps(results, indent=2))
    summary = dict(selected=len(results), downloaded=sum(row['status'] == 'downloaded' for row in results), unique=len({row['sha256'] for row in results if row['status'] == 'downloaded'}), bytes=sum(row['bytes'] for row in results if row['status'] == 'downloaded'), containers=dict(collections.Counter(row.get('container', 'error') for row in results)), split=dict(collections.Counter(row['split'] for row in results)), transferred_bytes=reader.transferred, archive_etag=reader.etag, metadata_sha256=plan['metadata_sha256'])
    (directory / 'summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary))


if __name__ == '__main__':
    main()

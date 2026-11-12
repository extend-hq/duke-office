"""Score every parser's text against the LibreOffice reference.

    score.py <work dir>

Writes `<work dir>/scores.json` and prints summary tables.

Text is compared as the repository's Word benchmark does: the longest common
subsequence of normalized characters, giving precision, recall and F1. Here
normalization keeps only letters and digits (after NFC), so Markdown syntax,
table rules, bullets and white space do not count for or against a parser.
"""
import collections, json, os, statistics, sys, unicodedata

GROUPS = {'text': ('odt', 'ott', 'odm', 'fodt'), 'slides': ('odp', 'otp', 'fodp'), 'drawings': ('odg', 'otg', 'fodg')}
MIN_REFERENCE = 20  # characters; shorter references say nothing about text recovery


def normalize(text):
    return ''.join(ch for ch in unicodedata.normalize('NFC', text) if ch.isalnum())


def lcs_length(left, right):
    """Bit-parallel LCS length, as in scripts/parser-benchmark/fidelity_metrics.py."""
    if left == right:
        return len(left)
    if len(right) > len(left):
        left, right = right, left
    positions = collections.defaultdict(int)
    for index, character in enumerate(right):
        positions[character] |= 1 << index
    state = 0
    for character in left:
        overlap = positions[character] | state
        state = overlap & ~(overlap - ((state << 1) | 1))
    return state.bit_count()


def tracked_changes(path):
    """Whether a text document records tracked changes. LibreOffice's export
    shows deleted text, so such a file has no single correct reading."""
    try:
        if path.lower().endswith(('.fodt', '.fodp', '.fodg')):
            data = open(path, 'rb').read()
        else:
            import zipfile
            data = zipfile.ZipFile(path).read('content.xml')
    except Exception:
        return False
    return b'<text:changed-region' in data


def read(path):
    try:
        return open(path, encoding='utf-8', errors='replace').read()
    except OSError:
        return None


def main():
    work = os.path.abspath(sys.argv[1])
    references = {}
    excluded = []
    for family in sorted(os.listdir(os.path.join(work, 'refs'))):
        for name in os.listdir(os.path.join(work, 'refs', family)):
            key = f'{family}/{name[:-4]}'
            if tracked_changes(os.path.join(work, 'corpus', key)):
                excluded.append(key)
                continue
            references[key] = normalize(read(os.path.join(work, 'refs', family, name)) or '')
    print(f'{len(excluded)} documents with tracked changes are not scored')
    results = {}
    for parser in sorted(os.listdir(os.path.join(work, 'results'))):
        base = os.path.join(work, 'results', parser)
        if not os.path.exists(os.path.join(base, 'records.jsonl')):
            continue
        meta = json.load(open(os.path.join(base, 'meta.json')))
        # Timings are the best warm repeat over every recorded pass, which
        # discounts interference from other processes on the machine.
        best, cpu = {}, {}
        for name in os.listdir(base):
            if name.startswith('records') and name.endswith('.jsonl'):
                for line in open(os.path.join(base, name), encoding='utf-8'):
                    record = json.loads(line)
                    if record.get('ok') and record.get('min_ms') is not None:
                        best[record['file']] = min(best.get(record['file'], float('inf')), record['min_ms'])
                    if record.get('ok') and record.get('cpu_ms') is not None:
                        cpu[record['file']] = min(cpu.get(record['file'], float('inf')), record['cpu_ms'])
        files = {}
        for line in open(os.path.join(base, 'records.jsonl'), encoding='utf-8'):
            record = json.loads(line)
            key = record['file']
            entry = {'ok': bool(record.get('ok')), 'error': record.get('error', ''), 'best_ms': best.get(key),
                     'cpu_ms': cpu.get(key)}
            reference = references.get(key)
            if reference is not None and len(reference) >= MIN_REFERENCE:
                actual = normalize(read(os.path.join(base, 'texts', key + '.txt')) or '') if entry['ok'] else ''
                matched = lcs_length(reference, actual)
                entry.update(expected=len(reference), found=len(actual), matched=matched)
            files[key] = entry
        results[parser] = {'meta': meta, 'files': files}
    json.dump(results, open(os.path.join(work, 'scores.json'), 'w'))

    def extension(key):
        return key.rsplit('.', 1)[-1].lower()

    for group, extensions in GROUPS.items():
        for ext in extensions:
            parsers = [p for p in results if ext in results[p]['meta']['extensions']]
            keys = sorted({k for p in parsers for k in results[p]['files'] if extension(k) == ext})
            if not keys:
                continue
            scored = [k for k in keys if all('expected' in results[p]['files'].get(k, {}) for p in parsers)]
            common_ok = [k for k in keys if all(results[p]['files'].get(k, {}).get('ok') for p in parsers)]
            print(f'\n== .{ext}: {len(keys)} files, {len(scored)} with a usable reference, '
                  f'{len(common_ok)} parsed by every parser ==')
            print(f"{'parser':16} {'parsed':>9} {'recall':>7} {'precis.':>7} {'F1':>7} {'meanF1':>7} "
                  f"{'mean ms':>9} {'median ms':>9} {'cpu mean':>9} {'cpu med':>8}  timing")
            rows = []
            for parser in parsers:
                files = results[parser]['files']
                ok = sum(1 for k in keys if files.get(k, {}).get('ok'))
                expected = sum(files[k]['expected'] for k in scored)
                found = sum(files[k]['found'] for k in scored)
                matched = sum(files[k]['matched'] for k in scored)
                per_file = [2 * files[k]['matched'] / (files[k]['expected'] + files[k]['found'])
                            if files[k]['expected'] + files[k]['found'] else 1.0 for k in scored]
                times = [files[k]['best_ms'] for k in common_ok if files[k].get('best_ms') is not None]
                cpu_times = [files[k]['cpu_ms'] for k in common_ok if files[k].get('cpu_ms') is not None]
                rows.append((2 * matched / (expected + found) if expected + found else 0, parser, ok,
                             matched / expected if expected else 0, matched / found if found else 0,
                             statistics.fmean(per_file) if per_file else 0,
                             statistics.fmean(times) if times else float('nan'),
                             statistics.median(times) if times else float('nan'),
                             statistics.fmean(cpu_times) if len(cpu_times) == len(common_ok) and cpu_times else float('nan'),
                             statistics.median(cpu_times) if len(cpu_times) == len(common_ok) and cpu_times else float('nan'),
                             results[parser]['meta']['timing']))
            for f1, parser, ok, recall, precision, mean_f1, mean_ms, median_ms, cpu_mean, cpu_median, timing in sorted(rows, reverse=True):
                print(f'{parser:16} {ok:>4}/{len(keys):<4} {recall:7.4f} {precision:7.4f} {f1:7.4f} {mean_f1:7.4f} '
                      f'{mean_ms:9.3f} {median_ms:9.3f} {cpu_mean:9.3f} {cpu_median:8.3f}  {timing}')


if __name__ == '__main__':
    main()

"""Run every parser over the corpus and store its text and timings.

    run.py <work dir> [parser ...]

The work directory holds `corpus/<family>/<file>`, the built Rust runners in
`target/release`, and the general-purpose tools in `tools/` (see README.md).
Results go to `<work dir>/results/<parser>/`.
"""
import json, os, queue, subprocess, sys, threading, time

HERE = os.path.dirname(os.path.abspath(__file__))
TEXT = ('odt', 'ott', 'odm', 'fodt')
SLIDES = ('odp', 'otp', 'fodp')
DRAWINGS = ('odg', 'otg', 'fodg')
# Extensions each parser handles by design, from its documentation and source.
SUPPORT = {
    'duke-office': TEXT + SLIDES + DRAWINGS,
    'anydoc': ('odt', 'ott', 'odp'),
    'docx-to-md': ('odt', 'ott'),
    'pptx-to-md': ('odp',),
    'litchi': ('odt', 'ott', 'odp'),
    'odox-core': ('odt', 'odp'),
    'quarb-text-odt': ('odt', 'fodt'),
    'markkit': ('odt', 'ott'),
    'omniparse': ('odt', 'odp'),
    'kreuzberg': ('odt',),
    'docling': ('odt', 'ott', 'fodt', 'odp', 'fodp'),
    'ebook-rs': ('odt',),
    'dotext': ('odt', 'odp'),
    'transmutation': ('odt',),
    'tika': TEXT + SLIDES + DRAWINGS,
    'officeparser': ('odt', 'odp'),
    'odfpy': ('odt', 'ott', 'odm', 'odp', 'otp', 'odg', 'otg'),
    'odfdo': ('odt', 'ott', 'odm', 'odp', 'otp', 'odg', 'otg'),
    'pandoc': ('odt',),
    'textutil': ('odt',),
}
CLI = ('pandoc', 'textutil')
STALL_SECONDS = 150


def corpus_files(work, extensions):
    files = []
    for family in sorted(os.listdir(os.path.join(work, 'corpus'))):
        directory = os.path.join(work, 'corpus', family)
        for name in sorted(os.listdir(directory)):
            if name.rsplit('.', 1)[-1].lower() in extensions:
                files.append(os.path.join(directory, name))
    return files


def command(parser, work, list_file, texts):
    tools = os.path.join(work, 'tools')
    if parser == 'tika':
        jar = os.path.join(tools, 'tika', 'tika-app-3.3.3.jar')
        classes = os.path.join(tools, 'tika', 'classes')
        if not os.path.exists(os.path.join(classes, 'TikaRunner.class')):
            os.makedirs(classes, exist_ok=True)
            subprocess.run(['javac', '-cp', jar, '-d', classes, os.path.join(HERE, 'general', 'TikaRunner.java')], check=True)
        return ['java', '-cp', f'{jar}:{classes}', 'TikaRunner', list_file, texts], None
    if parser == 'officeparser':
        return ['node', os.path.join(HERE, 'general', 'officeparser_runner.cjs'), list_file, texts], os.path.join(tools, 'node')
    if parser in ('odfpy', 'odfdo'):
        python = os.path.join(tools, 'py', '.venv', 'bin', 'python')
        return [python, os.path.join(HERE, 'general', 'python_runner.py'), parser, list_file, texts], None
    return [os.path.join(work, 'target', 'release', f'peer-{parser}'), list_file, texts], None


def run_in_process(parser, work, files, out):
    """Runs a list-driven runner, resuming after the file it crashes or stalls on."""
    texts = os.path.join(out, 'texts')
    records = []
    remaining = list(files)
    while remaining:
        list_file = os.path.join(out, 'list.txt')
        open(list_file, 'w', encoding='utf-8').write('\n'.join(remaining) + '\n')
        argv, cwd = command(parser, work, list_file, texts)
        process = subprocess.Popen(argv, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        lines = queue.Queue()
        threading.Thread(target=lambda: [lines.put(line) for line in process.stdout] + [lines.put(None)], daemon=True).start()
        done = 0
        reason = None
        while True:
            try:
                line = lines.get(timeout=STALL_SECONDS)
            except queue.Empty:
                process.kill()
                reason = 'timeout'
                break
            if line is None:
                break
            try:
                records.append(json.loads(line))
                done += 1
            except ValueError:
                pass  # a parser printing to stdout
        process.wait()
        if done >= len(remaining):
            break
        culprit = remaining[done]
        key = os.path.basename(os.path.dirname(culprit)) + '/' + os.path.basename(culprit)
        records.append({'file': key, 'ok': False, 'error': reason or f'crashed (exit {process.returncode})'})
        remaining = remaining[done + 1:]
    return records


def run_cli(parser, work, files, out):
    """Times a command-line tool. Each sample includes starting the process."""
    texts = os.path.join(out, 'texts')
    pandoc = os.path.join(work, 'tools', 'pandoc', 'pandoc-3.14-arm64', 'bin', 'pandoc')
    records = []
    for path in files:
        family, name = os.path.basename(os.path.dirname(path)), os.path.basename(path)
        argv = [pandoc, '-f', 'odt', '-t', 'plain', '--wrap=none', path] if parser == 'pandoc' else \
               ['textutil', '-convert', 'txt', '-stdout', path]
        samples, text, error = [], None, ''
        for _ in range(3):
            start = time.perf_counter()
            try:
                result = subprocess.run(argv, capture_output=True, timeout=120)
            except subprocess.TimeoutExpired:
                error = 'timeout'
                break
            samples.append((time.perf_counter() - start) * 1000)
            if result.returncode != 0:
                error = result.stderr.decode('utf-8', 'replace')[-300:] or f'exit {result.returncode}'
                break
            text = result.stdout.decode('utf-8', 'replace')
        ok = text is not None and not error
        if ok:
            os.makedirs(os.path.join(texts, family), exist_ok=True)
            open(os.path.join(texts, family, name + '.txt'), 'w', encoding='utf-8').write(text)
        records.append({'file': f'{family}/{name}', 'ok': ok, 'error': error, 'bytes': os.path.getsize(path),
                        'chars': len(text or ''), 'first_ms': samples[0] if samples else 0,
                        'mean_ms': sum(samples) / len(samples) if samples else 0,
                        'min_ms': min(samples) if samples else 0, 'reps': len(samples)})
    return records


def main():
    work = os.path.abspath(sys.argv[1])
    parsers = sys.argv[2:] or list(SUPPORT)
    for parser in parsers:
        files = corpus_files(work, SUPPORT[parser])
        out = os.path.join(work, 'results', parser)
        os.makedirs(out, exist_ok=True)
        if parser not in CLI and parser not in ('tika', 'officeparser', 'odfpy', 'odfdo') \
                and not os.path.exists(os.path.join(work, 'target', 'release', f'peer-{parser}')):
            print(f'{parser}: runner not built, skipped', flush=True)
            continue
        start = time.time()
        records = (run_cli if parser in CLI else run_in_process)(parser, work, files, out)
        with open(os.path.join(out, 'records.jsonl'), 'w', encoding='utf-8') as handle:
            for record in records:
                handle.write(json.dumps(record) + '\n')
        json.dump({'parser': parser, 'timing': 'command line, includes process start' if parser in CLI else 'warm, in process',
                   'extensions': SUPPORT[parser]}, open(os.path.join(out, 'meta.json'), 'w'))
        ok = sum(1 for record in records if record.get('ok'))
        print(f'{parser}: {ok}/{len(files)} parsed in {time.time() - start:.0f}s', flush=True)


if __name__ == '__main__':
    main()

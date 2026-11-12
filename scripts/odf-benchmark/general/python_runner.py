"""Warm in-process timings for the Python OpenDocument libraries, in the same
record shape as the Rust runners.

Usage: python_runner.py <odfpy|odfdo> <list file> <output directory>
"""
import io, json, os, sys, time

def odfpy_text(data):
    from odf.opendocument import load
    from odf import teletype
    from odf.namespaces import TEXTNS
    document = load(io.BytesIO(data))
    lines = []
    def walk(node):
        for child in node.childNodes:
            if child.nodeType != child.ELEMENT_NODE:
                continue
            if child.qname in ((TEXTNS, 'p'), (TEXTNS, 'h')):
                lines.append(teletype.extractText(child))
            else:
                walk(child)
    walk(document.body)
    return '\n'.join(lines)

def odfdo_text(data):
    from odfdo import Document
    return Document(io.BytesIO(data)).get_formatted_text()

def main():
    engine, list_file, output = sys.argv[1:4]
    extract = {'odfpy': odfpy_text, 'odfdo': odfdo_text}[engine]
    for line in open(list_file, encoding='utf-8').read().split('\n'):
        if not line:
            continue
        family, name = os.path.basename(os.path.dirname(line)), os.path.basename(line)
        data = open(line, 'rb').read()
        text, error = None, ''
        first_start = time.perf_counter()
        try:
            text = extract(data)
        except BaseException as problem:  # the libraries raise many exception types
            error = repr(problem)[:300]
        first = (time.perf_counter() - first_start) * 1000
        samples = []
        if text is not None and first < 5000:
            begin = time.perf_counter()
            while len(samples) < 3 or (time.perf_counter() - begin < 0.25 and len(samples) < 200):
                start = time.perf_counter()
                try:
                    extract(data)
                except BaseException:
                    pass
                samples.append((time.perf_counter() - start) * 1000)
        if not samples:
            samples = [first]
        if text is not None:
            os.makedirs(os.path.join(output, family), exist_ok=True)
            open(os.path.join(output, family, name + '.txt'), 'w', encoding='utf-8').write(text)
        print(json.dumps({'file': f'{family}/{name}', 'ok': text is not None, 'error': error, 'bytes': len(data),
                          'chars': len(text or ''), 'first_ms': first, 'mean_ms': sum(samples) / len(samples),
                          'min_ms': min(samples), 'reps': len(samples)}), flush=True)

if __name__ == '__main__':
    main()

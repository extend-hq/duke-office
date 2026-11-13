import argparse
import hashlib
import io
import json
import zipfile
from pathlib import Path

from artwork_math_gate import artwork_hashes
from feedback import OUT, invoke as compact
from fields_probes import package
from fields_review import verify_sources
from object_gate import invoke


def make_package(path, case, strict):
    package(path, strict=strict)
    with zipfile.ZipFile(io.BytesIO(path.read_bytes())) as archive:
        parts = {name: archive.read(name) for name in archive.namelist()}
    drawing = 'http://purl.oclc.org/ooxml/drawingml/main' if strict else 'http://schemas.openxmlformats.org/drawingml/2006/main'
    field = lambda ident, declarations='': '<a:fld ' + declarations + ' id="' + ident + '" type="custom"><a:t>cached</a:t></a:fld>'
    body = field('before')
    if case == 'repeated':
        body += ''.join(field('field-' + str(i)) for i in range(120))
    elif case == 'shadowed-prefix':
        body += '<e:item xmlns:a="urn:foreign">' + field('foreign') + '</e:item>'
        body += field('restored')
    elif case == 'default-scope':
        body += '<e:item xmlns="urn:default">' + field('inside-default') + '<e:item xmlns="">' + field('inside-empty-default') + '</e:item></e:item>'
    elif case == 'escaped-uri':
        body += field('uri', 'xmlns:z="urn:scope?x=&quot;&amp;y=1"')
    elif case == 'empty-events':
        body += '<a:fld xmlns:z="urn:empty" id="empty"/><e:item xmlns:a="urn:foreign"/>' + field('after-empty')
    elif case == 'nested-scope':
        body += '<e:item xmlns:z="urn:outer"><e:item xmlns:z="urn:inner">' + field('inner') + '</e:item>' + field('outer') + '</e:item>'
    elif case == 'math':
        body += '<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math" xmlns:z="urn:math"><m:r><m:t>x</m:t></m:r></m:oMath>'
    elif case == 'animation':
        body += field('animation-neighbor', 'xmlns:z="urn:field"')
    body += field('after')
    source = parts['ppt/slides/slide1.xml'].decode()
    start, end = source.index('<p:txBody>'), source.index('</p:txBody>') + len('</p:txBody>')
    source = source[:start] + '<p:txBody><a:bodyPr/><a:lstStyle/><a:p>' + body + '</a:p></p:txBody>' + source[end:]
    if case == 'animation':
        source = source.replace('</p:sld>', '<p:transition xmlns:z="urn:transition"><p:fade/></p:transition><p:timing xmlns:z="urn:timing"><p:tnLst><p:par><p:cTn id="1" dur="indefinite"/></p:par></p:tnLst></p:timing></p:sld>')
    parts['ppt/slides/slide1.xml'] = source.encode()
    with zipfile.ZipFile(path, 'w') as archive:
        for name, data in parts.items():
            info = zipfile.ZipInfo(name, (2000, 1, 1, 0, 0, 0)); info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data)


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--label', required=True); parser.add_argument('--baseline', default='image-fallback1'); args = parser.parse_args()
    run = OUT / 'experiments' / args.label; target = run / 'namespace-probes'; target.mkdir()
    manifest, records, receipts = [], [], []
    baseline = OUT / 'experiments' / args.baseline / 'candidate'
    for strict in [False, True]:
        for case in ['repeated', 'shadowed-prefix', 'default-scope', 'escaped-uri', 'empty-events', 'nested-scope', 'math', 'animation']:
            name = ('strict-' if strict else 'transitional-') + case
            path = (target / (name + '.pptx')).resolve(); make_package(path, case, strict)
            row = dict(id=name, path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest()); manifest.append(row)
            before = compact(baseline, 'duke-pptx', row, target / (name + '.baseline.json.gz'), 0)
            record, document = invoke(run / 'candidate', row, target / (name + '.full.json.gz'))
            assert before['status'] == record['status'] == 'ok', (name, before, record)
            assert record['full_sha256'] == before['full_sha256'], name
            counts = dict(verify_sources(path, document))
            result = compact(run / 'candidate', 'duke-pptx', row, target / (name + '.compact.json.gz'), 0)
            result.update(id=name, artwork_hashes=artwork_hashes(document)); records.append(result)
            receipts.append(dict(id=name, passed=True, fields=counts, full_sha256=record['full_sha256']))
            print(name, counts['fields'], flush=True)
    (target / 'manifest.json').write_text(json.dumps(manifest, indent=2))
    destination = target / 'records.jsonl'; destination.write_text(''.join(json.dumps(r) + '\n' for r in records))
    summary = dict(total=len(receipts), passed=len(receipts), receipts=receipts, candidate_sha256=hashlib.sha256((run / 'candidate').read_bytes()).hexdigest(), baseline_sha256=hashlib.sha256(baseline.read_bytes()).hexdigest(), records_sha256=hashlib.sha256(destination.read_bytes()).hexdigest(), harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), scope='Exact full canonical output versus the frozen baseline with repeated captures, namespace prefix shadowing, default namespace declarations and undeclarations, escaped namespace URIs, empty events, nested scopes, equations, transitions and timing. Every emitted field is independently checked against its qualified source subtree.')
    (run / 'namespace-probes-summary.json').write_text(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()

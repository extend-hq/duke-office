import argparse
import hashlib
import io
import json
import zipfile

from artwork_math_gate import artwork_hashes
from feedback import OUT, invoke as compact
from fields_probes import package
from fields_review import verify_sources, visit
from object_gate import invoke


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--label', required=True); args = parser.parse_args()
    run = OUT / 'experiments' / args.label; target = run / 'borrow-probes'; target.mkdir()
    cases = [('namespace-attribute', '<a:fld xmlns:id="urn:extension" id="number" type="slidenum"><a:t>7</a:t></a:fld>'), ('reference-attribute', '<a:fld id="number" type="A&#13;B&#9;C"><a:t>7</a:t></a:fld>'), ('shadowed-text', '<a:fld id="number" type="slidenum"><a:t xmlns:a="urn:extension">ignore</a:t><a:t>7</a:t></a:fld>'), ('foreign-run-attribute', '<a:fld id="number" type="slidenum"><a:rPr e:sz="9900" sz="1700"/><a:t>7</a:t></a:fld>')]
    rows, records, receipts = [], [], []
    for name, field in cases:
        path = (target / (name + '.pptx')).resolve(); package(path)
        with zipfile.ZipFile(io.BytesIO(path.read_bytes())) as archive:
            parts = {n: archive.read(n) for n in archive.namelist()}
        parts['ppt/slides/slide1.xml'] = parts['ppt/slides/slide1.xml'].replace(b'<a:fld id="number" type="slidenum"><a:t>7</a:t></a:fld>', field.encode())
        with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
            for part, value in parts.items():
                info = zipfile.ZipInfo(part, (2000, 1, 1, 0, 0, 0)); info.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(info, value)
        row = dict(id=name, path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest()); rows.append(row)
        old, before = invoke(OUT / 'experiments/fields2/candidate', row, target / (name + '.before.json.gz'))
        record, document = invoke(run / 'candidate', row, target / (name + '.full.json.gz'))
        assert old['status'] == record['status'] == 'ok' and before == document
        counts = dict(verify_sources(path, document))
        number = next(r for _, r in visit(document) if r['field']['id'] == 'number')
        assert number['text'] == '7'
        if name == 'reference-attribute':
            assert number['field']['type'] == 'A\rB\tC'
        if name == 'foreign-run-attribute':
            assert number['fontSizePt'] == 17
        result = compact(run / 'candidate', 'duke-slides', row, target / (name + '.compact.json.gz'), 0); result.update(id=name, artwork_hashes=artwork_hashes(document)); records.append(result)
        receipts.append(dict(id=name, input_sha256=row['sha256'], counts=counts, passed=True)); print(name, 'passed', flush=True)
    (target / 'manifest.json').write_text(json.dumps(rows, indent=2)); destination = target / 'records.jsonl'; destination.write_text(''.join(json.dumps(r) + '\n' for r in records))
    summary = dict(total=len(receipts), passed=len(receipts), receipts=receipts, records_sha256=hashlib.sha256(destination.read_bytes()).hexdigest(), candidate_sha256=hashlib.sha256((run / 'candidate').read_bytes()).hexdigest(), baseline_sha256=hashlib.sha256((OUT / 'experiments/fields2/candidate').read_bytes()).hexdigest(), harness_sha256=hashlib.sha256(__import__('pathlib').Path(__file__).read_bytes()).hexdigest(), scope='Full-model equivalence plus namespace-aware source checks at borrowed-tree eligibility boundaries.')
    (run / 'fields-borrow-summary.json').write_text(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()

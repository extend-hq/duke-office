import argparse
import hashlib
import json
from pathlib import Path
import zipfile

from feedback import OUT, invoke


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    destination = run / 'attribute-budget-probes'
    destination.mkdir(exist_ok=False)
    candidate, baseline = run / 'candidate', OUT / 'experiments/xml-bom1/candidate'
    identity = json.loads((run / 'identity.json').read_text())
    assert hashlib.sha256(candidate.read_bytes()).hexdigest() == identity['binary_sha256']
    proof = json.loads((OUT / 'experiments/xml-bom1/bom-source-reviewed-summary.json').read_text())
    assert hashlib.sha256(baseline.read_bytes()).hexdigest() == proof['candidate_sha256']
    namespace = b'http://schemas.openxmlformats.org/presentationml/2006/main'
    declaration = b' xmlns:p="' + namespace + b'"'
    remaining_bytes = 32 * 1024 * 1024 - len(b'xmlns:p') - len(namespace) - 1
    cases = [
        ('count_at_limit', b' a="0"' * 999_999, 'ok'),
        ('count_over_limit', b' a="0"' * 1_000_000, 'XML attribute count'),
        ('entity_error', b' a="&unknown;"', 'corrupt'),
        ('count_before_entity', b' a="&unknown;"' + b' a="0"' * 999_999, 'XML attribute count'),
        ('bytes_at_limit', b' a="' + b'x' * remaining_bytes + b'"', 'ok'),
        ('bytes_over_limit', b' a="' + b'x' * (remaining_bytes + 1) + b'"', 'XML attribute bytes'),
        ('bytes_before_entity', b' a="&unknown;" b="' + b'x' * remaining_bytes + b'"', 'XML attribute bytes'),
    ]
    records = []
    fields = lambda record: (record['status'], record.get('error'), record.get('full_sha256'))
    for name, attributes, expected in cases:
        xml = b'<p:presentation' + declaration + attributes + b'/>'
        path = destination / (name + '.pptx')
        with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=1) as archive:
            for part, data in [('[Content_Types].xml', b'<Types/>'), ('ppt/presentation.xml', xml)]:
                info = zipfile.ZipInfo(part, (2000, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(info, data, compresslevel=1)
        row = dict(path=str(path))
        old = invoke(baseline, 'duke-slides', row, destination / 'baseline.json.gz', 0)
        new = invoke(candidate, 'duke-slides', row, destination / 'candidate.json.gz', 0)
        assert fields(old) == fields(new), name
        assert old['status'] == 'ok' if expected == 'ok' else expected in old.get('error', ''), name
        records.append(dict(name=name, input_sha256=hashlib.sha256(path.read_bytes()).hexdigest(), expected=expected, baseline=old, candidate=new))
        print(name, new['status'], flush=True)
    result = dict(total=len(records), passed=len(records), records=records,
                  candidate_sha256=identity['binary_sha256'], baseline_sha256=proof['candidate_sha256'],
                  harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  scope='Default attribute-count and raw attribute-byte boundaries, including resource-limit precedence over entity-decoding failures. Duplicate attributes are intentional malformed-input probes.')
    (run / 'attribute-budget-summary.json').write_text(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()

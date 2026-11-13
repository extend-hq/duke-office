import argparse
import hashlib
import io
import json
import zipfile

from comments_probes import package
from comments_review import PRESENTATION
from feedback import OUT, invoke


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--label', required=True); args = parser.parse_args()
    run = OUT / 'experiments' / args.label; target = run / 'comment-budget-probes'; target.mkdir()
    records = []
    for depth, expected in [(255, 'ok'), (256, 'ok'), (257, 'error')]:
        path = target / f'depth-{depth}.pptx'; package(path)
        with zipfile.ZipFile(io.BytesIO(path.read_bytes())) as archive:
            parts = {info.filename: archive.read(info) for info in archive.infolist()}
        nested = depth - 3
        parts['review/thread.xml'] = (f'<p:cmLst xmlns:p="{PRESENTATION[0]}" xmlns:e="urn:extension"><p:cm authorId="1" idx="1"><p:text>Budget boundary</p:text><p:extLst>' + '<e:item>' * nested + '</e:item>' * nested + '</p:extLst></p:cm></p:cmLst>').encode()
        with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
            for name, value in parts.items():
                info = zipfile.ZipInfo(name, (2000, 1, 1, 0, 0, 0)); info.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(info, value)
        result = invoke(run / 'candidate', 'duke-pptx', dict(path=str(path)), target / f'depth-{depth}.json.gz', 0)
        passed = result['status'] == expected and (expected == 'ok' or result.get('error') == 'resource limit exceeded: XML nesting depth')
        records.append(dict(depth=depth, expected=expected, passed=passed, input_sha256=hashlib.sha256(path.read_bytes()).hexdigest(), result=result))
    summary = dict(total=len(records), passed=sum(r['passed'] for r in records), records=records, candidate_sha256=hashlib.sha256((run / 'candidate').read_bytes()).hexdigest())
    (run / 'comment-budget-summary.json').write_text(json.dumps(summary, indent=2)); print(json.dumps(summary))
    assert summary['passed'] == summary['total']


if __name__ == '__main__':
    main()

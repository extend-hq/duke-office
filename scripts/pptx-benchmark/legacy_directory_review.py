import argparse
import copy
import hashlib
import io
import json
from pathlib import Path

import olefile

from artwork_math_gate import artwork_hashes
from feedback import OUT
from object_gate import invoke


def digest(data):
    return hashlib.sha256(data).hexdigest()


def repair_unused(data):
    with olefile.OleFileIO(io.BytesIO(data)) as ole:
        offsets, seen = [], set()
        sector = ole.first_dir_sector
        while sector != olefile.ENDOFCHAIN:
            assert sector not in seen
            seen.add(sector)
            start = (sector + 1) * ole.sectorsize
            assert start + ole.sectorsize <= len(data)
            offsets.extend(range(start, start + ole.sectorsize, 128))
            sector = int(ole.fat[sector])
        assert len(offsets) == len(ole.direntries)
        repaired, changed = bytearray(data), []
        for index, start in enumerate(offsets):
            if data[start:start + 128] == bytes([255]) * 128:
                assert ole.direntries[index] is None
                repaired[start:start + 128] = bytes(128)
                repaired[start + 68:start + 80] = bytes([255]) * 12
                changed.append(dict(index=index, offset=start))
        assert changed
        streams = {tuple(path): ole.openstream(path).read() for path in ole.listdir()}
        with olefile.OleFileIO(io.BytesIO(repaired)) as after:
            assert set(map(tuple, after.listdir())) == set(streams)
            for path, payload in streams.items():
                assert after.openstream(list(path)).read() == payload
    return bytes(repaired), changed, [dict(path=list(path), sha256=digest(payload), bytes=len(payload)) for path, payload in streams.items()]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--manifest', type=Path, required=True)
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    raw_path = run / 'parity.jsonl'
    proof = json.loads((run / 'parity-summary.json').read_text())
    assert digest(raw_path.read_bytes()) == proof['records_sha256']
    assert digest(args.manifest.read_bytes()) == proof['manifest_sha256']
    binary, baseline = run / 'candidate', Path(proof['baseline_path'])
    assert digest(binary.read_bytes()) == proof['candidate_sha256']
    assert digest(baseline.read_bytes()) == proof['baseline_sha256']
    manifest = {r['id']: r for r in json.loads(args.manifest.read_text())}
    records = [json.loads(line) for line in raw_path.read_text().splitlines()]
    assert len(records) == len(manifest) == proof['total']
    destination = run / 'directory-reviewed.jsonl'
    assert not destination.exists(), 'Preserve completed review receipts.'
    reviews = []
    for record in records:
        record['passed'] = record['equal']
        if record['equal']:
            continue
        assert record['status'] == 'ok'
        row = manifest[record['id']]
        data = Path(row['path']).read_bytes()
        assert digest(data) == row['sha256']
        original, original_model = invoke(baseline, row, run / (row['id'] + '.before.json.gz'))
        assert original['status'] == 'error' and original_model is None
        repaired, entries, streams = repair_unused(data)
        repaired_path = run / (row['id'] + '.canonical.ppt')
        repaired_path.write_bytes(repaired)
        reference, expected = invoke(baseline, dict(row, path=str(repaired_path)), run / (row['id'] + '.reference.json.gz'))
        actual_record, actual = invoke(binary, row, run / (row['id'] + '.after.json.gz'))
        assert reference['status'] == actual_record['status'] == 'ok'
        assert actual_record['full_sha256'] == record['full_sha256']
        warnings = [w for w in actual['warnings'] if w['code'] == 'recovered-unused-directory']
        assert len(warnings) == 1
        assert warnings[0]['message'] == f'Recovered {len(entries)} unreachable, uninitialized OLE directory entries'
        normalized = copy.deepcopy(actual)
        normalized['warnings'] = [w for w in actual['warnings'] if w['code'] != 'recovered-unused-directory']
        assert normalized == expected
        record['passed'] = True
        record['artwork_hashes'] = artwork_hashes(actual)
        reviews.append(dict(id=row['id'], input_sha256=row['sha256'], repaired_sha256=digest(repaired), entries=entries, unchanged_streams=streams, reference_full_sha256=reference['full_sha256'], candidate_full_sha256=actual_record['full_sha256']))
    destination.write_text(''.join(json.dumps(r) + '\n' for r in records))
    summary = dict(total=len(records), passed=sum(r['passed'] for r in records), exact=sum(r['equal'] for r in records), recovered=len(reviews), reviews=reviews, candidate_sha256=proof['candidate_sha256'], baseline_sha256=proof['baseline_sha256'], records_sha256=digest(destination.read_bytes()), raw_records_sha256=proof['records_sha256'], manifest_sha256=proof['manifest_sha256'], harness_sha256=digest(Path(__file__).read_bytes()), scope='Exact full outputs/errors except newly recovered legacy containers. Independent olefile traversal identifies unreachable all-FF directory slots and verifies every live stream byte unchanged. The recovered full model must equal the frozen parser on a canonicalized container, except for one explicit recovery warning. Content accuracy is evaluated separately against POI.')
    (run / 'directory-reviewed-summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps({k:v for k,v in summary.items() if k != 'reviews'}))


if __name__ == '__main__':
    main()

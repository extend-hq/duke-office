import argparse
import copy
import hashlib
import json
from pathlib import Path
import struct

from feedback import OUT
from object_gate import invoke


FREE, END, FAT = 0xffffffff, 0xfffffffe, 0xfffffffd


def put(data, offset, value):
    struct.pack_into('<I', data, offset, value)


def directory_entry(name, kind, child=FREE, start=END, size=0):
    result = bytearray(128)
    encoded = (name + '\0').encode('utf-16-le') if name else b''
    result[:len(encoded)] = encoded
    struct.pack_into('<HBBIII', result, 64, len(encoded), kind, 1 if kind else 0, FREE, FREE, child)
    struct.pack_into('<IQ', result, 116, start, size)
    return result


def fixture(version):
    sector_size = 512 if version == 3 else 4096
    stream_sectors = 4096 // sector_size
    data = bytearray((3 + stream_sectors) * sector_size)
    data[:8] = bytes.fromhex('d0cf11e0a1b11ae1')
    struct.pack_into('<HHHHH', data, 24, 0x3e, version, 0xfffe, 9 if version == 3 else 12, 6)
    for offset, value in [(40, 0 if version == 3 else 1), (44, 1), (48, 1), (56, 4096), (60, END), (68, END)]:
        put(data, offset, value)
    for index in range(109):
        put(data, 76 + index * 4, 0 if index == 0 else FREE)
    for index in range(sector_size // 4):
        value = FAT if index == 0 else END if index in [1, stream_sectors + 1] else index + 1 if 2 <= index <= stream_sectors else FREE
        put(data, sector_size + index * 4, value)
    directory = sector_size * 2
    for index in range(sector_size // 128):
        data[directory + index * 128:directory + (index + 1) * 128] = directory_entry('', 0, start=0)
    data[directory:directory + 128] = directory_entry('Root Entry', 5, child=1)
    data[directory + 128:directory + 256] = directory_entry('PowerPoint Document', 2, start=2, size=4096)
    text = 'Recovery boundary'.encode('utf-16-le')
    atom = struct.pack('<HHI', 0, 4000, len(text)) + text
    slide = struct.pack('<HHI', 15, 1006, len(atom)) + atom
    data[sector_size * 3:sector_size * 3 + len(slide)] = slide
    return data, sector_size


def without_recovery(document):
    result = copy.deepcopy(document)
    result['warnings'] = [w for w in result['warnings'] if w['code'] != 'recovered-unused-directory']
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--baseline', default='xml-attributes2')
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    target = run / 'directory-probes'
    target.mkdir()
    binary, baseline = [OUT / 'experiments' / label / 'candidate' for label in [args.label, args.baseline]]
    records = []
    for version in [3, 4]:
        canonical, sector = fixture(version)
        directory = sector * 2
        unused = directory + 256
        damaged = bytearray(canonical)
        damaged[unused:unused + 128] = bytes([255]) * 128
        cases = [('canonical', canonical, 'exact'), ('unused_ff', damaged, 'recovery')]
        changes = {
            'reachable_ff': [(directory + 76, 2)],
            'directory_cycle': [(directory + 128 + 72, 1)],
            'directory_out_of_range': [(directory + 76, 99999)],
            'directory_sector_cycle': [(sector + 4, 1)],
            'stream_directory_alias': [(directory + 128 + 116, 1)],
            'root_directory_alias': [(directory + 116, 1), (directory + 120, 4096)],
            'mini_fat_directory_alias': [(60, 1), (64, 1)],
            'invalid_fat_marker': [(sector, END)],
            'root_sibling': [(directory + 68, 2)],
            'extended_difat': [(68, 0), (72, 1)],
        }
        for name, changeset in changes.items():
            data = bytearray(damaged)
            for offset, value in changeset:
                put(data, offset, value)
            cases.append((name, data, 'error'))
        partial = bytearray(damaged); partial[unused] = 254
        cases.append(('partial_ff', partial, 'error'))
        invalid_live = bytearray(damaged); struct.pack_into('<H', invalid_live, directory + 128 + 64, 65535)
        cases.append(('invalid_live_name', invalid_live, 'error'))
        cases.append(('truncated_stream', damaged[:-1], 'error'))
        live_ff = bytearray(canonical); live_ff[-128:] = bytes([255]) * 128
        cases.append(('stream_ff', live_ff, 'exact'))
        expected = None
        for name, data, kind in cases:
            path = target / f'v{version}-{name}.ppt'; path.write_bytes(data)
            row = dict(path=str(path))
            before, before_model = invoke(baseline, row, target / f'v{version}-{name}.before.json.gz')
            after, after_model = invoke(binary, row, target / f'v{version}-{name}.after.json.gz')
            if name == 'canonical':
                assert before['status'] == after['status'] == 'ok'
                assert len(before_model['slides']) == 1
                expected = before_model
            if kind == 'exact':
                passed = before['status'] == after['status'] == 'ok' and before['full_sha256'] == after['full_sha256']
            elif kind == 'recovery':
                passed = before['status'] == 'error' and after['status'] == 'ok' and without_recovery(after_model) == expected
                passed = passed and len([w for w in after_model['warnings'] if w['code'] == 'recovered-unused-directory']) == 1
            else:
                passed = before['status'] == after['status'] == 'error' and before['error'] == after['error']
            records.append(dict(version=version, name=name, expected=kind, passed=passed, input_sha256=hashlib.sha256(data).hexdigest(), baseline=before, candidate=after))
            print(version, name, passed, flush=True)
    summary = dict(total=len(records), passed=sum(r['passed'] for r in records), candidate_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), baseline_sha256=hashlib.sha256(baseline.read_bytes()).hexdigest(), harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), records=records)
    (run / 'directory-probes-summary.json').write_text(json.dumps(summary, indent=2))
    assert summary['passed'] == summary['total']


if __name__ == '__main__':
    main()

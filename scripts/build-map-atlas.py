"""Build the optional offline map boundary asset from pinned TopoJSON data."""
import argparse
import hashlib
import json
from pathlib import Path
import unicodedata
import zlib


def key(value):
    return ''.join(c for c in unicodedata.normalize('NFD', value.lower().replace('&', 'and')) if c.isascii() and c.isalnum())


COUNTRY_GROUPS = [
    ['United States of America', 'United States', 'US', 'USA', 'United States America'],
    ['United Kingdom', 'UK'], ['United Arab Emirates', 'UAE'],
    ['South Korea', 'Korea South'], ['North Korea', 'Korea North'],
    ['Russia', 'Russian Federation'], ['Vietnam', 'Viet Nam'],
    ['Czechia', 'Czech Republic'], ["Côte d'Ivoire", 'Ivory Coast', 'Cote Divoire'],
]
STATE_CODES = 'AL AK AZ AR CA CO CT DE DC FL GA HI ID IL IN IA KS KY LA ME MD MA MI MN MS MO MT NE NV NH NJ NM NY NC ND OH OK OR PA RI SC SD TN TX UT VT VA WA WV WI WY'.split()
STATE_IDS = '01 02 04 05 06 08 09 10 11 12 13 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 44 45 46 47 48 49 50 51 53 54 55 56'.split()


def unsigned(out, value):
    while value >= 128:
        out.append((value & 127) | 128)
        value >>= 7
    out.append(value)


def string(out, value):
    raw = value.encode()
    unsigned(out, len(raw))
    out.extend(raw)


def cut_ring(ring):
    if not any(abs(a[0] - b[0]) > 18000000 for a, b in zip(ring, ring[1:])):
        return [ring]
    unwrapped = []
    for x, y in ring:
        if unwrapped:
            while x - unwrapped[-1][0] > 18000000:
                x -= 36000000
            while x - unwrapped[-1][0] < -18000000:
                x += 36000000
        unwrapped.append((x, y))

    def clip(points, edge, left):
        result = []
        for a, b in zip(points, points[1:] + points[:1]):
            inside_a = a[0] >= edge if left else a[0] <= edge
            inside_b = b[0] >= edge if left else b[0] <= edge
            if inside_a:
                result.append(a)
            if inside_a != inside_b:
                t = (edge - a[0]) / (b[0] - a[0])
                result.append((edge, round(a[1] + t * (b[1] - a[1]))))
        return result

    result = []
    for offset in [-36000000, 0, 36000000]:
        shifted = [(x + offset, y) for x, y in unwrapped]
        clipped = clip(clip(shifted, -18000000, True), 18000000, False)
        if len(set(clipped)) >= 3:
            if clipped[0] != clipped[-1]:
                clipped.append(clipped[0])
            result.append(clipped)
    return result


def records(path, layer, states):
    atlas = json.loads(path.read_text())
    scale, translate = atlas['transform']['scale'], atlas['transform']['translate']
    arcs = []
    for arc in atlas['arcs']:
        x = y = 0
        decoded = []
        for dx, dy in arc:
            x += dx
            y += dy
            decoded.append((round((x * scale[0] + translate[0]) * 100000), round((y * scale[1] + translate[1]) * 100000)))
        arcs.append(decoded)
    codes = dict(zip(STATE_IDS, STATE_CODES))
    for geometry in atlas['objects'][layer]['geometries']:
        name = geometry['properties']['name']
        aliases = {key(name)}
        if states:
            if geometry['id'] not in codes:
                continue
            aliases.add(key(codes[geometry['id']]))
            if geometry['id'] == '11':
                aliases.add('washingtondc')
        else:
            for group in COUNTRY_GROUPS:
                if key(name) in map(key, group):
                    aliases.update(map(key, group))
        polygons = geometry['arcs'] if geometry['type'] == 'MultiPolygon' else [geometry['arcs']]
        rings = []
        for polygon in polygons:
            for indices in polygon:
                ring = []
                for index in indices:
                    arc = arcs[index] if index >= 0 else list(reversed(arcs[~index]))
                    ring.extend(arc if not ring else arc[1:])
                rings.extend(cut_ring(ring))
        yield name, sorted(aliases), rings


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('world', type=Path)
    parser.add_argument('states', type=Path)
    parser.add_argument('--out', type=Path, default=Path('crates/extend-office-charts/src/parse/chartex/map_atlas'))
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    groups = [list(records(args.world, 'countries', False)), list(records(args.states, 'states', True))]
    out = bytearray(b'OCMA1')
    for group in groups:
        unsigned(out, len(group))
        for name, aliases, rings in group:
            string(out, name)
            unsigned(out, len(aliases))
            for alias in aliases:
                string(out, alias)
            unsigned(out, len(rings))
            for ring in rings:
                unsigned(out, len(ring))
                previous = (0, 0)
                for point in ring:
                    for a, b in zip(point, previous):
                        delta = a - b
                        unsigned(out, delta * 2 if delta >= 0 else -delta * 2 - 1)
                    previous = point
    compressor = zlib.compressobj(level=9, wbits=-15)
    data = compressor.compress(out) + compressor.flush()
    (args.out / 'boundaries.deflate').write_bytes(data)
    manifest = {
        'format': 'OCMA1: unsigned LEB128 lengths and zigzag deltas, longitude/latitude at 1e-5 degrees, raw DEFLATE',
        'inputs': [{'name': p.name, 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in [args.world, args.states]],
        'countries': len(groups[0]), 'states': len(groups[1]),
        'bytes': len(data), 'inflatedBytes': len(out),
        'sha256': hashlib.sha256(data).hexdigest(),
    }
    (args.out / 'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    print(json.dumps(manifest))


if __name__ == '__main__':
    main()

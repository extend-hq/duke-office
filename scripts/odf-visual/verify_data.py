"""Verify generated ODF chart probes through the public extraction bundles."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import zipfile


def canonical(charts):
    return sorted(json.dumps(chart, sort_keys=True) for chart in charts)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('parser', type=Path)
    p.add_argument('inputs', type=Path)
    p.add_argument('output', type=Path)
    args = p.parse_args()
    binary = args.parser.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    results, expected = [], None
    for ext in ('odt', 'fodt', 'odp', 'fodp', 'odg', 'fodg', 'ods', 'fods'):
        source = args.inputs / f'visual.{ext}'
        target = args.output / f'{ext}.json'
        audit = ext in ('ods', 'fods')
        subprocess.run([str(binary), '--audit' if audit else '--bundle', str(source), str(target)], check=True)
        document = json.loads(target.read_text())
        charts = [chart['data'] for chart in document['charts']] if audit else list(document.get('charts', {}).values())
        assert len(charts) == 11, (ext, len(charts))
        normalized = canonical(charts)
        if expected is None:
            expected = normalized
        assert normalized == expected, f'Data differs in {ext}'
        scatter = next(c for c in charts if c['series'][0]['chartType'] == 'scatter')
        assert [(p['x'], p['y']) for p in scatter['series'][0]['points']] == [('4','2'),('1','5'),('6','3')]
        bubble = next(c for c in charts if c['series'][0]['chartType'] == 'bubble')
        assert [(p['x'], p['y'], p['bubbleSize']) for p in bubble['series'][0]['points']] == [('2','4','2'),('5','1','5'),('3','6','3')]
        results.append(dict(format=ext, charts=len(charts), route='shared ODF API' if audit else 'public extraction bundle',
                            inputSha256=hashlib.sha256(source.read_bytes()).hexdigest()))

    precision = '9007199254740993.123400'
    for ext in ('odt', 'odp'):
        source = args.inputs / f'visual.{ext}'
        modified = args.output / f'no-artwork.{ext}'
        with zipfile.ZipFile(source) as src, zipfile.ZipFile(modified, 'w', zipfile.ZIP_DEFLATED) as dst:
            for name in src.namelist():
                content = src.read(name)
                if name.endswith('/content.xml'):
                    content = content.replace(b'<style:chart-properties chart:series-source=',
                                              b'<style:chart-properties chart:three-dimensional="true" chart:series-source=')
                    content = content.replace(b'office:value="2"', f'office:value="{precision}"'.encode())
                    content = content.replace(b'office:value="5"', b'')
                dst.writestr(name, content)
        target = args.output / f'no-artwork-{ext}.json'
        subprocess.run([str(binary), '--bundle', str(modified), str(target)], check=True)
        document = json.loads(target.read_text())
        charts = list(document.get('charts', {}).values())
        assert len(charts) == 11, (ext, 'unsupported artwork lost data')
        bars = [c for c in charts if c['series'][0]['chartType'] == 'bar']
        for chart in bars:
            points = chart['series'][0]['points']
            assert points[0]['value'] == precision
            assert points[1]['index'] == 1 and 'value' not in points[1]
            assert points[2]['value'] == '3'
        results.append(dict(format=ext, charts=len(charts), route='public bundle with unsupported artwork',
                            precisionPreserved=True, sparseIndicesPreserved=True,
                            inputSha256=hashlib.sha256(modified.read_bytes()).hexdigest()))
    result = dict(parserSha256=hashlib.sha256(binary.read_bytes()).hexdigest(), passed=True, checks=results)
    (args.output / 'report.json').write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()

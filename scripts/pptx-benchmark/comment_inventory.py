import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import zipfile
from xml.etree import ElementTree as ET

MODERN = 'http://schemas.microsoft.com/office/powerpoint/2018/8/main'
DRAWING = 'http://schemas.openxmlformats.org/drawingml/2006/main'
CONTENT_TYPES = 'http://schemas.openxmlformats.org/package/2006/content-types'


def name(tag):
    return tag.split('}', 1)[-1]


def inspect(row):
    result = dict(id=row['id'], parts=[])
    if hashlib.sha256(Path(row['path']).read_bytes()).hexdigest() != row['sha256']:
        raise ValueError('Input identity changed: ' + row['id'])
    if not zipfile.is_zipfile(row['path']):
        result['status'] = 'non-zip'
        return result
    try:
        with zipfile.ZipFile(row['path']) as archive:
            types = ET.fromstring(archive.read('[Content_Types].xml'))
            declared = [part.get('PartName', '').lstrip('/') for part in types
                        if part.tag == '{' + CONTENT_TYPES + '}Override'
                        and any(key in part.get('ContentType', '').lower() for key in ['comment', 'author'])]
            for part in sorted(set(declared)):
                if part not in archive.namelist():
                    result['parts'].append(dict(part=part, status='missing'))
                    continue
                data = archive.read(part)
                root = ET.fromstring(data)
                record = dict(part=part, status='ok', root=root.tag, sha256=hashlib.sha256(data).hexdigest())
                if root.tag == '{' + MODERN + '}cmLst':
                    threads = root.findall('{' + MODERN + '}cm')
                    replies = [reply for thread in threads for reply in thread.findall('{' + MODERN + '}replyLst/{' + MODERN + '}reply')]
                    comments = [*threads, *replies]
                    bodies = [comment.find('{' + MODERN + '}txBody') for comment in comments]
                    record.update(threads=len(threads), replies=len(replies),
                        empty_bodies=sum(body is None or not ''.join(body.itertext()).strip() for body in bodies),
                        statuses=dict(Counter(comment.get('status', 'active') for comment in comments)),
                        anchors=dict(Counter(child.tag for thread in threads for child in thread
                            if name(child.tag) in ['sldMkLst', 'deMkLst', 'txMkLst', 'unknownAnchor'])),
                        task_properties=dict(Counter(key for thread in threads for key in thread.attrib
                            if key in ['startDate', 'dueDate', 'assignedTo', 'complete', 'title'])),
                        drawing_runs=sum(1 for body in bodies if body is not None for element in body.iter()
                            if element.tag == '{' + DRAWING + '}r'),
                        extension_elements=dict(Counter(element.tag for comment in comments
                            for child in comment if name(child.tag) == 'extLst' for element in child.iter())))
                elif root.tag == '{' + MODERN + '}authorLst':
                    record['authors'] = len(root.findall('{' + MODERN + '}author'))
                result['parts'].append(record)
            result['status'] = 'ok'
    except Exception as error:
        result.update(status='error', error=str(error))
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--manifest', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    rows = json.loads(args.manifest.read_text())
    assert not any(row.get('split') == 'validation' for row in rows)
    assert len(rows) == len({row['id'] for row in rows})
    records = [inspect(row) for row in rows]
    parts = [part for record in records for part in record['parts']]
    summary = dict(inputs=len(rows), manifest_sha256=hashlib.sha256(args.manifest.read_bytes()).hexdigest(),
        statuses=dict(Counter(record['status'] for record in records)),
        roots=dict(Counter(part.get('root', part['status']) for part in parts)),
        modern_documents=sum(any(part.get('root') == '{' + MODERN + '}cmLst' for part in record['parts']) for record in records),
        modern_threads=sum(part.get('threads', 0) for part in parts), modern_replies=sum(part.get('replies', 0) for part in parts),
        modern_authors=sum(part.get('authors', 0) for part in parts),
        scope='Declared comment/author parts in the established tuning corpus. This is a source inventory, not a parser accuracy score or rendered-fidelity result.')
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / 'records.json').write_text(json.dumps(records, indent=2))
    (args.output / 'summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary))


if __name__ == '__main__':
    main()

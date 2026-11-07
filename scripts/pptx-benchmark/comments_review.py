import argparse
import gzip
import hashlib
import json
from pathlib import Path
import posixpath
import zipfile
from xml.etree import ElementTree as ET

from artwork_math_gate import artwork_hashes
from feedback import OUT
from object_gate import invoke


PRESENTATION = ['http://schemas.openxmlformats.org/presentationml/2006/main', 'http://purl.oclc.org/ooxml/presentationml/main']
RELATIONSHIPS = ['http://schemas.openxmlformats.org/officeDocument/2006/relationships/', 'http://purl.oclc.org/ooxml/officeDocument/relationships/']


def relationships(archive, part):
    relpart = posixpath.join(posixpath.dirname(part), '_rels', posixpath.basename(part) + '.rels')
    if relpart not in archive.namelist():
        return []
    return [dict(type=node.get('Type'), external=node.get('TargetMode') == 'External', target=posixpath.normpath(posixpath.join(posixpath.dirname(part), node.get('Target', ''))).lstrip('/')) for node in ET.fromstring(archive.read(relpart))]


def kind(relationship, name):
    return not relationship['external'] and relationship['type'] in [prefix + name for prefix in RELATIONSHIPS]


def source_comments(archive, part, authors):
    root = ET.fromstring(archive.read(part))
    namespace = next((ns for ns in PRESENTATION if root.tag == '{' + ns + '}cmLst'), None)
    if namespace is None:
        return None
    comments = []
    for index, node in enumerate(root.findall('{' + namespace + '}cm')):
        text = node.find('{' + namespace + '}text')
        author_id, source_index = node.get('authorId'), node.get('idx')
        comment = dict(id=f'comment-{author_id}-{int(source_index)}' if author_id is not None and source_index is not None else f'comment-{index + 1}', text='' if text is None else text.text or '', sourcePart=part)
        if author_id is not None:
            comment['authorId'] = author_id
        if source_index is not None:
            comment['sourceIndex'] = int(source_index)
        if node.get('dt') is not None:
            comment['modifiedAt'] = node.get('dt')
        author = authors.get(author_id, {})
        if 'name' in author:
            comment['author'] = author['name']
        if 'initials' in author:
            comment['authorInitials'] = author['initials']
        position = node.find('{' + namespace + '}pos')
        if position is not None and position.get('x') is not None and position.get('y') is not None:
            comment['position'] = dict(x=int(position.get('x')), y=int(position.get('y')), unit='eighthPoint')
        comments.append(comment)
    return comments


def verify_sources(path, document):
    counts = dict(comments=0, empty=0, authors=0, positions=0, modified=0, unscored_parts=0)
    with zipfile.ZipFile(path) as archive:
        authors = {}
        for relationship in relationships(archive, 'ppt/presentation.xml'):
            target = relationship['target']
            if kind(relationship, 'commentAuthors') and target in archive.namelist():
                root = ET.fromstring(archive.read(target))
                namespace = next((ns for ns in PRESENTATION if root.tag == '{' + ns + '}cmAuthorLst'), None)
                if namespace:
                    authors.update({n.get('id'): n.attrib for n in root.findall('{' + namespace + '}cmAuthor')})
        for slide in document['slides']:
            part = slide['sourcePart']
            rels = relationships(archive, part)
            targets = [r['target'] for r in rels if kind(r, 'comments')]
            if not targets:
                targets = [r['target'] for r in rels if not r['external'] and r['target'].startswith('ppt/comments/')]
            assert len(targets) <= 1, 'Multiple comment parts require separate adjudication.'
            expected = source_comments(archive, targets[0], authors) if targets and targets[0] in archive.namelist() else []
            if expected is None:
                counts['unscored_parts'] += 1
                continue
            assert slide.get('comments', []) == expected, f'Comment source mismatch in {part}'
            counts['comments'] += len(expected)
            counts['empty'] += sum(not c['text'] for c in expected)
            counts['authors'] += sum('author' in c for c in expected)
            counts['positions'] += sum('position' in c for c in expected)
            counts['modified'] += sum('modifiedAt' in c for c in expected)
    return counts


def without_comments(document):
    result = dict(document)
    result['slides'] = [{key: value for key, value in slide.items() if key != 'comments'} for slide in document['slides']]
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--label', required=True)
    parser.add_argument('--manifest', type=Path, required=True)
    args = parser.parse_args()
    run = OUT / 'experiments' / args.label
    proof = json.loads((run / 'parity-summary.json').read_text())
    sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    raw = run / 'parity.jsonl'
    assert sha(raw) == proof['records_sha256'] and sha(args.manifest) == proof['manifest_sha256']
    candidate, baseline = run / 'candidate', Path(proof['baseline_path'])
    assert sha(candidate) == proof['candidate_sha256'] and sha(baseline) == proof['baseline_sha256']
    records = [json.loads(line) for line in raw.read_text().splitlines()]
    rows = {r['id']: r for r in json.loads(args.manifest.read_text())}
    destination = run / 'comments-reviewed.jsonl'
    assert not destination.exists(), 'Preserve existing receipts.'
    totals = {}
    for index, record in enumerate(records):
        row = rows[record['id']]
        assert sha(Path(row['path'])) == row['sha256']
        record['passed'] = record['equal']
        if record['status'] == 'ok' and zipfile.is_zipfile(row['path']):
            document = json.load(gzip.open(run / (row['id'] + '.compact.json.gz'), 'rt'))
            counts = verify_sources(row['path'], document)
            record['source_counts'] = counts
            for key, value in counts.items():
                totals[key] = totals.get(key, 0) + value
        if not record['equal']:
            assert record['status'] == 'ok'
            old, before = invoke(baseline, row, run / (row['id'] + '.before.json.gz'))
            new, after = invoke(candidate, row, run / (row['id'] + '.after.json.gz'))
            assert old['status'] == new['status'] == 'ok'
            assert new['full_sha256'] == record['full_sha256']
            assert without_comments(before) == without_comments(after)
            record['artwork_hashes'] = artwork_hashes(after)
            record['passed'] = True
        if index % 100 == 0:
            print(index, len(records), flush=True)
    destination.write_text(''.join(json.dumps(r) + '\n' for r in records))
    summary = dict(total=len(records), passed=sum(r['passed'] for r in records), exact=sum(r['equal'] for r in records), changed=sum(not r['equal'] for r in records), source_counts=totals, records_sha256=sha(destination), raw_records_sha256=sha(raw), manifest_sha256=sha(args.manifest), candidate_sha256=proof['candidate_sha256'], baseline_sha256=proof['baseline_sha256'], harness_sha256=sha(Path(__file__)), scope='Independent namespace-aware source XML checks of classic comment text, including empty/whitespace content, author identity/name/initials, source index/part, modification timestamp and eighth-point position. All non-comment model fields and payloads remain exact. Nonclassic comment parts are explicitly unscored; rendered comment placement is not assessed.')
    (run / 'comments-reviewed-summary.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary))
    assert summary['passed'] == summary['total']


if __name__ == '__main__':
    main()

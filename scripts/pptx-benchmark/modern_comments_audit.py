import argparse
from decimal import Decimal
import gzip
import hashlib
import json
from pathlib import Path
import zipfile
from xml.etree import ElementTree as ET

from comments_probes import package as classic_package
from comments_review import relationships, without_comments
from feedback import OUT, invoke as compact
from object_gate import invoke

M = 'http://schemas.microsoft.com/office/powerpoint/2018/8/main'
D = 'http://schemas.openxmlformats.org/drawingml/2006/main'
DS = 'http://purl.oclc.org/ooxml/drawingml/main'
A = 'http://schemas.microsoft.com/office/drawing/2013/main/command'
P = 'http://schemas.microsoft.com/office/powerpoint/2013/main/command'
R = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'
MR = 'http://schemas.microsoft.com/office/2018/10/relationships/'
Q = lambda ns, name: '{' + ns + '}' + name
sha = lambda p: hashlib.sha256(Path(p).read_bytes()).hexdigest()


def moniker(node):
    namespace, name = node.tag[1:].split('}', 1) if node.tag.startswith('{') else ('', node.tag)
    result = dict(namespace=namespace, name=name, attributes=node.attrib)
    if list(node):
        result['children'] = [moniker(child) for child in node]
    text = (node.text or '') + ''.join(child.tail or '' for child in node)
    if text:
        result['text'] = text
    return result


def extension_equal(source, value):
    if source is None:
        assert value is None
    else:
        root = ET.fromstring(value)
        assert len(root) == 1
        assert moniker(root[0]) == moniker(source)


def body_text(node):
    body = node.find(Q(M, 'txBody'))
    if body is None:
        return []
    paragraphs = []
    for p in body:
        if p.tag not in [Q(D, 'p'), Q(DS, 'p')]:
            continue
        text = ''
        for run in p:
            if run.tag in [Q(D, 'br'), Q(DS, 'br')]:
                text += '\n'
            elif run.tag in [Q(D, 'r'), Q(DS, 'r'), Q(D, 'fld'), Q(DS, 'fld')]:
                text += ''.join(t.text or '' for t in run if t.tag in [Q(D, 't'), Q(DS, 't')])
        paragraphs.append(text)
    return paragraphs


def check_comment(node, actual, authors, part, reply=False):
    expected = dict(id=node.get('id'), text='\n'.join(body_text(node)), sourcePart=part)
    for source, target in [('authorId', 'authorId'), ('created', 'createdAt')]:
        if source in node.attrib:
            expected[target] = node.get(source)
    author = authors.get(node.get('authorId'))
    if author is not None:
        for source, target in [('name', 'author'), ('initials', 'authorInitials')]:
            if source in author.attrib:
                expected[target] = author.get(source)
    assert {k: v for k, v in actual.items() if k not in ['paragraphs', 'replies', 'modern']} == expected
    paragraphs = actual.get('paragraphs', [])
    assert [''.join(run['text'] for run in p.get('runs', [])) for p in paragraphs] == body_text(node)
    metadata = dict(status=' '.join(node.get('status', 'active').split()))
    if author is not None:
        for source, target in [('userId', 'authorUserId'), ('providerId', 'authorProviderId')]:
            if source in author.attrib:
                metadata[target] = author.get(source)
    anchors = []
    if not reply:
        kinds = {Q(P, 'sldMkLst'): 'slide', Q(A, 'deMkLst'): 'drawingElement', Q(A, 'txMkLst'): 'textRange', Q(M, 'unknownAnchor'): 'unknown'}
        for item in node:
            if item.tag in kinds:
                anchors.append(dict(kind=kinds[item.tag], monikers=[moniker(x) for x in item]))
        if anchors:
            metadata['anchors'] = anchors
        pos = node.find(Q(M, 'pos'))
        if pos is not None:
            def emu(value):
                factors = dict(mm=36000, cm=360000, **{'in': 914400}, pt=12700, pc=152400, pi=152400)
                return float(Decimal(value[:-2]) * factors[value[-2:]]) if value[-2:] in factors else int(value)
            metadata['positionEmu'] = dict(x=emu(pos.get('x')), y=emu(pos.get('y')), sourceX=pos.get('x'), sourceY=pos.get('y'))
        task = dict(assignedTo=node.get('assignedTo', '').split(), title=node.get('title', ''), completion=node.get('complete', '0%'))
        for source, target in [('startDate', 'startDate'), ('dueDate', 'dueDate')]:
            if source in node.attrib:
                task[target] = node.get(source)
        try:
            fraction = float(task['completion'][:-1]) / 100 if task['completion'].endswith('%') else -1
            if 0 <= fraction <= 1:
                task['completionFraction'] = fraction
        except ValueError:
            pass
        metadata['task'] = task
    observed = dict(actual['modern'])
    extension_equal(node.find(Q(M, 'extLst')), observed.pop('extensionsXml', None))
    extension_equal(None if author is None else author.find(Q(M, 'extLst')), observed.pop('authorExtensionsXml', None))
    assert observed == metadata
    reply_list = node.find(Q(M, 'replyLst'))
    replies = [] if reply_list is None or reply else reply_list.findall(Q(M, 'reply'))
    assert len(actual.get('replies', [])) == len(replies)
    for source, result in zip(replies, actual.get('replies', [])):
        check_comment(source, result, authors, part, True)
    return len(replies)


def verify(path, document):
    counts = dict(parts=0, threads=0, replies=0)
    with zipfile.ZipFile(path) as archive:
        authors = {}
        for rel in relationships(archive, 'ppt/presentation.xml'):
            if not rel['external'] and rel['type'] == MR + 'authors' and rel['target'] in archive.namelist():
                root = ET.fromstring(archive.read(rel['target']))
                if root.tag == Q(M, 'authorLst'):
                    authors.update({n.get('id'): n for n in root.findall(Q(M, 'author'))})
        for slide in document['slides']:
            for rel in relationships(archive, slide['sourcePart']):
                if rel['external'] or rel['type'] != MR + 'comments' or rel['target'] not in archive.namelist():
                    continue
                part = rel['target']
                root = ET.fromstring(archive.read(part))
                if root.tag != Q(M, 'cmLst'):
                    continue
                nodes = root.findall(Q(M, 'cm'))
                assert len(nodes) == len(slide.get('comments', []))
                counts['parts'] += 1
                counts['threads'] += len(nodes)
                for node, actual in zip(nodes, slide.get('comments', [])):
                    counts['replies'] += check_comment(node, actual, authors, part)
    return counts


def probe_package(path, variant):
    classic_package(path)
    with zipfile.ZipFile(path) as archive:
        parts = {name: archive.read(name) for name in archive.namelist()}
    d = DS if variant == 'strict-drawing' else D
    ns = f'xmlns:m="{M}" xmlns:a="{d}" xmlns:ac="{A}" xmlns:pc="{P}" xmlns:r="{R}" xmlns:e="urn:extension"'
    body = '<m:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr b="1" i="1" lang="fr-FR" sz="1400"><a:hlinkClick r:id="link"/></a:rPr><a:t> leading\r\nline&#13;&amp;<![CDATA[tail]]> </a:t></a:r><e:r><a:t>foreign</a:t></e:r><a:br/><a:r><a:t>end</a:t></a:r></a:p><a:p/></m:txBody>'
    extension = '<m:extLst><e:ext uri="urn:probe">before<e:item e:code="7">inner</e:item>after</e:ext></m:extLst>'
    anchors = '<ac:deMkLst><pc:docMk/><pc:sldMk cId="9" sldId="256"/><ac:grpSpMk id="4"/><ac:spMk id="5" creationId="{11111111-1111-1111-1111-111111111111}"/></ac:deMkLst><ac:deMkLst><pc:docMk/><pc:sldMk sldId="256"/><ac:picMk id="8"/></ac:deMkLst><ac:txMkLst><pc:docMk/><pc:sldMk sldId="256"/><ac:spMk id="5"/><ac:txMk cp="2" len="3"><ac:context len="8" hash="9"/></ac:txMk></ac:txMkLst>'
    if variant == 'slide-anchor':
        anchors = '<pc:sldMkLst><pc:docMk/><pc:sldMk cId="9" sldId="256"/></pc:sldMkLst>'
    if variant == 'unknown-empty':
        anchors, body = '<m:unknownAnchor/>', ''
    status = ' status="closed"' if variant == 'closed' else ' status=" resolved "' if variant == 'token-whitespace' else ''
    completion = 'garbage' if variant == 'invalid-completion' else '25%'
    thread = f'<m:cm id="thread" authorId="author" created="2026-01-02T03:04:05Z" startDate="2026-01-01T00:00:00Z" dueDate="2026-01-10T00:00:00Z" assignedTo="author\tother" complete="{completion}" title=" title "{status}>{anchors}<e:pos x="999" y="999"/><m:pos x="-12700" y="25400"/><m:replyLst><m:reply id="reply" authorId="other" created="2026-01-03T00:00:00Z" status="resolved">{body}{extension}</m:reply><e:reply id="ignored"/><m:reply id="empty" authorId="missing" created="2026-01-04T00:00:00Z"/></m:replyLst>{body}{extension}</m:cm>'
    parts['review/thread.xml'] = f'<m:cmLst {ns}><e:cm>{body}</e:cm>{thread}</m:cmLst>'.encode()
    if variant.startswith('unit-'):
        unit = variant.split('-')[1]
        parts['review/thread.xml'] = parts['review/thread.xml'].replace(b'x="-12700" y="25400"', f'x="-1.5{unit}" y="0.0625{unit}"'.encode())
    parts['review/people.xml'] = f'<m:authorLst {ns}><e:author id="author" name="wrong"/><m:author id="author" name=" A\tB&#9;C " initials="" userId="opaque" providerId="provider">{extension}</m:author><m:author id="other" name="Second" initials="S" userId="opaque2" providerId="provider"/></m:authorLst>'.encode()
    rel = parts['ppt/_rels/presentation.xml.rels'].decode().replace(R + '/commentAuthors', MR + 'authors')
    if variant == 'external-authors':
        rel = rel.replace('Id="authors"', 'Id="authors" TargetMode="External"')
    if variant == 'foreign-authors':
        rel = rel.replace(MR + 'authors', 'urn:foreign/authors')
    parts['ppt/_rels/presentation.xml.rels'] = rel.encode()
    rel = parts['ppt/slides/_rels/slide1.xml.rels'].decode().replace(R + '/comments', MR + 'comments')
    if variant == 'external-comments':
        rel = rel.replace('Id="comments"', 'Id="comments" TargetMode="External"')
    parts['ppt/slides/_rels/slide1.xml.rels'] = rel.encode()
    parts['review/_rels/thread.xml.rels'] = b'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="link" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.invalid/path?a=1&amp;b=2" TargetMode="External"/></Relationships>'
    if variant.startswith('depth-'):
        depth = int(variant.split('-')[1])
        parts['review/thread.xml'] = (f'<m:cmLst {ns}><m:cm id="thread" authorId="author"><m:extLst>' + '<e:item>' * (depth - 3) + '</e:item>' * (depth - 3) + '</m:extLst></m:cm></m:cmLst>').encode()
    with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
        for name, value in parts.items():
            info = zipfile.ZipInfo(name, (2000, 1, 1, 0, 0, 0)); info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, value)


def probes(run):
    target = run / 'modern-probes'; target.mkdir(exist_ok=False)
    receipts, manifest = [], []
    variants = ['rich', 'strict-drawing', 'slide-anchor', 'unknown-empty', 'closed', 'invalid-completion', 'external-authors', 'foreign-authors', 'external-comments', 'depth-255', 'depth-256', 'depth-257', 'token-whitespace', *['unit-' + unit for unit in ['mm', 'cm', 'in', 'pt', 'pc', 'pi']]]
    for name in variants:
        path = target / (name + '.pptx'); probe_package(path, name)
        row = dict(id=name, path=str(path.resolve()), sha256=sha(path)); manifest.append(row)
        record, doc = invoke(run / 'candidate', row, target / (name + '.json.gz'))
        if name == 'depth-257':
            assert record['status'] == 'error' and 'XML nesting depth' in record.get('error', '')
            counts = {}
        else:
            assert record['status'] == 'ok', record
            counts = verify(path, doc)
            if name == 'external-comments':
                assert not doc['slides'][0].get('comments')
            elif name not in ['unknown-empty', 'depth-255', 'depth-256']:
                run_node = doc['slides'][0]['comments'][0]['paragraphs'][0]['runs'][0]
                assert run_node['bold'] is True and run_node['italic'] is True and run_node['fontSizePt'] == 14
                assert run_node['language'] == 'fr-FR'
                assert run_node['hyperlink'] == 'https://example.invalid/path?a=1&b=2'
        receipts.append(dict(id=name, sha256=row['sha256'], status=record['status'], source_counts=counts, passed=True))
        print(name, record['status'], flush=True)
    (target / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    summary = dict(total=len(receipts), passed=len(receipts), candidate_sha256=sha(run / 'candidate'), harness_sha256=sha(__file__), receipts=receipts)
    (run / 'modern-probes-summary.json').write_text(json.dumps(summary, indent=2) + '\n')


def corpus(run, manifest, reference_records, name):
    rows = json.loads(manifest.read_text()); assert len(rows) == len({r['id'] for r in rows})
    assert all(row.get('split') != 'validation' for row in rows)
    previous = {r['id']: r for r in map(json.loads, reference_records.read_text().splitlines())}
    records = []; totals = dict(parts=0, threads=0, replies=0)
    destination = run / (name + '.jsonl'); assert not destination.exists()
    fields = lambda r: (r['status'], r.get('full_sha256'), r.get('error'))
    with destination.open('w', buffering=1) as stream:
        for index, row in enumerate(rows):
            assert sha(row['path']) == row['sha256']
            output = run / (name + '.current.compact.json.gz')
            record = compact(run / 'candidate', 'duke-slides', row, output, 0)
            equal = fields(record) == fields(previous[row['id']])
            if not equal:
                old, before = invoke(OUT / 'experiments/xml-recent1/candidate', row, run / (row['id'] + '.before.json.gz'))
                new, after = invoke(run / 'candidate', row, run / (row['id'] + '.after.json.gz'))
                assert old['status'] == new['status'] == 'ok' and new['full_sha256'] == record['full_sha256']
                assert without_comments(before) == without_comments(after)
                counts = verify(row['path'], after)
                assert counts['parts'] > 0
                for key, value in counts.items(): totals[key] += value
                record['source_counts'] = counts
            record.update(id=row['id'], equal=equal, passed=True)
            records.append(record); stream.write(json.dumps(record) + '\n')
            if index % 50 == 0: print(index, len(rows), flush=True)
    output.unlink(missing_ok=True)
    summary = dict(total=len(records), passed=len(records), exact=sum(r['equal'] for r in records), changed=sum(not r['equal'] for r in records), source_counts=totals, candidate_sha256=sha(run/'candidate'), baseline_sha256=sha(OUT/'experiments/xml-recent1/candidate'), records_sha256=sha(destination), manifest_sha256=sha(manifest), reference_records_sha256=sha(reference_records), harness_sha256=sha(__file__), scope='Full model/error equality except independently checked typed modern comments. All non-comment fields and payloads are exact. Rendering, mentions and task/reaction extension semantics are not scored.')
    (run / (name + '-summary.json')).write_text(json.dumps(summary, indent=2) + '\n'); print(json.dumps(summary))


def main():
    p = argparse.ArgumentParser(); p.add_argument('--label', required=True); p.add_argument('--probes', action='store_true'); p.add_argument('--manifest', type=Path); p.add_argument('--reference-records', type=Path); p.add_argument('--name', default='modern-reviewed'); a = p.parse_args(); run = OUT/'experiments'/a.label
    assert sha(run/'candidate') == json.loads((run/'identity.json').read_text())['binary_sha256']
    if a.probes: probes(run)
    else: corpus(run, a.manifest, a.reference_records, a.name)


if __name__ == '__main__':
    main()

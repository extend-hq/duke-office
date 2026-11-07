import argparse
import hashlib
import json
from pathlib import Path
import zipfile

from artwork_math_gate import artwork_hashes
from comments_review import PRESENTATION, RELATIONSHIPS, verify_sources, without_comments
from feedback import OUT, invoke as compact
from object_gate import invoke


def package(path, strict=False, external_comments=False, external_authors=False, modern=False, missing_authors=False):
    namespace = PRESENTATION[int(strict)]
    relationship = RELATIONSHIPS[int(strict)].rstrip('/')
    root_namespace = 'urn:comment-extension' if modern else namespace
    comments = f'''<p:cmLst xmlns:p="{root_namespace}" xmlns:e="urn:extension">
<e:cm authorId="99" idx="1"><p:text>Extension content</p:text></e:cm>
<p:cm authorId="7" idx="2" dt="2020-01-02T03:04:05.123Z"><e:pos x="999" y="999"/><p:pos x="80" y="160"/><e:text>Extension content</e:text><p:text> leading\r\nline\rtab\t&amp;<![CDATA[&lt;\r\n]]>&#13; tail </p:text></p:cm>
<p:cm authorId="8" idx="2"><p:pos x="0" y="0"/><p:text/></p:cm>
<p:cm authorId="9" idx="3"><p:text> \t </p:text></p:cm>
</p:cmLst>'''
    authors = f'''<p:cmAuthorLst xmlns:p="{namespace}" xmlns:e="urn:extension"><e:cmAuthor id="7" name="Extension content"/><p:cmAuthor id="7" name="  Author\tA&#x9;B\r\nC  " initials=""/><p:cmAuthor id="8" name="Second" initials="S"/></p:cmAuthorLst>'''
    author_rel = '' if missing_authors else f'<Relationship Id="authors" Type="{relationship}/commentAuthors" Target="../review/people.xml"' + (' TargetMode="External"' if external_authors else '') + '/>'
    comment_target = '../comments/comment1.xml' if modern else '../../review/thread.xml'
    comment_part = 'ppt/comments/comment1.xml' if modern else 'review/thread.xml'
    comment_rel = f'<Relationship Id="comments" Type="{relationship}/comments" Target="{comment_target}"' + (' TargetMode="External"' if external_comments else '') + '/>'
    relns = 'http://schemas.openxmlformats.org/package/2006/relationships'
    parts = {
        '[Content_Types].xml': '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>',
        'ppt/presentation.xml': f'<p:presentation xmlns:p="{namespace}" xmlns:r="{relationship}"><p:sldIdLst><p:sldId id="256" r:id="slide"/></p:sldIdLst></p:presentation>',
        'ppt/_rels/presentation.xml.rels': f'<Relationships xmlns="{relns}"><Relationship Id="slide" Type="{relationship}/slide" Target="slides/slide1.xml"/>{author_rel}</Relationships>',
        'ppt/slides/slide1.xml': f'<p:sld xmlns:p="{namespace}"><p:cSld><p:spTree/></p:cSld></p:sld>',
        'ppt/slides/_rels/slide1.xml.rels': f'<Relationships xmlns="{relns}">{comment_rel}</Relationships>',
        comment_part: comments,
        'review/people.xml': authors,
    }
    with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
        for name, value in parts.items():
            info = zipfile.ZipInfo(name, (2000, 1, 1, 0, 0, 0)); info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, value.encode())


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--label', required=True); args = parser.parse_args()
    run = OUT / 'experiments' / args.label; destination = run / 'probes'; destination.mkdir()
    binary, baseline = run / 'candidate', OUT / 'experiments/legacy-directory1/candidate'
    cases = [('transitional', {}), ('strict', dict(strict=True)), ('external_comments', dict(external_comments=True)), ('external_authors', dict(external_authors=True)), ('missing_authors', dict(missing_authors=True)), ('nonclassic', dict(modern=True))]
    records, manifest, receipts = [], [], []
    for name, options in cases:
        path = (destination / (name + '.pptx')).resolve(); package(path, **options)
        row = dict(id=name, path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest()); manifest.append(row)
        old, before = invoke(baseline, row, destination / (name + '.before.json.gz'))
        new, after = invoke(binary, row, destination / (name + '.after.json.gz'))
        assert old['status'] == new['status'] == 'ok', name
        assert without_comments(before) == without_comments(after), name
        counts = verify_sources(path, after)
        if name == 'nonclassic':
            assert before == after and counts['unscored_parts'] == 1
        elif name == 'external_comments':
            assert counts['comments'] == 0 and not after['slides'][0].get('comments')
        else:
            assert counts['comments'] == 3 and counts['empty'] == 1 and counts['positions'] == 2 and counts['modified'] == 1
            assert counts['authors'] == (0 if name in {'missing_authors', 'external_authors'} else 2)
            comments = after['slides'][0]['comments']
            assert comments[0]['text'] == ' leading\nline\ntab\t&&lt;\n\r tail '
            assert comments[2]['text'] == ' \t '
            assert all('createdAt' not in c for c in comments)
        record = compact(binary, 'rust-pptx', row, destination / (name + '.compact.json.gz'), 0)
        assert record['full_sha256'] == new['full_sha256']
        record.update(id=name, artwork_hashes=artwork_hashes(after)); records.append(record)
        receipts.append(dict(id=name, input_sha256=row['sha256'], counts=counts, passed=True, full_sha256=new['full_sha256']))
        print(name, counts, flush=True)
    (destination / 'manifest.json').write_text(json.dumps(manifest, indent=2))
    records_path = destination / 'records.jsonl'; records_path.write_text(''.join(json.dumps(r) + '\n' for r in records))
    summary = dict(total=len(receipts), passed=len(receipts), receipts=receipts, candidate_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), baseline_sha256=hashlib.sha256(baseline.read_bytes()).hexdigest(), records_sha256=hashlib.sha256(records_path.read_bytes()).hexdigest(), harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), scope='Classic comment metadata, author/index identity, empty/whitespace text, CDATA/entities, XML newline and attribute normalization, strict namespaces, foreign child namespaces, nonstandard part paths and external/missing author/comment relationships. Nonclassic output is preserved without claiming typed support.')
    (run / 'comments-probes-summary.json').write_text(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()

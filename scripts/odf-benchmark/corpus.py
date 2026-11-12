"""Select public OpenDocument test files and print a download plan.

    corpus.py <listings dir> > plan.tsv

The listings are GitHub contents-API responses saved as JSON, one per source
directory (see README.md). Each output row is: family, source, file name,
download URL, size. Files over 5 MB and names that look like crash or fuzz
samples are skipped.
"""
import json, os, re, sys
lists = sys.argv[1]
RISKY = re.compile(r'(?:^|[^a-z])(?:cve|ofz|fuzz|crash|hang|poc)(?:[^a-z]|$)|forcepoint|exploit|malicious|bomb', re.I)
MAX = 5 * 1024 * 1024
FAMILY = {'.odt':'ODT','.ott':'ODT','.odm':'ODT','.otm':'ODT','.oth':'ODT','.fodt':'ODT',
          '.odp':'ODP','.otp':'ODP','.fodp':'ODP',
          '.odg':'ODG','.otg':'ODG','.fodg':'ODG',
          '.ods':'ODS','.ots':'ODS','.fods':'ODS',
          '.odc':'ODF other','.otc':'ODF other','.odf':'ODF other','.otf':'ODF other','.odi':'ODF other','.oti':'ODF other','.odb':'ODF other'}
# (listing, source prefix, {extension: how many to take; None = all})
PLAN = [
 ('lo-odfimport','libreoffice',{'.odt':None,'.fodt':None,'.ott':None}),
 ('lo-odfexport','libreoffice',{'.odm':None,'.ott':None,'.fodt':None,'.odt':None}),
 ('lo-odp','libreoffice',{'.odp':None,'.fodp':None}),
 ('lo-odg','libreoffice',{'.odg':None,'.fodg':None}),
 ('tika','tika',{e:None for e in FAMILY if FAMILY[e]!='ODS'}),
 ('odftk','odftoolkit',{'.odt':None,'.odp':None,'.otp':None,'.otg':None,'.odc':None,'.otc':None,'.odf':None}),
]
seen=set(); rows=[]; skipped=[]
for listing,prefix,wanted in PLAN:
    items=json.load(open(os.path.join(lists,listing+'.json')))
    by_ext={}
    for item in items:
        if item.get('type')!='file': continue
        name=item['name']; ext=os.path.splitext(name)[1].lower()
        if ext not in wanted or ext not in FAMILY: continue
        url=item.get('download_url') or ''
        if not url.startswith('https://raw.githubusercontent.com/'): continue
        if not re.fullmatch(r"[A-Za-z0-9 ._+()\-,&'\[\]]+", name) or name.startswith('.'):
            skipped.append((name,'name')); continue
        if RISKY.search(name): skipped.append((name,'risky name')); continue
        if not (0 < item.get('size',0) <= MAX): skipped.append((name,'size %s'%item.get('size'))); continue
        by_ext.setdefault(ext,[]).append((name,url,item['size']))
    for ext,files in by_ext.items():
        files.sort()
        count=wanted[ext]
        if count is not None and len(files)>count:
            step=len(files)/count
            files=[files[int(i*step)] for i in range(count)]
        for name,url,size in files:
            dest=f'{prefix}-{name}'
            key=(FAMILY[ext],dest.lower())
            if key in seen: continue
            seen.add(key)
            rows.append((FAMILY[ext],prefix,dest,url,size))
for r in rows: print('\t'.join(map(str,r)))
import collections
c=collections.Counter((r[0],r[1]) for r in rows)
sys.stderr.write('selected %d files, %.1f MB\n'%(len(rows),sum(r[4] for r in rows)/1e6))
for k,v in sorted(c.items()): sys.stderr.write('  %s / %s: %d\n'%(k[0],k[1],v))
sys.stderr.write('skipped: %s\n'%skipped[:25])

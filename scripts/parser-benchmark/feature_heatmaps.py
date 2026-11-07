import argparse
import csv
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import numpy as np

ROOT = Path(__file__).resolve().parents[2]
DOCX = [
    ('image_payload_occurrences', 'Images · payload occurrences'),
    ('explicit_bold_characters', 'Bold · aligned characters'),
    ('explicit_italic_characters', 'Italic · aligned characters'),
    ('hyperlink_target_characters', 'Links · targets at aligned characters'),
    ('table_text_literal', 'Tables · literal text retained'),
    ('horizontal_spans_literal', 'Horizontal spans · literal text + span'),
    ('headings', 'Headings · text + level'),
    ('lists', 'Lists · paragraph membership'),
    ('vertical_merges', 'Vertical merges · start markers'),
    ('textbox_text', 'Textboxes · exact text anchors'),
    ('nested_tables', 'Nested tables · exact anchors'),
    ('header_text', 'Headers · exact paragraph anchors'),
    ('footer_text', 'Footers · exact paragraph anchors'),
    ('footnotes_text', 'Footnotes · exact text anchors'),
    ('endnotes_text', 'Endnotes · exact text anchors'),
    ('equations', 'Equations · occurrence-count proxy'),
    ('comments_text', 'Comments · exact text anchors'),
    ('explicit_bold', 'Bold · source-run five-grams'),
    ('explicit_italic', 'Italic · source-run five-grams'),
    ('hyperlinks', 'Links · source-run five-grams'),
    ('tables', 'Tables · exact text anchors'),
    ('table_cells', 'Table cells · exact text anchors'),
    ('horizontal_merges', 'Horizontal spans · exact anchors'),
    ('images', 'Images · occurrence-count proxy'),
]
PPTX = [
    ('body_text','Slide text · five-grams'),('ordered_text','Slide text · ordered characters'),
    ('paragraphs','Paragraphs · exact anchors'),('tables_literal','Tables · literal text retained'),
    ('tables','Tables · exact anchors'),('cells','Table cells · exact anchors'),
    ('horizontal_spans','Horizontal spans · text + span'),('vertical_spans','Vertical spans · text + span'),
    ('images','Images · payload occurrences'),('bold','Bold · source-run five-grams'),
    ('italic','Italic · source-run five-grams'),('link','Links · source-run five-grams'),
    ('notes','Speaker notes · five-grams'),('comments','Comments · exact anchors'),
    ('chart_count','Charts · occurrence-count proxy'),('chart_series','Chart series · cached numeric values'),
    ('shape_positions','Top-level frames · ID + EMUs'),('hidden_slides','Hidden-slide flags'),
    ('slide_size','Slide dimensions · EMUs'),
]

def unscored(engine, key):
    if engine in ('pptxboss', 'betteroffice'):
        return False
    if engine == 'rustypptx':
        return key in ('shape_positions', 'slide_size', 'hidden_slides', 'comments')
    return (key in ('shape_positions','slide_size') and engine not in ('rust-pptx','python-pptx')) or (key=='hidden_slides' and engine in ('python-pptx','markitdown')) or (key=='comments' and engine!='rust-pptx')


def matrix(kind, source, output, features, engines, labels, stats, scope, identity, snapshot=False, unscored_features=None):
    rows=[]
    for key,label in features:
        expected={stats[e]['features'][key]['expected'] for e in engines if key in stats[e]['features']}
        if not expected: continue
        if len(expected)!=1:raise ValueError('Feature denominators must agree between engines')
        expected=expected.pop()
        if not expected:continue
        cells=[]
        for engine in engines:
            value=stats[engine]['features'].get(key,{})
            missing=kind=='pptx' and (unscored_features or unscored)(engine,key)
            recall=None if missing or 'recall' not in value else value['recall']
            if recall is not None:
                if not 0<=recall<=1:raise ValueError('Recall outside fixed scale')
                if not np.isclose(recall,value['matched']/value['expected'],rtol=1e-10,atol=1e-12):raise ValueError('Recall does not match source counts')
            cells.append(dict(engine=engine,recall=recall,matched=None if missing else value.get('matched'),expected=expected,unit=value.get('unit')))
        rows.append(dict(feature=key,label=label,expected=expected,cells=cells))
    receipt=dict(format=kind,scope=scope,identity=identity,generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),matplotlib_version=matplotlib.__version__,numpy_version=np.__version__,source=str(source.resolve().relative_to(ROOT)),source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),scale=[0,100],unscored='Not scored',rows=rows)
    (output/f'{kind}-feature-recall.json').write_text(json.dumps(receipt,indent=2))
    with (output/f'{kind}-feature-recall.csv').open('w',newline='') as stream:
        writer=csv.writer(stream);writer.writerow(['feature','expected_units',*labels])
        for row in rows:writer.writerow([row['label'],row['expected'],*[f"{cell['recall']*100:.6f}" if cell['recall'] is not None else 'Not scored' for cell in row['cells']]])
    values=np.array([[np.nan if c['recall'] is None else c['recall']*100 for c in r['cells']] for r in rows]);masked=np.ma.masked_invalid(values)
    cmap=plt.get_cmap('YlGnBu').copy();cmap.set_bad('#e6e8eb')
    plt.rcParams.update({'font.family':'DejaVu Sans','font.size':10})
    fig,ax=plt.subplots(figsize=(max(12,3.8+len(engines)*1.6),max(6,len(rows)*.43+2.6)),layout='constrained')
    image=ax.imshow(masked,vmin=0,vmax=100,cmap=cmap,aspect='auto')
    ax.set_xticks(range(len(engines)),labels,rotation=0);ax.xaxis.tick_top();ax.tick_params(length=0,pad=9)
    ax.set_yticks(range(len(rows)),[f"{r['label']}  ·  {r['expected']:,}" for r in rows])
    ax.set_xticks(np.arange(-.5,len(engines),1),minor=True);ax.set_yticks(np.arange(-.5,len(rows),1),minor=True);ax.grid(which='minor',color='white',linewidth=1.3);ax.tick_params(which='minor',length=0)
    for y,row in enumerate(values):
        for x,value in enumerate(row):
            ax.text(x,y,'Not scored' if np.isnan(value) else f'{value:.3f}%',ha='center',va='center',fontsize=9,color='white' if not np.isnan(value) and value>65 else '#162236')
    for spine in ax.spines.values():spine.set_visible(False)
    subtitle = identity['display_scope']
    stage = 'Measured snapshot' if snapshot else 'Preview'
    fig.suptitle(f'{kind.upper()} · feature recall against independent source XML\n{subtitle}\n{stage} · source extraction checks; not rendering fidelity',fontsize=13,ha='left',x=.02)
    bar=fig.colorbar(image,ax=ax,orientation='horizontal',fraction=.035,pad=.035,aspect=45);bar.set_label('Recall (%) · fixed 0–100 scale · gray cells were not scored')
    for suffix in ['png','svg']:fig.savefig(output/f'{kind}-feature-recall.{suffix}',dpi=180)
    plt.close(fig)
    table=['| Check | Expected units | '+' | '.join(label.replace('\n',' ') for label in labels)+' |','| --- | ---: | '+' | '.join(['---:']*len(engines))+' |']
    for row in rows:table.append('| '+row['label']+f" | {row['expected']:,} | "+' | '.join('Not scored' if cell['recall'] is None else f"{cell['recall']*100:.3f}%" for cell in row['cells'])+' |')
    (output/f'{kind}-feature-recall.md').write_text(scope+'\n\n'+'\n'.join(table)+'\n')
    return receipt


def main():
    p=argparse.ArgumentParser();p.add_argument('--docx',type=Path,default=ROOT/'crates/rust-docx/benchmarks/optimized-20261102/accuracy.json');p.add_argument('--pptx',type=Path,required=True);p.add_argument('--output',type=Path,default=ROOT/'output/pptx-benchmark-20261106/readme-preview');p.add_argument('--snapshot',action='store_true',help='label the figures as a measured snapshot instead of a preview');p.add_argument('--pptx-label',default='0.1.5 + local',help='version label for the measured rust-pptx build');args=p.parse_args();args.output.mkdir(parents=True,exist_ok=True)
    d=json.loads(args.docx.read_text());engines=['docx-core','docx-rs','office_oxide','undoc'];labels=['rust-docx\nNov 2 build',*[e+'\n'+d['versions'][e] for e in engines[1:]]]
    matrix('docx',args.docx,args.output,DOCX,engines,labels,d['docx']['engines'],d['scope'],{**{k:d[k] for k in ['measured_date','measured_commit','binary_sha256','versions']},'display_scope':'1,000 reference inputs · measured '+d['measured_date']},snapshot=args.snapshot)
    d=json.loads(args.pptx.read_text());engines=d['identity'].get('selected_engines',['rust-pptx','office_oxide','undoc','python-pptx','markitdown']);names={'rust-pptx':'rust-pptx\n'+args.pptx_label,'office_oxide':'office_oxide\n0.1.16','undoc':'undoc\n0.27.3','python-pptx':'python-pptx\n1.0.2','markitdown':'MarkItDown\n0.1.9','pptxboss':'pptxboss-core\n2.3.0','betteroffice':'BetterOffice\n0.5.1','rustypptx':'rustypptx\n0.2.0'};labels=[names[e] for e in engines];summary=d['summary']['rust-pptx']
    scope=f"{summary['total']} attempted OOXML inputs; {summary['reference_total']} source-reference packages. Failed parses count as zero on reference-bearing features. Reference exclusions and unscored adapter dimensions remain explicit. This is the retained {d['phase']} phase, not a benchmark of later code."
    identity={k:(str(Path(v).resolve().relative_to(ROOT)) if k.endswith('_path') and isinstance(v,str) and Path(v).is_absolute() and Path(v).resolve().is_relative_to(ROOT) else v) for k,v in d['identity'].items()}
    matrix('pptx',args.pptx,args.output,PPTX,engines,labels,d['summary'],scope,{**identity,'display_scope':f"{summary['reference_total']} reference packages · {d['phase']} · measured "+datetime.fromtimestamp(d['identity']['start'],timezone.utc).strftime('%Y-%m-%d')},snapshot=args.snapshot)
    print(json.dumps(dict(output=str(args.output),readme_changed=False,publication='measured snapshot' if args.snapshot else 'pending final optimization and validation')))

if __name__=='__main__':main()

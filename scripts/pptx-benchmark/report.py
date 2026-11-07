import html
import json
from pathlib import Path
import statistics
import shutil
import sys
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import numpy as np
from assess import percentile

OUT=Path(__file__).resolve().parents[2]/'output/pptx-benchmark-20261106'
ENGINES=['rust-pptx','office_oxide','undoc','python-pptx','markitdown']
LABELS=['rust-pptx 0.1.5','office_oxide 0.1.16','undoc 0.27.3','python-pptx 1.0.2','MarkItDown 0.1.9']
COLORS=['#165dcb','#8b5fbc','#dd8831','#27836b','#b75170']
FEATURES=[('body_text','Slide text · five-gram recall'),('ordered_text','Slide text · ordered character recall'),('paragraphs','Paragraphs · exact anchors'),('tables_literal','Tables · literal text retained'),('tables','Tables · exact anchors'),('cells','Table cells · exact anchors'),('horizontal_spans','Horizontal spans · text + span'),('vertical_spans','Vertical spans · text + span'),('images','Images · payload-matched occurrences'),('bold','Explicit bold · run five-grams'),('italic','Explicit italic · run five-grams'),('link','Text hyperlinks · run five-grams'),('notes','Speaker notes · five-grams'),('comments','Comments · exact anchors'),('chart_count','Charts · occurrence-count proxy'),('chart_series','Chart series · exact cached values'),('shape_positions','Explicit top-level frames · ID + EMUs'),('hidden_slides','Hidden-slide flags'),('slide_size','Slide dimensions · exact EMUs')]

def pct(x):return f'{100*x:.2f}%'
def ns(engine,key):return (key in ['shape_positions','slide_size'] and engine not in ['rust-pptx','python-pptx']) or (key=='hidden_slides' and engine in ['python-pptx','markitdown']) or (key=='comments' and engine!='rust-pptx')

def main(phase):
    run=OUT/phase;d=json.loads((run/'assessment.json').read_text());summary=d['summary'];site=OUT/'report';site.mkdir(exist_ok=True)
    if phase!='comparison' and not (site/'baseline').exists():
        (site/'baseline').mkdir()
        for p in site.iterdir():
            if p.is_file():shutil.copy2(p,site/'baseline'/p.name)
    labels=LABELS[:] if phase=='comparison' else [LABELS[0]+' + local changes',*LABELS[1:]]
    plt.rcParams.update({'font.size':10,'axes.spines.top':False,'axes.spines.right':False,'figure.facecolor':'#fafbfc','axes.facecolor':'#fafbfc'})
    fig,ax=plt.subplots(figsize=(10,4.5));y=np.arange(5);med=[summary[e]['median_ms'] for e in ENGINES];p95=[summary[e]['p95_ms'] for e in ENGINES]
    ax.barh(y-.16,med,.30,label='Median',color=COLORS);ax.barh(y+.16,p95,.30,label='p95',color=COLORS,alpha=.35)
    for i,(a,b) in enumerate(zip(med,p95)):
        ax.text(a+.15,i-.16,f'{a:.3f}',va='center',fontsize=9);ax.text(b+.15,i+.16,f'{b:.3f}',va='center',fontsize=9)
    ax.set_yticks(y,labels);ax.invert_yaxis();ax.set_xlabel('Milliseconds per presentation · warm parse + model construction');ax.legend(loc='upper right');ax.set_xlim(0,max(p95)*1.17);ax.set_title(f'PowerPoint parsing latency · {len(d["common_ids"])} common successful inputs',loc='left',pad=20);fig.tight_layout();fig.savefig(site/'latency.png',dpi=180);plt.close(fig)
    fig,axes=plt.subplots(1,3,figsize=(14,4.4))
    for ax,key,title in zip(axes,['bytes','slides','features'],['Input bytes','Slide count','Source feature count']):
        for e,label,color in zip(ENGINES,LABELS,COLORS):
            rows=[r for r in d['rows'] if r['engine']==e and r['id'] in d['common_ids']]
            ax.scatter([r[key] for r in rows],[r['median_ms'] for r in rows],s=13,alpha=.65,color=color,label=label)
        ax.set_xscale('symlog',linthresh=1);ax.set_yscale('log');ax.set_xlabel(title);ax.grid(alpha=.15)
    axes[0].set_ylabel('Median parse latency (ms)');axes[2].legend(fontsize=7,loc='lower right');fig.suptitle('Identical cohort · zero counts retained on symmetric-log x axes',x=.05,ha='left');fig.tight_layout();fig.savefig(site/'scatter.png',dpi=180);plt.close(fig)
    keys=['body_text','tables_literal','images','bold','italic','notes','chart_series'];values=np.array([[summary[e]['features'][k]['recall']*100 for e in ENGINES] for k in keys]);fig,ax=plt.subplots(figsize=(10,5));ax.imshow(values,vmin=0,vmax=100,cmap='YlGnBu',aspect='auto')
    ax.set_xticks(range(5),LABELS,rotation=18,ha='right');ax.set_yticks(range(len(keys)),[dict(FEATURES)[k] for k in keys]);ax.set_title('Feature recall against independent source XML',loc='left',pad=18)
    for j in range(len(keys)):
        for i in range(5):ax.text(i,j,f'{values[j,i]:.1f}%',ha='center',va='center',color='white' if values[j,i]>65 else '#162236')
    fig.tight_layout();fig.savefig(site/'features.png',dpi=180);plt.close(fig)
    summary_rows=[]
    for e,label in zip(ENGINES,labels):
        s=summary[e];f=s['features'];summary_rows.append(f'<tr><th>{label}</th><td>{s["reference_success"]}/{s["reference_total"]}</td><td>{s["median_ms"]:.3f}</td><td>{s["p95_ms"]:.3f}</td><td>{pct(f["body_text"]["macro_f1"])}</td><td>{pct(f["ordered_text"]["macro_f1"])}</td><td>{pct(f["body_text"]["recall"])}</td></tr>')
    common_rows=''.join(f'<tr><th>{label}</th><td>{pct(summary[e]["common_features"]["body_text"]["recall"])}</td><td>{pct(summary[e]["common_features"]["body_text"]["f1"])}</td></tr>' for e,label in zip(ENGINES,LABELS))
    feature_rows=[]
    for key,label in FEATURES:
        expected=summary['rust-pptx']['features'].get(key,{}).get('expected',0)
        if not expected:continue
        cells=''.join('<td class="muted">Not scored</td>' if ns(e,key) else f'<td>{pct(summary[e]["features"][key]["recall"])}</td>' for e in ENGINES)
        feature_rows.append(f'<tr><th>{label}</th><td>{expected:,}</td>{cells}</tr>')
    legacy_records=[json.loads(x) for x in (OUT/'legacy/records.jsonl').read_text().splitlines()];lb={(r['id'],r['engine']):r for r in legacy_records};ids={r['id'] for r in legacy_records};lc=[i for i in ids if all(lb[i,e]['status']=='ok' for e in ENGINES[:3])];legacy=[]
    for e in ENGINES[:3]:
        times=[statistics.median(lb[i,e]['samples_ms']) for i in lc];legacy.append(dict(engine=e,success=sum(lb[i,e]['status']=='ok' for i in ids),total=len(ids),common=len(lc),median_ms=statistics.median(times),p95_ms=percentile(times,.95)))
    (run/'legacy-summary.json').write_text(json.dumps(legacy,indent=2))
    legacy_rows=''.join(f'<tr><th>{r["engine"]}</th><td>{r["success"]}/{r["total"]}</td><td>{r["median_ms"]:.3f}</td><td>{r["p95_ms"]:.3f}</td></tr>' for r in legacy)
    compact=[dict(id=r['id'],engine=r['engine'],bytes=r['bytes'],slides=r['slides'],paragraphs=r['paragraphs'],features=r['features'],characters=r['characters'],latency=r['median_ms'],recall=r['metrics'].get('body_text',{}).get('recall',1)*100,f1=r['metrics'].get('body_text',{}).get('f1',1)*100) for r in d['rows'] if r['id'] in d['common_ids']]
    experiments=[]
    for p in sorted((OUT/'experiments').glob('*/timing-summary*.json')):
        timing=json.loads(p.read_text());parity=json.loads(p.with_name('parity-summary.json').read_text())
        reference=Path(parity.get('baseline_path',str(OUT/'baseline-parity-binary'))).parent.name if parity.get('baseline_path') else 'original'
        experiments.append(f'<tr><th>{html.escape(p.parent.name)} · {timing.get("split","tuning")} · vs {html.escape(reference)}</th><td>{parity["equal"]}/{parity["total"]}</td><td>{timing["inputs"]}</td><td>{timing["median_speedup"]:.3f}×</td><td>{timing["candidate_wins"]}/{timing["inputs"]}</td></tr>')
    comparison=''
    if phase!='comparison':
        baseline=json.loads((OUT/'comparison/assessment.json').read_text())['summary']['rust-pptx'];current=summary['rust-pptx']
        comparison='<h2>Change from the original parser</h2><p>Fresh five-parser timings retain the original 145-input timing cohort. Newly accepted empty decks cannot lower the measured median. Shared-machine timing variation remains possible. <a href="baseline/index.html">Original report and charts</a>.</p><div class="scroll"><table><thead><tr><th>Measurement</th><th>Original</th><th>Current</th></tr></thead><tbody>'
        for title,old,new in [('Reference packages accepted',str(baseline['reference_success'])+'/157',str(current['reference_success'])+'/157'),('Median latency',f'{baseline["median_ms"]:.3f} ms',f'{current["median_ms"]:.3f} ms'),('p95 latency',f'{baseline["p95_ms"]:.3f} ms',f'{current["p95_ms"]:.3f} ms'),('Chart-series recall',pct(baseline['features']['chart_series']['recall']),pct(current['features']['chart_series']['recall'])),('Notes recall',pct(baseline['features']['notes']['recall']),pct(current['features']['notes']['recall']))]:
            comparison+=f'<tr><th>{title}</th><td>{old}</td><td>{new}</td></tr>'
        comparison+='</tbody></table></div>'
    page=(Path(__file__).with_name('report.html')).read_text().replace('__SUMMARY__',''.join(summary_rows)).replace('__EXPERIMENTS__',''.join(experiments)).replace('__FEATURES__',''.join(feature_rows)).replace('__LEGACY__',legacy_rows).replace('__COMMON_QUALITY__',common_rows).replace('__COMMON__',str(len(d['common_ids']))).replace('__LEGACY_COMMON__',str(len(lc))).replace('__DATA__',json.dumps(compact)).replace('__IDENTITY__',html.escape(json.dumps(d['identity'],indent=2)))
    page=page.replace('__COMPARISON__',comparison).replace('__PHASE__',html.escape(phase))
    if phase!='comparison':
        page=page.replace('The current parser rejects zero-slide presentations, although the presentation schema permits an absent slide list.','Valid zero-slide presentations are now accepted. The previously unresolved-slide guard remains.')
        page=page.replace('Classic chart extraction misses 3D chart families and secondary plots in combination charts. Shared SVG rendering and source XML retention do not fill the missing structured series.','Classic chart extraction now includes 3D families and every combination plot. Per-series axis/plot ownership, bubble sizes and complete ChartEx structured data still need coverage.')
        page=page.replace('Notes flatten all text into a single string and include notes-page metadata; they do not retain rich text or a clean notes-body boundary.','Notes now preserve paragraphs, runs and explicit hyperlinks alongside flat text, and exclude notes-page placeholders. Notes-master/theme inheritance and native formatting fidelity remain open.')
        page=page.replace('<h2>Legacy PPT robustness</h2>','<h2>Validation</h2><p>All 318 inputs passed the semantic audit: 273 outputs/statuses were unchanged and 45 changed only in the intended empty-deck, chart or notes behavior. Every previously retained chart series survived; unrelated SVG, XML and payload bytes remain exact. All 392 existing Rust checks and the React model typecheck pass. Native/WASM acceptance agrees on all 318 inputs; all 278 successful compact semantic models and payload hashes agree. Shape-SVG serialized lengths differ in 21 documents, so full cross-platform SVG/pixel equivalence is not established. These are local, unpublished changes.</p><h2>Legacy PPT robustness</h2>')
    inserted_path=OUT/'experiments/inserted2/inserted-summary.json'
    inserted=json.loads(inserted_path.read_text()) if inserted_path.exists() else {}
    inserted_current=inserted.get('candidate_sha256')==d['identity']['binary_sha256'] and inserted.get('passed')==inserted.get('total')
    object_path=OUT/'experiments/objects3/objects-summary.json'
    if object_path.exists():
        objects=json.loads(object_path.read_text())
        if d['identity']['binary_sha256']==objects['candidate_sha256'] or (inserted_current and inserted.get('baseline_sha256')==objects['candidate_sha256']):
            counts=objects['counts']
            page=page.replace('The Rust model has no media node, shape action, alt-text or hidden-object fields, even though the React model already provides several of these fields.','The Rust/WASM/React contract now exposes media, OLE payloads and posters, shape actions, alt text, hidden flags and source ownership. Click actions support internal slide navigation; hover actions, sounds, fades and bookmarks are retained but are not played by the viewer.')
            page=page.replace('Animation/timing, transitions, embedded-object metadata, math, ink, sections and presentation-level settings need explicit coverage and downstream contracts.','Animation/timing, transitions, math, ink, sections and presentation-level settings still need explicit coverage and downstream contracts. OLE previews that only exist in VML remain incomplete.')
            details=f'<h2>Object and media coverage</h2><p>The independent object audit passed {objects["passed"]}/{objects["total"]} corpus inputs against the preceding semantic build. It verified {counts["sourcePart"]:,} object representations against their owning XML parts, including {counts["description"]} descriptions, {counts["altText"]} nonempty alt-text values, {counts["hidden"]} explicit hidden flags, {counts["clickAction"]} click actions, {counts["media_ole"]} OLE nodes and {counts["media_video"]+counts["media_audio"]} audio/video nodes. Counts include repeated inherited/master/layout representations; they are not unique source-object counts or competitor scores.</p><p>{counts["media_payloads"]} referenced media/object payloads matched their package bytes; {counts["new_assets"]} previously omitted assets are now retained. Existing image fallbacks remain attached as posters, including compatibility fallbacks. Focused package probes cover external relative targets, escaped URLs, link/embed precedence, nonstandard package paths and media time units. Browser interaction checks cover poster loading, decoded video, OLE downloads, hidden objects and internal slide navigation. These checks do not establish pixel fidelity or full media playback semantics.</p>'
            page=page.replace('<h2>Validation</h2>',details+'<h2>Validation</h2>')
            page=page.replace('All 318 inputs passed the semantic audit: 273 outputs/statuses were unchanged and 45 changed only in the intended empty-deck, chart or notes behavior. Every previously retained chart series survived; unrelated SVG, XML and payload bytes remain exact.','The earlier notes/chart audit passed all 318 inputs. The current object audit checks all unrelated content, retained posters and original assets. The subsequent compact-string/optional-metadata optimization preserves exact full output and status on all 318 inputs.')
            page=page.replace('All 392 existing Rust checks and the React model typecheck pass.','All 392 existing Rust checks, 41 viewer checks and the React model/viewer typechecks pass.')
    if inserted_current:
        counts=inserted['counts']
        details=f'<h2>Equations, ink and embedded controls</h2><p>The current audit passes {inserted["passed"]}/{inserted["total"]} original corpus inputs: {inserted["unchanged"]} complete outputs/statuses are unchanged. The two changed inputs now retain {counts["math"]} structured equations, {counts["ink_objects"]} ink objects with {counts["ink_strokes"]} traces, and {counts["controls"]} embedded controls with {counts["control_properties"]} properties. {counts["new_assets"]} added payloads match the package bytes; all unrelated output remains exact.</p><p>Equations expose Office Math trees, source XML and MathML. Ink exposes physical points, pressure, brush properties, slide frames and SVG artwork. Controls expose definitions and VML-linked saved previews. Twelve focused package checks cover strict namespaces, math defaults and fallbacks, ink delta coordinates and zero extents, and control relationships. Browser checks confirm loaded artwork and previews. LibreOffice omits the ink in the inspected deck, so it cannot establish ink pixel fidelity; font/layout differences also remain. ActiveX execution, complete equation typography and all InkML transforms remain outside this milestone.</p>'
        page=page.replace('<h2>Validation</h2>',details+'<h2>Validation</h2>')
        page=page.replace('The independent object audit passed','The preceding independent object audit passed')
        page=page.replace('Animation/timing, transitions, math, ink, sections and presentation-level settings still need explicit coverage and downstream contracts. OLE previews that only exist in VML remain incomplete.','Animation/timing, transitions, sections and presentation-level settings still need explicit coverage and downstream contracts. Equation typography, uncommon InkML transforms and general OLE previews that only exist in VML remain incomplete.')
    expanded_path=OUT/'expanded/summary.json'
    if expanded_path.exists():
        expanded=json.loads(expanded_path.read_text()); acceptance=json.loads((OUT/'expanded/baseline/summary.json').read_text())
        formats=sorted(expanded['by_format'],key=lambda f:-expanded['by_format'][f])
        fig,ax=plt.subplots(figsize=(10,4));ax.bar(formats,[expanded['by_format'][f] for f in formats],color='#165dcb');ax.set_ylabel('Unique additional files');ax.set_title('Expanded corpus · formats kept separate',loc='left');fig.tight_layout();fig.savefig(site/'corpus.png',dpi=180);plt.close(fig)
        expansion_rows=[]
        for kind in formats:
            statuses=[acceptance['engines'][e].get(kind,{}) for e in ENGINES[:3]]
            total=sum(statuses[0].values())
            expansion_rows.append('<tr><th>.'+kind+'</th><td>'+str(expanded['by_format'][kind])+'</td>'+''.join('<td>'+str(status.get('ok',0))+'/'+str(total)+'</td>' for status in statuses)+'</tr>')
        details=f'<h2>Expanded corpus</h2><p>The corpus now contains {expanded["combined"]:,} unique files: the original 318 plus {expanded["added"]:,} additions from pinned LibreOffice, Apache Tika and Apache POI repositories and Embrace. Downloads are SHA-256 verified and deduplicated. {expanded["by_split"]["tuning"]} additions form the tuning set; {expanded["by_split"]["validation"]} are reserved for later validation. These sources are regression suites and archive collections, not a prevalence sample.</p><img src="corpus.png" alt="Additional corpus files by format"><p>The table below is an <strong>acceptance-only</strong> check of the preceding validated build on the new tuning set. It does not measure text accuracy or visual fidelity. The main latency and accuracy charts above still use the original cohort.</p><div class="scroll"><table><thead><tr><th>Format</th><th>Added files</th><th>rust-pptx accepted/tested</th><th>Office Oxide accepted/tested</th><th>undoc accepted/tested</th></tr></thead><tbody>'+''.join(expansion_rows)+'</tbody></table></div><p>Of the 13 legacy inputs accepted by Office Oxide and rejected by rust-pptx, 12 are rejected by our CFB reader for malformed or truncated container structures; one fails slide-record resolution. Acceptance alone does not establish that the competitor recovered correct content. OpenDocument and older Impress files remain outside the current parser format support.</p>'
        if (site/'expanded.html').exists():
            details += '<p><a href="expanded.html">Open the five-parser results on the expanded corpus</a>: separate timing cohort, failure-inclusive accuracy, source-feature table and selectable complexity/performance axes.</p>'
        page=page.replace('<h2>Legacy PPT robustness</h2>',details+'<h2>Legacy PPT robustness</h2>')
    if (site/'expanded.html').exists():
        page=page.replace('<h1>PowerPoint parser comparison</h1>','<h1>PowerPoint parser comparison</h1><p class="notice"><a href="expanded.html">Latest expanded-corpus comparison</a> · The collected corpus now has 1,245 unique files. The charts on this page retain the original cohort; the linked report contains the new five-parser run and paired optimization measurements.</p>')
    (site/'index.html').write_text(page)
    md=['# PowerPoint parser benchmark — November 6, 2026','',f'Build `{d["identity"]["commit"]}`. {len(d["common_ids"])} common successful inputs; 157 reference-bearing packages from 169 PPTX-labelled inputs.','', '| Parser | Parsed reference packages | Median ms | p95 ms | Text F1 | Ordered F1 | Text recall |','| --- | ---: | ---: | ---: | ---: | ---: | ---: |']
    for e,label in zip(ENGINES,labels):
        s=summary[e];f=s['features'];md.append(f'| {label} | {s["reference_success"]}/{s["reference_total"]} | {s["median_ms"]:.3f} | {s["p95_ms"]:.3f} | {pct(f["body_text"]["macro_f1"])} | {pct(f["ordered_text"]["macro_f1"])} | {pct(f["body_text"]["recall"])} |')
    md+=['','Text F1 is macro-averaged over text-bearing decks; recall pools reference five-grams. Ordered F1 aligns whitespace-normalized characters within each slide. Source-literal F1 penalizes legitimate extra master/layout or chart text and is not a visual fidelity score.','', '| Feature | Expected units | '+' | '.join(LABELS)+' |','| --- | ---: | '+' | '.join(['---:']*5)+' |']
    for key,label in FEATURES:
        expected=summary['rust-pptx']['features'].get(key,{}).get('expected',0)
        if expected:md.append(f'| {label} | {expected:,} | '+' | '.join('Not scored' if ns(e,key) else pct(summary[e]['features'][key]['recall']) for e in ENGINES)+' |')
    md+=['','See the interactive report for method, distributions, source provenance and limitations.','', '[Expanded-corpus results](EXPANDED-RESULTS.md) retain a separate cohort and denominators.']
    (site/'RESULTS.md').write_text('\n'.join(md)+'\n')
    print(site/'index.html')

if __name__=='__main__':main(sys.argv[1] if len(sys.argv)>1 else 'comparison')

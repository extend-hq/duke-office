import argparse
from datetime import datetime, timezone
import gzip
import hashlib
import html
import json
from pathlib import Path
import re
import sys

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

from report import ENGINES as ORIGINAL_ENGINES, LABELS as ORIGINAL_LABELS, COLORS, ns
from peer_models import ENGINES as PEER_ENGINES, LABELS as PEER_LABELS, unscored as peer_unscored

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'output/pptx-benchmark-20261106'
sys.path.insert(0, str(ROOT / 'scripts/parser-benchmark'))
from feature_heatmaps import matrix, PPTX


def unscored(engine, feature):
    return peer_unscored(engine, feature) if engine in PEER_ENGINES else ns(engine, feature)


def main(phase):
    source = OUT / phase / 'assessment.json'
    data = json.loads(source.read_text())
    engines = data['identity']['selected_engines']
    assert engines == ORIGINAL_ENGINES + PEER_ENGINES
    labels = ['rust-pptx 0.1.5 + local', *ORIGINAL_LABELS[1:], *PEER_LABELS]
    colors = [*COLORS, '#008b9b', '#657d25', '#626b7b']
    summary = data['summary']
    site = OUT / 'report/peers'
    site.mkdir(parents=True, exist_ok=True)
    date = datetime.fromtimestamp(data['identity']['start'], timezone.utc).strftime('%Y-%m-%d')
    scope = f"{summary['rust-pptx']['reference_total']} reference packages · {phase} · measured {date}"
    receipt = matrix('pptx', source, site, PPTX, engines,
        ['rust-pptx\n0.1.5 + local', *[label.rsplit(' ', 1)[0] + '\n' + label.rsplit(' ', 1)[1] for label in labels[1:]]],
        summary, scope, {**data['identity'], 'display_scope': scope}, snapshot=True, unscored_features=unscored)
    fig, ax = plt.subplots(figsize=(11, 6))
    for index, (engine, color) in enumerate(zip(engines, colors)):
        row = summary[engine]
        for offset, key, alpha in [(-.17, 'median_ms', 1), (.17, 'p95_ms', .35)]:
            ax.barh(index + offset, row[key], height=.3, color=color, alpha=alpha)
            ax.text(row[key], index + offset, f' {row[key]:.3f}', va='center', fontsize=9)
    ax.set_yticks(range(len(engines)), labels)
    ax.invert_yaxis()
    ax.set_xlabel('Milliseconds · median (solid) and p95 (faint)')
    ax.set_title(f"Eight parsers · {len(data['common_ids'])} common nonempty inputs", loc='left')
    ax.set_xlim(0, max(summary[e]['p95_ms'] for e in engines) * 1.18)
    fig.tight_layout()
    fig.savefig(site / 'latency.png', dpi=180)
    plt.close(fig)
    diagnostics = {}
    for engine in engines:
        rows = [r for r in data['rows'] if r['engine'] == engine]
        diagnostics[engine] = dict(accepted_with_fewer_slides=sum(r['status'] == 'ok' and r['metrics']['slide_count']['found'] < r['metrics']['slide_count']['expected'] for r in rows))
    boss = dict(inputs_with_reported_errors=0, errors=0, inputs_with_unknown_elements=0, unknown_elements=0, inputs_with_unknown_graphics=0, unknown_graphics=0, unresolved_slides=0)
    for path in (OUT / phase / 'native').glob('*.pptxboss.json.gz'):
        value = json.load(gzip.open(path, 'rt'))
        errors = len(value['auxiliary_errors']) + sum(len(s['errors']) for s in value['slides'])
        unknown = sum(s['unknown_elements'] for s in value['slides'])
        graphics = sum(len(s['unknown_graphics']) for s in value['slides'])
        boss['inputs_with_reported_errors'] += bool(errors)
        boss['errors'] += errors
        boss['inputs_with_unknown_elements'] += bool(unknown)
        boss['unknown_elements'] += unknown
        boss['inputs_with_unknown_graphics'] += bool(graphics)
        boss['unknown_graphics'] += graphics
        boss['unresolved_slides'] += len(value['unresolved_slides'])
    diagnostics['pptxboss']['reported'] = boss
    (site / 'diagnostics.json').write_text(json.dumps(diagnostics, indent=2))
    heading = '| Parser | Accepted/reference | Median ms | p95 ms | Text recall | Macro text F1 |'
    markdown = ['# Eight-parser PowerPoint comparison', '', scope, '', heading, '| --- | ---: | ---: | ---: | ---: | ---: |']
    table = []
    for engine, label in zip(engines, labels):
        row = summary[engine]
        body = row['features']['body_text']
        values = [label, f"{row['reference_success']}/{row['reference_total']}", f"{row['median_ms']:.6f}", f"{row['p95_ms']:.6f}", f"{body['recall']:.3%}", f"{body['macro_f1']:.3%}"]
        markdown.append('| ' + ' | '.join(values) + ' |')
        table.append('<tr>' + ''.join(f'<{tag}>{html.escape(value)}</{tag}>' for tag, value in zip(['th', *['td'] * 5], values)) + '</tr>')
    method = [
        f"All eight parsers are freshly measured on the same {summary['rust-pptx']['total']} OOXML tuning packages. The independent ZIP/XML reference accepts {summary['rust-pptx']['reference_total']}; reference exclusions remain separate. Failures count as zero where source features exist. Timings use the {len(data['common_ids'])} nonempty inputs accepted by all eight engines. This cohort may differ from earlier five-parser results; cross-run ratios are not causal speed estimates.",
        'The input and engine order use a saved random seed. Each subprocess performs one cold parse followed by five warm samples averaging 1–10 parses. Input reads, process startup, imports, explicit destruction, comparison normalization, hashing and output serialization are excluded. Timing jobs run sequentially on a shared development machine after other owned heavy jobs finish. Background activity and load averages are retained; this is not an isolated laboratory measurement.',
        'Rust-pptx includes renderer-facing model construction and generated SVG artwork. Office Oxide includes IR projection. Python-pptx includes eager traversal of its lazy model; MarkItDown uses its dedicated PPTX converter without OCR or LLM calls. These APIs perform different amounts of work. The complete-output speed target remains unmet; the workload difference is not a claim of superior speed.',
        'pptxboss-core 2.3.0 is set to one thread. Timed work materializes slide content, notes, comments, charts, diagrams, hyperlink targets, image bytes, core/application properties and declared layout/master parts through its public APIs. Lazy content reads stay inside timing; comparison normalization and payload hashing stay outside. Failed subparts and unknown content diagnostics are retained. A successful top-level return is not proof of complete content.',
        'BetterOffice 0.5.1 uses its full typed package API. Its parsed media bytes are hashed outside timing. rustypptx 0.2.0 uses its byte API with a one-thread Rayon pool initialized outside timing. Its skipped slide content stays missing in source scores. No adapter reconstructs omitted facts from the independent reference. Peer output digests cover serialized comparison artifacts, not a complete native-model equivalence proof.',
        'Feature recall measures source extraction, not rendered fidelity or all PowerPoint capabilities. Macro text F1 is averaged over documents with source body text; it is not an average over feature rows. Literal text references can penalize legitimate inherited, equation, chart or diagram text additions. Image occurrence scores retain the selected-SVG versus fallback-payload distinction. Gray cells identify dimensions not scored by an adapter; they are not 100% support.',
        'The frozen source reference visits the first Markup Compatibility branch and the primary picture blip. It is not an application-compatibility oracle. Inspected text omissions in the new structured peers include text within an a14 Choice branch; selecting a raster fallback can preserve appearance while losing extractable text. Rust-pptx retains both the preferred SVG and saved fallback payload for its 113 previously reviewed image-score mismatches, without changing the raw score. Sparse-chart scores also retain omitted interior and trailing absent points.',
        'Adding BetterOffice removes one input from the original five-parser 611-input common timing cohort: it rejects a package with a missing referenced slide layout. That input remains in the 628-file source accuracy denominator. Source-reference definitions and failure penalties remain fixed. Build-specific regression receipts distinguish exact output preservation from reviewed semantic changes.',
        'This benchmark cohort was frozen within a collection of 1,819 distinct files, including legacy and alternative presentation containers, with 317 reserved inputs. The headline comparison covers OOXML tuning inputs; legacy agreement and the additional Strict-OOXML SDK study remain separate. Matching DOCX/PPTX README matrices are prepared locally; publication remains pending final optimization and validation.',
    ]
    optimization = OUT / 'experiments/xml-recent1/decision.json'
    if optimization.exists():
        decision = json.loads(optimization.read_text())
        if decision['candidate_sha256'] == data['identity']['binary_sha256'] and decision['status'] == 'retained':
            timing = decision['full_cohort']
            low, high = timing['median_speedup_ci95']
            method.append(f"This measured build retains the recent-XML cache lookup optimization. A separate paired run on all {timing['inputs']} common tuning inputs gives a {timing['median_paired_speedup']:.3f}× median speed ratio against picture-fallback2 (95% document-bootstrap interval {low:.3f}–{high:.3f}×); summed per-input medians fall {decision['summed_latency_reduction']:.2%}. The preliminary 40-input pilot was inconclusive and remains archived. These before/after results are separate from the fresh eight-parser measurements above and are not untouched-validation estimates.")
            method.append(f"The optimization preserves exact complete native output/error behavior on {decision['native_exact']:,} established and {decision['sdk_exact']} SDK inputs. All {decision['existing_rust_checks']} existing Rust checks and {decision['cache_snapshot_comparisons']} focused cache comparisons pass, including same-address mutation, collision checks, eviction and nested scope restoration. All {decision['wasm_regression_exact']} WASM outputs/errors remain unchanged relative to the preceding WASM build. Native/WASM acceptance matches, with the same single curved-3D numerical difference still failing the strict artwork comparison. No source extraction scores change.")
    modern_path = OUT / 'experiments/modern-comments2/decision.json'
    if modern_path.exists():
        modern = json.loads(modern_path.read_text())
        if modern['status'] == 'retained' and modern['candidate_sha256'] == data['identity']['binary_sha256']:
            review = modern['native_review']
            method.append(f"This build adds typed modern comment threads, ordered replies, rich text and comment-part hyperlinks, author identities, status/task properties, anchor moniker paths and physical-unit positions. The independent full-model audit passes {review['passed']:,} inputs: {review['exact']:,} outputs/errors remain exact and {review['changed']} change only in source-verified modern comments (five threads and two replies). All 156 SDK outputs/errors remain exact. The headline feature counts remain unchanged; modern thread metadata is supplemental coverage, not a newly credited headline score.")
            method.append('All 53 existing Rust checks, React model/viewer typechecks and 19 focused native/WASM probes pass. The 687-input prior-WASM regression is exact except one native-matched comment model. All four real modern-comment presentations match native/WASM compact models and artwork, including three outside that prior cohort. The known curved-3D native/WASM numerical failure remains open. Modern positions retain source strings and normalized EMUs; consumer anchor resolution, comment UI and typed mention/reaction/task-extension semantics remain incomplete.')
            timing = modern['timing']; low, high = timing['median_speedup_ci95']
            cost = modern['modern_cohort_timing']
            method.append(f"The modern-comment change is retained for accuracy. A separate 40-input tuning pilot gives a {timing['median_paired_speedup']:.3f}× median ratio against xml-recent1 (95% interval {low:.3f}–{high:.3f}×), but contains none of the four modern-comment inputs. Their separate summed warm medians change from {cost['before_ms']:.2f} to {cost['after_ms']:.2f} ms. These are separate scoped studies, not a universal speed gain. The earlier retained cache optimization was independently measured at 1.018× on all 610 common tuning inputs; ratios are not multiplied.")
    indexed_path = OUT / 'experiments/xml-indexed1/decision.json'
    indexed = json.loads(indexed_path.read_text()) if indexed_path.exists() else None
    indexed_current = bool(indexed and indexed['status'] == 'retained'
                           and indexed['candidate_sha256'] == data['identity']['binary_sha256'])
    table_path = OUT / 'experiments/table-grid1/decision.json'
    if table_path.exists():
        table_change = json.loads(table_path.read_text())
        if table_change['status'] == 'retained' and (table_change['candidate_sha256'] == data['identity']['binary_sha256']
                or indexed_current and table_change['candidate_sha256'] == indexed['baseline_sha256']):
            review = table_change['native_review']
            counts = review['source_counts']
            method.append(f"This build retains every source table cell, including covered cells within merged regions, with explicit merge flags and a resolved origin when available. Styling uses source grid columns, correcting band and edge selection after horizontal merges. The {review['total']:,}-input full-model review has {review['exact']:,} exact outputs/errors and {review['changed']} independently reviewed table changes; all 156 SDK outputs/errors remain exact. Changed models retain {counts['covered_cells']} covered cells, including {counts['covered_nonempty_cells']} with nonempty text. All non-table-row model values and payloads remain exact.")
            method.append('The unchanged literal-source reference includes covered-cell contents. Recovered text increases extraction recall, but is not additional visible slide text. React rendering, visible-text search and slide-text collection exclude covered cells; ten native-derived browser cases verify that behavior, spans, column widths and band/edge colors. Ten focused native/WASM cases and all 32 existing rust-pptx and 102 React checks pass. The table contract and supplemental audit do not establish full PowerPoint rendering parity.')
            method.append('A separate source adjudication attributes the remaining 30 unmatched body-text five-grams to typed equation insertions in two presentations: the literal reference omits OMML while the parser retains it inline. Removing only typed math runs from temporary diagnostic copies accounts for those differences. Production output, the independent reference and the displayed scores are unchanged; this does not establish mathematical or rendered fidelity.')
            timing = table_change['timing']; low, high = timing['median_speedup_ci95']
            cost = table_change['table_cohort_timing']
            method.append(f"The table change is retained for fidelity. A separate {timing['inputs']}-input tuning pilot gives a {timing['median_paired_speedup']:.3f}× median speed ratio against modern-comments2 (95% interval {low:.3f}–{high:.3f}×). Across all {cost['inputs']} changed table inputs, summed warm medians change from {cost['before_ms']:.2f} to {cost['after_ms']:.2f} ms. These paired studies are separate from the eight-parser measurements. The preceding modern-comment implementation and cache optimization remain included; their prior speed ratios are not multiplied. The known strict native/WASM curved-3D numerical mismatch remains open.")
    if indexed_current:
        timing = indexed['timing']; low, high = timing['median_speedup_ci95']
        method.append(f"The current cache optimization stores owned XML entries in an indexed vector and checks four recent entries with full content equality before hashing. Its predeclared {timing['inputs']}-input paired follow-up measures a {timing['median_paired_speedup']:.3f}× median speed ratio against table-grid1 (95% document-bootstrap interval {low:.3f}–{high:.3f}×); summed per-input medians fall {indexed['summed_latency_reduction']:.2%}. The 40-input pilot was inconclusive. The candidate beats Office Oxide on {timing['candidate_wins_vs_oxide']}/{timing['inputs']} inputs in that paired study. This is tuning evidence, separate from the fresh eight-parser run; prior speed ratios are not multiplied.")
        method.append(f"Complete native outputs/errors remain exact on {indexed['native_exact']:,} established and {indexed['sdk_exact']} SDK inputs. All {indexed['workspace_checks']['passed']} existing workspace checks and {indexed['cache_snapshot_comparisons']} focused cache comparisons pass. The actual WASM build is unchanged on {indexed['wasm_regression_exact']} inputs, including {indexed['wasm_artwork_hashes']:,} generated-artwork hashes. Strict native/WASM compact comparison remains 648/649 successful models, with the previously reported curved-3D difference unchanged. Three preselected profiling inputs show cold-process peak-RSS ratios of 1.003, 0.992 and 1.001; these limited resource diagnostics do not establish a corpus-wide memory bound.")
        corpus_path = OUT / 'corpus-followup/ppt4web/summary.json'
        if corpus_path.exists():
            corpus = json.loads(corpus_path.read_text())
            method.append(f"After timed measurement finished, a pinned public metadata sample added {corpus['new_unique']} distinct original presentations: 24 legacy PPT and 11 PPTX. The combined collection now contains {corpus['combined_unique']:,} files with {corpus['combined_reserved']} reserved. Five selected files exceeded the 16 MiB per-file cap and were not downloaded. Only headers and container directories were classified; these additions have not entered parser evaluation or changed the frozen comparison denominators. Converted copies in bulk archives were not downloaded.")
    editing_path = OUT / 'experiments/editing-namespaces1/decision.json'
    if editing_path.exists():
        editing = json.loads(editing_path.read_text())
        if editing['status'] == 'retained' and editing['candidate_sha256'] == data['identity']['binary_sha256']:
            timing = editing['full_cohort']; low, high = timing['median_speedup_ci95']
            cost = editing['complete_feature_cost']; cost_low, cost_high = cost['median_speedup_ci95']
            method.append('This measured build includes typed object locks, connector endpoint IDs/sites, explicit boolean properties and complete qualified source XML. Independent source review covers 1,312 inputs, with 911 editing-only changes and 401 exact previous outputs/errors. All preexisting model values, payloads and artwork remain exact. These additional fields are supplemental coverage; the feature matrix has no newly credited editing row. React exposes matching types and source-part-aware endpoint lookup; geometry routing and editor lock enforcement remain outside this contract.')
            method.append(f"Two output-identical optimizations retain parsed editing metadata in a bounded per-parse cache and reuse serialized namespace declarations until their XML scope changes. The latter has a separate {timing['inputs']}-input paired speed ratio of {timing['median_paired_speedup']:.3f}× versus object-editing-cache1 (95% document-bootstrap interval {low:.3f}–{high:.3f}×), with summed per-input medians reduced by {editing['summed_latency_reduction']:.2%}. A separate direct comparison of the complete richer contract against the pre-feature xml-indexed1 build gives {cost['median_paired_speedup']:.3f}× ({cost_low:.3f}–{cost_high:.3f}×). These are tuning measurements, not multiplied ratios, and are distinct from the fresh eight-parser run.")
            method.append('Complete native outputs/errors remain exact on 1,156 established and 156 SDK inputs. The actual WASM build preserves all previous models/errors and artwork/editing XML hashes on 711 inputs, plus 23 property and 16 namespace-scope probes. All 32 existing Rust checks pass. The prior strict native/WASM curved-3D numerical difference remains open; this regression evidence does not establish native rendering parity. Modern comments, covered table cells, preferred SVG with fallback payloads and typed equations remain included. Headline image and equation-reference discrepancies stay scored.')
            method.append('The collected corpus contains 1,854 unique documents, including legacy PowerPoint and other presentation containers, with 323 reserved. The latest 35 public original presentations have only container/header classification and do not enter this frozen OOXML comparison. Broader legacy fidelity, remaining specification/consumer gaps and untouched validation remain open.')
    replay_path = OUT / phase / 'undoc-replay-summary.json'
    if replay_path.exists():
        replay = json.loads(replay_path.read_text())
        method.append(f"Output verification found table-block ordering changes in {replay['inputs']} undoc results. All {replay['variable_inputs']} varied again in {replay['invocations']} diagnostic replays of the same executable; complete table contents and other fields remained unchanged. Its headline feature scores are identical to the preceding phase. This observation is retained as parser-output variability, without sorting or altering measured output or claiming extra timing evidence.")
    markdown += ['', *[text + '\n' for text in method], '[Feature matrix](pptx-feature-recall.md) · [Raw diagnostics](diagnostics.json) · [Interactive report](index.html)', '']
    (site / 'RESULTS.md').write_text('\n'.join(markdown))
    page = '<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Eight-parser PowerPoint comparison</title><style>body{max-width:1420px;margin:32px auto;padding:0 24px;font:16px/1.55 system-ui;color:#162236;background:#fafbfc}h1{font-size:32px}.scroll{overflow:auto}table{border-collapse:collapse;width:100%;min-width:900px}th,td{padding:9px 12px;text-align:right;border-bottom:1px solid #dce2eb}th:first-child{text-align:left}img{max-width:100%}pre{white-space:pre-wrap;overflow-wrap:anywhere}.controls{display:flex;flex-wrap:wrap;gap:16px;margin:20px 0}select{font:inherit;padding:7px;margin-left:7px}.legend{display:flex;gap:20px;flex-wrap:wrap}.legend span:before{content:"●";color:var(--color);margin-right:6px}svg{width:100%;height:auto;background:white}svg text{font:12px system-ui;fill:#536881}</style><a href="../expanded.html">Previous five-parser report and optimization audits</a><h1>Eight-parser PowerPoint comparison</h1>'
    page += '<p>' + html.escape(scope) + '</p><p>' + html.escape(method[0]) + '</p><div class="scroll"><table><thead><tr>' + ''.join('<th>' + item.strip() + '</th>' for item in heading.strip('|').split('|')) + '</tr></thead><tbody>' + ''.join(table) + '</tbody></table></div><img src="latency.png" alt="Median and p95 parsing latency on the common cohort">'
    template = Path(__file__).with_name('report.html').read_text()
    controls = template[template.index('<h2>Size, complexity and latency</h2>'):template.index('<h2>Feature recall</h2>')]
    controls = controls.replace('<option>markitdown</option>', '<option>markitdown</option>' + ''.join('<option>' + e + '</option>' for e in PEER_ENGINES))
    compact = [dict(id=r['id'], engine=r['engine'], bytes=r['bytes'], slides=r['slides'], paragraphs=r['paragraphs'], features=r['features'], characters=r['characters'], latency=r['median_ms'], recall=r['metrics']['body_text']['recall'] * 100, f1=r['metrics']['body_text']['f1'] * 100) for r in data['rows'] if r['id'] in data['common_ids']]
    script = template[template.index('<script>'):template.index('</script>') + len('</script>')]
    script = script.replace('__DATA__', json.dumps(compact))
    script, replacements = re.subn(r"engines=\[.*?\],colors=\[.*?\];", 'engines=' + json.dumps(engines) + ',colors=' + json.dumps(colors) + ';', script)
    assert replacements == 1
    page += controls + '<h2>Source feature recall</h2><img src="pptx-feature-recall.png" alt="Eight-parser feature recall matrix with expected counts and explicit unscored cells"><p><a href="pptx-feature-recall.csv">CSV</a> · <a href="pptx-feature-recall.json">Counts and source identity</a></p><div class="scroll"><table><thead><tr><th>Feature</th><th>Expected units</th>' + ''.join('<th>' + html.escape(label) + '</th>' for label in labels) + '</tr></thead><tbody>'
    for row in receipt['rows']:
        page += '<tr><th>' + html.escape(row['label']) + f"</th><td>{row['expected']:,}</td>" + ''.join('<td>' + ('Not scored' if cell['recall'] is None else f"{cell['recall']:.3%}") + '</td>' for cell in row['cells']) + '</tr>'
    page += '</tbody></table></div><h2>Scope and method</h2>' + ''.join('<p>' + html.escape(text) + '</p>' for text in method[1:])
    page += '<h2>Partial-output diagnostics</h2><p>These diagnostics are supplemental. Shorter-slide counts cover accepted reference-bearing inputs; pptxboss reported diagnostics cover all attempted inputs. A parser can return successfully with missing or unsupported content. Source scores retain those omissions; an empty diagnostic list does not establish completeness.</p><pre>' + html.escape(json.dumps(diagnostics, indent=2)) + '</pre>'
    page += '<p>Primary APIs: <a href="https://docs.rs/pptxboss-core/2.3.0/pptxboss_core/document/struct.Document.html">pptxboss-core 2.3.0</a> · <a href="https://docs.rs/betteroffice-pptx-parse/0.5.1/pptx_parse/">BetterOffice 0.5.1</a> · <a href="https://docs.rs/rustypptx/0.2.0/rustypptx/">rustypptx 0.2.0</a></p>'
    page += '<details><summary>Measurement identity</summary><pre>' + html.escape(json.dumps(data['identity'], indent=2)) + '</pre></details>' + script + '</html>'
    (site / 'index.html').write_text(page)
    generated_names = ['index.html', 'latency.png', 'RESULTS.md', 'diagnostics.json', *['pptx-feature-recall.' + suffix for suffix in ['md', 'csv', 'json', 'png', 'svg']]]
    generation = dict(phase=phase, assessment_sha256=hashlib.sha256(source.read_bytes()).hexdigest(), generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), heatmap_generator_sha256=receipt['generator_sha256'], common_n=len(data['common_ids']), engines=engines, diagnostics=diagnostics, files={name:hashlib.sha256((site / name).read_bytes()).hexdigest() for name in generated_names})
    (site / 'generation.json').write_text(json.dumps(generation, indent=2))
    print(json.dumps(dict(report=str(site / 'index.html'), common_n=len(data['common_ids']), engines=len(engines))))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('phase', nargs='?', default='expanded-peers')
    main(parser.parse_args().phase)

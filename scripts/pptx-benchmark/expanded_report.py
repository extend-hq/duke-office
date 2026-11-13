import html
from datetime import datetime, timezone
import json
from pathlib import Path
import sys
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from report import ENGINES, LABELS, COLORS, FEATURES, ns, pct

OUT = Path(__file__).resolve().parents[2] / 'output/pptx-benchmark-20261106'
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'parser-benchmark'))
from feature_heatmaps import matrix, PPTX


def main(phase):
    data = json.loads((OUT / phase / 'assessment.json').read_text())
    summary = data['summary']; site = OUT / 'report'
    resource_gate_path = OUT / 'experiments/xml-memory3/semantic-summary.json'
    resource_gate = json.loads(resource_gate_path.read_text()) if resource_gate_path.exists() else None
    group_path = OUT / 'experiments/chart-group1/semantic-summary.json'
    group_gate = json.loads(group_path.read_text()) if group_path.exists() else None
    cache_descendant = False
    cache_build = None
    cache_path = OUT / 'experiments/shape-cache1/parity-summary.json'
    if group_gate and cache_path.exists():
        cache = json.loads(cache_path.read_text())
        notes = json.loads((OUT / 'experiments/legacy-notes1/legacy-features-summary.json').read_text())
        math = json.loads((OUT / 'experiments/portable-math2/artwork-reviewed-summary.json').read_text())
        valid = (cache['equal'] == cache['total'] and notes['passed'] == notes['total'] and math['passed'] == math['total'] and cache['baseline_sha256'] == notes['candidate_sha256'] and notes['baseline_sha256'] == math['candidate_sha256'] and math['baseline_sha256'] == group_gate['candidate_sha256'])
        ancestry = [('shape-cache1', cache)] if valid else []
        for label, filename in [('portable-math3', 'artwork-reviewed-summary.json'), ('xml-cache2', 'parity-summary.json'), ('xml-scan2', 'parity-summary.json'), ('xml-bom1', 'bom-source-reviewed-summary.json'), ('xml-attributes2', 'parity-summary.json'), ('legacy-directory1', 'directory-reviewed-summary.json'), ('comments2', 'comments-reviewed-summary.json'), ('comments3', 'parity-summary.json'), ('relationships2', 'parity-summary.json'), ('fields1', 'fields-reviewed-summary.json'), ('fields2', 'parity-summary.json'), ('image-fallback1', 'image-fallback-reviewed-summary.json'), ('svg-namespace2', 'parity-summary.json'), ('picture-fallback2', 'picture-fallback-reviewed-summary.json')]:
            path = OUT / 'experiments' / label / filename
            if not ancestry or not path.exists():
                break
            proof = json.loads(path.read_text())
            if proof.get('passed', proof.get('equal')) != proof['total'] or proof['total'] < ancestry[-1][1]['total'] or proof['baseline_sha256'] != ancestry[-1][1]['candidate_sha256']:
                break
            ancestry.append((label, proof))
        cache_build = next((label for label, proof in ancestry if proof['candidate_sha256'] == data['identity']['binary_sha256']), None)
        cache_descendant = cache_build is not None
    if group_gate and ((group_gate['candidate_sha256'] != data['identity']['binary_sha256'] and not cache_descendant) or group_gate['passed'] != group_gate['total']):
        group_gate = None
    memory_descendant = False
    if group_gate and resource_gate:
        bounds_gate = json.loads((OUT / 'experiments/chart-bounds1/parity-summary.json').read_text())
        memory_descendant = bounds_gate['equal'] == bounds_gate['total'] and bounds_gate['candidate_sha256'] == group_gate['baseline_sha256'] and bounds_gate['baseline_sha256'] == resource_gate['candidate_sha256']
    if resource_gate and ((resource_gate['candidate_sha256'] != data['identity']['binary_sha256'] and not memory_descendant) or resource_gate['passed'] != resource_gate['total']):
        resource_gate = None
    labels = ['duke-slides 0.1.5 + local changes', *LABELS[1:]]
    scope = f"{summary['duke-slides']['reference_total']} reference packages · {phase} · measured " + datetime.fromtimestamp(data['identity']['start'], timezone.utc).strftime('%Y-%m-%d')
    matrix('pptx', OUT / phase / 'assessment.json', site, PPTX, ENGINES, ['duke-slides\n0.1.5 + local', *[label.rsplit(' ', 1)[0] + '\n' + label.rsplit(' ', 1)[1] for label in LABELS[1:]]], summary, scope, {**data['identity'], 'display_scope': scope}, snapshot=True)
    rows = []
    for engine, label in zip(ENGINES, labels):
        row = summary[engine]; features = row['features']
        rows.append(f'<tr><th>{label}</th><td>{row["reference_success"]}/{row["reference_total"]}</td><td>{row["median_ms"]:.3f}</td><td>{row["p95_ms"]:.3f}</td><td>{pct(features["body_text"]["recall"])}</td><td>{pct(features["body_text"]["macro_f1"])}</td></tr>')
    feature_rows = []
    for key, label in FEATURES:
        units = summary['duke-slides']['features'].get(key, {}).get('expected', 0)
        if not units: continue
        cells = ''.join('<td>Not scored</td>' if ns(engine, key) else '<td>' + pct(summary[engine]['features'][key]['recall']) + '</td>' for engine in ENGINES)
        feature_rows.append(f'<tr><th>{label}</th><td>{units:,}</td>{cells}</tr>')
    fig, ax = plt.subplots(figsize=(10, 4.5))
    for index, (engine, label, color) in enumerate(zip(ENGINES, labels, COLORS)):
        row = summary[engine]
        ax.barh(index - .17, row['median_ms'], height=.3, color=color)
        ax.barh(index + .17, row['p95_ms'], height=.3, color=color, alpha=.35)
        ax.text(row['median_ms'], index - .17, f' {row["median_ms"]:.3f}', va='center')
        ax.text(row['p95_ms'], index + .17, f' {row["p95_ms"]:.3f}', va='center')
    ax.set_yticks(range(5), labels); ax.invert_yaxis(); ax.set_xlabel('Milliseconds · median (solid) and p95 (faint)')
    ax.set_title(f'Expanded tuning corpus · {len(data["common_ids"])} common nonempty inputs', loc='left')
    ax.set_xlim(0, max(summary[e]['p95_ms'] for e in ENGINES) * 1.2); fig.tight_layout(); fig.savefig(site / 'expanded-latency.png', dpi=180); plt.close(fig)
    page = '<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Expanded PowerPoint benchmark</title><style>body{margin:32px auto;padding:0 24px;max-width:1200px;font:16px/1.55 system-ui;color:#162236;background:#fafbfc}h1{font-size:32px}table{border-collapse:collapse;min-width:800px;width:100%}th,td{padding:9px 12px;text-align:right;border-bottom:1px solid #dce2eb}th:first-child{text-align:left}.scroll{overflow:auto}img{max-width:100%}details{margin:24px 0}pre{white-space:pre-wrap;overflow-wrap:anywhere}</style><a href="index.html">Original cohort and full audit</a><h1>Expanded PowerPoint benchmark</h1>'
    reserved_main = data['identity'].get('reserved_validation_inputs', 206)
    page += f'<p>Phase <strong>{html.escape(phase)}</strong>. New tuning inputs only; the {reserved_main}-file validation set in the source manifest remains unused. The five engines receive identical bytes. Accuracy uses an independent ZIP/XML reference; failures on reference-bearing files count as zero. Timings use the {len(data["common_ids"])} files accepted by every engine, with zero-slide decks excluded. These are heterogeneous regression and archive inputs, not a production prevalence sample.</p>'
    page += '<div class="scroll"><table><thead><tr><th>Parser</th><th>Accepted/reference</th><th>Median ms</th><th>p95 ms</th><th>Text recall</th><th>Macro text F1</th></tr></thead><tbody>' + ''.join(rows) + '</tbody></table></div><img src="expanded-latency.png" alt="Latency on the expanded common cohort">'
    page += '<p>F1 uses literal source text and can penalize legitimate inherited, chart or SmartArt additions. Neither recall nor successful parsing establishes pixel fidelity. Macro-enabled and template packages remain in accuracy denominators even if a peer rejects their content type. Unsupported reference namespaces and malformed packages are recorded separately; they do not silently contribute empty references.</p>'
    experiment_rows = []
    for label, suffix in [('opt7', ''), ('opt8', ''), ('timing2', ''), ('timing3', ''), ('timing3', '-vs-timing2'), ('xml-memory1', '-vs-timing3'), ('xml-memory2', ''), ('zip1', ''), ('zip2', ''), ('portable-math2', ''), ('shape-cache1-pilot', ''), ('portable-math3', ''), ('xml-cache2', ''), ('xml-scan2', ''), ('relationship-cache1', ''), ('xml-attributes2', ''), ('xml-inline1', ''), ('relationships2', ''), ('fields2', ''), ('fields3', ''), ('image-fallback1', ''), ('namespace-cache1', ''), ('svg-namespace2', ''), ('picture-fallback2', '')]:
        timing_path = OUT / 'experiments' / label / ('expanded-timing' + suffix + '-summary.json')
        if not timing_path.exists():
            continue
        timing = json.loads(timing_path.read_text())
        interval = timing['median_speedup_ci95']
        parity_path = timing_path.with_name('parity-summary.json')
        parity = json.loads(parity_path.read_text()) if parity_path.exists() else {}
        baseline_label = timing.get('baseline_label') or ('opt8 (before timing support)' if timing.get('semantic_reference_label') else Path(parity['baseline_path']).parent.name)
        selection = ' (not retained)' if label in {'zip1', 'zip2', 'relationship-cache1', 'xml-inline1', 'fields3', 'namespace-cache1'} else ''
        experiment_rows.append(f'<tr><th>{label}{selection} vs {baseline_label}</th><td>{timing["inputs"]}</td><td>{timing["median_paired_speedup"]:.3f}×</td><td>{interval[0]:.3f}–{interval[1]:.3f}×</td><td>{timing["candidate_wins_vs_oxide"]}/{timing["inputs"]}</td></tr>')
    if experiment_rows:
        page += '<h2>Paired performance experiments</h2><p>These interleaved experiments are separate from the full five-parser corpus run above. Ratios are relative to each named reference and must not be multiplied. An interval crossing 1 does not establish a reliable speed improvement. All selected inputs are tuning data. Rows against opt8 measure added extraction work. The XML storage experiment preserves exact output; the SVG experiment permits only removal of unused definitions and does not establish a reliable speed improvement (interval 0.99996–1.02399). Ratios below 1 mean the candidate is slower.</p><div class="scroll"><table><thead><tr><th>Candidate / reference</th><th>Inputs</th><th>Median paired speedup</th><th>95% bootstrap interval</th><th>Faster than Oxide</th></tr></thead><tbody>' + ''.join(experiment_rows) + '</tbody></table></div>'
    namespace_trial = OUT / 'experiments/namespace-cache1/decision.json'
    if namespace_trial.exists():
        decision = json.loads(namespace_trial.read_text())
        timing = decision['field_heavy']; interval = timing['median_speedup_ci95']
        page += f'<p>The namespace-cache trial also used a predeclared subset of all {timing["inputs"]} common tuning inputs with at least 50 independently reviewed emitted fields. Its paired median speed ratio was {timing["median_paired_speedup"]:.3f}× ({interval[0]:.3f}–{interval[1]:.3f}×). Neither cohort establishes a gain, so the production change was reverted. Full outputs/errors remained exact on 1,156 established and 156 additional SDK inputs; all 16 focused native/WASM scope checks and 392 existing Rust checks passed. The prior cross-platform numerical difference remains open.</p>'
    svg_namespace_path = OUT / 'experiments/svg-namespace2/validation.json'
    if svg_namespace_path.exists():
        qualified = json.loads(svg_namespace_path.read_text())
        if qualified['baseline_sha256'] == data['identity']['binary_sha256']:
            page += f'<h2>Subsequent qualified SVG selection</h2><p>The retained svg-namespace2 build checks the SVG element namespace, the embedded relationship attribute namespace and the DrawingML extension-container path. Foreign local-name collisions are ignored, and unusable earlier entries do not hide a later eligible embedded image. All {qualified["established_exact"]:,} established and {qualified["sdk_exact"]} SDK full outputs/errors remain exact against the measured build above, preserving its source feature scores.</p>'
            page += f'<p>All {qualified["namespace_probes"]} focused cases pass, correcting {qualified["corrected_selections"]} prior selections. Native and WASM models and artwork match on those cases and all 17 existing image probes. The unchanged React viewer displays the selected colors and ellipse clipping in {qualified["browser_generated_artwork_cases"]} generated-artwork browser cases. Two malformed direct-child cases retain a raster and an explicit unsupported-appearance warning. All {qualified["existing_rust_checks"]} existing Rust checks pass; the broader cross-platform cohort retains one known curved-3D numerical difference.</p>'
            page += '<p>The paired interval above establishes no median speed change. Summed per-input medians increase from 494.958 to 512.871 ms. This later build has not replaced the frozen full five-parser timing or heatmaps above. Embedded-image decode-failure recovery inside generated artwork remains incomplete; these checks do not establish native PowerPoint pixel fidelity. <a href="../experiments/svg-namespace2/validation.json">Validation identity and receipts</a>.</p>'
    picture_path = OUT / 'experiments/picture-fallback2/validation.json'
    if picture_path.exists():
        picture = json.loads(picture_path.read_text())
        if picture['candidate_sha256'] == data['identity']['binary_sha256'] and picture['status'] == 'retained':
            page += f'<h2>Generated picture fallback</h2><p>The measured build adds optional compatibility artwork with its own paint bounds. Preferred artwork is retained. The independent review passes {picture["established_reviewed"]:,} corpus inputs: {picture["established_exact"]:,} complete outputs/errors remain exact and seven outputs add {picture["fallback_pictures"]} generated compatibility pictures. Every new SVG and its bounds exactly match the frozen prior parser on the same package with only qualified SVG extension elements removed. All {picture["sdk_exact"]} SDK tuning outputs remain exact.</p>'
            page += f'<p>Thirty native/WASM probes match exactly. Thirty unmodified native models pass Chromium artwork and layout checks, eight runtime checks cover bounded decoding and mount lifetime, and seven direct-image checks still pass. The viewer checks nested image resources before choosing an outer SVG, since the outer SVG can decode while its embedded picture is blank. It tries the saved compatibility artwork with the original crop, geometry, transform and effects. All {picture["existing_rust_checks"]} targeted existing Rust checks and 100 existing viewer checks pass.</p>'
            page += '<p>The earlier qualified SVG selection checks remain part of this build. The cross-platform corpus retains one known curved-3D numerical difference. These checks do not establish native PowerPoint visual fidelity or complete image codec support. The source-literal image metric still scores the selected representation, so its historical SVG/main-blip mismatches and all comparison denominators remain unchanged. <a href="../experiments/picture-fallback2/validation.json">Validation identity and receipts</a>.</p>'
    legacy_path = OUT / 'legacy-poi-notes1/summary.json'
    if legacy_path.exists():
        legacy = json.loads(legacy_path.read_text())
        previous = json.loads((OUT / 'legacy-poi-comparison/summary.json').read_text())
        legacy_rows = []
        for label, result in [('duke-slides, legacy-notes1', legacy['engines']['duke-slides']), ('duke-slides, before', previous['engines']['duke-slides']), ('Office Oxide', previous['engines']['office_oxide']), ('undoc', previous['engines']['undoc'])]:
            body, notes = [result['metrics'][key] for key in ['body_text', 'notes']]
            legacy_rows.append(f'<tr><th>{label}</th><td>{result["accepted"]}/{legacy["inputs"]}</td><td>{body["recall"]:.3%}</td><td>{body["f1"]:.3%}</td><td>{notes["recall"]:.3%}</td></tr>')
        page += '<h2>Legacy PPT content comparison</h2><p>This separate 328-input cohort compares content against Apache POI 6.0.0 on its 288 accepted inputs. Parser failures count as empty output. The current legacy candidate adds live speaker notes and dual-storage support; the peer runs and POI reference remain frozen. These are agreement measurements, not rendering fidelity or latency rankings. POI includes some repeated notes-page headers and footers; the denominator is unchanged.</p><div class="scroll"><table><thead><tr><th>Parser / build</th><th>Accepted</th><th>Body recall</th><th>Body F1</th><th>Notes recall</th></tr></thead><tbody>' + ''.join(legacy_rows) + '</tbody></table></div><p><a href="LEGACY-POI.md">Legacy method and raw results</a> · <a href="../legacy-poi-notes1/summary.json">Current build identity and metrics</a></p>'
    fixed_path = OUT / phase / 'assessment-fixed.json'
    directory_path = OUT / 'experiments/legacy-directory1/directory-reviewed-summary.json'
    if directory_path.exists():
        directory = json.loads(directory_path.read_text())
        directory_poi = json.loads((OUT / 'corpus-aspose/poi-directory1/summary.json').read_text())
        page += f'<h2>Additional legacy container recovery</h2><p>A separately validated legacy-directory1 build passes {directory["passed"]}/{directory["total"]} native checks: {directory["exact"]} exact outputs/errors and one recovered legacy presentation. Two unreachable all-FF directory slots are normalized; all five live streams remain byte-identical. The full recovered model matches the frozen parser on an independently canonicalized container, with one explicit recovery warning. This narrow recovery supports header-contained FAT tables and rejects cycles, invalid live links and sector aliasing. All 32 focused version-3/version-4 container probes pass in native and WASM.</p>'
        body = directory_poi['engines']['duke-slides']['metrics']['body_text']
        page += f'<p>On the separate six-input OLE tuning subset, Rust and Office Oxide accept all five POI-readable inputs and match all {body["expected"]} body-text five-grams; undoc accepts four. The sixth input is encrypted. These are content-agreement checks, not a latency measurement or rendering-fidelity result. The timing tables above still identify their frozen measured build.</p>'
    if fixed_path.exists():
        fixed = json.loads(fixed_path.read_text())
        fixed_rows = ''.join(f'<tr><th>{label}</th><td>{fixed["summary"][engine]["median_ms"]:.3f}</td><td>{fixed["summary"][engine]["p95_ms"]:.3f}</td></tr>' for engine, label in zip(ENGINES, labels))
        page += f'<h2>Fixed previous timing cohort</h2><p>The same fresh measurements restricted to the preceding {len(fixed["common_ids"])}-input cohort. Newly accepted files remain in all accuracy denominators and in the full common-cohort table above.</p><div class="scroll"><table><thead><tr><th>Parser</th><th>Median ms</th><th>p95 ms</th></tr></thead><tbody>{fixed_rows}</tbody></table></div>'
    if resource_gate:
        svg_gate = json.loads((OUT / 'experiments/xml-memory2/semantic-reviewed-summary.json').read_text())
        memory = json.loads((OUT / 'experiments/xml-memory/resource-summary.json').read_text())
        page += f'<h2>Memory and acceptance validation</h2><p>Compact XML storage and sequential loading of unused layouts preserve all previous output. Removing unused SVG definitions then passes all {svg_gate["passed"]} corpus checks: {svg_gate["counts"]["svg_fields_changed"]:,} SVG fields lose only unreferenced definitions, including embedded SVGs, and all other model fields remain exact. Two focused Chromium pixel comparisons match. With the default per-part limit raised to 500,000 elements, {resource_gate["exact"]} outputs/errors remain exact and {len(resource_gate["recoveries"])} previously rejected package is accepted with all scored source features recalled.</p>'
        page += f'<p>On the dense input, median peak process RSS across three standalone parses falls from {memory["baseline"]["median_peak_rss_bytes"]/1e9:.3f} GB to {memory["svg"]["median_peak_rss_bytes"]/1e9:.3f} GB ({memory["peak_rss_reduction"]*100:.1f}% lower); both measurements use the same 500,000-node configuration. This is still substantial memory, not a total-memory guarantee. The explicit boundary rejects at 325,457 nodes and accepts at 325,458. All 392 existing Rust checks pass.</p>'
    if group_gate:
        page += f'<h2>Grouped-chart geometry and new corpus</h2><p>The grouped-chart iteration passes {group_gate["passed"]}/{group_gate["total"]} native checks: {group_gate["exact"]} outputs/errors are exact, and ten grouped-chart SVGs use corrected display dimensions while every other model field stays exact. The shared 3D renderer also handles subpixel layout bounds without a panic. The actual React normalized viewer mounts the recovered chart at its expected 409.828 × 221.345 pixel frame. This is a geometry/contract check, not native PowerPoint visual fidelity.</p>'
        page += '<p>The followup corpus adds 152 unique downloads from the Library of Congress and Embrace, including 128 confirmed legacy presentations and 18 OOXML presentations; malformed/HTML/empty inputs are retained separately. The tuning subset accepts all 111 confirmed legacy and all 16 OOXML presentations. Twenty-one additional validation files remain unused. These inputs do not change the comparison denominator above.</p><p><a href="chart-group-viewer.png">React viewer geometry check</a></p>'
    if cache_descendant:
        cross = json.loads((OUT / ('wasm-check-' + cache_build) / 'comparison.json').read_text())['summary']
        open_artwork = cross['model_comparisons'] - cross['semantic_matches']
        page += f'<h2>Current build regression checks</h2><p>The shape-cache iteration preserves exact full canonical outputs and exact failure statuses/errors on {cache["equal"]}/{cache["total"]} audited inputs against the legacy-notes build. The cache is local to each parse, with an 8 MiB payload budget and 4,096-entry cap. Renders that emit diagnostics are not cached. For the selected {cache_build} build, native/WASM acceptance matches on {cross["status_matches"]}/{cross["total"]}; complete compact models and artwork hashes match on {cross["semantic_matches"]}/{cross["model_comparisons"]} successes. There are {open_artwork} numerical SVG mismatches retained as strict failures. Portable math also corrects three zero-radius brackets, independently checked against LibreOffice; native PowerPoint pixel fidelity is not established.</p>'
        if cache_build != 'shape-cache1':
            page += '<p>The subsequent portable-distance change passes 983 native regression checks: 977 outputs/errors are exact and six models have only reviewed numerical SVG changes, with unchanged paint bounds. Subsequent XML-cache and byte-scanner candidates preserve exact full outputs/errors on all 983 inputs against their preceding builds. All 392 existing Rust checks pass.</p>'
        if cache_build in {'xml-bom1', 'xml-attributes2', 'legacy-directory1', 'comments2', 'comments3', 'relationships2', 'fields1', 'fields2', 'image-fallback1', 'svg-namespace2', 'picture-fallback2'}:
            bom = json.loads((OUT / 'experiments/xml-bom1/bom-source-reviewed-summary.json').read_text())
            page += f'<p>The XML byte-order-mark correction passes {bom["passed"]}/{bom["total"]} checks, recovers {bom["recovered"]} previously rejected inputs and corrects captured source XML in {bom["changed_existing_models"]} previously accepted models. Changed outputs are compared against the frozen prior parser on copies with only the leading UTF-8 byte-order marks removed from XML members. All other model content and binary payloads must match. The raw source audit passes 1,154 inputs; a separate source-byte review resolves two chart-style/color BOM-prefix differences. The earlier raw exact-output gate retains its two expected source-capture failures.</p>'
        if cache_build in {'xml-attributes2', 'legacy-directory1', 'comments2', 'comments3', 'relationships2', 'fields1', 'fields2', 'image-fallback1', 'svg-namespace2', 'picture-fallback2'}:
            attributes = json.loads((OUT / 'experiments/xml-attributes2/parity-summary.json').read_text())
            page += f'<p>The single-pass attribute change preserves exact full outputs and errors on all {attributes["equal"]}/{attributes["total"]} audited inputs. Seven focused probes preserve attribute-count and raw-byte boundaries, including limit-error precedence over invalid entities. Near a configured limit, attributes are still counted before values are decoded or allocated. The paired 40-input experiment shows a 1.032× median speedup (95% bootstrap interval 1.025–1.051×); it is separate from the broad measurements above.</p>'
        if cache_build in {'comments2', 'comments3', 'relationships2', 'fields1', 'fields2', 'image-fallback1', 'svg-namespace2', 'picture-fallback2'}:
            comments = json.loads((OUT / 'experiments/comments2/comments-reviewed-summary.json').read_text())
            counts = comments['source_counts']
            page += f'<p>The classic-comment source audit passes {comments["passed"]}/{comments["total"]} inputs: {comments["exact"]} full outputs/errors are exact, and {comments["changed"]} models change only in comments. It independently verifies all {counts["comments"]} classic comments, including {counts["empty"]} empty comments previously omitted, their authors, modification timestamps and eighth-point positions. XML literal newlines/attribute whitespace are normalized, while character references retain their decoded values. Nonstandard paths resolve by relationship type; external targets are not fetched. Five nonclassic comment parts remain explicitly unscored by this audit. Six focused contract probes and three depth-boundary probes pass. Typed modern threads, classic threading extensions and viewer comment overlays remain incomplete.</p>'
            comment_metric = summary['duke-slides']['features']['comments']
            page += f'<p>The main source-literal comment metric matches {comment_metric["matched"]}/{comment_metric["expected"]} reference items against {comment_metric["found"]} output items. Recall is {comment_metric["recall"]:.3%} and precision is {comment_metric["precision"]:.3%}; full recall is not complete comment accuracy.</p>'
    if cache_build in {'relationships2', 'fields1', 'fields2', 'image-fallback1', 'svg-namespace2', 'picture-fallback2'}:
        packaging = json.loads((OUT / 'experiments/relationships2/parity-summary.json').read_text())
        probes = json.loads((OUT / 'experiments/relationships2/relationship-probes-summary.json').read_text())
        page += f'<h2>Package relationship resolution</h2><p>The relationship build preserves every full output and exact error on {packaging["equal"]}/{packaging["total"]} existing inputs. The main presentation and structural parts resolve by their relationship roles, including strict/transitional Office relationships and the separate modern-comment type. Relationship attributes are decoded once and reused for chart, diagram, control and comment discovery.</p>'
        page += f'<p>All {probes["passed"]}/{probes["total"]} focused checks pass in native and WASM with exact artwork: 24 package relocations preserve the entire model after reversing only part identities, and six checks exclude external or unrelated layout/notes/comment relationships. Non-relationship/non-content-type bytes are independently unchanged. These checks do not establish full OPC conformance or native PowerPoint visual fidelity. The paired timing interval crosses 1; this change is retained for correctness, without claiming a speed gain.</p>'
    if cache_build in {'fields2', 'image-fallback1', 'svg-namespace2', 'picture-fallback2'}:
        fields = json.loads((OUT / 'experiments/fields1/fields-reviewed-summary.json').read_text())
        final = json.loads((OUT / 'experiments/fields2/parity-summary.json').read_text())
        counts = fields['source_counts']
        page += f'<h2>DrawingML fields</h2><p>The independent source review passes {fields["passed"]}/{fields["total"]} inputs and checks {counts["fields"]:,} emitted field occurrences, including {counts["missing_cached"]:,} with no cached text and {counts["paragraph_properties"]:,} field paragraph-property objects. Tokens, open type strings, cache presence, cached values and namespace-qualified source subtrees are retained. After removing only new field metadata, complete models and payloads remain exact against the preceding build. The final SmartArt correction preserves all {final["equal"]}/{final["total"]} corpus outputs/errors from that source-reviewed build.</p>'
        page += '<p>Twenty focused packages pass in native and WASM, covering strict/transitional namespaces, foreign-name collisions, empty notes, run-style precedence, depth limits and both cached and semantic SmartArt fields. A focused probe caught character-reference whitespace loss in an intermediate SmartArt fragment; the corrected path preserves text and type values through reserialization. All 392 existing Rust checks and consumer model/viewer typechecks pass. This validates emitted field data, not source-wide field recall, dynamic field evaluation or complete paragraph-layout fidelity. Those dimensions are not added to the cross-parser accuracy score.</p>'
    if cache_build in {'image-fallback1', 'svg-namespace2', 'picture-fallback2'}:
        fallback = json.loads((OUT / 'experiments/image-fallback1/image-fallback-reviewed-summary.json').read_text())
        associated = json.loads((OUT / 'experiments/image-fallback1/image-fallback-gap-summary.json').read_text())
        page += f'<h2>Explicit image fallback associations</h2><p>The image-association iteration passes {fallback["passed"]}/{fallback["total"]} source-reviewed corpus checks: {fallback["exact"]} complete outputs/errors are unchanged and the remaining outputs add only fallbackAssetId. Independent qualified source-picture and inherited-placeholder checks verify {fallback["source_counts"]["associations"]} emitted associations and both byte-identical payloads. Every one of the {associated["matched"]} historical SVG/main-blip mismatches now has an explicit source-occurrence fallback link. The original image metric and all cross-parser denominators are unchanged.</p>'
        page += '<p>All 17 focused packages have exact native/WASM models and artwork. Seven browser checks verify the direct-image path, including missing or undecodable assets, preserved crop, bounded retries and stale-render handling. Generated pictureSvg artwork uses a separate path, covered by the later compatibility-artwork checks when present below. These checks establish preservation and direct fallback behavior, not complete visual fidelity.</p>'
    preservation = OUT / 'corpus-preservation/initial/summary.json'
    if preservation.exists():
        page += '<h2>Preservation format coverage</h2><p>The preservation expansion brought the corpus to 1,402 distinct inputs. Five additions from the pinned Open Preservation Foundation corpus include original PowerPoint 4 for Mac files, their migrated counterparts, and an ODF 1.3 presentation. The three tuning inputs receive acceptance-only checks: all three native parsers accept the migrated PowerPoint input and reject the original older format and ODF input. One original/migrated pair remains reserved, taking the additional reserved validation total to 229. Unsupported historical formats are not labeled invalid. These additions do not change the main accuracy denominator.</p>'
    additional_path = OUT / 'corpus-aspose/summary.json'
    if additional_path.exists():
        additional = json.loads(additional_path.read_text())
        total = 1402 + additional['new_unique']
        reserved = 229 + additional['splits'].get('validation', 0)
        page = page.replace('The preservation expansion brought the corpus to 1,402 distinct inputs.', 'The preservation extension brings the corpus to 1,402 distinct inputs.')
        additional_ids = {row['id'] for row in json.loads((OUT / 'corpus-aspose/manifest.json').read_text())}
        included = len(additional_ids & set(data['identity']['selected_ids']))
        scope = f'The main comparison includes {included} OOXML tuning inputs from this extension.' if included else 'The extension is evaluated separately from the fixed comparison above.'
        page += f'<p>A subsequent <a href="https://github.com/aspose-slides/Aspose.Slides-for-.NET/tree/73f0e15894510088edb04e60785e66ee46ff42fe">pinned public corpus</a> contributes {additional["new_unique"]} additional unique presentations after SHA-256 deduplication, bringing the combined corpus to {total:,}. It includes legacy, macro-enabled, OpenDocument and flat OpenDocument inputs. Source feature directories stay together; {additional["splits"].get("validation", 0)} new files remain reserved, bringing the additional reserved validation count to {reserved}. {scope} Download and container identification alone do not establish parser accuracy.</p>'
        after_path = OUT / 'corpus-aspose/bom1/summary.json'
        if after_path.exists():
            before = json.loads((OUT / 'corpus-aspose/initial/summary.json').read_text())
            after = json.loads(after_path.read_text())
            acceptance_rows = []
            for label, source, engine in [('duke-slides before BOM fix', before, 'duke-slides'), ('duke-slides with BOM fix', after, 'duke-slides'), ('Office Oxide', after, 'office_oxide'), ('undoc', after, 'undoc')]:
                accepted = sum(statuses.get('ok', 0) for statuses in source['engines'][engine].values())
                acceptance_rows.append(f'<tr><th>{label}</th><td>{accepted}/{source["total"]}</td></tr>')
            page += '<p>The separate acceptance-only check below includes all 173 tuning files, including unsupported formats. Seventy-nine PPTX failures in the preceding Rust build were caused by source slices using XML-reader offsets against bytes that still included a UTF-8 byte-order mark. The corrected build aligns those offsets; successful parsing alone is not an accuracy score.</p><div class="scroll"><table><thead><tr><th>Build</th><th>Accepted / attempted</th></tr></thead><tbody>' + ''.join(acceptance_rows) + '</tbody></table></div>'
    semantic_path = OUT / 'experiments/timing1/semantic-summary.json'
    current_gate = OUT / 'experiments/timing3/parity-summary.json'
    if semantic_path.exists() and current_gate.exists():
        semantic = json.loads(semantic_path.read_text()); gate = json.loads(current_gate.read_text())
        if gate['candidate_sha256'] == data['identity']['binary_sha256'] and gate['equal'] == gate['total']:
            counts = semantic['counts']
            page += f'<h2>Additional structured timing coverage</h2><p>A separate source audit passes {semantic["passed"]}/{semantic["total"]} original and expanded tuning inputs. It verifies {counts["transition"]:,} transitions and {counts["timing"]:,} timing trees with {counts["time_nodes"]:,} timing nodes across slides, layouts and masters. The final candidate preserves that complete output. These fields are available to the React model; animation playback and native visual fidelity remain unfinished. This audit is separate from the cross-library feature scores below.</p>'
    template = Path(__file__).with_name('report.html').read_text()
    controls = template[template.index('<h2>Size, complexity and latency</h2>'):template.index('<h2>Feature recall</h2>')]
    compact = [dict(id=r['id'], engine=r['engine'], bytes=r['bytes'], slides=r['slides'], paragraphs=r['paragraphs'], features=r['features'], characters=r['characters'], latency=r['median_ms'], recall=r['metrics'].get('body_text', {}).get('recall', 1) * 100, f1=r['metrics'].get('body_text', {}).get('f1', 1) * 100) for r in data['rows'] if r['id'] in data['common_ids']]
    script = template[template.index('<script>'):template.index('</script>') + len('</script>')].replace('__DATA__', json.dumps(compact))
    page += '<style>.controls{display:flex;flex-wrap:wrap;gap:16px;margin:20px 0}select{font:inherit;padding:7px;margin-left:7px}.legend{display:flex;gap:20px;flex-wrap:wrap}.legend span:before{content:"●";color:var(--color);margin-right:6px}svg{width:100%;height:auto;background:white}svg text{font:12px system-ui;fill:#536881}</style>' + controls
    page += '<h2>Source feature recall</h2><div class="scroll"><table><thead><tr><th>Feature</th><th>Expected units</th>' + ''.join('<th>' + label + '</th>' for label in LABELS) + '</tr></thead><tbody>' + ''.join(feature_rows) + '</tbody></table></div>'
    page += '<img src="pptx-feature-recall.png" alt="Annotated feature-recall matrix with explicit reference counts and unscored cells"><p><a href="pptx-feature-recall.csv">Numeric matrix CSV</a> · <a href="pptx-feature-recall.json">Matrix data and source identity</a></p>'
    gaps = OUT / phase / 'image-gap-audit.json'
    if gaps.exists():
        audit = json.loads(gaps.read_text())
        alternatives = audit['counts'].get('svg_alternative_with_original_payload', 0)
        body = summary['duke-slides']['features']['body_text']
        gap = f'The resource-limit recovery restores 6,678 body-text five-grams. The unchanged literal reference now has {body["expected"] - body["matched"]:,} unmatched units.' if resource_gate else 'One reference-readable 19-slide presentation exceeds the default 250,000-node per-part limit: its largest layout has 325,458 elements. That failed parse accounts for 6,678 of 6,723 unmatched body-text five-grams. It remains in every applicable accuracy denominator.'
        page += f'<h2>Interpreting the new gaps</h2><p>{gap}</p><p>Independent relationship and payload checks explain {alternatives} unmatched picture occurrences on successful parses: each node selects the source SVG alternative and retains both SVG and original fallback-image bytes exactly. The raw image score retains these mismatches. Other inspected differences include equation text inserted into source paragraphs and text in merged-cell continuation slots. An older audit also identified two omitted empty comments; the later classic-comment change preserves them. These observations explain source-literal scores; they do not prove rendered fidelity.</p>'
    keynote_path = OUT / 'corpus-followup/keynote-supplement/classification.json'
    if keynote_path.exists():
        page += '<h2>Additional reserved format coverage</h2><p>The Embrace and Keynote additions brought the collection to 1,623 distinct inputs, including unsupported and malformed controls, with 277 reserved. The Embrace supplement adds one independently confirmed legacy PowerPoint container and one malformed OLE control. Two pinned LibreOffice fixtures add XML-based and IWA-based Keynote packages with valid ZIP CRCs. These four files are reserved before parser evaluation and do not alter the measured PowerPoint cohort or reference denominators.</p>'
    sdk_path = OUT / 'corpus-followup/openxml-sdk/summary.json'
    if sdk_path.exists():
        sdk = json.loads(sdk_path.read_text())
        page += f'<h2>Open XML SDK corpus expansion</h2><p>A pinned Microsoft regression corpus contributes {sdk["new_unique"]} unique presentation packages after {sdk["duplicates"]} exact duplicates are removed. This brings the complete collection to {sdk["combined_unique"]:,} files and {277 + sdk["splits"]["validation"]} reserved inputs. The new files include {sdk["namespaces"].get("http://purl.oclc.org/ooxml/presentationml/main", 0)} Strict packages, {sdk["formats"].get("potx", 0)} templates and macro-enabled presentation variants. All package CRCs pass. The split was fixed before package inspection or parser evaluation: {sdk["splits"]["tuning"]} tuning and {sdk["splits"]["validation"]} reserved.</p>'
        sdk_check = sdk_path.parent / 'initial/summary.json'
        if sdk_check.exists():
            accepted = json.loads(sdk_check.read_text())
            counts = {engine: sum(group.get('ok', 0) for group in groups.values()) for engine, groups in accepted['engines'].items()}
            page += f'<p>Separate acceptance-only checks on the {accepted["total"]} new tuning inputs: duke-slides {counts["duke-slides"]}, Office Oxide {counts["office_oxide"]}, undoc {counts["undoc"]}. These are successful returns, not content accuracy or visual fidelity. The headline benchmark remains the frozen 630-input cohort; this new corpus is not mixed into its scores.</p>'
        source_check = sdk_path.parent / 'source-check1/assessment.json'
        if source_check.exists():
            source = json.loads(source_check.read_text())
            page += f'<h2>SDK source accuracy supplement</h2><p>The retained {html.escape(source["identity"]["label"])} build and the same five parser adapters are scored separately on {source["reference_inputs"]}/{source["inputs"]} new tuning packages. The source reader uses each package’s qualified Strict or Transitional namespaces with the existing literal-text and feature rules. Parser failures count as zero. No new latency ranking is made, and the main comparison and heatmap denominators remain unchanged.</p>'
            for family, stats in source['by_namespace'].items():
                sdk_rows = []
                for engine, label in zip(ENGINES, labels):
                    row = stats[engine]; text = row['features']['body_text']
                    sdk_rows.append(f'<tr><th>{label}</th><td>{row["accepted"]}/{row["total"]}</td><td>{text["matched"]:,}/{text["expected"]:,}</td><td>{pct(text["recall"])}</td><td>{pct(text["macro_f1"])}</td></tr>')
                page += f'<h3>{html.escape(family.title())} · {source["namespaces"][family]} packages</h3><div class="scroll"><table><thead><tr><th>Parser</th><th>Accepted</th><th>Matched / expected text units</th><th>Text recall</th><th>Macro text F1</th></tr></thead><tbody>' + ''.join(sdk_rows) + '</tbody></table></div>'
            page += '<p>These are source-extraction checks on a public regression corpus, not production prevalence, complete Strict conformance or rendered fidelity. <a href="../corpus-followup/openxml-sdk/source-check1/assessment.json">All feature scores, per-input results and identities</a>.</p>'
            gap_path = source_check.with_name('image-gap-audit.json')
            if gap_path.exists():
                gap = json.loads(gap_path.read_text())
                if gap['counts'] == {'missing_package_target_retained': 1} and gap['unclassified'] == 0:
                    page += '<p>The one unmatched image occurrence references a payload absent from the source ZIP. Rust retains the unresolved target on the image node. The metric remains 37/38; this inspection does not remove the malformed occurrence from the denominator. Rust also retains all 109 scored tables, 63 cached numeric chart series, 9,349 notes five-grams and 436 explicit top-level frames in this supplement.</p>'
    supplement_path = OUT / 'expanded/strict-supplement.json'
    if supplement_path.exists():
        supplement = json.loads(supplement_path.read_text())
        if supplement.get('phase') == phase:
            strict_rows = []
            for engine, label in zip(ENGINES, LABELS):
                records = [r['results'][engine] for r in supplement['records']]
                matched = sum(r['metrics']['body_text']['matched'] for r in records)
                expected = sum(r['metrics']['body_text']['expected'] for r in records)
                strict_rows.append(f'<tr><th>{label}</th><td>{sum(r["status"] == "ok" for r in records)}/{len(records)}</td><td>{matched}/{expected}</td></tr>')
            page += '<h2>Additional format checks</h2><p>The strict-OOXML input excluded by the frozen reference is scored separately with its actual namespaces. This is a small source check, not complete strict-format conformance. The comparison above retains its original denominators.</p><div class="scroll"><table><thead><tr><th>Parser</th><th>Strict packages accepted</th><th>Literal text units matched</th></tr></thead><tbody>' + ''.join(strict_rows) + '</tbody></table></div>'
    legacy_path = OUT / 'expanded/legacy-gap-audit/summary.json'
    if legacy_path.exists():
        legacy = json.loads(legacy_path.read_text())
        empty = sum(r.get('body_characters') == r.get('images') == r.get('tables') == 0 for r in legacy['records'])
        page += f'<p>Of {len(legacy["records"])} legacy inputs accepted by Office Oxide and rejected in the preceding native acceptance check, {empty} returned models contain no body text, images or tables in the measured adapter. Others contain some content. No independent legacy semantic oracle was established, so neither acceptance nor these counts prove correct recovery.</p>'
    page += '<details><summary>Recorded benchmark identity</summary><pre>' + html.escape(json.dumps(data['identity'], indent=2)) + '</pre></details>' + script + '</html>'
    page = page.replace('<title>Expanded PowerPoint benchmark</title>', '<title>Expanded PowerPoint benchmark</title><link rel="icon" href="data:,">')
    page = page.replace('One valid 19-slide presentation exceeds', 'One reference-readable 19-slide presentation exceeds')
    if phase != 'expanded-comparison':
        page = page.replace('<h1>Expanded PowerPoint benchmark</h1>', '<h1>Expanded PowerPoint benchmark</h1><p><a href="expanded-baseline/index.html">Frozen starting comparison for the expanded corpus</a></p>')
    (site / 'expanded.html').write_text(page)
    lines = ['# Expanded PowerPoint comparison', '', f'{len(data["common_ids"])} common nonempty timing inputs; {summary["duke-slides"]["reference_total"]} reference-bearing packages out of {summary["duke-slides"]["total"]} OOXML tuning inputs. The {reserved_main}-file validation set in the source manifest remains unused.', '', '| Parser | Accepted/reference | Median ms | p95 ms | Literal text recall | Macro text F1 |', '| --- | ---: | ---: | ---: | ---: | ---: |']
    for engine, label in zip(ENGINES, LABELS):
        row = summary[engine]; feature = row['features']['body_text']
        lines.append(f'| {label} | {row["reference_success"]}/{row["reference_total"]} | {row["median_ms"]:.3f} | {row["p95_ms"]:.3f} | {pct(feature["recall"])} | {pct(feature["macro_f1"])} |')
    lines += ['', 'Office Oxide and undoc remain faster. Literal scores include failures and retain penalties for inserted equation text, SVG alternative selection and covered merged-cell contents. See the interactive report for source-feature denominators, gap analysis and identities.', '']
    (site / 'EXPANDED-RESULTS.md').write_text('\n'.join(lines))
    print(site / 'expanded.html')


if __name__ == '__main__':
    main(sys.argv[1])

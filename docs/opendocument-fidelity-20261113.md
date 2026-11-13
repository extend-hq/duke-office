# OpenDocument chart fidelity and latency — 2026-11-13

Latest follow-up: [OpenDocument parity and latency hill-climb](opendocument-hillclimb-20261113.md).

The shared Rust changes improve median visual agreement on 21 corpus-derived charts from **32.2% to 62.2%**, and mean agreement from **40.6% to 56.4%**. Extracting data from an already parsed chart is **24.9× faster** than serializing and parsing chart XML, with identical results on all 33 embedded charts in the corpus. Whole-document latency is approximately unchanged. This is progress on specific chart gaps, not complete LibreOffice parity or evidence of market-wide superiority.

The baseline is frozen source at `6d8aa7b3e986e309270905245523037d7575319b`: `duke-odf` 0.2.0, `duke-office-charts` 0.2.0, `duke-office-art` 0.1.9, `duke-docx` 0.3.1 and `duke-pptx` 0.4.1. The candidate is that checkpoint plus the local fixes, with unchanged manifest versions. These fixes have not been published. No React package was changed or published in this evaluation.

## Implementation

Runtime edits are confined to `duke-odf` and `duke-office-charts`; `duke-office-art` remains unchanged.

- The ODF model retains saved title and legend coordinates, legend expansion, legacy coordinate regions, inner versus outer plot bounds, axis number formats and grid styles, and chart data-table properties. Automatic legacy plot coordinates are no longer mistaken for manual geometry.
- The shared renderer uses ODF axis fonts, number/date formats, rotation and wrapping, skips overlapping value labels independently of gridlines, reads bar gap widths from axis styles, preserves series line widths, and honors saved positions. It also renders chart data tables and improves radar geometry and bubble scaling/clipping at plot boundaries.
- `duke-office-charts::odf::extract_chart_data` now constructs `ChartData` directly. It preserves series grouping, sparse point indices, original numeric strings and scatter/bubble channel roles. It does not require supported artwork. ODF parsing also avoids unused fallback allocations, repeated point-style resolution and duplicate domain-range traversal.

The direct API is used by the ODT extraction path. ODP/ODG still use their existing chart-description XML path, so the microbenchmark speedup is not an ODP or worker-level speedup. This run did not deploy or smoke-test Extend Web. ODS/FODS chart data is checked through the shared ODF API; native spreadsheet integration is a separate remaining task.

## Visual results

| Set | Cases | Baseline median / mean | Candidate median / mean |
| --- | ---: | ---: | ---: |
| Corpus-derived charts | 21 | 32.2% / 40.6% | 62.2% / 56.4% |
| Controlled charts | 11 | 77.8% / 72.7% | 78.4% / 72.8% |
| Controlled artwork | 15 | 97.5% / 96.6% | 97.5% / 96.6% |

Nineteen corpus-derived cases improve, one is unchanged and one regresses. Bar cases improve as far as 67.8%→85.6%, 65.3%→78.9% and 57.3%→70.2%; a group of related charts improves from approximately 32% to 62%. The data-table, radar and bubble cases improve but remain low-scoring. The regressing sparse bar case falls from 26.6% to 19.1%; its corrected axis font, label stride and bar width do not resolve the remaining typography, wrapping and thin-gridline rasterization differences. It remains visible in the report and is included in every aggregate.

The six supplemental artwork probes are unchanged: median 94.9%, mean 64.5%. Two native pages are blank and remain flagged as inconclusive, contributing zero to that supplemental aggregate. They are not pooled into the primary sets.

The reference is LibreOffice 26.2.2.2, commit `1f77d10d6938fd34972958f64b2bcfa54f8b1ba5`. Both arms use the same frozen viewer bundle, Chromium 157.0.8103.78, device scale 1, and full 432×432 pages at 96 dpi. The metric is RGB agreement within 16 on the union of ink below 247. There is no alignment, masking, cropping or resizing to improve scores. Native references are the sealed references from the preceding evaluation.

The 21 distinct embedded chart parts retain their content and styles in isolated fitted frames, with preview images removed. They include converted Microsoft documents and several similar bar variants. This is not a production sample or an evaluation of complete original-page layout. Font fallback and antialiasing differ between the native PDF and browser; strict pixel agreement exposes those differences along with geometry errors. See the [visual methodology](opendocument-visual-evaluation.md) and [harness](../scripts/odf-visual/README.md).

## Whole-document latency and text accuracy

The existing Microsoft-style normalized-character LCS methodology is unchanged. All **756 readable inputs produce byte-identical text** before and after these fixes. Four encrypted files and four invalid/non-package inputs remain rejected, for **756/764 parsed**. Text accuracy and visual agreement measure different properties.

| Format | Parsed | Scored | Pooled / mean-file F1, unchanged | Baseline mean / median ms | Candidate mean / median ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| ODT | 418/426 | 220 | 0.9988 / 0.9761 | 0.374 / 0.078 | 0.366 / 0.079 |
| FODT | 79/79 | 37 | 0.9997 / 0.9961 | 0.037 / 0.031 | 0.037 / 0.031 |
| ODP | 217/217 | 86 | 0.9361 / 0.8907 | 0.353 / 0.163 | 0.354 / 0.165 |
| FODP | 22/22 | 6 | 0.9573 / 0.9112 | 0.078 / 0.008 | 0.077 / 0.008 |

The small differences do not establish a whole-document speed improvement. These are medians across files of their fastest warm calls, not request P50/P95, cold starts, browser layout or network latency. Runs use an Apple M4 Pro with 48 GiB memory, Rust 1.99.0, opt-level 3 and thin LTO. Baseline and candidate each ran two passes in alternating order, after compilation and visual capture completed. Each input uses at least three repeats, up to 200 repeats or 250 ms. File reading, process startup and output writing are outside the timed region. The shared host is not an isolated benchmark machine.

References are LibreOffice text/PDF exports from the existing 764-input corpus. NFC-normalized letters and digits are compared by LCS; references with fewer than 20 characters and 14 tracked-change documents are excluded from accuracy scoring. Failed supported inputs with usable references count against recall. Full small-format results and CPU timings remain in the machine-readable report.

Two pinned peers were rebuilt and rerun: `odox-core` 0.16.0 and `omniparse` 0.4.1, each with one fresh pass using the same warm-repeat loop. The candidate has two passes, so small timing differences should not be used to rank peers. Accuracy uses the same scorable inputs; latency below uses the 417 ODT and 213 ODP files all three parsed.

| Format / parser | Pooled F1 | Mean-file F1 | Mean / median ms |
| --- | ---: | ---: | ---: |
| ODT / candidate | 0.9988 | 0.9761 | 0.367 / 0.079 |
| ODT / odox-core | 0.9951 | 0.9580 | 0.604 / 0.177 |
| ODT / omniparse | 0.9883 | 0.9478 | 0.076 / 0.028 |
| ODP / candidate | 0.9361 | 0.8907 | 0.358 / 0.165 |
| ODP / odox-core | 0.8829 | 0.8279 | 1.026 / 0.548 |
| ODP / omniparse | 0.8402 | 0.7423 | 0.046 / 0.036 |

The candidate is more accurate on this text metric than both peers and faster than odox-core. Omniparse's simpler text extraction remains faster. Peer visual rendering was not evaluated. The broader [November 12 comparison](opendocument-benchmarks.md) is historical; its other peers were not rerun here. Neither comparison establishes leadership over every commercial product.

## Extraction cost and further speed work

For the same 33 parsed chart models, the sum of best-of-100 calls is **978.706 µs through chart XML versus 39.323 µs directly**. All outputs compare equal. Both paths use the current candidate implementation; document parsing and rendering are excluded. The [microbenchmark](../scripts/odf-benchmark/chart_data.rs) is included in the harness.

The fresh phase diagnostic totals approximately 152.1 ms in shared ODF reading, 42.0 ms in text-model construction, 41.1 ms in block extraction and 19.6 ms in presentation/drawing-model construction across 756 inputs. Shared ODF reading is about 60% of this total. Model costs are differences between independently timed calls, so these figures are a prioritization aid rather than an exact execution trace. Two large text inputs account for approximately 35% of the diagnostic total.

A separate diagnostic over the same inputs measured 144.2 ms of shared ODF reading: 20.0 ms inflating XML entries and 124.2 ms parsing and building models/styles, covering 60.5 MB of XML. Its independently measured total differs from the preceding run; the useful result is that inflation accounts for only about 14% of shared reading cost.

The next useful targets are:

1. Reduce allocation and repeated work in shared XML/style/content processing, measuring retained memory as well as latency. This is the largest phase and benefits documents without charts.
2. Carry direct chart data through the presentation extraction path and avoid artwork generation when only structured data is requested. Consumer integration is needed to realize that benefit; the direct API alone does not change ODP extraction cost.
3. Investigate allocation in large table block extraction. That phase is shared with the Microsoft document path and lies outside the runtime files changed here.

## Validation, provenance and remaining gaps

`cargo test --workspace --locked` passes **463 tests**, and `cargo check --workspace --locked --target wasm32-unknown-unknown` passes. No new unit tests were added. The existing evaluation probes verify 11 charts across ODT, FODT, ODP, FODP, ODG, FODG, ODS and FODS, plus two unsupported-artwork variants: 110 chart checks, including sparse positions and high-precision numeric strings. The corpus audit finds 33 charts with data and SVG, 2,346 drawing objects, and 708/710 evaluated custom geometries, of which 696 render as SVG; availability is not a fidelity score.

Sources were used to identify format semantics and observe behavior, with independent Rust implementations. The [pinned source manifest](../scripts/odf-visual/libreoffice-sources.json) records paths and SHA-256 hashes. Relevant references include the [ODF 1.3 schema specification](https://docs.oasis-open.org/office/OpenDocument/v1.3/OpenDocument-v1.3-part3-schema.html), LibreOffice's [plot-area import](https://github.com/LibreOffice/core/blob/1f77d10d6938fd34972958f64b2bcfa54f8b1ba5/xmloff/source/chart/SchXMLPlotAreaContext.cxx), [axis scaling](https://github.com/LibreOffice/core/blob/1f77d10d6938fd34972958f64b2bcfa54f8b1ba5/chart2/source/view/axes/ScaleAutomatism.cxx) and [tick placement](https://github.com/LibreOffice/core/blob/1f77d10d6938fd34972958f64b2bcfa54f8b1ba5/chart2/source/view/axes/Tickmarks.cxx). No third-party implementation code was imported into workspace crates. Downloaded reference source retains its notices in the ignored evaluation directory; existing bundled font/geometry assets and their provenance were unchanged. This bounded source review is not a universal originality audit.

Remaining gaps include font matching, title padding, table hyphenation, automatic layout, additional number formats, and some radar/bubble details. Standard saved coordinate regions are still used as layout hints even when automatic flags are present; automatic layout parity is incomplete. Native 3D and secondary-axis artwork remain unsupported while data is retained. Full Writer pagination, Calc rendering, external data resolution and formula recalculation were not validated.

All local evidence is under `output/odf-fidelity-20261113`:

- `real-charts/report/index.html`, `measured/report/index.html`, `supplemental/report/index.html`: native/baseline/candidate comparisons and per-case metrics.
- `baseline-source.json`, `candidate-source.json`, `candidate.patch`: frozen source/binary receipts and runtime/harness patch.
- `benchmark/{baseline,candidate}/results`, `*-scores.txt`, `final-metrics.json`: both final timing passes, text outputs and scores. Earlier passes are preserved separately.
- `chart-data.csv`, `*-phases.txt`, `candidate-odfphases.txt`: extraction and phase diagnostics.
- `data-contract/report.json`, `corpus-audit.json`, `workspace-tests.log`, `wasm-check.log`: structural and build verification.

Reproduction commands and timing definitions are in the [benchmark harness](../scripts/odf-benchmark/README.md). The frozen baseline must be retained before rebuilding the candidate; using the current checkout for both arms would invalidate this comparison.

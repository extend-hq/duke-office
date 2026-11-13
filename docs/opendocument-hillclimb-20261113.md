# OpenDocument parity and latency follow-up — 2026-11-13

This round builds on the [preceding fidelity candidate](opendocument-fidelity-20261113.md). It keeps three measured changes: lower-allocation ODF parsing, recovery of stored table values, and improved radar/sparse-category label placement. Runtime changes remain in `rust-odf` and `rust-office-charts`. No manifests were bumped, no crates were published, and no React packages were changed or published.

## Changes retained

- XML attributes are staged directly in the DOM attribute array and namespace-resolved after the complete start tag. Declarations appearing after attributes still apply. A whitespace fast path appends already-normalized text without a temporary string. Margin and padding names use static strings instead of four temporary allocations per property group.
- Empty table-cell presentations fall back to stored typed values; explicit string values replace stale plain-text presentations. Cells containing links, drawings, annotations or other non-text inline content retain their content. Original typed values and formulas remain in the model. Basic numeric date masks and booleans have readable fallbacks; this is not a formula engine or complete locale-aware formatting implementation.
- Radar category labels use radial spacing and quadrant alignment. Sparse category labels use tick spacing for wrapping, including empty category slots, instead of expanding to the distance between nonempty labels.

## Results against the previous candidate

The same 764-input corpus, sealed LibreOffice references, fixed viewer and existing Microsoft-file evaluation methods are retained. The visual metric is full-page RGB agreement within 16 on the union of ink below 247. Captures are 432 × 432 at 96 dpi, Chromium 157.0.8103.78, device scale 1; reference LibreOffice is 26.2.2.2. No alignment, cropping, masking or resizing was added.

| Visual set | Cases | Before median / mean | After median / mean |
| --- | ---: | ---: | ---: |
| Corpus-derived charts | 21 | 62.250% / 56.357% | 62.250% / 56.658% |
| Controlled charts | 11 | 78.396% / 72.783% | 78.396% / 72.778% |
| Controlled artwork | 15 | 97.465% / 96.573% | unchanged |
| Supplemental artwork | 6 | 94.880% / 64.519% | unchanged |

Three real chart cases improve and eighteen are unchanged. The two radar scores increase from **23.77% to 25.58%** and **26.34% to 30.52%**; the sparse bar score increases from **19.09% to 19.40%**. The controlled radar decreases from **2.932% to 2.882%**. Its automatic plot layout remains substantially wrong, including a clipped category label. The small aggregate visual gain does not resolve the broader typography and layout gaps. The low-scoring cases and two blank supplemental native references remain included in the reports.

The real table-value repair improves that document's normalized-character LCS F1 from **0 to 0.854**. Six formerly empty cells now expose stored dates/numbers and an explicit string replaces stale text. Its locale-specific date and grouped-number display still differs from LibreOffice. **755 of 756** extracted text outputs and shared ODF body representations are unchanged; the intended repair is the sole difference. Shared-body equality was checked using stable debug representations, independently of the downstream text scorer.

| Format | Parsed | Scored | Before mean / median ms | After mean / median ms | Mean latency change |
| --- | ---: | ---: | ---: | ---: | ---: |
| ODT | 418/426 | 220 | 0.3778 / 0.0827 | 0.3759 / 0.0797 | -0.5% |
| FODT | 79/79 | 37 | 0.0392 / 0.0318 | 0.0368 / 0.0301 | -6.3% |
| ODP | 217/217 | 86 | 0.3634 / 0.1755 | 0.3358 / 0.1591 | -7.6% |
| FODP | 22/22 | 6 | 0.0803 / 0.0085 | 0.0740 / 0.0080 | -7.9% |
| ODG | 10/10 | 5 | 0.2086 / 0.0821 | 0.1975 / 0.0709 | -5.3% |
| OTT | 2/2 | 2 | 0.2873 / 0.2873 | 0.2880 / 0.2880 | +0.2% |
| OTP | 1/1 | 0 | 0.1295 / 0.1295 | 0.1055 / 0.1055 | -18.5% |
| ODM | 1/1 | 0 | 0.0847 / 0.0847 | 0.0797 / 0.0797 | -5.9% |
| OTG | 1/1 | 0 | 0.0485 / 0.0485 | 0.0468 / 0.0468 | -3.5% |
| FODG | 5/5 | 1 | 0.0404 / 0.0211 | 0.0354 / 0.0168 | -12.5% |

The matched runs show **7.6% lower mean ODP latency**, **6.3% lower FODT latency**, and **7.9% lower FODP latency**. ODT mean latency is essentially flat (−0.5%); its median across files improves 3.6%. These are fastest warm calls per document, not request P50/P95 or cold starts. Both binaries ran two passes in AB/BA order with identical limits and output scope, after builds/captures completed. Each file uses up to 200 warm repeats or 250 ms, at least three. Input reads and output writes are excluded. The shared Apple M4 Pro host has 48 GiB RAM; release builds use Rust 1.99.0, opt-level 3 and thin LTO. Small differences and tiny format groups should not be treated as durable speed guarantees.

ODT pooled text F1: 0.998844 → 0.998900; mean-file F1: 0.976119 → 0.980001. ODP pooled text F1: 0.936054 → 0.936054; mean-file F1: 0.890733 → 0.890733. Other format text scores are unchanged. Accuracy uses the existing NFC-normalized letters/digits LCS scorer, excludes references under 20 characters and tracked-change cases, and retains failed scorable inputs in recall.


Allocation instrumentation around shared `OdfDocument::parse`, with input bytes already loaded, reports **1,311,323 → 994,555 allocation/reallocation calls (24.16% fewer)** across the 756 readable inputs. Total requested allocation bytes fall **751,591,619 → 747,052,796 (0.60%)**. Requested bytes count allocation traffic, not peak or retained memory. Instrumentation is a separate run and is excluded from latency measurements.

## Rejected experiment

Converting Calibri/Carlito chart text to existing Office Art glyph outlines avoided the browser's Helvetica fallback and improved eight real chart scores, but regressed another. It added roughly 0.7–1.9 ms on several chart-bearing documents, with individual whole-document slowdowns around 2–4×. The prototype and measurements are preserved under `font-prototype/`; the implementation was removed. No font assets or dependencies were added. Portable text remains an open design question rather than a default rendering cost.

## Validation and evidence

- **463 existing workspace tests pass**; the `wasm32-unknown-unknown` workspace check passes. No new unit tests were added.
- **756/764 inputs parse**, with the same eight rejected encrypted/invalid inputs. All 33 embedded charts retain data and SVG. Artwork availability remains 2,346 drawings, 708/710 evaluated custom geometries and 696 rendered custom geometries.
- **110 chart-data checks pass**: eleven charts across ODT/FODT, ODP/FODP, ODG/FODG and ODS/FODS, plus two unsupported-artwork variants. Precision, sparse indices and scatter/bubble roles remain intact. ODS/FODS use the shared ODF API; this does not establish native spreadsheet-viewer integration.
- Direct chart data and XML-roundtrip results remain equal on all 33 corpus charts. This run does not deploy or smoke-test Extend Web.

The reference review includes the [ODF schema specification](https://docs.oasis-open.org/office/OpenDocument/v1.3/os/part3-schema/OpenDocument-v1.3-os-part3-schema.html), LibreOffice's pinned [polar label placement](https://github.com/LibreOffice/core/blob/1f77d10d6938fd34972958f64b2bcfa54f8b1ba5/chart2/source/view/axes/VPolarAngleAxis.cxx) and [Cartesian label layout](https://github.com/LibreOffice/core/blob/1f77d10d6938fd34972958f64b2bcfa54f8b1ba5/chart2/source/view/axes/VCartesianAxis.cxx). Rust changes were implemented independently from format semantics and observed behavior. Reference source retains its notices outside workspace crates. Existing third-party assets and attribution are unchanged; no third-party implementation was imported. This is a bounded source review, not a universal originality audit.

Local evidence is in `output/odf-hillclimb-20261113`:

- `real-charts/report/index.html`, `measured/report/index.html`, `supplemental/report/index.html`: reference/before/after comparisons, including regressions.
- `final-metrics.json`, `benchmark/{baseline,candidate}/results/`, `*-scores.txt`: both timing passes, text outputs and scores.
- `allocations.jsonl`, `allocation-probe/`, `odf-source-baseline/`: instrumented allocation/body comparison with the frozen previous implementation.
- `baseline-source.json`, `candidate-source.json`, `baseline.patch`, `candidate.patch`: source and executable receipts. The baseline's five executable hashes match the preceding candidate. Reused build directories are not frozen artifacts; use the copied executables in this evaluation directory.
- `data-contract/report.json`, `corpus-audit.json`, `workspace-tests.log`, `wasm-check.log`: regression evidence.
- `final_run.py`, `summarize_final.py`: local orchestration. Existing [benchmark](../scripts/odf-benchmark/README.md) and [visual](../scripts/odf-visual/README.md) harnesses retain the evaluation definitions.

Remaining work includes automatic radar layout, fonts, thin-stroke rendering, locale-aware cell formatting, secondary axes/3D artwork and complete original-page layout. The chart corpus is isolated embedded charts, includes converted Microsoft files and related variants, and is not a representative production sample. Tiny format groups do not establish general performance for those formats. Peer parsers were not rerun in this follow-up; the prior comparison still does not support a claim of market-wide leadership.

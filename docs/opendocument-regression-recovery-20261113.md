# OpenDocument regression recovery — 2026-11-13

This round recovers all six visual regressions reported in the previous font/layout round. Against `c3960fd`, 17 of 60 chart cases improve, 43 are unchanged, and none regress. All 21 artwork pages are unchanged. Runtime changes are confined to the shared `duke-office-charts` crate, formerly `rust-office-charts`; no downstream React changes are needed.

The working checkout also received independent naming/version changes (`2e203d2`, `27ef292`) during this work. The final binaries use `duke-office-charts` 0.2.1, `duke-odf` 0.2.1, `duke-office-art` 0.1.10, `duke-pages` 0.3.1 and `duke-slides` 0.4.1. These local manifest versions do not establish registry publication. This change does not publish any package.

## Changes

- Use native-style padding, font ascent/descent, key dimensions, baseline placement and hairline frames for automatic ODF side legends. Wrapped Cartesian legends and explicitly positioned legends retain their existing layout path.
- Render simple Latin Calibri/Carlito radar labels with the existing licensed OfficeArt font contours. A bounded cache stores at most 380 glyphs across four styles, and each SVG reuses glyph definitions. Labels remain in SVG titles and structured chart data. Complex text, positioning and unsupported styles retain the text fallback.
- Let the existing bounded overflow solver place radar labels when the label span plus its required gap exceeds the canvas. This avoids forcing long labels against the wrong horizontal margin.
- Replace the full SVG DOM pass for ODF text widths with a streaming `quick-xml` pass. Preserve inherited styles, XML entities, unsupported text siblings and malformed-input fallback. Font-family matching avoids allocating a lowercased string on each width lookup.
- Stop serializing individual category/numeric cache points into the private rendering XML when the renderer already receives typed chart data. Preserve the cache structure and point counts used for layout. The public `chart_xml` API still includes all points, and direct extraction output is unchanged.

## LibreOffice parity

Both arms use the same complete 432 × 432 pages at 96 dpi, Chromium 157.0.8103.78, device scale 1 and the existing viewer. The metric is RGB agreement within 16 over the union of ink below 247. No registration, alignment, cropping, masking or object resizing is applied. Sealed LibreOffice 26.2.2.2 PDF/PNG references are reused after checking the input/reference receipts; this round does not claim a new native PDF export. Final executable output is byte-identical to the scored models for all six suites.

| Previously regressed case | Before font changes (`e2dc6ee`) | Last commit (`c3960fd`) | Candidate |
| --- | ---: | ---: | ---: |
| Real radar, case 6 | 33.338% | 32.604% | 36.255% |
| Controlled line | 78.396% | 78.394% | 79.114% |
| Controlled pie | 93.567% | 93.551% | 94.241% |
| Controlled ring | 90.202% | 90.182% | 91.066% |
| Controlled scatter | 85.285% | 85.279% | 85.656% |
| Courier New, 14 pt | 5.175% | 5.125% | 7.889% |

| Suite | Chart cases | Mean, last commit → candidate |
| --- | ---: | ---: |
| Real embedded charts | 21 | 57.608% → 57.830% |
| Controlled charts | 11 | 74.950% → 75.429% |
| Automatic radar layout | 8 | 20.135% → 20.135% |
| Font and canvas matrix | 16 | 14.991% → 15.465% |
| Title matrix | 4 | 19.440% → 19.440% |

The 15 controlled artwork pages and six supplemental pages are unchanged, including two blank native references. Low absolute scores in the layout matrices still expose substantial remaining differences. Embedded charts and adversarial matrices are not a production-weighted sample and do not measure complete Writer pagination, Calc sheet layout, or market-wide superiority.

## Latency

| Scope | Inputs | Pre-font ms | Last commit ms | Candidate ms | vs last commit | vs pre-font |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Full parsing | 756 | 0.318399 | 0.319725 | 0.314880 | -1.52% | -1.11% |
| Full parsing, chart-bearing ODT | 5 | 0.289400 | 0.313040 | 0.289200 | -7.62% | -0.07% |
| Full parsing, chart-bearing ODP | 8 | 1.278400 | 1.322050 | 1.260450 | -4.66% | -1.40% |
| Deferred extraction | 256 | 0.426189 | 0.427436 | 0.428605 | +0.27% | +0.57% |
| Deferred extraction, chart-bearing ODP | 8 | 2.129575 | 2.177112 | 2.132450 | -2.05% | +0.14% |

Full parsing improves in both passes versus the last commit (-2.56%, -1.37%). The original chart-bearing latency loss is recovered on this corpus; ODT is effectively at the pre-font time and ODP is 1.40% faster. Deferred extraction is inconclusive: its aggregate is +0.27%, while the two passes move in opposite directions (+2.33%, -2.26%). The chart-bearing deferred subset likewise swings (+7.31%, -6.63%); the aggregate -2.05% is not evidence of a reliable speedup. The retained optimizations target full chart rendering, which deferred extraction avoids.

The release harness uses Rust 1.99.0, optimization level 3 and thin LTO on the same Apple M4 Pro / 48 GiB host. Input bytes are read before timing. Each input has warm in-process repetitions, bounded at 200 or 250 ms, and the table uses the mean of each input's minimum across two counterbalanced passes. Startup, cold font-cache initialization and file I/O are excluded. No builds, raster comparisons or cleanup run during timing; unrelated host activity is uncontrolled.

The isolated width pass measures **846.996 → 450.661 µs across 60 SVGs (-46.79%)**, with byte-identical output. The full-XML/native rendering ablation measures **6231.622 → 5597.165 µs across 93 charts (-10.18%)**, with identical SVG and data.

The focused chart set measures **1.055089 → 0.985528 ms (-6.59%)** across four alternating pairs; every pair improves (5.96–8.30%). It includes the 13 chart-bearing corpus inputs plus five visual matrices. The isolated width-pass comparison requires byte-identical output. The rendering ablation compares the complete public XML route with the native typed-data route, so it includes the benefit of owned chart data that predates this change; it does not isolate only the new cache-point omission.

Complete outputs match both baselines for **756/756 extracted texts and 256/256 deferred bundles**. Text scores are unchanged in all scored formats: ODT pooled/mean F1 is **0.998900 / 0.980001** over 220 references; ODP is **0.936054 / 0.890733** over 86. Other products were not rerun.

## Correctness and provenance

All 467 workspace tests pass, including four new streaming/fallback/glyph-reuse tests, and the WASM workspace check passes on Rust 1.93.1. All 110 existing chart-data probes pass across packaged and flat text, presentation, drawing and spreadsheet formats, including precision, sparse indices, scatter/bubble values and unsupported artwork. Eager, deferred and serialized chart data agree for 67 charts across 260 documents. The direct-data route agrees with public XML extraction for all 33 corpus charts; the render ablation checks exact SVG and data equality for 93 charts.

Availability remains 756/764 parsed with the same eight encrypted/invalid inputs, 33/33 chart SVGs, 2,346 drawings, 708/710 evaluated custom geometries and 696 rendered custom geometries. Availability is distinct from visual fidelity. Spreadsheet containers are checked through the shared ODF API, not a spreadsheet viewer. No Extend Web deployment or endpoint validation is claimed.

Reference behavior was inspected in the pinned LibreOffice legend, diagram, polar-label, chart-view and text-shape sources at `1f77d10d6938fd34972958f64b2bcfa54f8b1ba5`, recorded in the [source manifest](../scripts/odf-visual/libreoffice-sources.json). The Rust implementation is independent; no third-party implementation code was copied or mechanically translated. This finite source review does not prove universal originality.

Font contours and metric tables are separately attributed third-party assets. This change reuses the existing Carlito-derived OfficeArtSans assets through the OfficeArt API; no font asset or metric table changes. Original licenses, provenance and hashes remain in the [font documentation](../crates/duke-office-art/src/fonts/README.md). `quick-xml` is a declared workspace dependency rather than copied implementation.

Rejected variants include broad outline substitution, broader radar margin changes and a larger namespace cache. The namespace experiment preserved DOM results on 2,010 parsed XML parts but measured slightly slower (33,989.966 → 34,027.059 microseconds); it was reverted entirely. Broader visual variants introduced regressions and were narrowed before acceptance. Arbitrary font embedding, full complex-script shaping, all legend wrapping, advanced axes/3D and complete host-document layout remain outside the demonstrated coverage.

## Cleanup and evidence

Removed 12.366 GB of logical file bytes from completed incremental caches and obsolete compiled artifacts under the previous crate names. Cleanup excluded source, fixtures, native references, frozen comparison executables, reports and licenses. APFS allocation and concurrent work prevent treating the logical total as an exact net free-space increase. The workspace had 18 GiB available after cleanup and final release builds.

Local evidence is retained under `output/odf-regression-recovery-20261113`: frozen executables and source receipts, six visual reports, final model equality checks, two-pass three-arm parsing/extraction results, four-pair focused timings, microbenchmarks, data/model checks, corpus audit, test/WASM logs and three cleanup receipts. Intermediate timings are isolated under `before-compact`; the larger namespace-cache experiment is recorded separately. Timing scripts use the frozen baseline harness consistently across all arms despite the intervening package rename.

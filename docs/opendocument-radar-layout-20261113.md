# OpenDocument radar and long-label layout — 2026-11-13

This round compares shared Rust chart changes against `5add6129e3a04d056a22bbf822a65509cea77e08`. Of 74 chart cases, **42 improve, 32 are unchanged and none regress**. The original 60-case suite has 28 improvements and no regressions; all 14 new aspect-ratio probes improve. All 21 artwork pages are unchanged. Runtime changes are confined to `duke-office-charts`; the other change is an evaluation-generator option. No package is published by this change.

## Changes and scope

- Fit the radar radius against horizontal and vertical label extents independently. Each dimension retains its own minimum plot extent. A wide, short canvas no longer imposes its height-derived minimum on horizontally overflowing labels. The vertical search is skipped when the horizontally fitted radius already satisfies it.
- Use font-derived, pixel-rounded ascent and descent for automatic radar legend rows and baselines. Retain native horizontal padding and placement, and reuse the shared legend frame renderer for thin borders.
- Use the Carlito line metrics for Calibri/Carlito polar labels rendered with the existing bundled contours. Fallback text retains its previous category-layout bounds. Applying the rounded metrics to every font regressed two probes and was rejected.
- Add two geometry tests for independent horizontal and vertical overflow constraints. Expand the native comparison generator with 14 cases spanning wide, tall and square canvases, multiple font sizes/families, and left/top/bottom/absent legends.

These changes apply to automatic ODF radar layout. Saved inner plot positions, Microsoft-format rendering and direct chart-data extraction retain their existing paths.

## Native visual measurements

The measurement uses complete 432 × 432 pages at 96 dpi, RGB agreement within 16 over the union of ink below 247, without alignment, cropping, masking or resizing. Chromium 157.0.8103.78, device scale 1 and the existing viewer render both arms. The six existing suites reuse their sealed LibreOffice 26.2.2.2 references; the new aspect-ratio suite has a fresh native PDF export and page rasters. Final executable models are byte-identical to the scored candidates in all seven suites.

| Suite | Chart cases | Mean baseline | Mean candidate |
| --- | ---: | ---: | ---: |
| Real embedded charts | 21 | 57.830% | 57.830% |
| Controlled charts | 11 | 75.429% | 76.125% |
| Automatic radar layout | 8 | 20.135% | 27.370% |
| Font and canvas matrix | 16 | 15.465% | 24.335% |
| Title matrix | 4 | 19.440% | 24.354% |
| New aspect-ratio matrix | 14 | 7.761% | 12.835% |

The controlled radar improves from **25.951% to 33.608%**. The existing wide long-label probe improves from **5.766% to 29.143%**, restoring the plot size and radial tick-label density. The bold polar-label probe improves from **19.425% to 38.536%**. No previously recovered case loses agreement. The two blank native artwork references remain in the unchanged artwork count.

These are bounded chart/graphics comparisons, including adversarial probes and related corpus variants. They are not complete Writer pagination, Calc sheet layout, a production-weighted sample, or evidence of market-wide superiority. Absolute agreement remains low in several long-label cases. Native exports themselves can contain label/legend overlaps; this work does not silently rearrange the reference to hide them.

## Latency and correctness

| Scope | Inputs | Baseline ms | Candidate ms | Change |
| --- | ---: | ---: | ---: | ---: |
| Full parsing | 756 | 0.321287 | 0.322199 | +0.28% |
| Full parsing, chart-bearing ODT | 5 | 0.291580 | 0.291620 | +0.01% |
| Full parsing, chart-bearing ODP | 8 | 1.283562 | 1.289287 | +0.45% |
| Deferred extraction | 256 | 0.427057 | 0.426906 | -0.04% |
| Deferred extraction, chart-bearing ODP | 8 | 2.145550 | 2.148163 | +0.12% |

Full parsing changes by -0.60% and +0.15% in the individual passes; deferred extraction changes by +0.45% and +0.32%. Per-input minima across passes can produce a different aggregate direction. These sub-percent changes do not establish a reliable latency improvement or regression on this host. The accepted result is a fidelity gain with effectively flat measured latency.

Release binaries use Rust 1.99.0, optimization level 3 and thin LTO on the same Apple M4 Pro / 48 GiB host. Full-corpus parsing and deferred extraction use counterbalanced AB/BA passes. The reported aggregate is the mean of each input's minimum warm time across both passes, with input bytes read before timing and up to 200 repetitions or 250 ms per input. Process startup, cold font-cache initialization and file I/O are excluded. Builds, image comparisons and cleanup finish before timing; unrelated host activity remains uncontrolled.

The focused set adds the new matrix to the prior 18-input chart set, for 19 inputs measured over four alternating pairs. It includes 13 chart-bearing corpus inputs and six generated visual matrices. The mean is **1.016289 → 1.016405 ms (+0.01%)**, effectively flat; individual pairs range from -0.85% to +0.72%. All 19 extracted outputs match. These timings must be interpreted separately from the full parsing corpus.

All **469 workspace tests** and the WASM workspace check pass. The 110 existing chart-data probes pass across packaged and flat text, presentation, drawing and spreadsheet formats, including precision, sparse indices, scatter/bubble values and unsupported artwork. Availability remains **756/764 parsed**, with the same eight encrypted/invalid inputs, 33/33 chart SVGs, 2,346 drawings, 708/710 evaluated custom geometries and 696 rendered custom geometries. Availability is not a visual-fidelity measure. Eager, deferred and serialized chart data agree for **81 charts across 261 documents**, including the new matrix. Direct extraction still agrees with the XML route for all 33 corpus charts.

Complete outputs match for **756/756 extracted texts and 256/256 deferred bundles**. Text scores are unchanged across all scored formats. ODT pooled/mean F1 remains **0.998900 / 0.980001** over 220 references; ODP remains **0.936054 / 0.890733** over 86. Other products were not rerun.

## Reference behavior, data provenance and limitations

The native comparison uses the pinned LibreOffice polar-axis, legend, diagram and text-placement behavior references in the [source manifest](../scripts/odf-visual/libreoffice-sources.json), at revision `1f77d10d6938fd34972958f64b2bcfa54f8b1ba5`. The geometric constraint solver is independently implemented from observed bounds and layout behavior. No third-party implementation code was copied or mechanically translated. This finite review does not establish universal originality.

Font ascent/descent values are metadata, separately recorded with font hashes in the local `font-metadata.json`. The new Carlito line values are 1536/512 units per 2048-unit em, distinct from the existing Windows bounds. The legend comparison verifies pixel-rounded line metrics against native PDF coordinates. Existing contours, advance tables and all font attribution/license notices are unchanged; no system font is redistributed and no dependency is added. See the [font provenance documentation](../crates/duke-office-art/src/fonts/README.md).

Rejected experiments include unrounded legend line metrics, which regressed two font probes before the frame/rounding corrections, and applying rounded category metrics to every fallback font, which regressed a no-legend case and a serif aspect-ratio case. The retained category correction is limited to the contour-backed family for which the behavior and parity gain were verified.

Remaining gaps include exact fallback-font glyph rendering, some plot translations and overflow scaling, long-label collisions, arbitrary font embedding, complex scripts, saved-layout variations and advanced axes/3D. ODS/FODS correctness is exercised through the shared ODF API, not a spreadsheet viewer. No downstream React release, Extend Web deployment or endpoint validation is claimed.

## Evidence and cleanup

Local evidence is retained under `output/odf-radar-layout-20261113`: frozen baseline/candidate binaries, source receipts, seven native comparison reports, final model hashes, new native PDF/PNG references, full/focused timing records, complete outputs, font metadata, data/model checks, corpus audit and test/WASM logs. The frozen runner uses its historical parser label consistently across both arms; that label does not change which current crate binaries run.

Removed **1.936 GB of logical file bytes** from completed test/snapshot incremental caches, after verifying that they contained no tracked files and their builds had finished. Source, fixtures, native references, frozen binaries, reports and font/license assets were preserved. Release builds use a separate target. APFS allocation and other host activity prevent interpreting this as an exact net free-space increase; the deletion receipt is `cleanup.json`.

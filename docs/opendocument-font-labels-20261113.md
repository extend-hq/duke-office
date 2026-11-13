# OpenDocument font and long-label measurements — 2026-11-13

This round compares unpublished shared Rust changes against commit `e2dc6eea39b529d078395def1540c07a46371fa3`. Runtime changes are confined to `duke-office-charts`; `duke-odf`, `duke-office-art`, and the document adapters consume the shared behavior without downstream patches. No React packages or Rust crates were published.

## Changes and scope

- Resolve the primary font in a CSS fallback list before selecting compatible advances. Automatic radar layout measures the requested bold/italic style and rounded pixel advances.
- Fit automatic radar labels using ascent/descent bounds, quadrant anchors, and a gap derived from the remaining plot area. Preserve a minimum plot diameter when long labels cannot fit. An independent bounded constraint solver places the plot, then expands the SVG view bounds when labels exceed the canvas.
- Reserve title space using font bounds and padding, use the observed 13 pt default, and wrap automatic radar titles within the title region. Saved inner plot layouts retain their previous baseline placement; the broader positioning experiment regressed the real-chart set and was narrowed.
- Preserve radar axis and legend bold, italic and color properties. Use the native thin fallback for absent or zero ODF stroke widths, with a visible one-pixel radar hairline minimum verified against the rasterized reference.
- Give simple ASCII chart text measured SVG `textLength` values using existing compatible font data. Unsupported fonts, non-ASCII text, nested text runs, explicit spacing and existing lengths retain their original behavior. This preserves readable SVG text without embedding fonts or generating glyph outlines. Font fallback still changes glyph shapes and height.
- Move native `ChartData` into rendering to avoid reconstructing it from generated chart XML or cloning it again. Use a validated direct table lookup for printable ASCII advances, retaining the generic lookup for other characters. Direct extraction semantics remain unchanged.

## Native visual parity

The existing Microsoft-file methodology is unchanged: fixed complete 432 × 432 pages at 96 dpi, RGB agreement within 16 over the union of ink below 247, without registration, alignment, cropping, masks or object resizing. Chromium 157.0.8103.78, device scale 1 and the same viewer bundle render both arms. References use LibreOffice 26.2.2.2 at source revision `1f77d10d6938fd34972958f64b2bcfa54f8b1ba5`. Input, PDF, PNG, model and capture hashes are retained.

| Suite | Cases | Mean before → after | Median before → after |
| --- | ---: | ---: | ---: |
| Real embedded charts | 21 | 56.900% → 57.608% | 62.250% → 63.269% |
| Controlled charts | 11 | 73.039% → 74.950% | 78.396% → 78.394% |
| Automatic radar layout | 8 | 7.227% → 20.135% | 6.191% → 21.155% |
| Font and canvas matrix | 16 | 5.687% → 14.991% | 5.390% → 11.972% |
| Title matrix | 4 | 5.519% → 19.440% | 5.548% → 20.031% |

**Of 60 chart cases, 52 improve, 2 are unchanged and 6 regress.** One real radar loses 0.733 percentage points. Four controlled charts lose 0.002–0.020 points and one font probe loses 0.051 points. These regressions remain in the report and aggregate; this is an aggregate fidelity improvement, not a regression-free result. All 21 artwork pages are unchanged, including the two blank native references.

The additional matrices cover four font families, three font sizes, explicit bold/italic styles, four long-label canvas dimensions, title spacing at two heights and title wrapping at two font sizes. Nineteen of the 20 new cases improve. All eight original automatic-layout probes improve. The original long-label radar rises from 4.970% to 13.416%; the controlled radar rises from 5.745% to 25.951%. These low absolute scores still indicate substantial remaining visual differences.

The corpus charts are isolated embedded objects, including converted Microsoft inputs and related variants. The layout matrices are adversarial probes, not a production-weighted sample. This does not measure complete Writer pagination, Calc sheet layout, or market-wide product superiority.

## Correctness and latency

Frozen release binaries use Rust 1.99.0, optimization level 3 and thin LTO on the same Apple M4 Pro / 48 GiB host. The harness reads each input before timing, then measures warm in-process parsing for up to 200 repetitions or 250 ms. The table reports the mean of each input's best warm time across fresh AB and BA passes. Process startup and input-file I/O are excluded. No builds or visual comparisons run during the timed passes; unrelated host activity remains uncontrolled.

| Scope | Inputs | Baseline ms | Candidate ms | Change |
| --- | ---: | ---: | ---: | ---: |
| Full parsing, all readable inputs | 756 | 0.302496 | 0.305897 | +1.12% |
| Full parsing, chart-bearing ODT | 5 | 0.275200 | 0.296900 | +7.89% |
| Full parsing, chart-bearing ODP | 8 | 1.239825 | 1.296450 | +4.57% |
| Deferred extraction, all readable inputs | 256 | 0.406682 | 0.408017 | +0.33% |
| Deferred extraction, chart-bearing ODP | 8 | 2.056987 | 2.059613 | +0.13% |

The two final full-parsing passes individually change by **+0.15%** and **+1.28%**. Earlier iterations also show substantial order/host variation, so these results do not establish a general parsing speedup. The chart-bearing rows expose the rendering cost of the accuracy fixes rather than hiding it in the all-file aggregate. Deferred extraction avoids the SVG work.

A separate four-pair comparison isolates the retained ASCII-lookup and owned-data optimizations from the preceding fidelity candidate: **1.077311 → 1.059017 ms across 18 chart-bearing inputs (-1.70%)**. All four pairs improve, by 0.72–2.11%, and all 18 text outputs are identical. All six visual models are byte-identical through this optimization step. This comparison precedes the final radar hairline paint adjustment; the full parsing/extraction table above uses the final binaries.

Complete outputs are unchanged for **756/756 extracted texts and 256/256 deferred extraction bundles**. ODT pooled/mean text F1 remains **0.998900 / 0.980001** over 220 scored references, and ODP remains **0.936054 / 0.890733** over 86. Peer products were not rerun.


All **463 existing workspace tests** pass, and the WASM workspace check passes on Rust 1.93.1. The 110 existing chart-data probes pass across packaged and flat text, presentation, drawing and spreadsheet containers, including precision, sparse indices and scatter/bubble data. Eager, deferred and serialized model data agree for **67 charts across 260 documents**. The native direct-data route also agrees with the XML route for all 33 corpus charts.

The availability audit remains **756/764 parsed**, with the same eight encrypted/invalid failures, 33/33 chart SVGs, 2,346 drawings, 708/710 evaluated custom geometries and 696 rendered custom geometries. Availability is not a visual-fidelity measure. ODS/FODS are checked through the shared ODF API, not a spreadsheet viewer. No Extend Web deployment or endpoint validation is claimed.

## Provenance, rejected experiments and limitations

The [pinned source manifest](../scripts/odf-visual/libreoffice-sources.json) includes the additional diagram, polar-label and text-shape behavior references. Rust changes are independently implemented from format semantics and observed layout behavior. No third-party implementation was copied or mechanically translated into workspace crates. The inspected reference sources and their notices remain in local evaluation output. This finite source review does not prove universal originality.

Existing licensed font assets and advance tables are unchanged. The ascent/descent values are font metadata, separately recorded with source font hashes in `native-font-metadata.json`; they are not implementation code. No font asset or new dependency was added. The original font attribution and license notices remain intact.

Rejected experiments include a layout variant that reduced automatic-layout agreement, an incorrect 12 pt title default, and applying the new baseline placement to saved inner plot layouts. Width-only, rounded-width and spacing-only variants were compared before selecting the bounded text-width path. A prior glyph-outline implementation remains rejected because of its measured rendering cost. Raster inspection rejected the nominal PDF hairline width for radar paint: a one-pixel minimum improves the measured result more than the alternative crisp-edge rendering. The viewer strips `font-size-adjust`, so the prototype using that attribute provides no evidence of x-height correction.

Remaining gaps include fallback glyph shapes and height, complex-script shaping, exact legend layout, long-label collisions and bounds, title handling outside automatic radar charts, advanced axes/3D, and complete host-document layout. The minimum plot and overflow solver approximates native positioning; it does not reproduce every native layout decision.

## Disk cleanup and retained evidence

Removed obsolete `scripts/odf-visual/target`, `target/debug/incremental` and `target/debug/examples` caches after checking that no tracked files were included and no Rust build used the caches. The deleted files total 10.552 GB logically; APFS allocation and other concurrent work prevent treating that as an exact net free-space increase. No fixture, native reference, frozen comparison binary, report, source or license was deleted. Current build targets and prior evidence referenced by the new run remain available.

Local evidence is in `output/odf-font-labels-20261113`: frozen baseline/candidate executables and source receipts; six visual reports; AB/BA parsing and extraction runs; complete outputs; data/model checks; corpus audit; test and WASM logs; native font metadata; rejected experiment records; and `cleanup.jsonl`. The baseline receipt verifies all 230 source/manifest hashes against the baseline commit and verifies the copied executable hashes. The new probe generator is [generate_font_layout.py](../scripts/odf-visual/generate_font_layout.py).

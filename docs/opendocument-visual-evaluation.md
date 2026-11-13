# OpenDocument charts and artwork — 2026-11-12

Latest follow-up: [OpenDocument parity and latency hill-climb](opendocument-hillclimb-20261113.md).

The [November 13 follow-up](opendocument-fidelity-20261113.md) fixes additional corpus-derived chart gaps and reports fresh latency, extraction and visual measurements against this checkpoint.

The ODF work adds shared Rust geometry evaluation, SVG artwork and chart data extraction. It does not establish complete visual parity with LibreOffice. The existing OpenDocument text benchmark measures a different property and is not evidence of graphic fidelity.

Implementation lives in `rust-odf`, `rust-office-art::odf` and `rust-office-charts::odf`, with thin ODT and ODP/ODG adapters. Enhanced paths retain equations, modifiers, arcs, independent paint flags, mirrors and affine transforms. Chart parsing retains styles, cached values, repeated cells, sparse positions, series orientation and scatter/bubble domains. Named chart gradients, font properties, saved plot coordinates and circular-chart direction feed the shared chart renderer.

## Native comparison

The reference engine is LibreOffice 26.2.2.2 at commit `1f77d10d6938fd34972958f64b2bcfa54f8b1ba5`. Pinned source observations and reproduction commands are in [the harness](../scripts/odf-visual/README.md). The comparison uses full 432 × 432 pages at 96 dpi, the existing presentation viewer, Chromium 157.0.8103.78 and device scale 1. Neither version receives per-object alignment, cropping or resizing. The score is RGB agreement within 16 on the union of visible ink below 247; white margins cannot raise it.

Measured reports, frozen binaries, source receipts and captures are under `output/odf-visual-20261112`. The primary reports are `measured/report/index.html` (controlled probes), `real-charts/report/index.html` (21 distinct embedded charts) and `supplemental/report/index.html` (additional behavior probes). `provenance.json` records the final executable and source hashes.

| Set | Cases | Baseline median | Candidate median |
| --- | ---: | ---: | ---: |
| Controlled artwork | 15 | 43.1% | 97.5% |
| Controlled charts | 11 | 0.07% | 77.8% |
| Corpus-derived charts | 21 | 0.7% | 32.2% |

The six supplemental probes include matrix/skew transforms, independent fill/stroke opacity, lazy conditionals, logical coordinates and transformed presets. Two produced blank native pages and are inconclusive; both remain explicitly flagged and score zero in that aggregate. The other four have 94.9–100% candidate agreement. They are not pooled into the primary matrix.

The corpus-derived charts preserve chart content and styles in uniformly fitted frames and remove saved previews to exercise native rendering. This measures isolated chart behavior, not complete original-page layout. The substantially lower corpus score matters: the controlled matrix does not establish real-document chart parity.

## Downstream chart extraction

`rust-office-charts::odf::extract_chart_data` returns the same `ChartData` structure as the Microsoft chart parsers: title, series type/name, zero-based point indices, categories, value, X, Y and bubble size. Original numeric spellings are retained instead of round-tripping through floating point. This path is independent of native SVG support, including unsupported 3D or secondary-axis artwork. Stored cache values are read; external workbooks and formulas are not recalculated.

ODT figures carry `chartPartName` and `chart_data`; document bundles collect model-owned chart data before looking for OOXML parts. ODP/ODG chart nodes supply the shared chart description, so existing presentation extraction bundles and `extract_chart_data` work. Existing saved previews remain usable while chart data is retained separately. With `includeCharts: true`, the public ODT, FODT, ODP, FODP, ODG and FODG bundles each return all 11 probe charts with identical data. ODS and FODS retain identical data through the shared ODF API.

Extend's native Word chart-table consumer already reads this `charts` map and its value/X/Y/bubble-size fields. **ODS is not integrated end to end:** Extend currently excludes ODS from its native spreadsheet route. Publishing these crates and updating the downstream worker are separate from local library validation; neither was performed here.

`data-contract/report.json` records cross-container bundle checks, scatter/bubble domain checks, exact high-precision numeric strings and sparse positions with unavailable artwork. `corpus-audit-final.json` records 756 parsed documents out of 764, 33 extracted chart objects, and 708 evaluated custom geometries out of 710. The eight rejected inputs and all unsupported artwork remain in the report. These are availability counts, not visual scores.

## Remaining coverage

Radar layout, automatic label placement/rotation, chart labels and number formats, some gradients, advanced chart layouts and typography still differ. Native 3D and secondary-axis chart artwork is rejected while cached data remains available. Drawing extrusions, text warps, hatch/bitmap fills and extended geometry commands are not complete. Full Writer pagination, full Calc rendering, external chart-data resolution and every OpenDocument subtype were not visually validated. A saved preview can preserve appearance for features the native renderer does not yet reproduce.

Validation includes workspace compilation, existing ODF import tests, shared chart tests, existing Word shape tests and the public-bundle evaluation probes. No new unit tests were added.

The [automatic-layout and direct-extraction follow-up](opendocument-layout-extraction-20261113.md) records the next frozen-candidate comparison and its limitations.

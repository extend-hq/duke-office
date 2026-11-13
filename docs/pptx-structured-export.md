# Presentation bundles and semantic export

`duke-slides` provides a portable extraction and export contract for native and WebAssembly callers. Storage, OCR, figure descriptions, chunking, access control, and process scheduling remain host responsibilities.

## Bundle

```rust
use duke_slides::{
    parse_presentation_bundle, export_presentation, ExtractPresentationBundleOptions,
    ExportRequest,
};

let options = ExtractPresentationBundleOptions {
    include_charts: true,
    deduplicate_assets: true,
    ..Default::default()
};
let bundle = parse_presentation_bundle(&bytes, options)?;
let output = export_presentation(&ExportRequest {
    blocks: bundle.document.blocks,
    assets: bundle.assets,
    ..Default::default()
})?;
```

`extract_presentation_bundle(&document, options)` accepts an existing immutable `PresentationDocument`. Both entry points return `Result<PresentationBundle, String>` and follow the DOCX bundle's flattened JSON shape: `schemaVersion`, `parserVersion`, `documentId`, `blocks`, `geometryStatus`, `charts`, `assets`, and `comments`.

Presentations additionally expose `format`, `size` in EMU, `slides`, document `warnings`, and optional document metadata. Each slide manifest retains its original zero-based `slideIndex`, ID, optional name, hidden state and source part, root `blockIds`, and slide warnings. Empty slides remain in the manifest. Comments retain their typed content and replies together with their owning `slideIndex`.

Options use camelCase in JSON. `includeWords` and `includeNotes` are flattened from `ExtractPresentationBlocksOptions`. `includeCharts`, `deduplicateAssets`, and `includeArtwork` default to false. Parsing a bundle defers generated artwork unless requested; extracting from an existing model only uses artwork already present. Notes are associated with their original slide and do not have slide-content geometry.

The lightweight `extract_presentation_blocks` API remains available. Rich extraction preserves its block IDs, content, hierarchy, source refs, and optional word refs. Paragraph details add typed `ParagraphInline` entries with original UTF-16 `start`/`end` offsets, text, resolved run properties, hyperlinks, optional fields, and Office Math. Paragraph properties, list labels, and label formatting remain separate from the text addressed by source offsets. Common Arabic, alphabetic, and Roman numbering schemes produce labels; other schemes and picture bullets retain their original metadata without invented labels.

Table details include `rowCount`, `columnCount`, column widths, and row heights. Origin cells retain `rowIndex`, `columnIndex`, `rowSpan`, `columnSpan`, fill, borders, insets, alignment, and rich paragraphs. Covered merged cells do not become duplicate content. Containers summarize descendants; consumers must not concatenate both summaries and descendants.

## Shared chart data

With `includeCharts: true`, `charts` maps chart part names to the exact `duke-office-charts::ChartData` type also used by `duke-pages::DocumentBundle`. Chart figures expose `details.chartPartName` and `details.chartId`. If an imported model lacks a chart part name, its block ID is the chart key and `chartPartName` is omitted. Repeated occurrences of the same chart part share one data entry.

Cached and literal values retain their original string precision, sparse point indices, hierarchical categories, X/Y coordinates, bubble sizes, series names, and per-series chart types. Formulas and external workbook references are not evaluated. Missing or unreadable chart data produces a scoped `chart-data-unavailable` warning. An empty cache remains empty; the exporter does not fabricate values from artwork.

The shared types are available under `duke_slides::chart::{ChartData, ChartSeries, ChartPoint, ChartCategory}`. `ChartData`, `ChartPoint`, and `ChartCategory` are also re-exported at the crate root. `ExtractedChartSeries` aliases the shared series type; the existing root `ChartSeries` remains the simplified rendering-model type for compatibility. `extract_chart_data(&document, part_name)` reads shared chart data independently of bundle extraction or artwork rendering.

## Assets and appearance

Deduplicated bundles use document-local content hashes in `assets`, whose values are data URLs. Blocks reference them through `imageAssetId`, `sourceImageAssetId`, and `previewImageAssetId`, matching the DOCX contract. Without deduplication, the corresponding `imageUrl`, `sourceImageUrl`, and `previewImageUrl` fields contain the payloads. Assets are shared across repeated placements, while each placement retains its own source refs, transform, crop, opacity, and effects. An existing `assetId` in lightweight figure details identifies the original model asset rather than a bundle payload.

With artwork requested, available generated SVG becomes the display image while the original image remains separately accessible. Media can expose a saved poster; extraction does not download external media or execute embedded content. This bundle does not opt into embedded 3D model payloads.

Object transforms remain in their source coordinate frames. `geometryStatus` stays `requires_layout`; they are not paragraph or word rectangles. The original drawing-tree order and exact-input document identity remain unchanged.

## Export

`export_presentation(&ExportRequest)` returns combined `content` and a `blockContent` map keyed by every block ID, following DOCX export. Combined content traverses root blocks once. Parent summaries are not emitted alongside their children. Export is semantic HTML, not slide layout.

`ExportRequest` accepts `blocks`, `options`, `overrides`, and `assets`. Both parsers re-export the same `TableFormat`, `ContentOverride`, and `ExportOptions` types from `duke-office-art::export`. The existing DOCX import paths and defaults remain compatible. `groupChanges` applies only to Word revision export.

Text retains supported run and paragraph styles, hyperlinks, list labels, and equations regenerated through the shared Office Math parser. Text and attributes are escaped, CSS values are constrained, and links and image URLs use restricted schemes. Unsupported math retains its text fallback. HTML supplied in a content override is trusted caller markup, as in DOCX, and must be sanitized by the host when appropriate.

`tableFormat` selects `html` (default), `markdown`, or `csv` for tables. HTML retains merged origins and cell styling. Markdown and CSV preserve grid positions, place merged content at its origin, and leave covered cells empty. Export rejects duplicate block IDs, invalid spans, overlapping cells, and excessive table dimensions. It accepts both rich bundles and lightweight blocks, using plain text where rich details are absent.

Asset-map values may be replaced with caller-owned URLs. A `ContentOverride` can replace content or provide a figure caption; `listLabelPresent` prevents repeated list labels, and `suppressImage` omits the figure image. Chart data is returned independently, so hosts can use it to generate their preferred chart tables or descriptions and supply those as overrides.

## Limits

Bundle options contain `parseLimits` and `limits`. The bundle defaults to `ParseLimits::server()`: 128 MiB input, 128 MiB per ZIP entry, and 512 MiB expanded archive size, together with the existing entry-count and XML-complexity limits. Partial JSON parse-limit overrides retain these bounded defaults. The existing Rust `ParseLimits::default()` remains unchanged.

`ExtractionLimits` defaults are:

| Limit | Default |
|---|---:|
| Block depth | 64 |
| Blocks | 100,000 |
| Words | 1,000,000 |
| Slides | 10,000 |
| Table columns | 1,024 |
| Cells per table | 1,000,000 |
| Unique exported assets | 10,000 |
| Unique asset bytes | 128 MiB |
| Serialized output bytes | 128 MiB |

`extract_presentation_blocks_with_limits` and `extract_presentation_block_input_with_limits` expose the same limits to lightweight callers. The original lightweight APIs apply default extraction limits. Structure is checked before recursive projection; word and asset production are bounded; rich details and final serialized output are checked without allocating a second JSON buffer. Comment threads separately obey the same count and depth limits before cloning. Limits return errors prefixed with `LIMIT:` rather than silently truncating output. Shared `ExportOptions` separately bound export depth, block count, table dimensions, and output size.

These budgets constrain parsing and extraction, not total process memory or elapsed time. Hosts still need cancellation, deadlines, memory isolation, and bounded concurrency.

## WebAssembly

The DOCX-style JSON entry points are `parse_presentation_bundle_wasm(bytes, options_json)` and `export_presentation_wasm(request_json)`. The object-valued `extract_presentation_bundle(document, options)` can enrich an existing model. Unlike the lightweight extractor, it needs the full selected model and assets.

`parse_presentation_with_limits(bytes, options, limits)` and `extract_presentation_blocks_with_limits(document, options, limits)` accept explicit budgets without changing existing JavaScript signatures. Bundle parsing defaults to the same bounded limits in native and WASM builds. Rebuild and publish the corresponding WASM/viewer packages when adopting the new API.

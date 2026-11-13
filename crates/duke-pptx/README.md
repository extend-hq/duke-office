# duke-pptx

by Extend

Parse PowerPoint presentations from bytes. The parser detects OOXML presentations and legacy PowerPoint binary presentations and returns a structured model of slides, shapes, text, charts, and embedded assets. Input size, entry size, and total expanded size are unlimited by default, subject to available memory. Entry-count and XML-complexity limits remain enabled; callers can configure size limits through `ParseLimits`.

The default XML element limit is 500,000 per part. Explicit caller limits remain strict. XML trees use compact child and attribute storage, and unused declared layouts are converted one at a time to reduce temporary memory. Generated artwork and retained assets also contribute to memory use; the element limit is not a total-memory limit.

Install with `cargo add duke-pptx`. The Rust library target is named `duke_pptx`.

```rust
use duke_pptx::{parse_presentation, ParseError, ParseLimits, PresentationDocument};

fn parse(bytes: &[u8]) -> Result<PresentationDocument, ParseError> {
    parse_presentation(bytes, &ParseLimits::default())
}
```

The WebAssembly bindings are available as `duke-pptx-wasm`.

`extract_presentation_blocks` provides nested object, paragraph, table-cell and optional word blocks with document-scoped source references for React PPTX citation overlays. See the [citation contract](../../docs/pptx-citations.md).

`parse_presentation_bundle` and `extract_presentation_bundle` add rich paragraph inlines, slide manifests, comments, optional shared chart data, and deduplicated image assets. `export_presentation` provides semantic HTML with HTML, Markdown, or CSV tables, asset substitutions, and content overrides. The shared chart and export types match the Word APIs, while presentation source references remain unchanged. These APIs expose bounded parse, extraction, and export options. See the [structured export contract](../../docs/pptx-structured-export.md).

Legacy binary presentations follow the current edit/persist graph, including the newer stream in dual-storage containers. Speaker notes follow the live notes references and retain plain text and paragraph boundaries; unresolved or mismatched references produce explicit warnings. Notes-page headers, footers, dates and slide numbers are excluded from speaker notes. Legacy rich-text formatting and notes-page rendering remain incomplete.

Use `parse_presentation_with_options(&bytes, &ParseLimits::default(), &ParseOptions { include_3d_models: true, ..ParseOptions::default() })` to include original embedded 3D models. Image nodes then expose `model3d`, containing the model asset bytes, part name, content type, relationship IDs, optional preview metadata and source XML preserving camera, transform and lighting settings. The saved 2D preview remains the image's `assetId`. Missing previews do not discard the model asset. The option defaults to `false`: `parse_presentation` omits `model3d` and 3D model payloads from the asset map. This does not decode meshes or render new camera views.

Set `defer_artwork: true` to omit the generated shape, picture, chart and ink SVG (`shapeSvg`, `pictureSvg`, `chartSvg`, `paintBounds`, `fallbackPicture`) and the shape, picture and chart warnings only rendering can raise. Every other field is identical to the eager parse, including geometry, fills, lines, text, assets and source XML. Ink content parts are still rendered internally, because whether the render succeeds decides between the ink and its saved preview in markup-compatibility fallbacks; only their SVG is omitted, so `ink-fallback` warnings are unchanged. `render_node_artwork(&bytes, &limits, &options, &ArtworkTarget { slide_index: Some(0), node_id: "7".into(), ..ArtworkTarget::default() })` then produces one node's artwork on demand, equal to what the eager parse stores; `layout_id` or `master_id` selects a layout or master node list instead, and inherited decorative nodes keep their `layout:`/`master:` id prefixes. Each call parses the presentation again without rendering, so it suits viewers that render a few visible nodes rather than every node of a large deck.

For many renders of one deck, `parse_presentation_retained(&bytes, &limits, &options)` parses once with artwork deferred and returns a `ParsedPresentation` that keeps the inflated slide, layout, master and theme parts with their XML trees, relationships, color maps and related chart, diagram, ink and control parts. `handle.document()` is the deferred model, `handle.into_document()` gives it up, and `handle.render_node_artwork(&target)` renders one node from the retained state without reading the archive again; it parses only the node list that can hold the target and renders that node, and the result equals the eager parse's artwork and what `render_node_artwork` returns. Each part sees exactly the assets the parse had loaded when it reached that part, as the eager parse does. The handle holds the parts in addition to the model, so it uses more memory than the model alone, and it is not `Sync`; drop it when the viewer closes the presentation.

DrawingML text runs can retain typed field identity, open type strings, cached-value presence, paragraph properties and scoped source XML. Run styles apply field defaults before explicit field formatting. Empty fields survive in notes and semantic SmartArt fallbacks; fields are not dynamically evaluated. See the [field contract and remaining layout boundaries](../../docs/pptx-fields.md).

Tables retain their complete source cell grid. Covered cells inside merged regions keep their text and formatting with explicit merge flags and an optional origin position. Display and visible-text consumers must skip covered cells. See the [table grid contract and measurement scope](../../docs/pptx-tables.md).

OOXML object metadata retains typed editing locks, connector target IDs and connection-site indices, explicit object-property booleans, and qualified source XML. React can resolve endpoints within their original source part, including nested groups and inherited nodes. This does not reroute connectors or enforce editing restrictions in the viewer. See the [object editing contract](../../docs/pptx-editing.md).

Pictures retain an optional `fallbackAssetId` when a selected embedded SVG has a distinct saved representation in the main blip. The association follows picture fill inheritance and media posters. See the [image representation contract and renderer boundaries](../../docs/pptx-images.md).

Picture and shape paint is produced by `duke-office-art` when supported. Image nodes keep their original `assetId` and `sourceXml` alongside optional `pictureSvg`; shape nodes expose optional `shapeSvg`. Both can include `paintBounds`, measured in CSS pixels relative to the unrotated layout frame. Group resizing determines the paint frame before local rotation. Zero-axis connectors use a one-pixel paint layout basis while retaining zero in their source transform. The SVG already contains local rotation, flips, clipping and effects. Consumers should position it using the paint bounds without applying those operations twice. Shape text remains structured so viewers can lay it out separately. Themes retain `formatSchemeXml` for inspecting styles that are not fully represented by the simplified model.

Charts use `duke-office-charts` with extended chart support and retain chart XML, companion styles and cached series. An unavailable shared chart SVG is reported explicitly so a consumer can choose its source or cached-data fallback.

SmartArt uses the shared bounded data-model and cached-drawing readers. Cached shapes, connectors, pictures and nested groups retain drawing order and authoritative saved text; absent or empty cached text bodies stay empty. Diagram relationships are resolved against their owning parts. Independent text-frame offsets, extents, rotation and flips are exposed separately from the shape transform for the viewer to render; incomplete frames produce a warning and use the shape frame. When a cached drawing is absent, the parser emits a clearly marked semantic layout fallback; this does not reproduce the original SmartArt layout algorithm. The [coverage report](../../docs/smartart-coverage.md) distinguishes the tested layout families from native pixel parity.

Coverage is bounded by the shared renderers. Unsupported picture and shape appearances retain source XML and structured fallback properties with warnings. General 3D rendering, arbitrary effect graphs, unsupported codecs, and regeneration of every SmartArt layout remain outside this implementation. This is not a claim of universal PowerPoint visual parity.

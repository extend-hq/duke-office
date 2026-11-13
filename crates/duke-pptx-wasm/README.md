# duke-pptx-wasm

by Extend

WebAssembly bindings for the Extend PowerPoint parser. The library target is named `duke_pptx_wasm`.

Build with `wasm-pack build --target web` and initialize the generated JavaScript module before calling `detect_presentation_format` or `parse_presentation`. Both functions accept a `Uint8Array`. Parsing uses the default limits from `duke-pptx` and returns a slide document with keyed assets and byte buffers.

`parse_presentation(bytes, { include3dModels: true })` includes original embedded 3D model payloads and adds `model3d` to their image nodes. Model bytes are exposed as `model3d.asset.data` (`Uint8Array`), alongside the part name, content type, preview metadata and scene XML. The option defaults to `false`; saved 2D previews remain available either way.

`parse_presentation(bytes, { deferArtwork: true })` omits generated artwork (`shapeSvg`, `pictureSvg`, `chartSvg`, `paintBounds`, `fallbackPicture`) and the shape, picture and chart rendering-only warnings; all other fields match the eager parse, and ink is still rendered internally to choose its saved-preview fallback, so `ink-fallback` is unchanged. `render_presentation_node_artwork(bytes, options, { slideIndex: 0, nodeId: "7" })` returns that node's `{ svg, paintBounds?, fallbackPicture? }` or `undefined`; use `layoutId` or `masterId` in place of `slideIndex` for layout and master nodes. Each call parses the presentation again without rendering.

To render many nodes of one deck, create `const handle = new ParsedPresentation(bytes, options)`: it parses once with `deferArtwork` and keeps the inflated parts and their XML trees in WebAssembly memory. `handle.document()` returns the deferred model, and `handle.render_node_artwork({ slideIndex: 0, nodeId: "7" })` returns the same `{ svg, paintBounds?, fallbackPicture? }` or `undefined` as `render_presentation_node_artwork`, without parsing the file again. Call `handle.free()` when the presentation is closed; the handle is larger than the model it returns.

## Source-grounded blocks

Version 0.2.0 exports `extract_presentation_blocks(document, { includeWords, includeNotes })`. Pass the model returned by `parse_presentation`. The result contains nested blocks, document/slide/node source identity and optional UTF-16 word references. The extractor reads only identity and text fields, without copying assets or generated artwork back into WASM. React PPTX resolves these references against mounted slide DOM to render citation overlays.

## Structured bundles and export

`parse_presentation_bundle_wasm(bytes, options_json)` and `export_presentation_wasm(request_json)` expose the same JSON contracts as the native bundle and semantic exporter. `extract_presentation_bundle(document, options)` accepts a full existing model and returns a JavaScript object. Bundles include slide manifests, rich inlines, comments, optional shared chart data, and deduplicated assets.

`parse_presentation_with_limits(bytes, options, limits)` and `extract_presentation_blocks_with_limits(document, options, limits)` allow explicit parser and extraction budgets. Existing signatures remain unchanged. See the [structured export contract](../../docs/pptx-structured-export.md) for options, defaults, chart types, and content overrides.

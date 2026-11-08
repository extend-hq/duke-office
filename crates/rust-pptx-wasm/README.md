# rust-pptx-wasm

by Extend

WebAssembly bindings for the Extend PowerPoint parser. The library target is named `pptx_wasm`.

Build with `wasm-pack build --target web` and initialize the generated JavaScript module before calling `detect_presentation_format` or `parse_presentation`. Both functions accept a `Uint8Array`. Parsing uses the default limits from `rust-pptx` and returns a slide document with keyed assets and byte buffers.

`parse_presentation(bytes, { include3dModels: true })` includes original embedded 3D model payloads and adds `model3d` to their image nodes. Model bytes are exposed as `model3d.asset.data` (`Uint8Array`), alongside the part name, content type, preview metadata and scene XML. The option defaults to `false`; saved 2D previews remain available either way.

`parse_presentation(bytes, { deferArtwork: true })` omits generated artwork (`shapeSvg`, `pictureSvg`, `chartSvg`, `paintBounds`, `fallbackPicture`) and the shape, picture and chart rendering-only warnings; all other fields match the eager parse, and ink is still rendered internally to choose its saved-preview fallback, so `ink-fallback` is unchanged. `render_presentation_node_artwork(bytes, options, { slideIndex: 0, nodeId: "7" })` returns that node's `{ svg, paintBounds?, fallbackPicture? }` or `undefined`; use `layoutId` or `masterId` in place of `slideIndex` for layout and master nodes. Each call parses the presentation again without rendering.

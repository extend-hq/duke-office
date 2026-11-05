# rust-pptx-wasm

by Extend

WebAssembly bindings for the Extend PowerPoint parser. The library target is named `pptx_wasm`.

Build with `wasm-pack build --target web` and initialize the generated JavaScript module before calling `detect_presentation_format` or `parse_presentation`. Both functions accept a `Uint8Array`. Parsing uses the default limits from `rust-pptx` and returns a slide document with keyed assets and byte buffers.

`parse_presentation(bytes, { include3dModels: true })` includes original embedded 3D model payloads and adds `model3d` to their image nodes. Model bytes are exposed as `model3d.asset.data` (`Uint8Array`), alongside the part name, content type, preview metadata and scene XML. The option defaults to `false`; saved 2D previews remain available either way.

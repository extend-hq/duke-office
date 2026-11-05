# rust-docx-wasm

by Extend

WebAssembly bindings for [rust-docx](https://crates.io/crates/rust-docx). They expose parsing, editing model construction, structured extraction and serialization to JavaScript. The [React DOCX viewer](https://github.com/extend-hq/react-docx) packages these bindings for browsers. Rust applications can call the parser directly through `rust-docx`.

Build the browser module from the Rust Office workspace with `wasm-pack build crates/rust-docx-wasm --target web --out-name docx_wasm`.

Use `build_doc_model_from_bytes` for combined package and model import, or `build_doc_model_from_package_js` to construct a model from an existing package. These APIs transfer image data outside JSON and share immutable `Uint8Array` asset buffers across repeated images. Copy a buffer before mutating its bytes. The legacy `build_doc_model_from_package` JSON-string API remains available for compatibility.

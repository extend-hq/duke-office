# extend-docx-wasm

WebAssembly bindings for [extend-docx](https://crates.io/crates/extend-docx). They expose parsing, editing model construction, structured extraction and serialization to JavaScript. The [React DOCX viewer](https://github.com/extend-hq/react-docx) packages these bindings for browsers. Rust applications can call the parser directly through `extend-docx`.

Build the browser module from the Extend Office workspace with `wasm-pack build crates/extend-docx-wasm --target web --out-name docx_wasm`.

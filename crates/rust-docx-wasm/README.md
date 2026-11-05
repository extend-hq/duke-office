# rust-docx-wasm

by Extend

WebAssembly bindings for [rust-docx](https://crates.io/crates/rust-docx). They expose parsing, editing model construction, structured extraction and serialization to JavaScript. The [React DOCX viewer](https://github.com/extend-hq/react-docx) packages these bindings for browsers. Rust applications can call the parser directly through `rust-docx`.

Build the browser module from the Rust Office workspace with `wasm-pack build crates/rust-docx-wasm --target web --out-name docx_wasm`.

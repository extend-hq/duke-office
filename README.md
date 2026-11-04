# Extend Office

Rust libraries for parsing Word documents and PowerPoint presentations, and rendering Office charts. The chart renderer is shared by [React DOCX](https://github.com/extend-hq/react-docx), [React PPTX](https://github.com/extend-hq/react-pptx) and [React XLSX](https://github.com/extend-hq/react-xlsx). React XLSX uses DukeSheets to read workbooks and resolve worksheet values.

| Crate | Purpose |
| --- | --- |
| `office-charts` | Read Office chart definitions and render them to SVG |
| `extend-docx` | Parse Word documents into a structured document model |
| `extend-docx-wasm` | WebAssembly bindings for the Word parser |
| `extend-pptx` | Parse PowerPoint presentations into a structured slide model |
| `extend-pptx-wasm` | WebAssembly bindings for the PowerPoint parser |

The Rust workspace builds with Cargo. Run `cargo test --workspace --locked` for native validation. For the browser bindings, install the `wasm32-unknown-unknown` Rust target.

The libraries are distributed under the [MIT license](LICENSE). Third-party notices accompany the crates that use them.

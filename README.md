# Extend Office

[![CI](https://github.com/extend-hq/extend-office/actions/workflows/check.yml/badge.svg)](https://github.com/extend-hq/extend-office/actions/workflows/check.yml) [![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Rust libraries for parsing Word documents and PowerPoint presentations, and rendering Office charts. The chart renderer is shared by [React DOCX](https://github.com/extend-hq/react-docx), [React PPTX](https://github.com/extend-hq/react-pptx) and [React XLSX](https://github.com/extend-hq/react-xlsx). React XLSX uses DukeSheets to read workbooks and resolve worksheet values.

| Crate | Purpose | Version | Downloads | API docs |
| --- | --- | --- | --- | --- |
| [extend-office-charts](crates/extend-office-charts) | Render Office chart definitions to SVG | [![extend-office-charts version](https://img.shields.io/crates/v/extend-office-charts.svg)](https://crates.io/crates/extend-office-charts) | [![extend-office-charts downloads](https://img.shields.io/crates/d/extend-office-charts.svg)](https://crates.io/crates/extend-office-charts) | [![extend-office-charts documentation](https://docs.rs/extend-office-charts/badge.svg)](https://docs.rs/extend-office-charts) |
| [extend-office-charts-wasm](crates/extend-office-charts-wasm) | Render column and line charts in the browser | [![extend-office-charts-wasm version](https://img.shields.io/crates/v/extend-office-charts-wasm.svg)](https://crates.io/crates/extend-office-charts-wasm) | [![extend-office-charts-wasm downloads](https://img.shields.io/crates/d/extend-office-charts-wasm.svg)](https://crates.io/crates/extend-office-charts-wasm) | [![extend-office-charts-wasm documentation](https://docs.rs/extend-office-charts-wasm/badge.svg)](https://docs.rs/extend-office-charts-wasm) |
| [extend-docx](crates/extend-docx) | Parse Word documents into a structured document model | [![extend-docx version](https://img.shields.io/crates/v/extend-docx.svg)](https://crates.io/crates/extend-docx) | [![extend-docx downloads](https://img.shields.io/crates/d/extend-docx.svg)](https://crates.io/crates/extend-docx) | [![extend-docx documentation](https://docs.rs/extend-docx/badge.svg)](https://docs.rs/extend-docx) |
| [extend-docx-wasm](crates/extend-docx-wasm) | WebAssembly bindings for the Word parser | [![extend-docx-wasm version](https://img.shields.io/crates/v/extend-docx-wasm.svg)](https://crates.io/crates/extend-docx-wasm) | [![extend-docx-wasm downloads](https://img.shields.io/crates/d/extend-docx-wasm.svg)](https://crates.io/crates/extend-docx-wasm) | [![extend-docx-wasm documentation](https://docs.rs/extend-docx-wasm/badge.svg)](https://docs.rs/extend-docx-wasm) |
| [extend-pptx](crates/extend-pptx) | Parse PowerPoint presentations into a structured slide model | [![extend-pptx version](https://img.shields.io/crates/v/extend-pptx.svg)](https://crates.io/crates/extend-pptx) | [![extend-pptx downloads](https://img.shields.io/crates/d/extend-pptx.svg)](https://crates.io/crates/extend-pptx) | [![extend-pptx documentation](https://docs.rs/extend-pptx/badge.svg)](https://docs.rs/extend-pptx) |
| [extend-pptx-wasm](crates/extend-pptx-wasm) | WebAssembly bindings for the PowerPoint parser | [![extend-pptx-wasm version](https://img.shields.io/crates/v/extend-pptx-wasm.svg)](https://crates.io/crates/extend-pptx-wasm) | [![extend-pptx-wasm downloads](https://img.shields.io/crates/d/extend-pptx-wasm.svg)](https://crates.io/crates/extend-pptx-wasm) | [![extend-pptx-wasm documentation](https://docs.rs/extend-pptx-wasm/badge.svg)](https://docs.rs/extend-pptx-wasm) |

The Rust workspace builds with Cargo. Run `cargo test --workspace --locked` for native validation. For the browser bindings, install the `wasm32-unknown-unknown` Rust target.

See [Publishing Rust crates](RELEASING.md) for versioning, CI checks and automatic publication from `main`.

The libraries are distributed under the [MIT license](LICENSE). Third-party notices accompany the crates that use them.

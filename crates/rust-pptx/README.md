# rust-pptx

by Extend

Parse PowerPoint presentations from bytes. The parser detects OOXML presentations and legacy PowerPoint binary presentations and returns a structured model of slides, shapes, text, charts, and embedded assets. Parsing limits bound input size, archive expansion, and XML complexity.

Install with `cargo add rust-pptx`. The Rust library target is named `rust_pptx`.

```rust
use rust_pptx::{parse_presentation, ParseError, ParseLimits, PresentationDocument};

fn parse(bytes: &[u8]) -> Result<PresentationDocument, ParseError> {
    parse_presentation(bytes, &ParseLimits::default())
}
```

The WebAssembly bindings are available as `rust-pptx-wasm`.

# rust-pptx

by Extend

Parse PowerPoint presentations from bytes. The parser detects OOXML presentations and legacy PowerPoint binary presentations and returns a structured model of slides, shapes, text, charts, and embedded assets. Input size, entry size, and total expanded size are unlimited by default, subject to available memory. Entry-count and XML-complexity limits remain enabled; callers can configure size limits through `ParseLimits`.

Install with `cargo add rust-pptx`. The Rust library target is named `rust_pptx`.

```rust
use rust_pptx::{parse_presentation, ParseError, ParseLimits, PresentationDocument};

fn parse(bytes: &[u8]) -> Result<PresentationDocument, ParseError> {
    parse_presentation(bytes, &ParseLimits::default())
}
```

The WebAssembly bindings are available as `rust-pptx-wasm`.

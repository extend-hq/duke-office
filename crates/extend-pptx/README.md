# extend-pptx

Parse PowerPoint presentations from bytes. The parser detects OOXML presentations and legacy PowerPoint binary presentations and returns a structured model of slides, shapes, text, charts, and embedded assets. Parsing limits bound input size, archive expansion, and XML complexity.

Install with `cargo add extend-pptx`. The Rust library target is named `pptx_core`.

```rust
use pptx_core::{parse_presentation, ParseError, ParseLimits, PresentationDocument};

fn parse(bytes: &[u8]) -> Result<PresentationDocument, ParseError> {
    parse_presentation(bytes, &ParseLimits::default())
}
```

The WebAssembly bindings are available as `extend-pptx-wasm`.

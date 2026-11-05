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

Picture and shape paint is produced by `rust-office-art` when supported. Image nodes keep their original `assetId` and `sourceXml` alongside optional `pictureSvg`; shape nodes expose optional `shapeSvg`. Both can include `paintBounds`, measured in CSS pixels relative to the unrotated layout frame. Group resizing determines the paint frame before local rotation. Zero-axis connectors use a one-pixel paint layout basis while retaining zero in their source transform. The SVG already contains local rotation, flips, clipping and effects. Consumers should position it using the paint bounds without applying those operations twice. Shape text remains structured so viewers can lay it out separately. Themes retain `formatSchemeXml` for inspecting styles that are not fully represented by the simplified model.

Charts use `rust-office-charts` with extended chart support and retain chart XML, companion styles and cached series. An unavailable shared chart SVG is reported explicitly so a consumer can choose its source or cached-data fallback.

SmartArt uses the shared bounded data-model and cached-drawing readers. Cached shapes, connectors, pictures and nested groups retain drawing order and semantic text associations. Diagram relationships are resolved against their owning parts. Independent text-frame transforms are preserved in source XML and reported as unsupported; text uses the shape frame as a rendering fallback. When a cached drawing is absent, the parser emits a clearly marked semantic layout fallback; this does not reproduce the original SmartArt layout algorithm.

Coverage is bounded by the shared renderers. Unsupported picture and shape appearances retain source XML and structured fallback properties with warnings. General 3D rendering, arbitrary effect graphs, unsupported codecs, and regeneration of every SmartArt layout remain outside this implementation. This is not a claim of universal PowerPoint visual parity.

# Changelog

## 0.2.22

- Re-export shared semantic export options, overrides, and escaping from `rust-office-art`, preserving existing import paths and behavior.
- Speed up native UTF-8 validation and use SIMD base64 when available in WebAssembly.
- Update shared Office Art and chart dependencies.

## 0.2.21

- Continue numbering across related instances of the same abstract list, preserving parent counters, explicit starts, and level restart rules.
- Include resolved list-marker character formatting in structured blocks and preserve it in HTML exports.
- Keep numbered paragraphs with non-heading styles as text blocks when they carry direct outline levels, while preserving numbered heading styles and original style metadata.
- Export minimum line spacing with normal inline line heights so wrapped text can expand above the requested minimum without overlapping.

## 0.2.20

- `rust-docx-wasm`: `build_doc_model_from_bytes` and `build_doc_model_from_package_js` return a model whose JSON is longer than the JavaScript engine's maximum string length (about 512 MiB in V8) instead of failing with "Cannot create a string longer than 0x1fffffe8 characters". Model JSON above 128 MiB is parsed in pieces; the returned object is the same as before for every other document. The parser crate itself is unchanged from 0.2.19.

## 0.2.19

- Write legacy `.doc` media content types in a fixed order, so a file with several image formats gets the same document identity on every parse. For such a file the model's document id, and the block ids that embed it, can differ from the ones an earlier version returned.
- Resolve paragraph, run and table styles in `styles.xml` order instead of hash-map order, so the parsed model no longer varies between runs or, in WebAssembly, with what the module parsed before.
- Share one work queue between the calling thread and the pool workers when inflating ZIP entries and normalizing XML parts in parallel.
- Speed up case-insensitive XML searches, XML name comparisons and block extraction with unchanged output.

## 0.2.18

- Use the shared portable curve math dependencies for consistent native and WebAssembly artwork.
- Group floating drop caps with their logical paragraphs in structured exports, preserving their source ranges.

## 0.2.17

- Depend on `rust-office-art` 0.1.6 and `rust-office-charts` 0.1.8. The shared Office Art crate adds the `defer_artwork` parse option, which rust-docx leaves off, and speeds up its XML lookups, path-number formatting and pattern tiles with unchanged output.

## 0.2.16

- Accept image `data` as a base64 string or out-of-band transport token, and image `src` as a transport token, so large images no longer expand into one buffered JSON value per byte inside untagged model nodes. Number arrays remain accepted.
- Write DOCX archives into a single output buffer and compress XML and uncompressed media with DEFLATE; already-compressed media is stored.
- Borrow model image bytes during serialization instead of cloning them for every image.
- `rust-docx-wasm`: `serialize_docx_wasm` and the other object-model entry points transfer image bytes and data URIs outside the model JSON. A 66 MB image now adds about 84 MiB of WebAssembly memory instead of about 3 GiB.

## 0.2.15

- Depend on `rust-office-art` 0.1.5 and `rust-office-charts` 0.1.7, which add shared ink and Office Math parsing, deterministic trigonometry and empty 3D chart series handling.

## 0.2.14

- Preserve page-field instructions without a stored result separator and retain instruction-run formatting.
- Exclude the terminal legacy Word footer story guard from rendered content.

## 0.2.13

- Preserve uncached page-field instructions and formatting without synthesizing stored text.

## 0.2.12

- Preserve explicit table and cell margins from legacy Word documents, including per-cell overrides and row-specific defaults.

## 0.2.6

- Render picture transforms and effects through the shared native/WebAssembly `rust-office-art` crate, preserving source presentation properties for export.
- Expand image relationships, legacy bitmap headers, and grouped picture rendering; retain explicit fallbacks where native appearance is not implemented.

## 0.2.5

- Render opaque 24-bit and 32-bit EMF bitmap transfers as PNG-backed SVG previews in native and WebAssembly parsing.

## 0.2.3

- Return visible outline and list markers as paragraph `listLabel` metadata while preserving source text offsets.

## 0.2.2

- Use the Rust allocator for ZIP decompression so debug WebAssembly builds link on current stable Rust.

## 0.2.1

- Move the Rust parser to the Rust Office workspace and render native charts through its shared `rust-office-charts` crate.

## 0.2.0

- Export native classic, 3D, and extended charts as SVG figures, including region maps drawn from the document’s cached geography. Retain the chart’s data and source attribution.

## 0.1.5

- Expose native chart series and indexed data points, preserving stored values and sparse categories.

## 0.1.4

- Return stored comment timestamps from legacy binary Word documents.

## 0.1.3

- Add opt-in typed revision extraction with source block references and UTF-16 text ranges.
- Preserve authors and timestamps on tracked text insertions and deletions from binary Word documents.

## 0.1.2

- Preserve ellipse outlines in DrawingML shape images.

## 0.1.1

- Expand the package overview with native latency, scatter plots, feature recall, and Microsoft Word reference comparisons.
- Use the published `rust-docx` name throughout benchmark charts and tables.
- Publish aggregate precision, recall, and coverage with links to measurement methods and limitations.

The parser implementation is unchanged from 0.1.0. Benchmark measurements retain their original dates and measured revisions.

## 0.1.0

Initial crates.io release of Extend's shared Rust parser, previously named `docx-core` within the repository.

- Publish the Cargo package as `rust-docx`, imported as `rust_docx`.
- Parse DOCX and Word 97–2003 binary DOC into a shared document model and structured blocks.
- Preserve source references and support DOCX serialization through the shared model.
- Support native Rust and WebAssembly, with optional native ZIP decompression enabled by default on native builds.

Page geometry requires the viewer's layout engine. Rust versions are independent of the React viewer's npm versions.

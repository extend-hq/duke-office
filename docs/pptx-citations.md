# Presentation blocks and citations

`extract_presentation_blocks(&document, &options)` returns a `StructuredPresentation` containing nested blocks and source references. The JavaScript binding is `extract_presentation_blocks(document, options)`; React PPTX exports the asynchronous `extractPresentationBlocks(document, options)` wrapper.

```rust
use duke_slides::{extract_presentation_blocks, ExtractPresentationBlocksOptions};

let structured = extract_presentation_blocks(
    &document,
    &ExtractPresentationBlocksOptions { include_words: true, include_notes: true },
)?;
```

The block types are `text_box`, `text`, `table`, `table_cell`, `group`, `figure` and `note`. Text boxes and table cells contain paragraph blocks; groups retain their nested object hierarchy. Only paragraph blocks carry optional word spans. Container content summarizes children and must not be concatenated with those children a second time. Covered cells in merged regions are skipped; the origin cell retains its row and column spans. Figures identify an image, chart, media object or unsupported object; chart titles identify the whole chart, not an individual rendered series label.

Source references carry `documentId`, `slideIndex`, `slideId`, `nodeId`, `nodePath`, `story`, optional `partName` and `sourceId`, and optional cell/paragraph coordinates. `nodePath` indexes the imported slide node tree, including nested groups, and disambiguates repeated object IDs. It identifies the displayed occurrence of inherited content on a particular slide. The source part remains the originating part when available.

Word and arbitrary text-range references use inclusive `start` and exclusive `end` UTF-16 offsets within one paragraph's original run text. Bullet labels are excluded. Offsets include tabs, line breaks and supplementary Unicode characters; styling changes do not reset offsets. Word segmentation uses the pinned Unicode segmentation implementation. Ranges spanning paragraphs are represented by multiple refs.

The parser assigns `documentId` from SHA-256 of the exact input bytes, with a contract prefix. This is intentionally an upload-byte identity, unlike DOCX's canonical package identity. Repacking the same content produces a different ID. Eager and deferred artwork modes share the identity and block refs. Keep models immutable, persist the parser version with extracted blocks, and re-extract citations after editing or upgrading an incompatible parser. Models without source identity are rejected by block extraction; they must be reparsed from the original bytes.

Native PPTX and legacy PPT use the same extraction contract. Legacy source-part and formatting detail remain limited by the imported model. Speaker notes can be extracted with `includeNotes`, but have no slide-content rectangle.

`geometryStatus` is `requires_layout`. Object transforms are not returned as claimed paragraph or word rectangles. React PPTX resolves refs against mounted slide DOM, accounting for its zoom and nested transforms. Geometry consists of axis-aligned, slide-relative CSS-pixel fragments, which may be normalized to fractional slide coordinates. Rotated text can produce conservative enclosing rectangles. This is source grounding in the current viewer, not a claim of PowerPoint rendering fidelity.

The geometry resolver distinguishes rendered, partially rendered, unmounted, invisible and unresolved results. It checks document identity, slide/node paths, source parts and text ranges; it does not match by visible text. Notes, invalid or stale refs, unsupported mathematical subranges, surrogate-splitting ranges and changed text do not receive invented boxes. Non-text artwork receives its object frame; individual chart data marks and embedded-document contents do not have text-range citation support.

The WASM extractor deserializes only the identity and text projection. It does not copy image payloads, generated SVG, chart caches or theme data back into WASM.

The block and source-reference API is introduced in duke-slides and duke-slides-wasm 0.2.0.

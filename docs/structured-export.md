# Structured extraction and export

The unreleased `rust-docx` APIs provide application-independent extraction and HTML export for native and WebAssembly callers. They use stable document block IDs and UTF-16 source offsets, without requiring a React renderer, storage service, or an application response schema.

## Extraction

`parse_document_bundle(bytes, options)` parses DOCX or legacy DOC bytes. `extract_document_bundle(model, package, options)` accepts an existing model and package. Both return a `DocumentBundle` containing:

- Structured blocks, including resolved table styles, row styles, and field-to-paragraph source ranges.
- Document comments and, when requested, tracked revisions and ordered `revisionInlines` for affected paragraphs.
- Optional chart data keyed by package part name.
- Optional deduplicated `assets`, keyed by document-local IDs, containing image or model data URLs. The corresponding block details use `imageAssetId`, `sourceImageAssetId`, `previewImageAssetId`, or `model3d.asset.dataAssetId`.

`ExtractDocumentBundleOptions` defaults to ordinary blocks with comments. `includeWords`, `include3dModels`, `includeRevisions`, `includeCharts`, and `deduplicateAssets` opt into additional output. The existing block and reviewed-document APIs remain available.

A dropped or margin initial and its following paragraph are emitted as one logical paragraph. The block retains the initial's ID, combines the text without inserting a separator, and uses the following paragraph's formatting with the initial's `style.dropCap` metadata. `details.dropCap` identifies the initial with an explicit `{ start, end }` range in the combined text, using zero-based UTF-16 offsets with an exclusive end. The initial's font and other run formatting remain on its text inlines. Ordinary paragraphs omit this range.

Inline offsets address the combined text. Its ordered `sourceRefs` retain both original paragraph IDs and their UTF-16 ranges; a word spanning the join has references to both sources. Revisions and comments retain their original source coordinates, while `revisionInlines` uses the combined paragraph's offsets. The editable document model and serialized Word structure retain the separate source paragraphs. Pairing stays within a story and table cell and does not cross a section break or an explicit page break before the following paragraph.

`revision_inlines(blocks, revisions)` is also public independently. Each span contains a typed paragraph inline and an `unchanged`, `insertion`, or `deletion` change. Removed text is anchored at an empty current-text range; it does not shift later offsets. Table-row and cell changes propagate to their paragraphs. Formatting and structural revision metadata remain in the revision list rather than being approximated as text edits. Invalid ranges, including ranges splitting a Unicode scalar, are ignored.

## Export

`export_document(&ExportRequest)` returns combined `content` and `blockContent` keyed by block ID. Paragraphs and figures are exported as HTML; `tableFormat` selects HTML, Markdown, or CSV for tables. This is semantic content export, not page layout or a full-document Markdown conversion.

```rust
use rust_docx::{parse_document_bundle, export_document, ExportRequest};

fn convert(bytes: &[u8]) -> Result<String, String> {
    let bundle = parse_document_bundle(bytes, Default::default())?;
    let output = export_document(&ExportRequest {
        blocks: bundle.document.blocks,
        assets: bundle.assets,
        revision_inlines: bundle.revision_inlines,
        ..Default::default()
    })?;
    Ok(output.content)
}
```

Styled export preserves paragraph/run styles, script-specific fonts and language, hyperlinks, list labels, table borders, widths, row styles, and merged cells. Revision spans use standard `ins` and `del` elements; `groupChanges` optionally wraps adjacent changes in a `change` element. Omit `revisionInlines` to export current text only.

Callers can supply `ContentOverride` entries by block ID. `replaceContent` replaces a block's content, while a figure override provides its caption. `text` is escaped for HTML; `html` is trusted caller-provided markup and must be sanitized by the caller when appropriate. `listLabelPresent` avoids duplicating a label already present in replacement text, and `suppressImage` allows callers to display a separate media viewer. Overrides do not override explicit revision spans. An `assets` map may contain the extracted payloads or caller-provided display URLs.

Document text and attributes are escaped, style values are constrained, and links/image URLs are restricted to supported schemes. Export does not fetch URLs. Configurable limits bound block depth/count, table dimensions, and accumulated output size; duplicate block IDs are rejected. Host applications still own process isolation, deadlines, uploads, access control, OCR, figure descriptions, API mapping, and chunking.

## WebAssembly

`parse_document_bundle_wasm(bytes, options_json)` and `export_document_wasm(request_json)` expose the same contracts using JSON. Existing WASM exports are unchanged. Generated browser packages and release versions are not updated by this migration.

# Changelog

## 0.5.1

- Update the shared OpenDocument, artwork and chart dependencies.

## 0.5.0

- Recover text from OpenDocument chart previews, vector images and embedded objects, including modern SVM, SVG, WMF and EMF text records, with bounded extraction and source metadata.
- Order extracted OpenDocument objects geometrically without changing drawing order, object identity or source paths; tolerate sub-point edge overlaps and preserve ambiguous regions.
- Resolve cached slide fields and transparent text wrappers, and avoid resolving unused style properties.
- Add `ObjectText`, `NodeMetadata.text_content`, `BlockNodeInput.text_content`, and `presentation_node_reading_order`; Rust struct literals must initialize the new fields. Older serialized models remain readable.
- Update the shared OpenDocument, artwork and chart dependencies and the WebAssembly bindings.

## 0.4.1

- Render supported OpenDocument charts and enhanced drawing geometry, retain chart styles and scatter/bubble domains, and expose cached chart data through existing extraction APIs.

## 0.4.0

- Read OpenDocument presentations and drawings (`.odp`, `.otp`, `.fodp`, `.odg`, `.otg`, `.fodg`) into `PresentationDocument` through the new `duke-odf` crate; see [OpenDocument input](../../docs/opendocument.md) for what is imported.
- `PresentationFormat` has new `Odp` and `Odg` variants, reported as `odp` and `odg`. Exhaustive Rust matches on the format need the new arms.
- `duke-slides-wasm` reports the new formats and needs no new export.

## 0.3.0

- Add bounded presentation bundles with rich paragraph inlines, slide manifests, comments, shared chart data, and deduplicated assets.
- Add semantic HTML export with merged HTML/Markdown/CSV tables, asset substitutions, and content overrides using shared Office export types.
- Preserve chart part names and expose the shared lossless chart-data contract independently of rendering. The new public `SlideNode::Chart` field requires updates to exhaustive Rust patterns and constructors.
- Improve inherited text styles and bullet formatting, and use SIMD base64 when available in WebAssembly.
- Add native and WASM extraction limits, bounded bundle parse defaults, and matching JSON export entry points while preserving citation references.

## 0.2.2

- Look up cached shape artwork by a memoized keyed hash and build the owned cache key only when a shape is stored, with unchanged output.
- Build each part's relationship cache key once instead of once per shape.
- Skip subtrees that cannot contain a match when collecting XML descendants.
- Settle XML name comparisons on length and first byte before comparing the full name.
- Detect namespace declarations and namespace-tracking triggers with plain byte searches.

## 0.2.1

- Enable runtime-checked ARM SHA-256 acceleration in standalone builds, preserving exact document identity and parser outputs.
- Avoid resolving unused XML attribute values during name lookups.
- Read run-style attributes in one pass and prevent foreign-qualified attributes from masking unqualified style values.
- Use native SIMD base64 encoding for presentation assets while preserving the existing WASM output.
- Avoid allocating lowercase copies when parsing Boolean XML attributes.
- Reuse validated XML text when detecting namespace-retained source.
- Use portable curve math in the shared geometry dependency graph to align native and WASM artwork calculations.

## 0.2.0

- Add source-grounded presentation blocks for objects, paragraphs, merged table origins and optional words and notes.
- Assign exact-input SHA-256 document identity to native, deferred and retained parses.
- Include slide occurrence, node path, source-part identity and UTF-16 ranges in citation references.
- Expose a lightweight extraction input for WASM consumers, excluding assets and generated artwork from the return trip.
- Add the optional `document_id` field to `PresentationDocument`; downstream Rust struct literals must supply it. Serialized older models remain deserializable, but citation extraction requires a model reparsed with source identity.

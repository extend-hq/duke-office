# Changelog

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

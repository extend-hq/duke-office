# Changelog

## 0.2.0

- Add source-grounded presentation blocks for objects, paragraphs, merged table origins and optional words and notes.
- Assign exact-input SHA-256 document identity to native, deferred and retained parses.
- Include slide occurrence, node path, source-part identity and UTF-16 ranges in citation references.
- Expose a lightweight extraction input for WASM consumers, excluding assets and generated artwork from the return trip.
- Add the optional `document_id` field to `PresentationDocument`; downstream Rust struct literals must supply it. Serialized older models remain deserializable, but citation extraction requires a model reparsed with source identity.

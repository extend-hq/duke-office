# OpenDocument text extraction release — 2026-11-16

This release packages the verified round 11 checkpoint from the [accuracy and latency report](opendocument-hillclimb-20261116.md). The final counter-state optimization was reverted before release preparation. Runtime source matches the accepted benchmark checkpoint; release changes update versions, dependency pins, changelogs and generated browser assets.

| Package | Version |
| --- | --- |
| `duke-odf` | 0.3.0 |
| `duke-office-art` | 0.2.0 |
| `duke-office-charts` | 0.3.0 |
| `duke-office-charts-wasm` | 0.3.0 |
| `duke-pages` and `duke-pages-wasm` | 0.4.0 |
| `duke-slides` and `duke-slides-wasm` | 0.5.0 |
| `@extend-ai/duke-office-charts` browser distribution | 0.3.8 |

Minor Rust versions account for new public model fields and updated public OpenDocument types. Code constructing affected structs with literals must initialize those fields. The added JSON fields are optional; older serialized models remain readable. The browser JavaScript API is unchanged. Native and WASM packages pin the matching shared dependency versions.

The retained changes recover vector/object text, improve geometric slide reading order, preserve cached fields and note citations, and correct OpenDocument list numbering. Across the fixed scored corpus, 38 documents improve and none regress relative to the original reading-order/SVM baseline. Pooled F1 reaches 95.8711% for ODP, 97.8951% for FODP, 99.9072% for ODT and 100% for FODT. These are measured corpus results, not universal format-coverage guarantees; latency improvements are modest and host-sensitive.

The accepted checkpoint passes 469 existing Rust tests and the WebAssembly check. Its 256 presentation/drawing models and bundles were checked for eager/deferred/model-round-trip agreement. Remaining limitations include overlapping-object and column text interleaving, legacy SVM, some hierarchical-list continuations and unsupported encodings. Imported OpenDocument numbering and citations are not guaranteed to round-trip through Word XML export.

The browser package is distributed through the `charts-browser-v0.3.8` GitHub Release tarball, following [the repository release workflow](../RELEASING.md). Third-party notices remain bundled with generated assets. No downstream viewer dependency is updated by this release.

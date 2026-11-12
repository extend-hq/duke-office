# Crate source comparison — 2026-11-12

No substantive copied third-party implementation was identified in the comparison corpus described below. This is a bounded source comparison, not a claim that every line is unique or that all possible upstream packages have been checked. Short generic implementation fragments and standard data do match other packages. Bundled third-party data is explicitly identified below.

The primary snapshot includes all eight workspace crates: 212 Rust files and 120,571 lines, including tests, examples, and uncommitted sources. Its base commit is `bca6d0ae0ea198d2a82749c8a842b27c3d7b26f1`; the exact audited working-tree contents are recorded by SHA-256 in the local evidence. Other work changed OpenDocument sources during the audit, so the scans use frozen snapshots rather than mixing successive working-tree revisions. Two subsequently changed OpenDocument chart modules are captured separately in the follow-up snapshot and comparison reports.

## Comparison coverage

- Whole-file SHA-256 comparison and whitespace-insensitive sequences of at least six nontrivial lines, with a minimum of 120 characters in the matching seed.
- A separate token comparison ignores comments and formatting, preserves identifiers and literals, and reports matching sequences of at least 60 tokens. A lightweight lexer handles strings, raw strings, character literals, and nested block comments.
- 44,878 Rust source files from 1,376 cached package versions, representing 765 package names. All 142 external package versions in the workspace lockfile are present. Identical external contents are scanned once; their individual paths and hashes remain in the evidence.
- 82 cached first-party package versions were excluded using verified repository metadata. This includes older `extend-*` package names as well as the current crate names.
- The additional corpus contains 194 Rust sources from `yagaltd/office-parser` at revision `6c05f6eab46ac54298701fef453dbef622418f36`, two JavaScript sources from `dingbat-to-unicode` 1.0.1, and 334 JavaScript/TypeScript sources from `docx-preview` 0.4.1, `mammoth` 1.13.0, and `officeparser` 8.7.1. The same token matcher provides a supplemental literal comparison for these languages; it does not detect translated implementations.

## Reviewed matches

No complete Rust source file matched a third-party source file.

The line scan reported three matches. The Rust token scan reported 22,326 matches: 22,293 repeated numeric bitmap sequences, 16 PNG fixture prefixes, six Roman numeral tables, five Windows-1252 tables, four Serde visitor fragments, one CFB test-header fragment, and one style-overlay fragment. The additional upstream token scan reported only 1,170 repeated numeric bitmap sequences. No source read or decoding errors occurred.

Both follow-up OpenDocument modules returned zero matches against the same registry and additional upstream corpora. A later formatting change to the DOCX bundle module was checked for token equivalence, allowing whitespace, comments, and optional trailing commas; it introduced no new implementation. The final coverage receipt records these versions separately.

| Match | Assessment |
| --- | --- |
| Office theme color-slot pairs | Standard mapping data; the matching sequence contains no rendering algorithm. |
| Assignments for optional bold, italic, underline and strike fields | Short, repetitive field-overlay code. The surrounding implementations differ. This match alone does not establish copying. |
| Serde visitor methods returning `None` | Routine implementation of the trait's required interface; the deserialization behavior around these methods differs. |
| CFB fixture header writes | Fixed format offsets and constants in a test helper; the surrounding fixture builders differ. |
| Windows-1252 and Roman numeral tables | Standard character and numeral mappings. |
| PNG fixture byte prefixes | Embedded image data, including the format header. |
| Repeated zero/two values in pattern coverage tables | Numeric data also occurring in unrelated tables; these account for nearly all token matches. |

These matches were reviewed rather than renamed or reformatted to suppress detection. No parser or renderer implementation was replaced as a result of this audit.

## Bundled third-party data

| Location | Provenance and verification |
| --- | --- |
| `crates/rust-docx/src/parse/symbols.rs` | All 1,061 entries in `dingbat-to-unicode` 1.0.1 match the local mapping. There are also 103 local-only entries. This is a data conversion into Rust match arms; the package's JavaScript lookup implementation is not reproduced verbatim. Existing attribution is in the crate README, `third-party/dingbat-to-unicode-LICENSE`, and the Unicode notice in the source. |
| `crates/rust-office-art/src/geometry_data/presetShapeDefinitions.xml` | Byte-for-byte identical to the pinned Apache POI resource: 538,970 bytes, SHA-256 `4a762444d8d85876881c02a5b1dedf6f73006fcd8acb7b4e393435615b37c780`. Retrieved upstream and compared during this audit. The adjoining source record, license and notice identify it. |
| `crates/rust-office-art/src/geometry_data/presetTextWarpDefinitions.xml` | Recorded as unchanged ECMA-376 geometry data. Local SHA-256 matches the existing source record: `89640bd4aeedfa2c578d1d1acc50d569865ebdef315fd4b468087c25e36acada`. The upstream archive was not downloaded again during this audit. |
| `crates/rust-office-art/src/fonts/` and `crates/rust-office-charts/src/parse/font_metrics/` | Bundled fonts and derived metrics have existing OFL notices. Font processing and upstream identities are recorded by the existing generation scripts and source manifests. Every upstream font binary was not independently reverified here. |
| `crates/rust-office-charts/src/parse/chartex/map_atlas/` | Derived geographic data from `world-atlas` and `us-atlas`, documented by the existing README, manifest and ISC notice. This audit did not re-download the geographic datasets. |

Keeping attributed data is distinct from copying a library implementation. This workspace therefore cannot accurately be described as containing no third-party material.

## Evidence and limits

Local evidence is under `output/source-overlap-audit-20261112/`: `audited-snapshot.json`, `source-snapshot/`, `delta-snapshot/`, the corpus manifests, download receipts, `scan_overlap.py`, final and follow-up comparison JSON, and the reviewed-match summary. This output directory is intentionally outside version control. Reports retain source hashes and line locations; the comparison program can be rerun against the recorded manifests and snapshots.

This audit does not establish authorship, detect every short fragment or rewritten/translated implementation, search all historical versions or private packages, or analyze compiled/generated dependency code. Source changes after the captured snapshot require another comparison. Existing license notices were preserved. No crate implementation was edited and no Rust build or test result is claimed by this audit.

Repository instructions now prohibit importing third-party implementations through copying, mechanical translation, or cosmetic rewriting, and require honest reporting of comparison scope.

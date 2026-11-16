# OpenDocument reading order and SVM text — 2026-11-16

Presentation extraction now orders separated text regions geometrically and recovers stored text from modern SVM/StarView drawings. Against the preceding [chart and vector text parser](opendocument-visual-text-20261116.md), ODP pooled text F1 increases from **95.0992% to 95.7803%** and average-document F1 from **92.3003% to 93.5462%**. Fourteen scored ODP documents improve, with no scored regressions in any format.

## Results

| Metric | Previous object-text parser | Reading order + SVM |
| --- | ---: | ---: |
| ODP pooled text F1, 86 scored | 95.0992% | 95.7803% |
| ODP average-document F1 | 92.3003% | 93.5462% |
| FODP pooled text F1, 6 scored | 95.7265% | 97.8951% |
| ODP inputs parsed | 217/217 | 217/217 |
| All supported inputs parsed | 756/764 | 756/764 |
| ODP mean warm parse/extraction time | 0.3357 ms | 0.3302 ms |
| ODP median warm parse/extraction time | 0.1580 ms | 0.1540 ms |
| ODP mean deferred bundle time, including JSON | 0.5031 ms | 0.5053 ms |
| ODP median deferred bundle time, including JSON | 0.1805 ms | 0.1888 ms |

The ODP gains are **0.6811 percentage points pooled and 1.2459 points per document**. One scored FODP also improves. Twenty-three readable text outputs change: 22 ODPs and one FODP. The remaining 733 outputs are byte-identical. ODT retains 99.8910% pooled F1, and every other format aggregate is unchanged. The same eight ODT inputs remain unreadable.

Both arms reproduce identical text across their two passes. The fresh baseline also reproduces every readable output from the previous investigation. No inputs, reference exports or scoring rules changed.

## Presentation reading order

The old extraction traversal followed the drawing stack. That can place a later column or lower text box before an earlier region. The new shared `presentation_node_reading_order` helper returns original sibling indexes after partitioning at clear horizontal or vertical whitespace. Complete rectangular grids read by row; ambiguous overlapping regions retain source order. Non-text nodes, invalid dimensions and rotated or otherwise unsupported text geometry retain their slots. A group with unsupported text geometry is not moved based only on its supported children.

This order is used by normal structured extraction and the text benchmark for packaged, flat and template presentations. Groups retain their boundaries and are processed recursively. Block IDs and source node paths remain unchanged, and bundle enrichment follows those original paths to the matching node. The arrays used for drawing are never reordered. ODG keeps its previous extraction order because applying this presentation rule to drawings regressed a scored drawing. PPT/PPTX behavior is unchanged.

An accuracy-only ablation on the frozen pre-SVM models reached 95.7710% pooled ODP F1 and 93.5258% average-document F1, with 13 improvements and no regressions. This accounts for most of the combined score gain. Individual improvements include a separated-region case rising from 68.85% to 100%, and a mixed picture/text layout rising from 68.52% to 85.19%.

## Modern SVM text

The new bounded decoder recognizes the `VCLMTF` signature and versioned action envelopes. It reads Unicode and supported legacy character sets, substring ranges measured in UTF-16 units, text-array and stretched-text records, text rectangles, map modes, saved map state, and nested transparent drawings. Modern Unicode copies take precedence over legacy representations. The stream character set follows font records independently of drawing-state restoration.

SVM may paint a wrapped label as several ranges of one stored string, with a separately drawn wrap hyphen. The decoder joins only explicitly drawn contiguous ranges of the same string, within text-shape and paragraph boundaries. It removes an intervening hyphen only when the next range confirms the continuation. It never emits the undisplayed remainder of a stored string. This keeps split labels together before geometric span ordering.

The corpus contains **51 modern SVM assets across 25 ODPs**. All 51 decode; 40 yield nonempty text. Some previews reproduce labels already obtained from native chart data, so format coverage expands more than the aggregate score. On the chart that exposed fragmented labels, F1 improves from 79.90% to 81.65%. A broader geometry experiment inside SVM charts caused six regressions among eight affected documents and was excluded. Legend/table interleaving remains a limitation.

Recovered SVM text uses the existing `textContent` metadata and structured-extraction path. Assets now retain the SVM media type. Clipping, EPS substitutes and unsupported text operations can mark output partial. A false `partial` value does not prove that every rendered glyph was recovered.

## Performance and limits

Warm ODP text extraction measures 1.64% lower on average in this run; serialized deferred bundles measure 0.45% higher. These small differences on a shared host do not establish a speed improvement. The bundle median rises by 0.0083 ms. Builds and tests finished before the counterbalanced measurements. A preliminary run made during heavy host load was excluded from these timing tables and retained separately as pilot evidence.

Vector inputs are capped at 32 MiB, recovered text at 1 MiB, records at 250,000, saved state at 256 levels and nested SVM drawings at 32 levels. Lengths, ranges, arithmetic and finite transforms are checked. Reading-order inference stops at 64 levels or 4096 siblings and retains source order beyond those limits.

Remaining gaps include multicolumn text inside a single frame, paragraph/line interleaving across overlapping objects, semantic diagrams, chart legend/table order, legacy `SVGD` streams, unsupported font encodings, raster or arbitrary outlined text, and full clipping/occlusion. Saved-preview numeric formatting can differ from a new PDF export and is preserved. Native embedded-content fallback and environment-dependent fields retain their previous limits.

The final ODP character-multiset diagnostic is 98.9515%, compared with 95.7803% sequence F1. Thirteen scored ODPs have exactly the reference character multiset but a different sequence. Multiset agreement helps distinguish missing text from ordering loss; it does not establish correct reading order.

## Validation and reproducibility

- All **469 existing workspace tests** and the **WASM workspace check** pass. No new unit tests were added.
- All **256 presentation/drawing inputs** pass eager/deferred bundle text comparisons and model round trips. Text compares exactly; serialization checks tolerate absent-versus-null optional fields and relative floating-point differences within `1e-12`.
- Generated SVM probes pass **1004 expected-result checks** and **1000 deterministic byte mutations** without a panic. They cover record versions, encodings, range joining, paragraph boundaries, transforms, nesting, limits, malformed offsets and truncations. These are bounded probes, not exhaustive fuzzing.
- Ten generated reading-order checks cover separated regions, grids, stable overlaps, groups, unsupported geometry, stable IDs/source paths, matching bundle enrichment and unchanged drawing nodes.
- The new modules pass `rustfmt --check`. The repository-wide formatter check reports existing differences, including an untouched example; this work does not reformat the workspace.
- The benchmark uses the same **764 inputs**, LibreOffice reference exports, normalized-character LCS metric and scoring exclusions. Both text and deferred-bundle comparisons run baseline/candidate/candidate/baseline with frozen executables. Each input uses up to 200 warm repetitions or 250 ms, with at least three calls. Tables use each input's fastest warm call across both passes. File I/O, process startup and cold initialization are excluded. Deferred bundles enable charts, disable artwork output and include JSON serialization.

The baseline is commit `6a38ec8` plus the preceding local field, wrapper and object-text improvements. Its 231 source hashes were verified before this iteration. The candidate records 233 source hashes, 1513 input/reference hashes and four frozen runner hashes. Evidence is under `output/odp-order-svm-20261116`, including both passes, extracted texts, models/bundles, scores, source/input receipts, patches, checks and rejected diagnostic prototypes. This is a local checkpoint; no release or deployment was performed.

## Reference-only source use

SVM field layouts and serialization behavior were checked against LibreOffice core commit `7d5a5b55d0ea14a404957218e71e716a73524c57`, including the [SVM writer](https://github.com/LibreOffice/core/blob/7d5a5b55d0ea14a404957218e71e716a73524c57/vcl/source/filter/svm/SvmWriter.cxx) and [text hierarchy markers](https://github.com/LibreOffice/core/blob/7d5a5b55d0ea14a404957218e71e716a73524c57/drawinglayer/source/processor2d/vclmetafileprocessor2d.cxx). The decoder and ordering routine were implemented independently; no third-party implementation was copied, renamed or translated into the crates.

Seventeen reference files remain outside the workspace under `/tmp/odp-svm-reference-20261116` with their original notices. The artifact manifest records their URLs and hashes; independent field notes record the observations used. This is selected source and corpus coverage, not a comprehensive format specification or a universal originality audit. Existing corpus assets retain their provenance.

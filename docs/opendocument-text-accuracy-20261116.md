# OpenDocument presentation text accuracy — 2026-11-16

The investigation found real field-resolution and foreign-content bugs, but most of the ODP score deficit is not ordinary paragraph text being dropped. The retained changes fix those bugs without changing drawing order, the benchmark runner, reference exports, normalization, scoring exclusions, or chart-text scope.

## Results

| Metric | Baseline | Candidate |
| --- | ---: | ---: |
| ODP pooled text F1 (86 scored) | 93.6054% | 93.6342% |
| ODP average-document F1 | 89.0733% | 89.1751% |
| ODT pooled text F1 (220 scored) | 99.88995% | 99.89095% |
| ODT average-document F1 | 98.0001% | 98.0188% |
| Supported inputs parsed | 756/764 | 756/764 |
| ODP mean warm time | 0.3343 ms | 0.3391 ms |
| ODP median warm time | 0.1581 ms | 0.1602 ms |

The ODP gain is **0.0288 percentage points pooled and 0.1018 points per document**: a small, verified improvement, not a resolution of the larger fidelity gap. Three scored ODP documents and one ODT document improve; **no scored document regresses**. The affected ODP case scores rise from 94.33% to 95.36%, 51.34% to 55.56%, and 96.49% to 100%. The affected ODT case rises from 95.88% to 100%.

Seven extracted texts change and 749 remain byte-identical. Some corrected field values do not change LCS because their order still differs from the PDF reference; one changed document has too little reference text to score. FODP and all other scored format aggregates remain unchanged. The same eight encrypted/invalid inputs are rejected. Both arms produce identical texts across their two passes, and the fresh baseline reproduces all 756 historical baseline texts byte for byte.

ODP mean warm time is 1.44% higher and median is 1.33% higher in this run. There is no speedup claim; small overhead and shared-host noise are not separated by this measurement.

## Root causes

The existing benchmark compares text emitted in model/drawing order with `pdftotext` output from LibreOffice's rendered PDF. Its normalized-character LCS therefore measures sequence agreement as well as content recovery.

- **Reading order:** 13 scored ODP documents have identical normalized character multisets between parser and reference but different sequences. Across all 86 scored ODP documents, character-multiset F1 is 97.6871%, compared with LCS F1 of 93.6054%. The multiset result is only a diagnostic upper bound: it ignores order and can match characters from unrelated words. It is not a replacement accuracy score. Multi-column text, overlapping objects, tables and grouped shapes account for visible sequence differences.
- **Chart text:** the runner emits stored titles, categories and series names. PDF export additionally contains generated tick values, data labels and chart tables, orders those labels spatially, and may omit hidden legends. A chart-heavy document alone contributes 439 normalized-character insertions/deletions under the LCS comparison. Stored chart data is present; its extraction scope differs from rendered-page text.
- **Vector and embedded-object text:** two documents with no extracted text have 461 reference characters in total. Their visible text is in vector artwork or embedded-object previews. The current model retains those assets, but the text runner does not decode their text records or perform OCR.
- **Environment-dependent fields:** LibreOffice refreshes dates, times and document paths when exporting. One reference contains the exporter machine's absolute temporary path. A byte-only parser cannot reproduce that path; matching the export clock would also make parsing nondeterministic. These cases remain in the original score.
- **Actual parser defects:** decorative master fields were built with no destination slide and then cloned with page number `0`; slide-name fields retained a placeholder. Unnamespaced wrapper elements also lost their child text. Presentation footer/date declarations were initialized after the slide's own shapes, too late for fields in those shapes.
- **List labels:** the presentation runner concatenates text runs and does not emit automatic numbering, although the structured bundle already carries list labels. This is another benchmark extraction-scope limitation, retained for direct comparability.

A top-to-bottom/left-to-right node-order experiment was rejected. It raised pooled F1 only slightly while reducing average-document F1 to 87.81%, with regressions in grouped diagrams and other layouts. The experiment is retained locally; no sorting heuristic was added to the parser or benchmark.

## Retained implementation

`duke-odf` now traverses unnamespaced foreign wrappers at both block and inline level, under the existing depth/content limits. It also retains the slide-name extension as a typed field instead of ordinary placeholder text.

`duke-slides` preserves presentation field identity on text runs and resolves page number, page count, slide name, footer and fixed presentation date text after master nodes have been cloned into their destination slide. Resolution covers nested groups and table cells. Footer/date declarations are available before importing slide-owned shapes. Embedded text documents retain their own cached field values rather than receiving the host presentation's page count.

The field values use the parsed document and slide declarations. No filesystem path, current date or current time is injected. Drawing nodes are not sorted or repositioned. Field metadata is additive in the serialized model; the field source identifies the original ODF part and does not invent Word markup.

The foreign-content and field semantics were checked against [ODF 1.3, Part 3](https://docs.oasis-open.org/office/OpenDocument/v1.3/OpenDocument-v1.3-part3-schema.html), together with the stored XML and reference exports. No third-party implementation code or assets were copied.

## Validation and method

The baseline is clean commit `6a38ec8`. Both baseline and candidate runners are freshly built using Rust 1.93.1, locked dependencies, release optimization level 3 and thin LTO. The baseline executable is frozen before edits. The same 764 inputs and existing LibreOffice references are used in both arms; 14 tracked-change documents and references shorter than 20 normalized characters retain the original scoring exclusions.

Final timing runs use baseline/candidate/candidate/baseline order after builds and checks finish. Each input uses up to 200 warm repetitions or 250 ms, with at least three calls. Timings summarize each input's fastest warm call across both passes; file I/O, process startup and cold initialization are excluded. The shared host is not isolated, so small timing differences do not establish a durable speed change.

All **469 existing workspace tests** and the **WASM workspace check** pass. No new unit tests were added. A separate two-slide probe verifies destination-specific fields in nested master groups and slide-owned text, unnamespaced block/inline wrappers, and preservation of an embedded text document's cached page count. Both slides pass all checks.

Local evidence is retained under `output/odp-text-accuracy-20261116`: frozen runners and source hashes, build/test/WASM logs, both benchmark passes, complete extracted texts, per-file scores, the rejected ordering probe, parsed diagnostic models, and the focused probe output.

# OpenDocument chart, vector and embedded-object text — 2026-11-16

ODP text extraction now includes generated chart labels and text stored in supported vector images and embedded-object previews. On the same benchmark, pooled ODP text F1 rises from **93.6342% to 95.0992%** and average-document F1 from **89.1751% to 92.3003%**. This compares against the corrected parser from the [earlier text investigation](opendocument-text-accuracy-20261116.md), including its field and foreign-wrapper fixes.

## Results

| Metric | Previous corrected parser | With object text |
| --- | ---: | ---: |
| ODP pooled text F1 (86 scored) | 93.6342% | 95.0992% |
| ODP average-document F1 | 89.1751% | 92.3003% |
| ODP inputs parsed | 217/217 | 217/217 |
| All supported inputs parsed | 756/764 | 756/764 |
| ODP mean warm parse/extraction time | 0.3331 ms | 0.3351 ms |
| ODP median warm parse/extraction time | 0.1578 ms | 0.1577 ms |
| ODP mean deferred bundle time, including JSON | 0.4948 ms | 0.5054 ms |
| ODP median deferred bundle time, including JSON | 0.1829 ms | 0.1829 ms |

The gains are **1.4650 percentage points pooled and 3.1252 points per document**. Nine scored ODP documents improve and one regresses. Thirteen extracted texts change: twelve ODP files and one unscored flat drawing. The other 743 readable inputs remain byte-identical. ODT, FODP and every other scored format aggregate are unchanged. Both arms reproduce identical text across their two passes; the fresh baseline also reproduces every readable output from the preceding investigation.

Representative individual-document changes:

| Content | Previous F1 | New F1 |
| --- | ---: | ---: |
| Chart-heavy presentation | 71.84% | 99.47% |
| Chart with vector preview | 62.26% | 100.00% |
| Embedded text document | 72.29% | 97.83% |
| Embedded objects with vector previews | 0.00% | 83.14% |
| Vector diagram partly outside the slide | 0.00% | 71.54% |

The two previously empty outputs now contain the same normalized character multisets as their references: 338 and 123 characters respectively. Their remaining LCS loss is ordering, not missing characters; character-multiset agreement is a diagnostic and does not establish correct text order.

The single regression is **58.7332% to 58.4559%** (−0.2773 points). That reference interleaves overlapping charts and watermark text; the new output adds axis labels while retaining object order. It remains in the score. There is no document-specific filtering or benchmark exclusion to hide it.

## What changed

**Chart labels.** Previously the text runner emitted titles, category strings and series names from the native data model. It missed generated axis ticks, formatted data labels and chart tables, and could emit names even when a legend was not drawn. The importer now recovers chart text from a supported saved vector preview or from the existing native chart renderer's SVG. This follows its label formatting and display decisions. Text in the renderer's outlined-label annotations is recovered without attempting to recognize arbitrary glyph paths.

**Vector records.** A bounded reader in `duke-odf::vector_text` decodes WMF text records, embedded EMF streams, EMF Unicode/ANSI text records and SVG text elements. Font selection and saved drawing states supply character sets; drawing transforms supply approximate reading order within the object. A complete embedded EMF stream takes precedence over its WMF fallback to avoid duplicate text. Glyph-index records are skipped rather than mistaken for Unicode. SVG definitions, descriptions, hidden elements and scripts do not contribute text.

For unrotated image frames crossing a page edge, text origins are filtered to the visible portion when image bounds are available. This removes fully off-slide text in the recovered diagram. It is not character-level clipping or a complete renderer.

**Embedded content.** Native embedded text and spreadsheet content is retained even when the viewer uses a drawable preview. Supported vector previews also recover text from otherwise opaque embedded objects. The implementation does not decode legacy Office binary payloads, search arbitrary bytes for strings, or perform OCR.

**Public extraction.** Optional `ObjectText` metadata is serialized as `textContent` on the owning node, with `text`, `source`, `sourcePart` and `partial`. Sources distinguish `vector-records`, `chart-rendering` and `embedded-content`. Structured blocks and bundles use it as the image/chart/object block's content and retain the metadata in `details.textContent`. Artwork remains on the original node; no duplicate drawable text shape is introduced. Chart groups attach recovered text to the chart, not again to the preview image. Existing chart data remains available.

`partial` flags skipped decoding or use of the embedded-content fallback; `false` is not a guarantee that every visible glyph has been recovered. Unsupported charts retain the previous title/category/series fallback in the text benchmark. Slide nodes retain drawing order; only spans within a vector object are ordered geometrically.

## Cost, limits and remaining gaps

Mean warm ODP parsing is 0.60% higher in this run, with a slightly lower median. Deferred bundle extraction is 2.16% higher on average with an unchanged median. Chart labels require layout even when artwork output is deferred; the largest measured increase is the chart-heavy presentation, from 10.4927 to 11.9029 ms for the complete serialized bundle. Shared-host noise limits precision, and these measurements do not establish a speed improvement.

Each vector input is capped at 32 MiB, recovered text at 1 MiB, records/nodes at 250,000, SVG nesting at 128 and saved drawing states at 256. Offsets and lengths are checked before access. The WMF object table reuses free slots without repeatedly scanning the entire table. Recovered text is included in structured-extraction output budgets. `encoding_rs` is a declared dependency for legacy character sets; no external implementation was copied into the workspace.

Remaining limitations include SVM/StarView records, raster text, arbitrary outlined glyphs, unsupported font character sets and chart layouts, full SVG CSS/use-element handling, complete clipping/occlusion, and semantic reading order across slide objects. Native embedded-content fallback may include text outside the saved preview's visible area. Existing environment-dependent field differences and automatic-list-label scope also remain. Generating labels through the current chart renderer inherits its fidelity limits.

## Validation and reproducibility

- All **469 existing workspace tests** pass, as does the **WASM workspace check**. No new unit tests were added.
- All **256 presentation/drawing inputs** pass the eager/deferred bundle comparison and model serialization round trip. Text is compared exactly; the round-trip comparison ignores absent-versus-null optional fields and floating-point differences within a relative tolerance of `1e-12`.
- Separate generated probes pass **859 expected-result checks**, covering legacy font selection, saved state, Unicode, compact EMF text, embedded EMF precedence, glyph rejection, SVG visibility, resource caps, invalid offsets and exhaustive truncations of representative generated records. **994 deterministic byte mutations** finish without a panic. These are focused checks, not exhaustive fuzzing or a proof of format completeness.
- The text benchmark uses the same **764 inputs**, LibreOffice reference exports, normalized-character LCS metric and scoring exclusions. The runner now consumes the new object text, so extraction scope expands; this is not solely an internal parser change. No reference text or scoring rule was changed.
- Both comparisons run baseline/candidate/candidate/baseline after builds and tests finish, with frozen executables. Each input uses up to 200 warm repetitions or 250 ms, with at least three calls. Tables report each input's fastest warm call across the two passes. File I/O, process startup and cold initialization are excluded. The deferred benchmark includes bundle JSON serialization with charts enabled and artwork output disabled.

The baseline is commit `6a38ec8` plus the preceding local field/wrapper changes, not clean main. Its source snapshot matches all 226 source hashes retained by the preceding investigation. Local evidence lives under `output/odp-visual-text-20261116`: frozen runners, source/input hashes, source snapshots and patches, both benchmark passes, all extracted texts and bundles, per-file scores, build/test logs and focused probes. Changes remain local; no release or downstream deployment was performed.

The binary readers were implemented from Microsoft's format specifications, including [WMF text records](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-wmf/7d07c44a-a828-4b82-9af0-e0a81cced5a8), [embedded EMF records](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-wmf/cfc88064-d86d-4b52-9374-3ce27d456179), [EMF text objects](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-emf/dd585d0a-5d7c-4034-963a-1141af836972), [compact EMF text](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-emf/20eee81d-0bd4-42d1-a624-860adfe62358) and [glyph-index flags](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-emf/e7ffcc53-40d1-4873-8eda-c5c5ee104aa5). No third-party source or assets were copied.

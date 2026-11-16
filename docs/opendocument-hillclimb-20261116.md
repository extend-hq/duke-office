# OpenDocument F1 and latency iterations — 2026-11-16

This log continues from the [reading-order and SVM checkpoint](opendocument-order-svm-20261116.md): 95.7803% pooled ODP F1, 93.5462% average-document F1 and 756/764 readable inputs. Each iteration freezes its predecessor and reruns the unchanged corpus and reference scoring. Results below describe local measurements before publication; packaging of the accepted checkpoint is tracked in the [release notes](opendocument-release-20261116.md).

The accepted implementation through round 11 reaches **95.8711% pooled ODP F1** and **99.9072% pooled ODT F1**. A fresh comparison through round 9 against the original task baseline confirms **38 scored improvements and no regressions**, with lower mean warm extraction times for both formats. The [cumulative comparison](#cumulative-comparison-through-round-9) includes those accuracy and timing tables; later rounds report their own incremental measurements.

Work is paused at the user's request on the verified round 11 source checkpoint. Round 12 was rejected and reverted. Cumulative accuracy through round 11 still has 38 distinct scored improvements and no regressions; timing gains are modest and sensitive to host variation.

## Round 1: small-container allocations

Profiling the slowest inputs showed allocation and copying costs in model construction and structured extraction. In the 20-input phase sample, ZIP inflation accounted for 3.8 ms of 57.3 ms of shared ODF parsing. Many table cells contain one block or one text run, but growing an empty vector reserved four slots.

The shared text reader now inspects at most four immediate children when choosing initial capacity. The page adapter sizes each story from its owned input blocks and reserves paragraph runs exactly. Larger containers continue growing normally; content limits and traversal behavior are unchanged.

| Warm metric | Previous checkpoint | Allocation change |
| --- | ---: | ---: |
| ODT mean text extraction, 418 readable | 0.3800 ms | 0.3697 ms |
| ODT median text extraction | 0.0782 ms | 0.0807 ms |
| FODT mean text extraction, 79 readable | 0.0358 ms | 0.0367 ms |
| ODP mean text extraction, 217 readable | 0.3340 ms | 0.3322 ms |
| ODP median text extraction | 0.1561 ms | 0.1537 ms |
| ODP mean deferred bundle, including JSON | 0.5046 ms | 0.4965 ms |
| ODP median deferred bundle, including JSON | 0.1880 ms | 0.1836 ms |

Mean ODT time decreases **2.73%**, and the two largest ODT outliers decrease from 63.5200 to 60.5476 ms and 23.5345 to 22.4447 ms. Mean ODP bundle time decreases **1.61%**. The ODT median increases 0.0025 ms and the FODT mean increases 0.0009 ms; the result is most useful for large documents, and small differences remain sensitive to shared-host noise.

In a separate run repeating one large document through the native runner, maximum resident memory decreases from 247.3 to 211.5 MB (14.5%), and reported peak physical footprint from 192.6 to 156.3 MB (18.9%). These are process-level observations for that workload, not a corpus-wide memory guarantee.

Every readable text output remains **byte-identical**; all F1 scores and parse outcomes are unchanged. Canonical hashes of complete page models and structured blocks match for all **500 readable text inputs**, with the same eight errors among 508 text inputs. All **469 existing tests** and the **WASM workspace check** pass. No new unit tests were added.

Evidence is under `output/odf-hillclimb-20261116/round1`: frozen runners, source/input receipts and patch, complete model/block hashes, counterbalanced text and bundle passes, scores, phase measurements, process sample and memory measurements. Timings use the same baseline/candidate/candidate/baseline protocol and fastest warm repetition per input as the preceding checkpoint. The fresh baseline reproduces all preceding text outputs.

## Round 2: numbered labels

The importer now preserves custom list-label formats, legal numbering and decimal padding from two through five digits. It accepts the standard and extension forms of `num-list-format`, converts supported placeholders into the existing numbering model, and retains padding through an optional `minimumDigits` field. Legal numbering converts Roman or lettered labels to decimal while preserving imported Arabic padding. Zero starting values remain unpadded, matching the observed application behavior.

The additional field applies to imported numbering styles. Word import leaves it unset, preserving its existing legal-numbering behavior. Formats that cannot be represented by the model's nine-level placeholder syntax retain the prior prefix/suffix fallback.

| Accuracy metric | Allocation checkpoint | Numbering change |
| --- | ---: | ---: |
| ODT pooled F1, 220 scored | 99.89095% | 99.89132% |
| ODT average-document F1 | 98.01878% | 98.02787% |
| FODT pooled F1, 37 scored | 99.97228% | 99.99653% |
| FODT average-document F1 | 99.61459% | 99.97155% |
| ODP pooled F1, 86 scored | 95.78032% | 95.78032% |

Two scored documents improve to 100%: one from 86.79%, the other from 98.00%. Six extracted texts change, and 750 readable texts remain byte-identical. There are no scored regressions or changed parse outcomes. The full model/block hash comparison changes only nine text inputs; three changes affect metadata without altering benchmark text.

Mean ODT extraction measures 0.3705 to 0.3687 ms, and mean ODP serialized bundles 0.4948 to 0.4931 ms. These differences are under 0.5% and are treated as broadly unchanged latency on this shared host.

Forty generated flat-text documents vary padding width, starting value, legal numbering and custom labels. All match fresh LibreOffice text exports after the benchmark's normalization. The initial probe exposed zero-value padding differences, which were corrected before the final corpus runs. All 469 existing tests and the WASM check pass; no new unit tests were added.

Behavior was checked against the pinned LibreOffice [numbering importer](https://github.com/LibreOffice/core/blob/7d5a5b55d0ea14a404957218e71e716a73524c57/xmloff/source/style/xmlnumi.cxx) and [numbering value formatter](https://github.com/LibreOffice/core/blob/7d5a5b55d0ea14a404957218e71e716a73524c57/editeng/source/items/numitem.cxx), alongside the [OOXML legal-numbering definition](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.islegalnumberingstyle?view=openxml-3.0.1). Source was reference only; no implementation was copied or translated. Downloaded originals and notices remain outside the workspace in `/tmp/odf-numbering-reference-20261116`.

Evidence is under `output/odf-hillclimb-20261116/round2`, including source URLs and hashes, both benchmark passes, generated inputs and independent exports, model differences and validation logs. Corpus inputs, reference exports and exclusions remain unchanged, and the fresh baseline reproduces the preceding checkpoint.

## Round 3: shared text-style attribute scanning

The shared style reader now collects its supported text attributes in one pass, then applies the existing conversions. This avoids repeatedly scanning the same attribute list. Font fallback, language construction, inherited values and malformed-value handling are unchanged.

| Warm metric | Numbering checkpoint | Single scan |
| --- | ---: | ---: |
| ODT mean text extraction | 0.3663 ms | 0.3657 ms |
| ODT median text extraction | 0.0784 ms | 0.0778 ms |
| ODP mean text extraction | 0.3282 ms | 0.3217 ms |
| ODP median text extraction | 0.1520 ms | 0.1485 ms |
| ODP mean deferred bundle, including JSON | 0.4913 ms | 0.4817 ms |
| ODP median deferred bundle, including JSON | 0.1833 ms | 0.1765 ms |

ODP mean extraction and bundle times decrease about 2%; page latency is essentially unchanged. These are counterbalanced local measurements, not production latency guarantees. All 756 readable text outputs, complete models and blocks for all 500 readable text inputs, and all 256 complete presentation/drawing bundle JSON outputs match their baseline hashes exactly. All F1 scores and parse outcomes are unchanged.

An additional 130 generated text-style combinations cover fonts, font sizes, colors, languages, character spacing and hidden text; complete model and block hashes match the predecessor for every case. All 469 existing tests and the WASM check pass. Evidence, frozen binaries, source receipts and the patch are under `output/odf-hillclimb-20261116/round3`.

## Round 4: stored note citations

The text reader already retained citation labels, but the page adapter discarded them. The adapter now preserves them in optional `noteReference.label` metadata. Structured text includes those labels and updates inline UTF-16 offsets, including nested note stories. The underlying run text stays empty, so the existing Word exporter does not emit duplicate markers. Existing viewers and exporters that generate note numbers retain their prior presentation behavior; they do not yet use this metadata to reproduce custom labels.

| Accuracy metric | Style-scan checkpoint | Citation recovery |
| --- | ---: | ---: |
| ODT pooled F1, 220 scored | 99.89132% | 99.90344% |
| ODT average-document F1 | 98.02787% | 98.08925% |
| FODT pooled F1, 37 scored | 99.99653% | 100.00000% |
| FODT average-document F1 | 99.97155% | 100.00000% |
| ODP pooled F1, 86 scored | 95.78032% | 95.78032% |

Fifteen scored inputs improve, thirteen to 100%, with no scored regressions. Twenty-two extracted texts change; 734 readable texts remain byte-identical. Complete page models and blocks change only for the 23 inputs containing notes. All 256 presentation/drawing bundles remain byte-identical. The same 756/764 inputs parse successfully.

Mean ODT extraction measures 0.3648 to 0.3691 ms, a 1.18% increase; median time changes from 0.0810 to 0.0815 ms. ODP mean bundle time measures 0.4848 to 0.4832 ms. These small shared-host timing differences do not demonstrate a speed improvement for this fidelity change.

Twenty generated documents validate footnotes, endnotes, custom labels, supplementary Unicode, source preservation, model round trips, inline offsets and absence of duplicate exported markers. All eighteen nonempty labels match fresh LibreOffice exports. The two empty source labels remain empty; LibreOffice generates new labels for those, which is a documented remaining difference. All 23 note-containing corpus models also pass JSON round trips, including nested notes. The corpus-wide note probe omits Word XML export because unrelated unsupported image paint can reject that conversion; duplicate-marker checks use the generated documents.

All 469 existing tests and the WASM check pass. One existing assertion was updated to include the previously omitted citation; no unit tests were added. Evidence and frozen receipts are under `output/odf-hillclimb-20261116/round4`.

## Round 5: list labels in aggregate text and fresh-list starts

The shared block aggregator now includes a child paragraph's existing list label when composing table cells and other aggregate stories. Paragraph content, its inline offsets and its separate label metadata remain unchanged. Nested aggregates include each label once. This also corrects aggregate text for numbered Word table cells, using the same existing label metadata.

New top-level OpenDocument lists now carry an explicit starting value from their list style. This prevents the common numbering model from treating a fresh list as a continuation of earlier numbering. Explicit continuations retain their existing path. The [OpenDocument list-continuation rules](https://docs.oasis-open.org/office/OpenDocument/v1.3/os/part3-schema/OpenDocument-v1.3-os-part3-schema.html) distinguish these cases; the initial generated table probes exposed the missing restart.

ODT pooled F1 increases from **99.90344% to 99.90662%**, and average-document F1 from **98.08925% to 98.15963%**. Five scored inputs improve, with no regressions; two reach 100%, including a table-list case previously at 85.42%. FODT remains at 100% on 37 scored inputs, and slide scores are unchanged. Sixteen text outputs change, leaving 740 readable texts byte-identical. Ninety-one page model/block outcomes change, mostly through explicit numbering-start metadata; all parse outcomes are unchanged.

Mean ODT extraction measures 0.3662 to 0.3684 ms (0.60% higher), with median 0.0812 to 0.0818 ms. This fidelity change does not establish a latency improvement. Thirty-two generated documents cover restarted and continued lists, numeric/letter/Roman/bullet labels, empty items, nested tables and table headers. All match fresh LibreOffice exports and pass model JSON round trips. All 469 existing tests and the WASM check pass; no unit tests were added.

Evidence and frozen receipts are under `output/odf-hillclimb-20261116/round5`. Remaining numbering gaps include style overrides, consecutive numbering across levels and list levels beyond the common model's nine-level limit.

## Round 6: rejected storage experiments

A diagnostic probe examined 60 XML parts from the 20 slowest inputs. Node reservations already accommodated every part, while attribute storage grew for 36 parts. Counting assignment characters before parsing made the isolated XML phase about 2.5% slower, so that approach was discarded. A larger fixed reservation improved the isolated phase about 1.4% and was then evaluated alongside boxed note-reference payloads, which reduce the optional payload from 40 to 8 bytes on the measured architecture.

The combined candidate preserved every text, all 500 complete readable page models/blocks, all 256 presentation bundles, and the 52 generated note/table model outputs. All 469 existing tests and the WASM check passed. However, mean ODT extraction increased from 0.3718 to 0.3778 ms (1.62%), while mean ODP bundle time changed from 0.4873 to 0.4886 ms. The full measurements did not justify retaining the candidate.

Both storage changes were reverted. All 233 production-source hashes were verified against the accepted round 5 checkpoint. No boxed-note API change remains. The isolated phase measurement does not establish an end-to-end benefit, and the combined result does not isolate each change's individual cost. Evidence and the rejected source patch are retained under `output/odf-hillclimb-20261116/round6`.

## Round 7: rejected graphic-style scanning experiment

A single-pass graphic-attribute reader preserved all 756 readable texts, all 500 readable page models/blocks, all 256 presentation/drawing models and all 256 bundle JSON outputs. Eager, deferred and round-tripped bundles agreed, and all 469 existing tests and the WASM check passed.

Mean ODP extraction decreased from 0.3264 to 0.3202 ms (1.91%), and mean serialized bundle time from 0.4871 to 0.4779 ms (1.88%). Mean ODT time increased from 0.3732 to 0.3776 ms (1.20%). An additional counterbalanced repeat of the two largest page inputs confirmed a cost: their median warm times increased from 62.6948 to 63.5581 ms and from 23.3561 to 24.8160 ms, with sixteen samples per input and arm.

The candidate was reverted because the slide improvement did not justify that page tradeoff. All 233 production-source hashes again match accepted round 5. Evidence, the rejected patch and repeated workload measurements are under `output/odf-hillclimb-20261116/round7`; the rejected candidate changes no reported accuracy results.

## Round 8: rejected slide-list label experiment

A candidate preserved resolved list labels in slide paragraph metadata and included them in aggregate extraction text without changing paragraph offsets. An accuracy pilot improved three scored ODPs but regressed two. Thirty-two generated documents passed eager/deferred and model-round-trip checks, but fresh application exports exposed different numbering behavior in Impress: text-document rules for restarts, continuations and displayed ancestor levels do not directly reproduce its slide output. The first generated styles also required correction from common to automatic styles before the application recognized them.

The candidate was reverted rather than shipping an incomplete numbering interpretation. All 233 accepted production-source hashes were restored. No counterbalanced final latency claim is made for this candidate. The pilot, generated inputs, application exports, diagnostic variants and rejected patch are under `output/odf-hillclimb-20261116/round8`. Missing slide list labels remain a known limitation.

## Round 9: sub-point reading-order boundaries

The whitespace partitioner treated a 0.17-point overlap between a heading and body as an ambiguous region and retained the drawing order. Extraction now insets each edge by at most a quarter point and at most 0.5% of that dimension before finding partitions. This allows up to half a point of overlap, capped at 1% of the dimension. Larger overlaps keep the prior behavior. Drawing geometry, model arrays, block IDs and source paths are unchanged.

ODP pooled F1 increases from **95.78032% to 95.87109%**, and average-document F1 from **93.54625% to 93.65187%**. Three scored ODPs improve, with no regressions in any format. One heading/body case rises from 93.71% to 99.78%. Three text outputs change; 753 remain byte-identical. Parse results remain 756/764, and the fresh baseline reproduces the preceding accepted checkpoint.

Mean ODP text extraction measures 0.3220 to 0.3306 ms (2.69% higher), while mean serialized bundles measure 0.5075 to 0.5023 ms (1.02% lower). Median text time increases from 0.1478 to 0.1543 ms; bundle median decreases from 0.1813 to 0.1800 ms. The unchanged page path also measures a 4.07% mean decrease in this run, illustrating host variation. These measurements do not establish a speed improvement for the boundary change.

All 256 complete presentation/drawing models are unchanged. Of 256 bundle JSON outputs, 253 are byte-identical; the other three differ only in block order, corresponding slide block-ID order and group aggregate content. Each slide's block-ID sequence matches its extracted blocks. Eager, deferred and model-round-tripped bundles agree for every input. Sixteen generated checks cover the prior ordering cases, sub-point overlaps on both axes, larger overlaps and small-object limits. All 469 existing tests and the WASM check pass. No unit tests were added.

Evidence and frozen receipts are under `output/odf-hillclimb-20261116/round9`. Cumulative accuracy through this accepted candidate improves 38 distinct scored documents with no regressions relative to the original reading-order/SVM task baseline. Sixty-six readable text outputs change, and 690 remain identical. Rejected rounds 6–8 are excluded.

## Cumulative comparison through round 9

A separate baseline/candidate/candidate/baseline run compares the frozen original object-text parser with accepted round 9. This measures the cumulative change directly; it does not add percentages from separate iterations. Both arms reproduce all 756 outputs from their respective earlier checkpoints. All 764 inputs, reference exports, exclusions and scoring rules remain unchanged.

| Format | Scored | Original pooled F1 | Final pooled F1 | Original average-document F1 | Final average-document F1 |
| --- | ---: | ---: | ---: | ---: | ---: |
| ODP | 86 | 95.09918% | 95.87109% | 92.30035% | 93.65187% |
| FODP | 6 | 95.72650% | 97.89514% | 91.11664% | 91.49361% |
| ODT | 220 | 99.89095% | 99.90662% | 98.01878% | 98.15963% |
| FODT | 37 | 99.97228% | 100.00000% | 99.61459% | 100.00000% |

Thirty-eight distinct scored documents improve and none regress. Parse outcomes remain 756/764, with the same eight ODT failures. Other format accuracy aggregates are unchanged. Sixty-six readable text outputs change; 690 are byte-identical.

| Warm metric | Original | Final | Mean change |
| --- | ---: | ---: | ---: |
| ODP mean text extraction | 0.3369 ms | 0.3280 ms | −2.65% |
| ODP median text extraction | 0.1578 ms | 0.1537 ms | — |
| ODP mean deferred bundle, including JSON | 0.5041 ms | 0.4918 ms | −2.45% |
| ODP median deferred bundle, including JSON | 0.1815 ms | 0.1798 ms | — |
| ODT mean text extraction | 0.4004 ms | 0.3812 ms | −4.81% |
| ODT median text extraction | 0.0829 ms | 0.0817 ms | — |
| FODP mean text extraction | 0.0733 ms | 0.0730 ms | −0.41% |
| FODT mean text extraction | 0.0382 ms | 0.0377 ms | −1.38% |

These are local, warm, in-process measurements on a shared host. The differences are modest and do not establish cold-start or production latency. FODT median time increases from 0.0325 to 0.0328 ms, and FODP median from 0.0081 to 0.0082 ms. The original baseline is the frozen checkpoint described in the reading-order/SVM report, including the preceding local field and object-text fixes; it is not unmodified repository HEAD.

Final receipts verify all 233 current production-source hashes, 1513 input/reference hashes, four benchmark-script hashes and four frozen runner hashes. All 469 existing tests, the WASM check, 256 bundle-equivalence checks and sixteen ordering probes pass. The original bounded SVM expected-result and mutation checks remain applicable to the unchanged decoder. Evidence is under `output/odf-hillclimb-20261116/final-comparison`, with cumulative accuracy also recorded in `cumulative-through-round9.json`.

Remaining gaps include line interleaving across overlapping objects, multicolumn frames, slide list numbering, chart legend/table ordering, legacy `SVGD`, unsupported text encodings and raster/outlined text. Stored dynamic fields and saved-preview formatting can differ from fresh application exports. Source references were used to understand format behavior; no third-party implementation was copied or translated. This remains local work, with no commit, release or deployment performed.

## Round 10: resolve only the slide style properties being used

Slide runs now inherit text properties directly, while graphic and table consumers request their respective property groups. This avoids resolving and cloning unrelated style groups. Existing percentage-size inheritance and default-style behavior are preserved.

All 756 readable texts, all 256 complete presentation/drawing models and all 256 bundle outputs are identical to round 9. Another 48 generated style cases preserve their complete models and bundles, including nested spans, defaults, percentage sizes, cycles and table cells. Eager, deferred and model-round-tripped bundles agree. All 469 existing tests and the WASM check pass.

Mean ODP text extraction decreases from 0.3267 to 0.3212 ms (1.68%), with median 0.1539 to 0.1523 ms. FODP mean decreases from 0.0725 to 0.0699 ms (3.60%). ODT mean is effectively unchanged at 0.3732 to 0.3730 ms. The full ODP bundle run measures 0.4840 to 0.4866 ms (0.53% higher), so it does not establish a bundle improvement.

To check that mixed result, the eight largest presentation/drawing bundle workloads were repeated in counterbalanced order, with sixteen samples per input and arm. Seven median times decrease, one increases 0.71%, and the mean of their per-input medians decreases 0.68%. The candidate is retained for the text-path improvement and preserved behavior, with bundle performance treated as broadly unchanged at this scale. These incremental percentages are not added to the earlier cumulative comparison.

Evidence, source/input/runner receipts, exact-output comparisons and the repeated workload measurements are under `output/odf-hillclimb-20261116/round10`. No new unit tests were added.

## Round 11: consecutive numbering and nested labels

The importer preserves `text:consecutive-numbering`, which the [OpenDocument specification](https://docs.oasis-open.org/office/OpenDocument/v1.3/os/part3-schema/OpenDocument-v1.3-os-part3-schema.html) defines as one sequence across all list levels. Optional numbering metadata groups explicit restarts with their enclosing list and keeps independent lists separate. Structured extraction advances this shared sequence without losing each ancestor's displayed number. The metadata survives model JSON round trips; generic Word XML export does not yet reproduce this OpenDocument numbering mode.

Fresh application exports also exposed two adjacent issues. A restart attached to an item containing only sublists must not restart the next visible paragraph. A bulleted ancestor contributes no numeric placeholder to a descendant's label; previously that placeholder could suppress the complete label. Both are corrected in the OpenDocument adapter.

All 44 generated consecutive-numbering cases now match fresh LibreOffice exports. Across 88 generated documents, normalized exact matches increase from 33 to 83, with 50 improved F1 scores and no regressions. The five remaining mismatches involve ordinary hierarchical numbering: resuming an older independent list and a nested restart inside a parent item without text. These are retained in the evidence, rather than excluded. All 88 models pass JSON round trips.

On the unchanged full corpus, ODT pooled F1 increases from **99.90662% to 99.90717%**, with average-document F1 from **98.15963% to 98.16220%**. One scored document improves from 98.49% to 99.06%; there are no scored regressions. One readable text changes and 755 remain byte-identical. Parse outcomes stay 756/764. Thirteen page models change, two page block outputs change, and 495 of 508 page outcomes remain identical. All 256 slide/drawing models and all 256 bundle outputs are unchanged. The existing 469 tests and WASM check pass; no unit tests were added.

Mean ODT extraction measures 0.3744 to 0.3775 ms (0.83% higher), with median 0.08155 to 0.08195 ms. Mean ODP extraction measures 0.3225 to 0.3232 ms (0.22% higher), while mean ODP bundle time measures 0.4833 to 0.4774 ms (1.22% lower). This round is retained for its fidelity improvement; these small shared-host timing differences do not establish a speed benefit for the numbering change.

Frozen receipts, all generated cases and references, full corpus passes, model comparisons and checks are under `output/odf-hillclimb-20261116/round11`.

## Round 12: rejected counter-state optimization

A candidate allocated consecutive-numbering state only when needed and skipped its abstract-style lookup in documents without that mode. All 756 readable texts, 500 readable page models/blocks, 88 generated model/block outputs and 256 bundle outputs remained identical. The existing 469 tests and WASM check passed.

Mean ODT extraction measured 0.37962 to 0.37979 ms (0.04% higher), with median 0.08170 to 0.08205 ms. This did not establish a latency benefit, so the candidate was reverted. The bundle runner binaries were byte-identical between arms, yet mean ODP bundle time varied from 0.4906 to 0.5154 ms, further demonstrating the limits of small timing claims on this shared host.

All 233 production-source hashes were restored to the previously tested round 11 checkpoint. Evidence and the rejected patch remain under `output/odf-hillclimb-20261116/round12`. The pause receipt is `output/odf-hillclimb-20261116/paused-checkpoint.json`; the accepted runners are the frozen round 11 candidates. No benchmark process remains running. The work is local and uncommitted, with no release or deployment.

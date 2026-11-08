# PowerPoint parser benchmark

Run from the repository root. Generated inputs, native output, digests, profiles and reports stay under `output/pptx-benchmark-20261106/`. Source fixtures are fetched at pinned public repository commits; packages are content-hash deduplicated. The corpus is a regression suite, not a production prevalence sample.

The current retained native candidate is `deferred-artwork2`, measured in its eager default; the current eight-parser phase is `expanded-deferred-artwork` (630 attempted inputs, 628 source references, 610 common nonempty timing inputs, measured November 8, 2026). The [current report](../../output/pptx-benchmark-20261106/report/peers/index.html) and [audit](../../docs/pptx-parser-audit.md) distinguish paired optimization evidence from cross-parser timings. The combined collection contains 1,854 distinct inputs with 323 reserved; the latest 35 additions are metadata-classified only. Commands and phase descriptions below retain their historical scope.

Regenerate the current report and the published README heatmaps from the completed assessment:

```sh
output/pptx-benchmark-20261106/venv/bin/python scripts/pptx-benchmark/peer_report.py expanded-deferred-artwork --rust-label '0.1.6 + local'
output/pptx-benchmark-20261106/venv/bin/python scripts/parser-benchmark/feature_heatmaps.py --pptx output/pptx-benchmark-20261106/expanded-deferred-artwork/assessment.json --snapshot --pptx-label '0.1.6 + local'
```

Without `--snapshot`, figures are labelled as previews. The generator writes to `output/pptx-benchmark-20261106/readme-preview/` and never edits the root README; receipt paths stay repository-relative when `output/` is a symbolic link into another checkout, as in a worktree. The published copies are `docs/readme-assets/pptx-expanded-deferred-artwork-20261108-feature-recall.*` and `docs/readme-assets/docx-optimized-20261102-feature-recall.*`. The `expanded-deferred-artwork` assessment's recorded Rust source hashes match the retained `deferred-artwork2` candidate identity exactly; that source is rust-pptx 0.1.6 plus the three unreleased commits described at the end of this document, so the measured build is labelled `0.1.6 + local`. The previous phase, `expanded-editing`, measured `editing-namespaces1`, whose sources match the rust-pptx 0.1.6 sources exactly; its report is archived under `report/peers-before-deferred-artwork2/` and its preview under `readme-preview-before-deferred-artwork2/`.

```sh
python3 scripts/pptx-benchmark/download.py
uv venv output/pptx-benchmark-20261106/venv --python 3.12
uv pip install --python output/pptx-benchmark-20261106/venv/bin/python -r scripts/pptx-benchmark/requirements.txt
cargo build --release --locked --manifest-path scripts/pptx-benchmark/Cargo.toml
python3 scripts/pptx-benchmark/run.py --phase comparison
python3 scripts/pptx-benchmark/run.py --phase legacy --legacy
python3 scripts/pptx-benchmark/assess.py comparison
output/pptx-benchmark-20261106/venv/bin/python scripts/pptx-benchmark/report.py comparison
python3 -m http.server 8776 --bind 127.0.0.1 --directory output/pptx-benchmark-20261106/report
```

Use a new phase name after changing a parser, dependency, timed adapter or executable. A phase resumes its existing records; do not mix identities. Current corpus/scorer hashes and dependency locks belong with the report. Native timings include complete production parsing and corresponding native model/IR construction. Python-pptx additionally materializes the fields used by the benchmark, because its object model is lazy. MarkItDown uses the dedicated PPTX converter without OCR or an LLM.

Five warm samples average 1–10 parses after one cold parse. Reads, imports, process launch, explicit destruction, reference construction, hashing and output serialization are outside the warm timer. Python GC may run during timed work. Run timing jobs sequentially after builds, accuracy checks and other owned CPU work finish. Save raw samples, load averages, failure statuses, source identity and executable hashes. Source drawing-tree order is the ordered-text reference, not a claim about human reading order.

The independent reference reads ZIP and XML with Python's standard library and verifies CRCs. It covers literal slide text, table structure, explicit formatting, note-body text, classic chart caches and explicit top-level frames. Failures count as zero on reference-bearing content. Empty reference units do not contribute to macro text scores. Timings use only inputs completed by every compared engine. Raw source-literal precision/F1 remains visible because inherited layout/master text, SmartArt caches and chart expansions can legitimately add text. No reference has been altered to credit parser omissions.

Picture recall matches explicit slide-picture placements to actual retained bytes, preserving multiplicity per slide. Background/style fills, inherited placements and rendered-pixel fidelity are separate scopes. The table-literal metric permits added characters but requires every source non-whitespace character in order within a single one-to-one matched output table. Exact-anchor results remain alongside it. Formatting uses explicitly marked contiguous run five-grams and remains sensitive to segmentation. Native output models without a scored field are explicitly labeled; untouched source/package XML is not credited as parsed semantics.

The Rust runner takes `ENGINE INPUT OUTPUT REPEATS [full]`. `OUTPUT=-` disables serialization and digesting. With an output path, `full_sha256` covers every serialized native field before compacting diagnostic storage, including payloads, XML and SVG. `full` saves the complete output for diagnosing parity failures. The JSON object representation canonicalizes key order; array order is retained. Freeze the runner and dependency graph before optimization.

The current [audit](../../docs/pptx-parser-audit.md) maps measured gaps to the specification and React model. `spec_inventory.py` inventories all declarations in the locally extracted relevant ECMA schemas; inventory presence does not establish support. The original scores, saved baseline and tuning/holdout split remain immutable during optimization.

For performance changes, `feedback.py --label NAME --stage parity` builds and snapshots a release candidate, then verifies every serialized field and failure status against the frozen baseline. Use `--stage timing --count 15` for tuning. After selecting a candidate, `--stage timing --split holdout --count 0` measures the saved untouched holdout, with interleaved baseline/candidate/Office Oxide runs and a paired bootstrap interval. Do not tune on holdout results.

`semantic_gate.py --label NAME` audits the specific semantic changes in this iteration: valid empty presentations, complete classic-chart series and structured notes. It requires all unrelated output to remain exactly equal, every previously retained chart series to survive, and changed notes/chart caches to match the independent source. Notes retain the existing flattened text and add paragraphs/runs; the adapter reads this structure when present. The reference and original-output scores remain unchanged.

After validation, run a fresh five-parser phase and retain the original timing cohort:

```sh
python3 scripts/pptx-benchmark/run.py --phase optimized --binary output/pptx-benchmark-20261106/experiments/accuracy2/candidate
python3 scripts/pptx-benchmark/assess.py optimized --fixed-cohort comparison
output/pptx-benchmark-20261106/venv/bin/python scripts/pptx-benchmark/report.py optimized
```

The runner snapshots the executable, checks input hashes, and refuses to resume with changed identities. Source hashes identify local changes separately from the Git commit. Newly accepted empty decks do not alter the fixed timing cohort. A new report preserves the original report under `report/baseline/`.

Build the WASM bridge with `wasm-pack build crates/rust-pptx-wasm --target nodejs --out-name pptx_wasm --out-dir /absolute/path/to/output/pptx-benchmark-20261106/wasm-candidate --release -- --locked`, then run `node --expose-gc scripts/pptx-benchmark/wasm_check.mjs`. This checks the actual JS bridge on the entire corpus and retains compact models and byte-payload hashes. It is a correctness check, not a WASM latency benchmark or browser layout comparison.

`object_gate.py --label NAME` compares a candidate to the frozen notes/chart build. It independently checks nonvisual properties, actions, media relationships, OLE metadata and retained payload bytes; unrelated content and poster artwork remain equal. Inherited media wrappers retain their prefixed identity while nested posters may have local IDs. `object_probes.py` builds a synthetic package probe for external targets, entity decoding, media precedence, nonstandard payload paths and time offsets.

A speed-only candidate can use a newer full-output reference with `feedback.py --label NAME --stage parity --reference PATH --reference-records PATH`. The reference path and executable hash are retained with the experiment. The current `opt5` experiment uses `objects2`, not the original parser. Its 30-input sample is tuning data, not an untouched holdout.

The earlier object five-parser phase is `objects-optimized`, using `experiments/opt5/candidate`; assess it with `--fixed-cohort comparison`. Run the object WASM build into `wasm-objects`, then `node --expose-gc scripts/pptx-benchmark/wasm_check.mjs wasm-objects wasm-check-objects`. Compare with `compare_wasm.py --phase objects-optimized --semantic-label objects3 --record-file objects.jsonl --wasm-dir wasm-check-objects`. Original WASM and baseline report artifacts remain available.

The latest original-cohort comparison is `inserted`, built from `experiments/inserted2/candidate`. `inserted_gate.py` independently checks Office Math structure, InkML geometry/brushes and control definitions/preview relationships, while requiring unrelated models and assets to remain exact. `inserted_probes.py` exercises namespace/default/fallback behavior and nonstandard relationship paths. The `wasm-inserted` build is compared through `compare_wasm.py --phase inserted --semantic-label inserted2 --record-file inserted.jsonl --wasm-dir wasm-check-inserted`.

The frozen expanded manifest adds 927 unique presentation files from pinned LibreOffice, Apache Tika, Apache POI and Embrace sources. Its 721 tuning and 206 reserved validation files retain provenance, hashes and container classifications. `expanded_check.py` measures acceptance only, separately by format; OpenDocument and older Impress formats remain unsupported. The original manifest and reports are unchanged.

```sh
python3 scripts/pptx-benchmark/run.py --phase expanded-comparison --manifest output/pptx-benchmark-20261106/expanded/manifest.json --split tuning --containers pptx --binary output/pptx-benchmark-20261106/experiments/inserted2/candidate
python3 scripts/pptx-benchmark/assess.py expanded-comparison --exclude-empty
python3 scripts/pptx-benchmark/expanded_gap_audit.py expanded-comparison
output/pptx-benchmark-20261106/venv/bin/python scripts/pptx-benchmark/expanded_report.py expanded-comparison
```

The expanded phase has 466 OOXML tuning inputs, 464 source-reference packages and 446 common nonempty timing inputs. `expanded_gap_audit.py` classifies picture-score discrepancies against source relationships and both payload hashes without changing any scores. Reports preserve failed-input penalties, merged-cell contents, empty comments and the source-literal effects of equation insertion and SVG selection. An input beyond the default XML-node limit remains a real acceptance gap.

Fresh phases snapshot all harness/source identities. Use a new phase after modifying them; an already completed phase is an immutable historical result. For speed-only changes, compare complete output hashes on both original and expanded tuning inputs. The reserved validation set must not be used to choose optimizations.

`expanded_parity.py --label NAME --phase PHASE --reference-phase REFERENCE --baseline BINARY` compares full output/status hashes across all 466 new OOXML tuning files. If document-warning order differs, it performs repeated full-output replays and permits only that array ordering to differ; all bytes and other fields must match. `opt7` fixes a reproduced nondeterministic warning order. `opt8` matches all 318 original and all 466 expanded outputs exactly against `opt7`.

`expanded_timing.py --label NAME --count 40` interleaves the frozen reference, candidate, Office Oxide and undoc on even latency quantiles of the expanded tuning cohort. It saves seven warm samples, executable identities and a paired bootstrap interval. A run that overlapped another owned check is retained under `excluded-overlap/` and excluded from the reported experiment. Do not multiply ratios across candidates or interpret a tuning result as untouched validation.

The final byte-scan candidate is `opt8`. Its shared OfficeArt XML scanner uses the same `memchr` escape fast path as DOCX, plus quoted-attribute and tag-boundary scans. Build identity and exact-output comparisons precede timing; the full five-parser refresh is `expanded-optimized` with `experiments/opt8/candidate`. Assess that phase with `--exclude-empty`, run its image-gap audit, and pass it to `expanded_report.py`. The original expanded report remains under `report/expanded-baseline/`.

`python3 scripts/pptx-benchmark/supplemental_audits.py expanded-optimized` reproduces the separate strict-namespace source check and counts content returned on the prior legacy acceptance-gap set. Neither changes the frozen main reference or turns legacy acceptance into an accuracy score.


## Transition and timing checks

`timing_gate.py --label timing1` compares all 318 original and 466 expanded OOXML tuning inputs with the frozen `opt8` build. It verifies retained source subtrees, ordered timing nodes, common timing values, conditions, targets, keyframes, build references and transition fields, and requires all unrelated output to match exactly. `timing_probes.py BINARY LABEL` exercises the classic timing/transition types, namespace choices, strict color percentages, defaults, sound payload paths, and layout/master ownership. These are extraction checks, not animation-playback or visual-fidelity scores.

`timing_delta_gate.py --label timing2 --reference-label timing1` verifies complete output/error equivalence after the follow-up edge-case fixes; those fixes are additionally exercised by the focused probes. After both gates pass and other owned checks finish, `expanded_timing.py --label timing2 --semantic-reference-label timing1` measures the cost against the pre-animation `opt8` build on the existing 40-input tuning selection. Its semantic reference distinguishes this comparison from an output-identical speed-only change.

## Memory and grouped-chart checks

The `expanded-memory` phase uses the validated `xml-memory3` build, accepts all 464 reference-valid packages and has 447 common nonempty timing inputs. Its secondary `assessment-fixed.json` retains the earlier 446-input cohort. Compact XML storage, sequential unused-layout loading and removal of unused SVG definitions reduce the dense-input peak RSS by 32.0%; the default per-part node limit is 500,000. `svg_definition_gate.py`, `review_svg_gate.py` and `node_budget_gate.py` retain the full equivalence and acceptance evidence. Historical phase counts above remain historical.

Two ZIP trials (`zip1`, `zip2`) preserve all 784 outputs/errors but fail to establish a reliable median speed gain, so their production code is not retained. `expanded_timing.py --label NAME --exact-gate` uses a completed full-output gate and its exact baseline. `zip_probes.py --label NAME` separately verifies archive edge cases against the frozen baseline.

`chart_group_gate.py --label NAME` checks the corrected group-coordinate chart renderer against `chart-bounds1` on the original/expanded 784 inputs plus all 131 new tuning inputs. Only grouped-chart SVG viewport changes are permitted; all other fields must stay exact. The preceding `chart-bounds1` gate verifies the shared subpixel-bound fix. The current `chart-group1` gate passes 915/915, with ten corrected charts across four models. Native and WASM pass all 335 acceptance comparisons, while the strict WASM comparison retains two newly found artwork geometry differences. See `wasm-check-chart-group1/open-findings.json`.

The retained cache five-parser refresh is `expanded-cache`, using `experiments/shape-cache1/candidate`, `expanded/manifest.json`, `--split tuning --containers pptx`. It has 2,330 completed records, five warm repeats per engine/input and the same 447 common timing inputs and feature scores as `expanded-chartgroup`. Assess only after the runner finishes; the assessor rejects incomplete or duplicate record sets. `followup-chartgroup` separately measures the 16 new OOXML tuning packages and does not change the frozen expanded corpus or denominators.

`exact_candidate_gate.py --label shape-cache1 --baseline legacy-notes1 --manifest output/pptx-benchmark-20261106/legacy-features-cohort.json` verifies all 983 complete outputs/errors against the validated legacy build. The shape-cache pilot uses the same 40 selected tuning inputs as the preceding math experiment. Its 1.086× paired median ratio is a tuning result; summed medians are slightly worse. The complete WASM comparison uses `wasm-legacy-notes-cohort.json`, 514 inputs and `--exact-artwork`, retaining two strict numerical SVG failures.

The independent preservation extension pins `openpreserve/format-corpus` and verifies downloaded blobs with `corpus/classify_preservation.py`. Its five new files form three tuning inputs and one reserved original/migrated pair. Run `expanded_check.py` with `corpus-preservation/manifest.json` and `--split tuning` for acceptance-only checks. These inputs do not change the frozen main comparison denominators.


## Earlier expanded comments comparison

The earlier completed phase `expanded-comments`: 630 OOXML tuning inputs, 628 independent source-reference packages, 611 common nonempty timing inputs and 3,150 completed engine/input records. It uses `experiments/comments3/candidate` with the merged `expanded-with-aspose.json` manifest. The frozen earlier phases remain available. The combined corpus has 1,619 unique inputs, with 273 additional inputs reserved before semantic evaluation.

```sh
python3 scripts/pptx-benchmark/bom_source_gate.py --label xml-bom1 --baseline xml-scan2 --manifest output/pptx-benchmark-20261106/bom-source-cohort.json
python3 scripts/pptx-benchmark/bom_source_review.py --label xml-bom1 --manifest output/pptx-benchmark-20261106/bom-source-cohort.json
python3 scripts/pptx-benchmark/exact_candidate_gate.py --label xml-attributes2 --baseline xml-bom1 --baseline-proof bom-source-reviewed-summary.json --baseline-records bom-source.jsonl --manifest output/pptx-benchmark-20261106/bom-source-cohort.json
python3 scripts/pptx-benchmark/exact_candidate_gate.py --label legacy-directory1 --baseline xml-attributes2 --baseline-proof parity-summary.json --baseline-records parity.jsonl --manifest output/pptx-benchmark-20261106/bom-source-cohort.json
output/pptx-benchmark-20261106/venv/bin/python scripts/pptx-benchmark/legacy_directory_review.py --label legacy-directory1 --manifest output/pptx-benchmark-20261106/bom-source-cohort.json
python3 scripts/pptx-benchmark/exact_candidate_gate.py --label comments2 --baseline legacy-directory1 --baseline-proof directory-reviewed-summary.json --baseline-records directory-reviewed.jsonl --manifest output/pptx-benchmark-20261106/bom-source-cohort.json
python3 scripts/pptx-benchmark/comments_review.py --label comments2 --manifest output/pptx-benchmark-20261106/bom-source-cohort.json
python3 scripts/pptx-benchmark/exact_candidate_gate.py --label comments3 --baseline comments2 --baseline-proof comments-reviewed-summary.json --baseline-records comments-reviewed.jsonl --manifest output/pptx-benchmark-20261106/bom-source-cohort.json
python3 scripts/pptx-benchmark/comments_probes.py --label comments3
python3 scripts/pptx-benchmark/comments_budget_probes.py --label comments3
python3 scripts/pptx-benchmark/run.py --phase expanded-comments --repeats 5 --manifest output/pptx-benchmark-20261106/expanded-with-aspose.json --split tuning --containers pptx --binary output/pptx-benchmark-20261106/experiments/comments3/candidate
python3 scripts/pptx-benchmark/assess.py expanded-comments --exclude-empty
python3 scripts/pptx-benchmark/assess.py expanded-comments --exclude-empty --fixed-cohort expanded-comparison --output-name assessment-fixed.json
python3 scripts/pptx-benchmark/expanded_gap_audit.py expanded-comments
output/pptx-benchmark-20261106/venv/bin/python scripts/pptx-benchmark/expanded_report.py expanded-comments
```

These commands reproduce the workflow in a fresh experiment directory; existing immutable receipts cause the gates to refuse overwriting. The source gate compares changed outputs against the frozen preceding parser on copies with only leading UTF-8 BOMs removed from XML members. It preserves the two raw chart-style/color source-prefix findings; the separate review verifies original source bytes. The reviewed audit passes 1,156/1,156, with 79 recovered inputs and two corrected existing source captures. Native/WASM comparison uses `wasm-bom-cohort.json` and `--exact-artwork`: all 687 statuses match, with 624/625 successful compact models and complete artwork sets exact. One numerical 3D-artwork difference remains open.

The later legacy review recovers one additional presentation with byte-identical live streams. The classic-comment audit verifies 15 comments, including two empty comments, while every non-comment field stays exact; five nonclassic parts remain unscored. The final prepass refinement preserves all 1,156 outputs/errors. Raw exact-output gates retain their intentional semantic mismatches; run those gates before each corresponding review, with the predecessor binaries and records identified by their summaries.

The current source may contain a subsequent candidate. Each report names its frozen executable and source hashes; do not describe historical timing as a measurement of unvalidated later code.

## Additional corpus collection

`corpus/collect_loc.py --plan-only` reads the Library of Congress public metadata and the archive central directory with bounded HTTP ranges. Without `--plan-only`, it downloads a seeded selection, verifies published SHA-256/SHA-512 values plus ZIP CRCs, and resumes verified files. The default selection adds 150 files, including rare presentation extensions, within a 500 MB total budget. Downloads retain the original provenance and a frozen tuning/validation split.

`corpus/classify_followup.py` uses standard ZIP metadata and `olefile` 0.47 to classify those downloads and the additional Embrace manifest without invoking the presentation parser. The 152 additions contain 146 confirmed presentations, three malformed OLE containers, two HTML responses and an empty file. Twenty-one are reserved for validation. Use `expanded_check.py --manifest PATH --output-root PATH --label NAME --binary BINARY --split tuning` for a separate acceptance-only check. The frozen main manifests remain unchanged.

## README heatmap previews

Run `feature_heatmaps.py` from `scripts/parser-benchmark` with `--pptx` pointing to a validated assessment and `--output` pointing to a preview directory. The generator reads the retained DOCX accuracy artifact and the selected PPTX assessment, checks denominators and ratios, and exports matching annotated PNG/SVG matrices plus JSON, CSV and Markdown tables. Unscored dimensions remain gray, including DOCX dimensions missing from the retained competing adapters. Source hashes, measured dates and build identities travel with the exports. Generation does not modify the README; publication remains gated on completion of optimization and final validation.


## Package relationship validation

The `relationships2` build preserves all 1,156 canonical outputs/errors against `comments3`. Its separate `relationship_probes.py --label relationships2` harness checks 24 relocated-package variants and six external/foreign-type relationship cases. Full model comparisons reverse only explicitly relocated part identities, and an independent package check requires all other payload bytes to remain exact. Native/WASM models and artwork agree on all 30 probes.

```sh
python3 scripts/pptx-benchmark/exact_candidate_gate.py --label relationships2 --baseline comments3 --baseline-proof parity-summary.json --baseline-records parity.jsonl --manifest output/pptx-benchmark-20261106/bom-source-cohort.json
python3 scripts/pptx-benchmark/relationship_probes.py --label relationships2
python3 scripts/pptx-benchmark/expanded_timing.py --label relationships2 --exact-gate --count 40
```

Use a fresh experiment directory with the same frozen source/binary identity when reproducing. The first candidate's modern-comment regressions remain archived. A disk-interrupted record stream is retained under `incomplete-disk-attempt`; it is not a completed audit. The successful rerun and neutral paired timing have independent receipts. No reference or denominator was relaxed.

The corresponding full comparison is `expanded-relationships`. Reproduce the established five-parser workflow with `relationships2/candidate`, then run `assess.py`, `expanded_gap_audit.py`, and `expanded_report.py` for that phase. The common timing cohort remains 611 inputs and every engine feature aggregate is unchanged from `expanded-comments`. The older phase remains frozen. The report-generation script changed after measurement began; its exact premeasurement source is archived under `expanded-relationships/harness-at-measurement/`. Execution and scoring code stayed unchanged, and a separate report-generation receipt identifies the final rendering source.

## DrawingML field validation

The field source review checks the complete original model against `relationships2`, allowing only additive field metadata. It independently resolves each emitted field to its qualified source subtree and checks identity, type, cache presence/value and supported paragraph/run properties. The subsequent SmartArt correction has an exact full-corpus gate and separate source probes; the failed intermediate probe remains recorded.

```sh
python3 scripts/pptx-benchmark/fields_review.py --label fields1 --manifest output/pptx-benchmark-20261106/bom-source-cohort.json
python3 scripts/pptx-benchmark/exact_candidate_gate.py --label fields2 --baseline fields1 --baseline-proof fields-reviewed-summary.json --baseline-records fields-reviewed.jsonl --manifest output/pptx-benchmark-20261106/bom-source-cohort.json
python3 scripts/pptx-benchmark/fields_probes.py --label fields2
python3 scripts/pptx-benchmark/fields_boundary_probes.py --label fields2
python3 scripts/pptx-benchmark/fields_smartart_probes.py --label fields2
python3 scripts/pptx-benchmark/expanded_timing.py --label fields2 --fields-gate --fields-source-label fields1 --count 40
```

Use fresh directories when reproducing; completed receipts are not overwritten. Complete-model source review serializes binary payloads and is substantially slower than the measured parser operation. The allocation trial `fields3` is reverted because its timing interval does not establish a gain. The retained source and measured candidate for the `expanded-fields` refresh are `fields2`. New full-comparison runs archive the exact harness files listed in their measurement identity before execution.

## Image fallback validation

The image association audit permits only additive `fallbackAssetId` metadata. It resolves each emitted association to its qualified source picture or inherited placeholder and checks both payloads directly against ZIP contents. All non-association model fields, artwork and payload bytes remain exact against `fields2`. A separate check maps the 113 existing image-metric mismatches to explicit fallback associations without changing the scoring adapter.

```sh
python3 scripts/pptx-benchmark/exact_candidate_gate.py --label image-fallback1 --baseline fields2 --baseline-proof parity-summary.json --baseline-records parity.jsonl --manifest output/pptx-benchmark-20261106/bom-source-cohort.json
python3 scripts/pptx-benchmark/image_fallback_review.py --label image-fallback1 --manifest output/pptx-benchmark-20261106/bom-source-cohort.json
python3 scripts/pptx-benchmark/image_fallback_probes.py --label image-fallback1
python3 scripts/pptx-benchmark/image_fallback_gap_check.py --label image-fallback1
python3 scripts/pptx-benchmark/expanded_timing.py --label image-fallback1 --image-fallback-gate --count 40
```

The first command intentionally reports 22 additive model differences; the independent review must pass before timing. The 17 focused packages cover strict/transitional namespaces, aliases, inheritance, media posters, groups, crop, and unavailable or external representations. Native/WASM compact models and artwork are exact on these probes. Initial generated PNG fixtures had an invalid IDAT CRC and remain archived under `probes-invalid-png`; the final probes use a deterministic CRC-valid PNG.

`image_fallback_browser.py` generates a browser harness for the direct-image contract after bundling the current viewer with esbuild. It explicitly omits generated `pictureSvg` artwork to exercise that path. Seven browser cases check primary pixels, fallback pixels, crop, bounded failure and destruction. Generated artwork recovery remains open and is not included in these passing cases. Full comparison phase `expanded-images` uses the same established scoring/reference/cohort; fresh runs archive their exact execution and report-generation sources.
## Additional namespace and SDK checks

The namespace-cache experiment was reverted after output-identical validation and neutral paired timings. Its frozen receipts remain under `output/pptx-benchmark-20261106/experiments/namespace-cache1/`. To reproduce against newly named experiment/output directories:

```sh
python3 scripts/pptx-benchmark/namespace_cache_probes.py --label NEW_CANDIDATE --baseline image-fallback1
python3 scripts/pptx-benchmark/additional_exact_gate.py --label NEW_CANDIDATE --baseline image-fallback1 --name sdk-exact --manifest output/pptx-benchmark-20261106/corpus-followup/openxml-sdk/manifest.json
python3 scripts/pptx-benchmark/field_timing.py --label NEW_CANDIDATE --select-only
python3 scripts/pptx-benchmark/field_timing.py --label NEW_CANDIDATE
python3 scripts/pptx-benchmark/sdk_source_check.py --label image-fallback1 --output NEW_SDK_OUTPUT
```

The targeted timing requires the complete established exact-output gate and the additional SDK gate. The field-count selection must be saved before measurement. The SDK source checker scores only the 156 tuning inputs, using each package's Strict or Transitional namespaces; it does not modify the original reference cache or report latency rankings. The completed supplement is `corpus-followup/openxml-sdk/source-check1/assessment.json`.

## Qualified image selection checks

The retained `svg-namespace2` build has exact established/SDK corpus output, 50 focused namespace/container probes, 17 existing image probes and 48 React generated-artwork browser checks. The complete proof is `experiments/svg-namespace2/validation.json`. These focused probes improve representation selection; they do not alter the frozen headline reference or establish generated-artwork decode-failure recovery.

```sh
python3 scripts/pptx-benchmark/svg_namespace_probes.py --label NEW_CANDIDATE
python3 scripts/pptx-benchmark/image_fallback_probes.py --label NEW_CANDIDATE
python3 scripts/pptx-benchmark/svg_namespace_browser.py --label NEW_CANDIDATE
```

The browser generator verifies the frozen consumer bundle and its source hashes. Execute `window.runSvgNamespaceProbe()` on `report/svg-namespace-probe.html` and save the returned per-case results. The page consumes unmodified native models; the two intentionally mislocated child variants have no generated artwork and are verified separately through their explicit warning and retained raster. Run paired timing only after native, WASM and browser checks have finished.



## Modern comment validation

`modern_comments_audit.py --label NAME --probes` checks rich bodies, replies, qualified extensions, author relationships, anchor paths, physical units and depth boundaries. Its corpus mode takes an explicit tuning manifest and prior full-output records, requires all non-comment model fields to remain exact, and independently verifies changed comments against source XML. The completed `modern-comments2` audit uses `bom-source-cohort.json` and the retained `xml-recent1/parity.jsonl`; a separate SDK run remains exact.

`modern_comments_wasm.py --label NAME` verifies the focused bridge probes and prior-WASM corpus. The four real changed inputs are checked separately because three lie outside the previous WASM manifest. `expanded_timing.py --modern-comments-gate --cohort-phase expanded-xml-recent --label modern-comments2 --count 40` validates source/build receipts before timing. `comment_timing.py --label NAME` measures every changed real input separately. Preserve existing outputs rather than rerunning into the same directory.

The native comparison adapter recursively includes reply text without changing the independent reference. The fresh `expanded-modern-comments` eight-parser phase preserves all feature scores. A separate undoc diagnostic records table-order variability; raw output and scored ordering remain untouched.


## Table grid source review

`table_grid_review.py --label NAME --manifest MANIFEST --reference-records RECORDS` requires exact non-table-row output and independently verifies source grid cells, spans, merge flags and origins. For changed tables, the preceding parser on byte-preserving unmerged packages checks rich content and grid-column styling. This is a metamorphic regression check, not a native rendering oracle. The independent headline source reference remains unchanged and includes covered-cell data.

`table_grid_probes.py --label NAME` covers horizontal, vertical, rectangular and adjacent merges, strict namespaces, false and inconsistent flags, ragged rows and large spans. The generated native models feed `table_grid_browser.mjs`, which verifies visible DOM text, spans, column widths, colors and search/text collection. Covered text must remain retained but invisible.

`table_grid_wasm.py --label NAME` compares the prior WASM cohort plus focused probes and changed inputs outside that cohort. It independently replays the existing strict curved-3D numerical mismatch instead of waiving it. `expanded_timing.py --table-grid-gate --label NAME --cohort-phase expanded-modern-comments --count 40` checks source/build proofs before the tuning pilot. `table_grid_timing.py --label NAME` measures every changed real table input separately. Complete builds and validation jobs before running either timing study; preserve immutable receipts in distinct experiment directories.


## Indexed XML cache follow-up

`xml-indexed1` retains owned cache entries in an indexed vector and checks four recent indices with full content equality. The 40-input pilot was inconclusive; the predeclared 610-input follow-up measures a 1.014× median ratio (95% document-bootstrap interval 1.011–1.019×) against `table-grid1`. Complete native outputs/errors are exact on 1,156 established and 156 SDK inputs. All 392 existing workspace checks and 886 standalone cache snapshots pass. The actual WASM build is unchanged on 711 inputs and 63,716 artwork hashes; the prior strict native/WASM curved-3D difference remains open.

Use `exact_candidate_gate.py` and `additional_exact_gate.py` with explicit manifests before `expanded_timing.py --exact-gate --cohort-phase expanded-table-grid --count 610 --run-tag full`. Preserve pilot receipts separately before widening the audit. `exact_wasm_regression.py` verifies exact prior-WASM models, errors and artwork; `cache_resource.py` records scoped cold-process peak RSS on an explicit preselected manifest. Neither is a rendering-fidelity oracle.

The completed `expanded-xml-indexed` comparison uses the retained candidate and verifies every native result against its exact-output gate. `table_grid_measurement.py --phase expanded-xml-indexed --label xml-indexed1 --baseline-phase expanded-table-grid --exact-gate` preserves and diagnoses undoc table-block order variability. Run measurement verification before report-only edits; the post-measurement report receipt identifies the subsequent corpus-wording update. Both heatmaps retain fixed scales and raw count exports.

The pinned `corpus-followup/ppt4web/selection.json` chooses forty source groups before downloading. `corpus/collect_ppt4web.py` downloads original URLs within explicit per-file and total budgets, retains failures and hashes, deduplicates against prior manifests, and classifies only container directories. The completed sample adds 24 legacy PPT and 11 PPTX; five files exceeded the 16 MiB per-file cap. Bulk converted archives remain undownloaded. New inputs do not change the frozen benchmark cohort or reserved evaluation policy.

### Object editing metadata and bounded reuse

The latest retained native build is `editing-namespaces1`, measured in the completed `expanded-editing` eight-parser phase. The new source contract is described in [the editing documentation](../../docs/pptx-editing.md). `object_editing_review.py` validates qualified properties and proves every preexisting complete model value unchanged. Its final clean run covers 1,156 established inputs plus 156 SDK inputs. `object_editing_probes.py` generates 23 focused packages, and `object_editing_consumer.mjs` exercises the built React model resolver. `object_editing_wasm.py` checks the new fields and complete retained XML alongside exact previous artwork/model/error behavior, preserving the known strict native/WASM discrepancy.

For efficient source auditing, build the separate harness with `cargo build --release --locked --manifest-path scripts/pptx-benchmark/Cargo.toml --bin editing-audit`. Its `editing` output mode retains compact model data and source-node records, computes the complete original model hash, and computes another complete hash after removing only node editing fields. It is an audit tool, not a faster parser mode. The final source review checks its complete hash against the frozen candidate and its stripped hash against the previous source-reviewed baseline. Every payload and artwork byte participates in those hashes. Historical interrupted runs and diagnostic fixes are archived; the final clean replay reproduces all prior outputs, errors and source counts.

Use `object_editing_review.py --label object-editing1 --manifest MANIFEST --reference-records BASELINE_RECORDS --audit-binary AUDIT_BINARY` for a fresh source review. The `--name` argument isolates supplemental receipts. `--resume` is available only with a hash-verified checkpoint and archived harness provenance. The retained cache uses `exact_candidate_gate.py` against those source-reviewed records and `additional_exact_gate.py` for the SDK cohort. Preserve terminal build success and executable identities before freezing either binary.

The cache's 40-input `expanded_timing.py --exact-gate --cohort-phase expanded-xml-indexed` pilot measures a 1.031× median ratio versus `object-editing1` (95% interval 1.019–1.045×), with effectively unchanged summed medians. The separate `--object-editing-gate --object-editing-source-label object-editing1 --run-tag total-cost` comparison verifies the semantic audit plus subsequent exact-output chain and measures the richer cached model directly against `xml-indexed1`: 0.972× (0.962–0.987×), about 2.9% greater median latency. Do not multiply ratios from separate runs or present these pilots as the full eight-parser report. The wider paired measurements below are complete; final reserved validation and root README publication remain pending.

### Namespace reuse with the editing contract

The `editing-namespaces1` trial repeats the previously reverted namespace-cache approach on the new workload, with every prior complete output preserved. Both the 40-input pilot and unchanged 610-input follow-up support retention; the latter measures 1.024× median speed (1.020–1.030×) versus `object-editing-cache1`, with 1.92% lower summed medians. All 1,312 native outputs/errors, 711 previous WASM outputs/errors and 39 focused native/WASM cases remain exact.

`expanded_timing.py --label editing-namespaces1 --count 610 --cohort-phase expanded-xml-indexed --exact-gate --run-tag full` reproduces the immediate-predecessor comparison in a fresh experiment directory. The separate `--object-editing-gate --object-editing-source-label object-editing1 --run-tag total-cost-full` run follows hash-verified, exact-output links back to the source-reviewed feature build, then compares directly against its pre-feature baseline. It measures 0.975× (0.969–0.979×); this complete model still costs latency. The earlier pilot harness is archived as `timing-harness-v1.py`.

The fresh eight-parser phase is `expanded-editing`. Verify it with `table_grid_measurement.py --phase expanded-editing --label editing-namespaces1 --baseline-phase expanded-xml-indexed --editing-source-label object-editing1` before generating reports. The verifier requires every measured Rust output to equal the source-reviewed model, every prior Rust output to equal that review’s baseline, and every peer output to remain exact except independently replayed undoc table-block permutation. Editing metadata remains supplemental coverage outside the unchanged feature matrix.

### Office Art lookup index, exact path numbers and pattern tiles

Three chained output-identical candidates follow `editing-namespaces1`: `xml-offset-index1` (byte-offset element index for registered Office Art render inputs), `xml-numbers1` (exact fixed-point custom-geometry numbers) and `pattern-cache1` (block-wise pattern tiles with a per-parse encoded-tile cache). Each is staged under `output/pptx-benchmark-20261106/experiments/<label>/` with its `identity.json`, gated against its predecessor and timed on the fixed 40-input selection:

```sh
python3 scripts/pptx-benchmark/exact_candidate_gate.py --label xml-offset-index1 --baseline editing-namespaces1 --baseline-proof parity-summary.json --baseline-records parity.jsonl --manifest output/pptx-benchmark-20261106/bom-source-cohort.json
python3 scripts/pptx-benchmark/expanded_timing.py --label xml-offset-index1 --exact-gate --count 40 --cohort-phase expanded-xml-indexed
```

Repeat with `--label xml-numbers1 --baseline xml-offset-index1` and `--label pattern-cache1 --baseline xml-numbers1`. The final WASM build is checked with `node --expose-gc scripts/pptx-benchmark/wasm_check.mjs wasm-pattern-cache1 wasm-check-pattern-cache1 output/pptx-benchmark-20261106/wasm-bom-cohort.json --hash-artwork --hash-editing` followed by `exact_wasm_regression.py --label pattern-cache1 --manifest output/pptx-benchmark-20261106/wasm-bom-cohort.json --baseline-check wasm-check-editing-namespaces1 --current-check wasm-check-pattern-cache1`. All three pilots are neutral at the median (0.995×, 1.002× and 0.992×); the [audit](../../docs/pptx-parser-audit.md) records the per-candidate measurements and the profile attribution behind the next steps. The benchmark `Cargo.lock` now records the released path-crate versions, so `--locked` builds of the runner succeed again.

### Arena-backed XML documents

`arena-dom1` builds one arena-backed `XmlDocument` per part with borrowed names, values, text and retained source for valid UTF-8 parts. It follows `pattern-cache1` in the exact-output chain: `exact_candidate_gate.py --label arena-dom1 --baseline pattern-cache1 --baseline-proof parity-summary.json --baseline-records parity.jsonl --manifest output/pptx-benchmark-20261106/bom-source-cohort.json`, then `expanded_timing.py --label arena-dom1 --exact-gate --count 40 --cohort-phase expanded-xml-indexed` (1.149×, interval 1.111–1.183×). The WASM build `wasm-arena-dom1` is checked with `wasm_check.mjs ... --hash-artwork --hash-editing` and `exact_wasm_regression.py --label arena-dom1 --baseline-check wasm-check-pattern-cache1 --current-check wasm-check-arena-dom1`. Peak RSS is compared with `/usr/bin/time -l` on the three retained profiling inputs.

### Deferred artwork and on-demand node rendering

`deferred-artwork2` follows `arena-dom1`; `deferred-artwork1` is superseded because it skipped the ink render and failed its semantic gate on the four inputs with ink content parts inside `mc:AlternateContent`. The eager default is gated as before with `exact_candidate_gate.py --label deferred-artwork2 --baseline arena-dom1 --baseline-proof parity-summary.json --baseline-records parity.jsonl --manifest output/pptx-benchmark-20261106/bom-source-cohort.json`, and the WASM build `wasm-deferred-artwork2` with `wasm_check.mjs ... --hash-artwork --hash-editing` and `exact_wasm_regression.py --label deferred-artwork2 --baseline-check wasm-check-arena-dom1 --current-check wasm-check-deferred-artwork2`. The deferred mode has two scripts of its own:

```sh
python3 scripts/pptx-benchmark/artwork_deferral_gate.py --label deferred-artwork2 --manifest output/pptx-benchmark-20261106/bom-source-cohort.json --check-limit 6
python3 scripts/pptx-benchmark/deferred_timing.py --label deferred-artwork2 --count 40 --cohort-phase expanded-xml-indexed
```

The gate parses every manifest input with the runner's `rust-pptx` and `rust-pptx-deferred` engines and compares the compact models after removing `shapeSvg`, `pictureSvg`, `chartSvg`, `paintBounds`, `fallbackPicture` and the rendering-only warnings; it then runs the `rust-pptx-artwork-check` engine (`pptx-benchmark rust-pptx-artwork-check INPUT - LIMIT`), which renders up to `LIMIT` nodes of the input on demand through `render_node_artwork` and compares each with the eager artwork. It writes `artwork-deferral.jsonl` and `artwork-deferral-summary.json` and refuses to overwrite them. The timing script requires a passing gate for the same executable, selects the same 40 tuning inputs as `expanded_timing.py`, interleaves eager, deferred, Office Oxide and undoc with seven warm samples each and writes `deferred-timings.jsonl` and `deferred-timing-summary.json` (eager over deferred 1.248× (1.207–1.414×), Office Oxide over deferred 0.490× (0.416–0.596×)). Deferred ratios describe a different workload from the eager default and are never combined with it.

### Eight-parser comparison of the retained build

The fresh eight-parser phase is `expanded-deferred-artwork`, measured on the eager default of `deferred-artwork2` with the same manifest, engines, frozen peer binary and fixed 610-input timing cohort as `expanded-editing`:

```sh
python3 scripts/pptx-benchmark/run.py --phase expanded-deferred-artwork --repeats 5 --manifest output/pptx-benchmark-20261106/expanded-with-aspose.json --split tuning --containers pptx --binary output/pptx-benchmark-20261106/experiments/deferred-artwork2/candidate --engines rust-pptx,office_oxide,undoc,python-pptx,markitdown,pptxboss,betteroffice,rustypptx --peer-binary output/pptx-benchmark-20261106/expanded-editing/peer-benchmark --deduplicate-phase expanded-editing
python3 scripts/pptx-benchmark/assess.py expanded-deferred-artwork --exclude-empty --fixed-cohort expanded-editing
python3 scripts/pptx-benchmark/table_grid_measurement.py --phase expanded-deferred-artwork --label deferred-artwork2 --baseline-phase expanded-editing --exact-gate
python3 scripts/pptx-benchmark/assessment_verification.py expanded-deferred-artwork --baseline-phase expanded-editing
```

The exact-gate verifier requires every measured Rust output to equal both the candidate's gate record and the `expanded-editing` output, and every peer output to remain exact except independently replayed undoc table-block permutation (six inputs, 26 replays). `assessment_verification.py` then requires the common timing cohort and every engine's feature aggregates to be identical to the previous phase and writes `assessment-verification.json`. Run `peer_report.py` only after both verifications: its build-specific paragraphs read `measurement-verification.json`. The run shared the machine with unrelated user processes; the one-minute load average sampled after each invocation ranged from 8.2 to 21.3 on 14 logical cores (median 10.0, against 4.6–8.2 during `expanded-editing`), so medians are not compared across phases as paired evidence. The phase directory also holds `rebuild-check.json`: the runner rebuilt from the retained commit in a separate worktree matches the candidate's 78 recorded source hashes and all named Rust symbols, but not its executable hash, because Cargo embeds the absolute checkout path of path dependencies in panic locations and symbol metadata; the phase runs the retained candidate executable, as every phase does. The previous report is archived under `report/peers-before-deferred-artwork2/`, the previous preview under `readme-preview-before-deferred-artwork2/`.

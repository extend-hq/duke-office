# OpenDocument automatic chart layout and direct extraction — 2026-11-13

This round builds on the [previous unpublished candidate](opendocument-hillclimb-20261113.md). Core chart geometry and data semantics remain in `duke-odf` / `duke-office-charts`; thin ODP/ODG adapters select the shared APIs. No package versions were bumped or published, and React packages were not modified.

## Changes

- Automatic ODF radar bounds reserve legend/title space and solve for a circle plus measured category labels. Explicit inner plot bounds remain authoritative. Category labels use the same date formatting for measurement and paint, and no longer use the Cartesian chord-length truncation rule. If labels cannot fit beside the legend, horizontal fitting uses the available chart canvas. Very long labels and legend collisions remain imperfect.
- ODF radar axes now draw radial spokes and cross ticks, honor axis paint, and skip crowded numeric tick labels. Legend padding/key lengths use physical dimensions. This is gated to OpenDocument; OOXML rendering is unchanged.
- ODP/ODG model nodes retain native `ChartData`. Both the public chart API and extraction bundles prefer it over intermediary DrawingML. Deferring artwork now avoids generated chart XML/SVG and shape SVG in the ODF adapter. Saved assets remain available. Retained on-demand artwork for ODF is still unsupported; callers needing generated artwork must parse eagerly.

The ODT adapter already uses native chart data. A proposed preview-render skip was excluded because the corpus did not demonstrate a benefit: its chart replacements use an unsupported native metafile format, so that early-return path was not exercised. The experiment and original measurements are retained separately. No data-only full ODT parse mode is introduced.

`SlideNode::Chart.chart_data` is optional in serialized models and omitted when absent. Rust code constructing this enum variant must initialize the new field; the OOXML constructor and existing serialization check do so. The native values, sparse indices and scatter/bubble roles use the same shared data contract as Microsoft charts. No Extend Web deployment or endpoint smoke test is claimed.

## Evaluation

The baseline is the frozen preceding candidate, including its uncommitted shared-parser fixes. The final change set includes those earlier measured improvements, with their evidence recorded in the linked reports. The current comparison isolates the automatic-layout and direct-extraction work.

Visual measurements reuse the Microsoft-file methodology: complete fixed 432 × 432 pages at 96 dpi; RGB agreement within 16 over the union of ink below 247; no registration, crop, masks or per-object resizing. Both versions use the same viewer, Chromium 157.0.8103.78 and device scale 1. LibreOffice 26.2.2.2 supplies sealed native PDF/PNG references. Eight new probes use explicit axis/legend font sizes; the preliminary font-inheritance probe/reference is preserved separately.

| Visual set | Cases | Before median / mean | After median / mean |
| --- | ---: | ---: | ---: |
| Corpus-derived charts | 21 | 62.250% / 56.658% | 62.250% / 56.900% |
| Controlled charts | 11 | 78.396% / 72.778% | 78.396% / 73.039% |
| Automatic radar matrix | 8 | 3.273% / 3.316% | 6.191% / 7.227% |
| Controlled artwork | 15 | 97.465% / 96.573% | 97.465% / 96.573% |
| Supplemental artwork | 6 | 94.880% / 64.519% | 94.880% / 64.519% |

**Eleven of forty chart cases improve; twenty-nine are unchanged, with no measured regression.** All 21 artwork pages are unchanged, including two blank native references. The real radar scores rise from 25.584% to 27.855% and 30.525% to 33.338%. The controlled radar rises from 2.882% to 5.745%. All eight new automatic-layout probes improve, but their mean remains only 7.227%: fonts, exact plot bounds and long labels still need work. These results are a bounded gain, not automatic-layout parity.

### Deferred chart extraction

| Inputs | Count | Before mean / median ms | After mean / median ms | Mean change |
| --- | ---: | ---: | ---: | ---: |
| All supported presentations/drawings | 256 | 0.4467 / 0.1626 | 0.4200 / 0.1600 | -6.0% |
| Chart-bearing presentations | 8 | 2.5915 / 1.0113 | 2.1537 / 0.9086 | -16.9% |
| ODP | 217 | 0.4973 / 0.1727 | 0.4670 / 0.1692 | -6.1% |
| FODP | 22 | 0.0960 / 0.0112 | 0.0881 / 0.0106 | -8.2% |
| ODG | 10 | 0.3960 / 0.0867 | 0.3886 / 0.0874 | -1.9% |
| FODG | 5 | 0.0415 / 0.0260 | 0.0405 / 0.0241 | -2.3% |
| OTP | 1 | 0.1243 / 0.1243 | 0.1178 / 0.1178 | -5.2% |
| OTG | 1 | 0.0436 / 0.0436 | 0.0442 / 0.0442 | +1.4% |

**Complete bundles are structurally identical on all 256 inputs.** The combined native-data/deferred-generation path reduces mean latency by 6.0% overall and 16.9% on the eight chart-bearing presentations. The extraction executable remains byte-identical after dropping the unverified ODT experiment, so these matched passes measure the retained implementation.

### Full document parsing

| Format | Parsed | Before mean / median ms | After mean / median ms | Mean change |
| --- | ---: | ---: | ---: | ---: |
| ALL | 756 | 0.3456 / 0.1076 | 0.3355 / 0.1067 | -2.9% |
| ODT | 418 | 0.4238 / 0.0846 | 0.4079 / 0.0841 | -3.8% |
| FODT | 79 | 0.0394 / 0.0344 | 0.0391 / 0.0333 | -0.8% |
| ODP | 217 | 0.3507 / 0.1637 | 0.3465 / 0.1608 | -1.2% |
| FODP | 22 | 0.0765 / 0.0082 | 0.0757 / 0.0082 | -1.0% |
| ODG | 10 | 0.2163 / 0.0791 | 0.2130 / 0.0772 | -1.5% |
| FODG | 5 | 0.0393 / 0.0194 | 0.0389 / 0.0192 | -1.0% |
| OTT | 2 | 0.2945 / 0.2945 | 0.2933 / 0.2933 | -0.4% |
| OTP | 1 | 0.1194 / 0.1194 | 0.1189 / 0.1189 | -0.4% |
| ODM | 1 | 0.0813 / 0.0813 | 0.0839 / 0.0839 | +3.2% |
| OTG | 1 | 0.0474 / 0.0474 | 0.0488 / 0.0488 | +3.0% |

The final whole-document aggregate is 2.9% lower, but concurrent host activity substantially affected wall time (the baseline first pass took 141 seconds, versus 36 seconds for the candidate second pass). The earlier experimental round measured +0.7%. **No general full-parsing speedup is attributed to this change.** Preserve both measurements; the deferred extraction result is the performance improvement being retained.

**756/764 inputs parse**, with the same eight encrypted/invalid inputs rejected. All 756 text outputs are identical to the preceding candidate. ODT pooled/mean-file F1 remains 0.998900 / 0.980001; ODP remains 0.936054 / 0.890733. The corpus still exposes 33 charts with data and SVG, 2,346 drawings, 708/710 evaluated custom geometries and 696 rendered custom geometries.

**463 existing workspace tests and the WASM workspace check pass** on Rust 1.93.1. No new unit tests were added. All 110 existing chart-data probes pass across eight containers and unsupported-artwork variants. Eager/deferred/serialized-model checks pass for 47 charts across 258 inputs. The shared direct-data API also agrees exactly with the XML route on all 33 corpus charts; its microbenchmark is retained separately and is not a newly introduced speedup in this round.

Timing uses an Apple M4 Pro / 48 GiB host, release Rust 1.99.0, opt-level 3 and thin LTO. Both executables run in AB/BA order after builds and captures complete. Each document uses up to 200 warm repeats or 250 ms, with at least three calls. Reported means/medians aggregate each document's fastest warm call across both passes. Reads and output writes are excluded. Chart-extraction timing includes full bundle JSON serialization; ordinary text parsing retains its previous output scope. The host was shared and not isolated from other workloads. These are not request P50/P95, cold-start or production measurements. Tiny differences and small format groups are not durable speed guarantees.

The semantic checks compare complete extraction bundles, public eager/deferred chart APIs and a serialized deferred-model round trip. The existing eight-container matrix separately checks precision, missing values and scatter/bubble domains. ODS/FODS are evaluated through the shared ODF API, not a native spreadsheet viewer. Parsing success and SVG availability are not visual fidelity.

## Reference review and limits

The [pinned source manifest](../scripts/odf-visual/libreoffice-sources.json) records the LibreOffice behavior references. This round reviews chart layout orchestration, polar axes, legend geometry and line-key dimensions. The constraint solver and Rust changes are independently implemented from format semantics and observed behavior. Reference source and notices remain outside workspace crates. Existing bundled third-party font data and attribution are unchanged; the automatic radar measurement uses existing Arimo advances for the compatible Liberation Sans family, without adding fonts or outlining text. This finite source review does not prove universal originality.

Remaining gaps include font substitution/shaping, precise automatic plot geometry, long-label/legend collisions, advanced legend wrapping, secondary-axis/3D rendering and full host-document page layout. The real chart set consists of isolated embedded charts and includes converted Microsoft inputs and related variants. The additional radar matrix is deliberately adversarial, not production-weighted. Peer products were not rerun; these results do not support market-wide leadership.

Local evidence: `output/odf-layout-extraction-20261113` contains frozen binaries/source receipts; `real-charts`, `measured`, `supplemental` and `automatic-layout` reports; both AB/BA timing passes under `benchmark` and `extraction`; complete output bundles; data/model checks; corpus audit; and workspace/WASM logs. `automatic-layout-pilot` retains the preliminary matrix. The isolated snapshot rebuild replaces a shared-cache build that lost dependency artifacts; final models are checked against the captured models before accepting the receipts.

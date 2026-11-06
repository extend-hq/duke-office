# Rust Office

by Extend

[![CI](https://github.com/extend-hq/rust-office/actions/workflows/check.yml/badge.svg)](https://github.com/extend-hq/rust-office/actions/workflows/check.yml) [![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Rust libraries for parsing Word documents and PowerPoint presentations, and rendering Office charts and picture artwork. The chart renderer is shared by [React DOCX](https://github.com/extend-hq/react-docx), [React PPTX](https://github.com/extend-hq/react-pptx) and [React XLSX](https://github.com/extend-hq/react-xlsx). React XLSX uses DukeSheets to read workbooks and resolve worksheet values.

[rust-office-art](crates/rust-office-art) provides shared picture, shape and connector SVG generation for the Word and PowerPoint parsers and their WebAssembly integrations. It also reads SmartArt data and saved drawing trees. Supported formatting uses separate paint bounds. [Coverage and comparison method](docs/picture-coverage.md) distinguishes implemented rendering, stored previews and remaining fidelity gaps.

[Office fonts](packages/office-fonts) provides shared, lazily loaded Calibri, Arial, Times New Roman and Courier New fallbacks in viewers and backend rendering. Viewers use host-provided fonts or generic fallbacks for Aptos and Aptos Narrow. Font loading is explicit and stays separate from the chart WASM bundle. The Office Art glyph-outline renderer separately includes portable font outlines and substitutes its Carlito-derived face for unavailable Aptos; substituted contours can differ from Office. Build the font assets and metric tables with `python scripts/build-office-fonts.py` using FontTools and Brotli, and package them with `npm pack ./packages/office-fonts`.

| Crate | Purpose | Version | Downloads | API docs | Release |
| --- | --- | --- | --- | --- | --- |
| [rust-office-charts](crates/rust-office-charts) | Render Office chart definitions to SVG | [![rust-office-charts version](https://img.shields.io/crates/v/rust-office-charts.svg)](https://crates.io/crates/rust-office-charts) | [![rust-office-charts downloads](https://img.shields.io/crates/d/rust-office-charts.svg)](https://crates.io/crates/rust-office-charts) | [![rust-office-charts documentation](https://docs.rs/rust-office-charts/badge.svg)](https://docs.rs/rust-office-charts) | [0.1.6](https://github.com/extend-hq/rust-office/releases/tag/rust-office-charts-v0.1.6) |
| [rust-office-art](crates/rust-office-art) | Render Office pictures, shapes and connectors | [![rust-office-art version](https://img.shields.io/crates/v/rust-office-art.svg)](https://crates.io/crates/rust-office-art) | [![rust-office-art downloads](https://img.shields.io/crates/d/rust-office-art.svg)](https://crates.io/crates/rust-office-art) | [![rust-office-art documentation](https://docs.rs/rust-office-art/badge.svg)](https://docs.rs/rust-office-art) | [0.1.4](https://github.com/extend-hq/rust-office/releases/tag/rust-office-art-v0.1.4) |
| [rust-office-charts-wasm](crates/rust-office-charts-wasm) | WebAssembly bindings for Office chart rendering | [![rust-office-charts-wasm version](https://img.shields.io/crates/v/rust-office-charts-wasm.svg)](https://crates.io/crates/rust-office-charts-wasm) | [![rust-office-charts-wasm downloads](https://img.shields.io/crates/d/rust-office-charts-wasm.svg)](https://crates.io/crates/rust-office-charts-wasm) | [![rust-office-charts-wasm documentation](https://docs.rs/rust-office-charts-wasm/badge.svg)](https://docs.rs/rust-office-charts-wasm) | [0.1.6](https://github.com/extend-hq/rust-office/releases/tag/rust-office-charts-wasm-v0.1.6) |
| [rust-docx](crates/rust-docx) | Parse Word documents into a structured document model | [![rust-docx version](https://img.shields.io/crates/v/rust-docx.svg)](https://crates.io/crates/rust-docx) | [![rust-docx downloads](https://img.shields.io/crates/d/rust-docx.svg)](https://crates.io/crates/rust-docx) | [![rust-docx documentation](https://docs.rs/rust-docx/badge.svg)](https://docs.rs/rust-docx) | [0.2.11](https://github.com/extend-hq/rust-office/releases/tag/rust-docx-v0.2.11) |
| [rust-docx-wasm](crates/rust-docx-wasm) | WebAssembly bindings for the Word parser | [![rust-docx-wasm version](https://img.shields.io/crates/v/rust-docx-wasm.svg)](https://crates.io/crates/rust-docx-wasm) | [![rust-docx-wasm downloads](https://img.shields.io/crates/d/rust-docx-wasm.svg)](https://crates.io/crates/rust-docx-wasm) | [![rust-docx-wasm documentation](https://docs.rs/rust-docx-wasm/badge.svg)](https://docs.rs/rust-docx-wasm) | [0.2.11](https://github.com/extend-hq/rust-office/releases/tag/rust-docx-wasm-v0.2.11) |
| [rust-pptx](crates/rust-pptx) | Parse PowerPoint presentations into a structured slide model | [![rust-pptx version](https://img.shields.io/crates/v/rust-pptx.svg)](https://crates.io/crates/rust-pptx) | [![rust-pptx downloads](https://img.shields.io/crates/d/rust-pptx.svg)](https://crates.io/crates/rust-pptx) | [![rust-pptx documentation](https://docs.rs/rust-pptx/badge.svg)](https://docs.rs/rust-pptx) | [0.1.5](https://github.com/extend-hq/rust-office/releases/tag/rust-pptx-v0.1.5) |
| [rust-pptx-wasm](crates/rust-pptx-wasm) | WebAssembly bindings for the PowerPoint parser | [![rust-pptx-wasm version](https://img.shields.io/crates/v/rust-pptx-wasm.svg)](https://crates.io/crates/rust-pptx-wasm) | [![rust-pptx-wasm downloads](https://img.shields.io/crates/d/rust-pptx-wasm.svg)](https://crates.io/crates/rust-pptx-wasm) | [![rust-pptx-wasm documentation](https://docs.rs/rust-pptx-wasm/badge.svg)](https://docs.rs/rust-pptx-wasm) | [0.1.5](https://github.com/extend-hq/rust-office/releases/tag/rust-pptx-wasm-v0.1.5) |

## Packages, benchmarks, and chart accuracy

Expand a package for its usage, measured results, and detailed reports.

<details open>
<summary><strong>Excel and Word chart comparisons</strong></summary>

### Excel charts — 382 charts, 48 gallery examples

React XLSX now renders charts through Rust and WebAssembly, including 3D shapes, modern charts, source-linked number formats, shadows and bevels. Shared Office font fallbacks include Carlito, Arimo, Tinos and Cousine.

[![Microsoft Excel and React XLSX side by side across 24 chart examples](docs/readme-assets/excel-chart-overview.png)](docs/excel-chart-comparisons.md)

The November 5, 2026 corpus run rendered **382/382 charts in 43 workbooks**, with **381 native Excel image pairs**. The gallery contains 48 examples selected for feature coverage, including visible differences. These are visual comparisons, **not a claim of pixel-exact Excel parity**. Native 3D lighting, some plot and label placement, and map geometry still differ.

| Gallery | Examples |
| --- | ---: |
| [Bars, lines, areas and secondary axes](docs/readme-assets/excel-chart-bars-lines-areas.png) | 8 |
| [Pies, exploded slices and multiple rings](docs/readme-assets/excel-chart-pies-and-rings.png) | 8 |
| [Scatter, bubbles, radar, error bars and stock](docs/readme-assets/excel-chart-scatter-radar-stock.png) | 8 |
| [Modern charts and filled maps](docs/readme-assets/excel-chart-modern-charts.png) | 8 |
| [3D areas, pies, cones, cylinders, pyramids and surfaces](docs/readme-assets/excel-chart-three-dimensional.png) | 8 |
| [Shadows, bevels, labels and trendline equations](docs/readme-assets/excel-chart-formatting-and-effects.png) | 8 |

[Comparison method and limitations](docs/excel-chart-comparisons.md) · [All 382 measurements](docs/readme-assets/excel-chart-comparison.csv) · [Capture provenance](docs/readme-assets/excel-chart-provenance.json)

### Word charts — native chart rendering comparisons

The gallery compares **157 chart drawings** with Microsoft Word for Mac. It includes 110 charts from a chart family atlas, 40 additional map configurations, and seven charts from an independent Open XML SDK document. Viewer **0.14.0** produced a native chart image for **155 of 157**. The two volume-stock combinations produced no image.

[![Microsoft Word and react-docx side by side for 28 chart and map examples](docs/readme-assets/word-chart-overview.png)](docs/readme-assets/word-chart-overview.png)

The overview shows 28 pairs. The linked galleries include every measured case, including the missing images. **Ink overlap** measures the share of visible ink that lands in the same place in both images. **Color match** measures the share of that same area whose RGB values agree within 16 levels per channel. Both scores include titles, axes and legends.

| Chart group | Viewer images | Median ink overlap | Median color match |
| --- | ---: | ---: | ---: |
| [Bars, lines, areas and pies](docs/readme-assets/word-chart-bars-lines-areas-and-pies.png) | 20/20 | 96.8% | 77.2% |
| [Scatter, bubble, radar and stock](docs/readme-assets/word-chart-scatter-bubble-radar-and-stock.png) | 15/17 | 88.0% | 46.9% |
| [3D columns, bars, areas and lines](docs/readme-assets/word-chart-3d-columns-bars-areas-and-lines.png) | 19/19 | 68.3% | 39.3% |
| [Surfaces and split pies](docs/readme-assets/word-chart-surfaces-and-split-pies.png) | 13/13 | 96.0% | 88.8% |
| [Axes, statistics and missing data](docs/readme-assets/word-chart-axes-statistics-and-missing-data.png) | 21/21 | 82.1% | 62.5% |
| [Fills, colors and markers](docs/readme-assets/word-chart-fills-colors-and-markers.png) | 12/12 | 98.6% | 49.9% |
| [Modern charts and filled maps](docs/readme-assets/word-chart-modern-charts-and-filled-maps.png) | 8/8 | 95.3% | 94.7% |
| [Map areas and projections](docs/readme-assets/word-chart-map-areas-and-projections.png) | 24/24 | 86.3% | 82.5% |
| [Map data and styling](docs/readme-assets/word-chart-map-data-and-styling.png) | 16/16 | 95.7% | 92.6% |
| [Independent Open XML SDK sample](docs/readme-assets/word-chart-independent-open-xml-sdk-sample.png) | 7/7 | 56.2% | 39.3% |

The map cases cover countries, states, counties and ZIP areas, numeric and categorical fills, several projections, missing values, point colors, and legend positions. The SDK sample preserves Word’s two-column page clipping. [Download all per-chart measurements](docs/readme-assets/word-chart-comparison.csv) or [open the independent source document](https://github.com/dotnet/Open-XML-SDK/blob/main/test/DocumentFormat.OpenXml.Tests.Assets/assets/TestDataStorage/v2FxTestFiles/wordprocessing/chart/chart2.docx).

Measured November 4, 2026 using the React DOCX 0.14.0 release. These are retained measurements of that integration, not a fresh benchmark of the current standalone chart packages. [Full comparison and measurement method](docs/chart-comparisons.md).

</details>

<details open>
<summary><strong>Office Art comparisons — SmartArt, WordArt, 3D models, emoji, icons, pictures and shapes</strong></summary>

[![Microsoft Word versus Rust Office Art: SmartArt, WordArt, 3D emoji models, icons, ink and 3D shapes](docs/readme-assets/word-officeart-expanded-overview.png)](docs/office-art-comparisons.md)

The expanded **30-case gallery**, captured November 6, 2026, compares native Word with the current Rust Office Art renderer through the DOCX adapter. It includes SmartArt diagrams, WordArt fills and outlines, warped and 3D text, five 3D models including emoji, SVG icons, shapes, ink, and an artistic-effect cache control. Both sides use the same page coordinates, crop and display scale. Visible differences are retained.

| Gallery | Cases |
| --- | ---: |
| [3D models, emoji and SmartArt](docs/readme-assets/word-officeart-expanded-models-and-smartart.png) | 7 |
| [WordArt, warped text, extrusion, bevels and materials](docs/readme-assets/word-officeart-expanded-wordart-and-3d.png) | 12 |
| [SVG icons, shapes, ink and cached artistic effects](docs/readme-assets/word-officeart-expanded-icons-shapes-and-ink.png) | 11 |

**3D models use their saved Office previews.** This preserves the saved view without regenerating meshes or lighting. SmartArt uses its saved drawing tree. WordArt uses shaped font outlines, preset warp geometry and measured text fitting. 3D shapes and text use projected extrusion, bevels and material lighting; ink uses its saved brush and pressure data. Font substitution, curved-text fitting, pencil texture and native shading can still differ. The artistic-effect control checks cache preservation, not regeneration. Scores also retain the native print-PDF page offset. [Comparison method and remaining gaps](docs/office-art-comparisons.md) · [All 30 measurements](docs/readme-assets/word-officeart-expanded-comparison.csv) · [Capture provenance](docs/readme-assets/word-officeart-expanded-provenance.json).

### Pictures, effects, groups, shapes and connectors

The shared `rust-office-art` renderer supports pictures, 187 preset geometries, custom paths and connectors; SmartArt adapters reuse its paint for saved drawing trees. [Shared architecture and current boundaries](docs/office-art-architecture.md). These galleries compare Word for Mac 16.106.1 with React DOCX using the **workspace DOCX/WASM 0.2.9 candidate**, captured November 5, 2026. They measure the complete parser-to-viewer path at fixed page coordinates. The capture provenance identifies the actual WASM and viewer sources; these measurements do not establish registry availability or complete OfficeArt compatibility.

[![Microsoft Word and React DOCX picture appearance side by side](docs/readme-assets/word-officeart-overview.png)](docs/readme-assets/word-officeart-overview.png)

| Matrix | Cases at 96% color agreement | Median | All-case mean |
| --- | ---: | ---: | ---: |
| [Pictures and effects](docs/readme-assets/word-picture-comparison.csv) | 37/40 | 100.00% | 94.11% |
| [Group transforms](docs/readme-assets/word-group-comparison.csv) | 4/5 | 98.55% | 97.67% |
| [Shapes and connectors](docs/readme-assets/word-shape-comparison.csv) | 10/16 | 97.51% | 93.78% |

[![Microsoft Word and React DOCX shapes and connectors side by side](docs/readme-assets/word-officeart-shapes.png)](docs/readme-assets/word-officeart-shapes.png)

The reference is Word's PDF rasterized with Cairo at 96 dpi. RGB values must agree within 16 levels over the union of visible ink; white margins do not inflate the score. The unchanged bitmap control is exact. All 61 cases remain in the measurements, including the blank native blur, missing SVG alternative, soft-edge and thin-stroke differences. The original Splash references and their resampling differences remain in the provenance. **The 96–97% target is not met across all cases.** [Coverage and method](docs/picture-coverage.md) · [Picture provenance](docs/readme-assets/word-picture-provenance.json) · [Group provenance](docs/readme-assets/word-group-provenance.json) · [Shape provenance](docs/readme-assets/word-shape-provenance.json).

A broader check of **123 drawings in two complete documents** reaches **50.00% mean color agreement**, with **0/123 at 96%**, using the same fixed native page coordinates. Table flow, pattern sampling and unsupported effects remain visible in this harder document-level comparison. Another **206 drawings lack successful native references** and remain unmeasured. These results are separate from the 61 controlled cases above. [All 123 measurements](docs/readme-assets/word-art-document-comparison.csv) · [Document comparison provenance](docs/readme-assets/word-art-document-provenance.json).

The [SmartArt coverage report](docs/smartart-coverage.md) separately inventories 176 native-authored saved layout types and cached text coverage. Saved drawing reconstruction and semantic parsing do not establish pixel parity or complete data-to-layout generation.

Download self-contained Word documents for manual testing: [pictures, crops and effects](docs/readme-assets/office-art-pictures.docx), [effect ordering](docs/readme-assets/office-art-effect-order.docx), [group transforms](docs/readme-assets/office-art-groups.docx), [shapes and connectors](docs/readme-assets/office-art-shapes-connectors.docx), [tile density controls](docs/readme-assets/office-art-tile-dpi.docx), and [saved SmartArt probes](docs/readme-assets/office-art-smartart-probes.zip). The [589-case manual atlas](docs/readme-assets/office-art-manual-atlas.zip) adds seven editable packs covering WordArt, every shape preset, fills, outlines, effects, 3D, artistic extensions and legacy groups; its new visual cases are explicitly unmeasured. [Fixture and comparison tools](scripts/image-fidelity/README.md) reproduce the inputs and reports.

</details>

<details>
<summary><strong>rust-office-charts-wasm — chart WebAssembly bindings</strong></summary>

The default build includes two-dimensional bar, line, area, and scatter charts. Enable `extended-charts` for the other chart families supported by the Rust renderer. See the [crate guide](crates/rust-office-charts-wasm/README.md). The [Word comparison](docs/chart-comparisons.md) measures the React DOCX integration; it does not measure standalone WASM loading or throughput.

</details>

<details>
<summary><strong>@extend-ai/rust-office-charts — browser package</strong></summary>

In the browser package, import `loadOfficeCharts` from `@extend-ai/rust-office-charts` for two-dimensional bar, line, area and scatter charts, or from `@extend-ai/rust-office-charts/full` to also load the Rust pie, bubble, radar, stock, 3D, surface and ChartEx renderers. Both expose `(await loadOfficeCharts()).renderChart(chartXml, options)`. Unsupported layouts can return `null`; support for a chart family alone does not establish Excel fidelity.

The retained [Word gallery](docs/chart-comparisons.md) records the React DOCX 0.14.0 integration. Browser package versions and entry points have their own release history.

</details>

<details>
<summary><strong>rust-docx — parser latency, accuracy, and feature recall</strong></summary>

The native parser builds an editable document model and structured blocks with source references. Timings cover native parsing and extraction; browser loading, layout, pagination, and editing are outside their scope.

### DOCX latency and text accuracy

Measured **November 2, 2026**, on a seeded **1,000-document DOCX corpus**. Latency uses the **999 inputs all four parsers completed**; accuracy includes every reference-bearing input and counts parser failures as zero. Comparison versions: docx-rs **0.4.22**, office_oxide **0.1.15**, undoc **0.27.1**.

| Parser | Parsed | Median | p95 | Body-text F1 | Ordered-text F1 |
| --- | ---: | ---: | ---: | ---: | ---: |
| rust-docx | 1,000/1,000 | 0.673 ms | 5.080 ms | 99.983% | 99.995% |
| docx-rs | 999/1,000 | 1.193 ms | 9.499 ms | 99.740% | 99.747% |
| office_oxide | 1,000/1,000 | 0.750 ms | 5.380 ms | 99.917% | 99.926% |
| undoc | 1,000/1,000 | 1.130 ms | 14.260 ms | 99.988% | 99.975% |

![Native DOCX median and p95 latency for all four libraries](docs/readme-assets/docx-core-optimized-20261102-latency-docx-summary.png)

Rust-docx has the lowest median and p95 in this run. Its median is 10.3% lower than office_oxide's; this does **not** establish a 5× speedup. Undoc has the highest five-gram text F1, while rust-docx has the highest ordered-text F1.

Each document has five warm samples, each averaging 1–20 parses. Timings include parsing and native model/IR construction; rust-docx also projects structured blocks. File reads, process startup, JSON serialization, object destruction, and reference generation are excluded. The p95 is across document medians. Results come from a shared development machine and the libraries retain different amounts of structure.

### Latency by document size and complexity

The same 999 successful inputs appear for every library. Each point is one document's median latency. Complexity counts come from the source reference and are shared across parsers. Logarithmic axes retain zero complexity counts.

![DOCX latency scatter plots by input size, source paragraph count, and source feature count](docs/readme-assets/docx-core-optimized-20261102-latency-docx-scatter.png)

### Feature recall

These are **recall checks against an independent source-XML extractor**, not universal accuracy or Word rendering scores. Image occurrences match retained payload bytes and multiplicity. Literal table/span checks allow additional output characters; exact-anchor checks remain separately available. Positive explicit-formatting recall does not establish precision or inherited-style correctness.

| Check | Expected units | rust-docx | docx-rs | office_oxide | undoc |
| --- | ---: | ---: | ---: | ---: | ---: |
| Images · payload-matched occurrences | 1,069 | 100.000% | 97.381% | 95.603% | 97.568% |
| Explicit bold · aligned characters | 680,435 | 100.000% | 94.577% | 99.948% | 98.016% |
| Tables · literal text retained | 2,267 | 100.000% | 96.427% | 99.868% | 97.706% |
| Horizontal spans · literal text + exact span | 3,706 | 99.946% | 98.354% | 100.000% | 99.217% |
| Headings · text + level | 6,741 | 99.985% | 53.004% | 99.466% | 86.219% |
| Lists · paragraph membership | 15,361 | 99.980% | 98.568% | 91.758% | 93.184% |
| Vertical merges · start markers | 1,398 | 99.571% | 99.928% | 98.069% | 94.134% |
| Textboxes · exact text anchors | 2,652 | 99.774% | 35.633% | 99.623% | 100.000% |

### Microsoft Word reference sample

Measured **November 3, 2026**, on **25 documents** selected from the start of the fixed manifest without score filtering. All four parsers completed all inputs. Text uses Microsoft Word's object model; image payload checks use the original source bytes. This small sample supplements the larger XML-reference benchmark.

| Parser | Median / p95 | Word body recall | Word body F1 | Source image payloads |
| --- | ---: | ---: | ---: | ---: |
| rust-docx | 0.530 / 6.880 ms | 99.949% | 99.952% | 52/52 · 100.000% |
| docx-rs | 1.089 / 6.606 ms | 99.939% | 99.947% | 48/52 · 92.308% |
| office_oxide | 0.680 / 3.668 ms | 99.937% | 99.946% | 48/52 · 92.308% |
| undoc | 1.084 / 11.990 ms | 99.949% | 96.999% | 52/52 · 100.000% |

Rust-docx has the lowest median in this sample; office_oxide has the lowest p95. Body F1 here pools ordered character counts; it is a different metric and cohort from the macro-averaged source-XML scores above.

These are retained completed runs; source revisions and executable hashes identify the measured builds. See the [complete benchmark report](docs/docx-benchmarks.md) for every feature check, other Word formats, per-format charts, timing methodology, and reference limitations. [Benchmark tooling](scripts/parser-benchmark/README.md) · [Source-reference data](crates/rust-docx/benchmarks/optimized-20261102/accuracy.json) · [Word-reference data](crates/rust-docx/benchmarks/word-reference-20261103/summary.json).

</details>

<details>
<summary><strong>rust-docx-wasm — Word parser WebAssembly bindings</strong></summary>

Exposes document parsing, model construction, structured extraction, and serialization to JavaScript. [React DOCX](https://github.com/extend-hq/react-docx) packages these bindings for browsers. See the [crate guide](crates/rust-docx-wasm/README.md) and [WASM benchmark workflow](scripts/parser-benchmark/README.md#webassembly-comparisons). The native parser timings above do not measure browser or JavaScript bridge latency.

</details>

<details>
<summary><strong>rust-pptx — PowerPoint parsing</strong></summary>

Parses OOXML and legacy PowerPoint presentations into slides, shapes, text, charts, and embedded assets. Install with `cargo add rust-pptx`; the Rust library target is `rust_pptx`. See the [crate guide](crates/rust-pptx/README.md). No PowerPoint parser benchmark is published here yet.

</details>

<details>
<summary><strong>rust-pptx-wasm — PowerPoint WebAssembly bindings</strong></summary>

Exposes `detect_presentation_format` and `parse_presentation` for `Uint8Array` inputs. See the [crate guide](crates/rust-pptx-wasm/README.md) for build and initialization instructions. No separate WASM benchmark is published here yet.

</details>

## Development and publishing

The Rust workspace builds with Cargo. Run `cargo test --workspace --locked` for native validation. For the browser bindings, install the `wasm32-unknown-unknown` Rust target.

See [Publishing Rust crates](RELEASING.md) for versioning, CI checks and automatic publication from `main`.

The libraries are distributed under the [MIT license](LICENSE). Third-party notices accompany the crates that use them. [THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt) covers the chart browser package's Cargo dependency graph, including its full build; it is not an inventory for every workspace crate. Bundled font and map data notices are in [FONT-DATA-LICENSE](FONT-DATA-LICENSE) and [MAP-DATA-LICENSE](MAP-DATA-LICENSE).

For React viewers and editors, see [React DOCX](https://github.com/extend-hq/react-docx), [React PPTX](https://github.com/extend-hq/react-pptx), and [React XLSX](https://github.com/extend-hq/react-xlsx).

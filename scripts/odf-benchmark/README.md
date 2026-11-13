# OpenDocument benchmark

Compares this repository's OpenDocument import (`rust-docx` for text documents, `rust-pptx` for presentations and drawings, both through `rust-odf`) with every other Rust crate found that reads these formats, and with general-purpose parsers. Results and method are written up in [docs/opendocument-benchmarks.md](../../docs/opendocument-benchmarks.md).

Everything the harness produces lives in a work directory outside version control, for example `output/odf-benchmark-<date>/`:

```text
<work>/corpus/<family>/<file>     ODT, ODP and ODG folders, files at the root
<work>/refs/<family>/<file>.txt   LibreOffice reference text
<work>/target/                    built Rust runners (CARGO_TARGET_DIR)
<work>/tools/                     Apache Tika, pandoc, officeparser, odfpy, odfdo
<work>/results/<parser>/          records.jsonl, meta.json, texts/
<work>/scores.json
```

## Steps

1. **Corpus.** Save GitHub contents-API listings of the source directories as JSON, then `python3 corpus.py <listings dir> > plan.tsv` and download each row. The sources are the LibreOffice import and export test data (`sw/qa/extras/odfimport/data`, `sw/qa/extras/odfexport/data`, `sd/qa/unit/data/odp`, `sd/qa/unit/data/odg`), Apache Tika's `test-documents` for the miscoffice module, and the ODF Toolkit's `odfdom/src/test/resources/test-input`.
2. **References.** `python3 references.py <work>` needs LibreOffice (`soffice`) and `pdftotext`.
3. **Runners.** `./build_peers.sh <work>/target` builds one small binary per Rust parser from `peers/`. Building all of them needs about 5 GB; once built, everything in `<work>/target/release` except the `peer-*` binaries can be deleted. Each is its own package so the crates' dependency trees never have to agree. All use the same release profile.
4. **General tools.** Put `tika-app-3.3.3.jar` in `<work>/tools/tika/`, pandoc 3.14 in `<work>/tools/pandoc/`, `npm install officeparser` in `<work>/tools/node/`, and a virtual environment with `odfpy` and `odfdo` in `<work>/tools/py/.venv`.
5. **Run.** `python3 run.py <work> [parser ...]`. Copy a parser's `records.jsonl` to `records-pass1.jsonl` before running it again; the scorer takes the best time over all passes.
6. **Score.** `python3 score.py <work>`.

`peers/rust-office` also builds two diagnostics: `phases <list file>` splits this repository's time into reading with `rust-odf`, building the document or presentation model and extracting blocks, and `odfphases <list file>` splits `rust-odf`'s read into inflating, XML parsing and model building.

## What is measured

- **Support.** Each parser is run only on the extensions it handles by design (`SUPPORT` in `run.py`). A file a parser is designed for and cannot read counts as zero recall.
- **Accuracy.** Precision, recall and F1 from the longest common subsequence of normalized characters between a parser's text and the reference, pooled over files and also averaged per file. Normalization is NFC and keeps letters and digits only, so Markdown syntax, table rules, bullets and white space neither help nor hurt. References shorter than 20 characters are not scored. Documents with tracked changes are not scored: LibreOffice's export shows deleted text, so they have no single correct reading.
- **This repository's output.** `rust-docx` blocks carry their story, so the runner emits main-story blocks only (no header, footer, footnote, endnote or comment stories, and no figures, formulas or text boxes), with list and heading numbers, which is the scope of LibreOffice's text export. Labels are read from the typed block details, so the timing does not include converting details to JSON. For presentations it emits the text of every slide object in model order, without speaker notes.
- **Latency.** For in-process parsers, the fastest warm repeat of one call that turns the file's bytes into text (at least 3 repeats, up to 250 ms per file, over every pass recorded for the parser), reported as a mean and median over the files every compared parser read. The fastest repeat is used because the measurements were taken on a shared machine. The Rust runners also record process CPU time per call (`cpu_ms`), which other programs cannot inflate; the scorer prints it next to the elapsed figures. Reading the file, starting the process and writing output are outside the timed region, except for the two crates that only accept a path. `pandoc` and `textutil` are timed as commands, which includes starting the process.

## Limits

LibreOffice is an application reference, not ground truth: its text export leaves out headers and footers, and its PDF export of slides includes master-page text and omits speaker notes. A parser that outputs those stories loses precision here even when the text is correct. The corpus is dominated by small regression-test files, so pooled scores follow the larger documents and per-file means follow the many small ones; both are reported.

## Chart-data extraction cost

The `chart-data` binary in `peers/rust-office` compares the direct shared ODF
extraction API with serialization followed by the shared chart XML parser. Pass
a file containing one input path per line. It visits embedded charts, requires
identical `ChartData` values, and reports the fastest of 100 warm calls per chart
in microseconds. Document parsing and rendering are outside this measurement;
it is not an end-to-end document latency result.

```sh
cargo build --release --locked --manifest-path scripts/odf-benchmark/peers/rust-office/Cargo.toml --bin chart-data
scripts/odf-benchmark/peers/rust-office/target/release/chart-data INPUT_LIST
```

### Direct presentation chart extraction

The `chart-extraction` binary measures `parse_presentation_bundle` with `include_charts: true` and `include_artwork: false`, including JSON serialization of the complete bundle. Supply only supported presentation/drawing inputs. Its output files contain JSON, so compare them structurally; do not feed them to the text-fidelity scorer.

```sh
cargo build --release --manifest-path scripts/odf-benchmark/peers/rust-office/Cargo.toml --bin chart-extraction
scripts/odf-benchmark/peers/rust-office/target/release/chart-extraction PRESENTATION_LIST JSON_OUTPUT_DIRECTORY > records.jsonl
```

Freeze both executables, use the same input list, run AB/BA passes after builds finish, and preserve output equality alongside timings. Report the whole corpus and chart-bearing subset separately. Deferred generation retains native chart data and saved assets; it is not a ZIP/XML-only benchmark.

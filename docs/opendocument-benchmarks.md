# OpenDocument benchmarks

Latest follow-up: [OpenDocument parity and latency hill-climb](opendocument-hillclimb-20261113.md).

For the November 13 chart fixes, matched before/after timings and two freshly rerun peers, see [OpenDocument chart fidelity and latency](opendocument-fidelity-20261113.md). The broader comparison below remains the November 12 snapshot.

November 12, 2026. This compares the OpenDocument import in this repository (`rust-docx` for text documents, `rust-pptx` for presentations and drawings, both reading through `rust-odf`) with every other Rust crate found on crates.io that reads these formats, and with six general-purpose parsers. The harness is in [scripts/odf-benchmark](../scripts/odf-benchmark); its work directory, including the corpus, references and every parser's output, is `output/odf-benchmark-20261112`.

## Result

- **Accuracy:** this repository scores highest, or ties for highest, on every format in the corpus, on both pooled F1 and mean per-file F1: 0.9988 on `.odt`, 0.9997 on `.fodt` and 0.9361 on `.odp`.
- **Speed:** it is **not** the fastest Rust crate. On text documents it is faster than four of the twelve other Rust crates by mean (`docling`, `odox-core`, `anydoc`, `docx-to-md`) and seven by median; the five that are faster on both only extract text, and the fastest of them is 3 to 5 times faster. On presentations it is faster by mean than four of the seven other Rust crates (`odox-core`, `anydoc`, `docling`, `pptx-to-md`) and slower than three text extractors. It is faster than every general-purpose parser on every format, from 1.2 times (odfdo on presentations) to more than 300 times (Apache Tika).

The remaining gap follows from the amount of work. The faster crates read `content.xml` and emit its text. This import also reads `styles.xml`, resolves style inheritance, and builds the full document or presentation model that the viewers and the block extractor use. Where the time goes is broken down under [Where the time goes](#where-the-time-goes).

### Change since the first run

The first complete run was followed by a round of accuracy and speed work, and then by a rewrite of the text import that builds the document model directly. All three were measured with the same harness:

| | First run | Second round | Direct model |
| --- | ---: | ---: | ---: |
| `.odt` F1 | 0.9969 | 0.9988 | 0.9988 |
| `.fodt` F1 | 0.9976 | 0.9997 | 0.9997 |
| `.odp` F1 | 0.9089 | 0.9361 | 0.9361 |
| `.odt` mean / median ms | 0.968 / 0.187 | 0.810 / 0.166 | 0.380 / 0.085 |
| `.fodt` mean / median ms | — | 0.098 / 0.088 | 0.039 / 0.032 |
| `.odp` mean / median ms | 0.672 / 0.338 | 0.423 / 0.220 | 0.330 / 0.174 |

- **Second round, accuracy:** embedded charts, text documents and spreadsheets on slides are read; a body with more than one content element is read in full; list and heading numbers are part of the scored text, as they are in LibreOffice's export.
- **Second round, speed:** `rust-odf` builds its own XML tree with namespaces resolved to an enum, in place of a general-purpose tree; entries are inflated with `zlib-rs`; style lookups no longer allocate.
- **Direct model:** until this round an OpenDocument text file was written out as Word markup and parsed again by the DOCX reader. `rust-docx` now builds its document model straight from what `rust-odf` read, with no Word package in between, and moves text and image bytes into the model instead of copying them. In `rust-odf`, the XML tree is built in one scan with no tokenizer in front, styles are stored boxed and by family so that a document with thousands of automatic styles no longer shifts kilobytes per insertion, and one property group can be resolved without the other three. The scored text is unchanged: the new path produced the same letters and digits as the old one on all 756 readable files, and identical output on every presentation and drawing.

## Corpus

764 files: 508 text (426 `.odt`, 79 `.fodt`, 2 `.ott`, 1 `.odm`), 240 presentation (217 `.odp`, 22 `.fodp`, 1 `.otp`) and 16 drawing (10 `.odg`, 5 `.fodg`, 1 `.otg`).

- 710 are public test files: LibreOffice's import and export test data, Apache Tika's test documents and the ODF Toolkit's test input. Two crash-test samples were left out.
- 50 are LibreOffice conversions of Word and PowerPoint files from this repository's local corpora, and 4 are small feature samples.

Most public files are regression tests with little text. A reference needs at least 20 letters or digits to be scored, which leaves 220 `.odt`, 37 `.fodt` and 86 `.odp` files; the other formats have 6 or fewer scored files each and are shown for completeness, not as evidence.

## Method

- **Reference.** LibreOffice 26.2, headless. Text documents are exported as UTF-8 text. Presentations and drawings are exported to PDF and read back with `pdftotext`. 15 files have no reference because LibreOffice could not open or finish them.
- **Accuracy.** Precision, recall and F1 from the longest common subsequence of characters, as in the [Word reference benchmark](word-reference-benchmarks.md). Normalization is NFC and keeps letters and digits only, so Markdown syntax, table rules, bullets and white space do not count for or against a parser. Scores are pooled over files and also averaged per file. A file a parser is designed for and cannot read counts as zero recall.
- **Excluded.** 14 text documents with tracked changes are not scored. LibreOffice's export includes deleted text, so those files have no single correct reading.
- **Support.** Each parser runs only on the extensions it handles by design. No other Rust crate reads drawings, and only two read flat XML.
- **This repository's output.** Blocks carry their story, so the runner emits main-story blocks only, as the Word reference benchmark does: no header, footer, footnote, endnote or comment stories, and no figures, formulas or text boxes. List and heading numbers are included, because LibreOffice's export includes them. For presentations it emits the text of every slide object in model order, a chart's title, categories and series names, and no speaker notes. Other parsers' output is used as they return it.
- **Latency.** The fastest warm repeat of one in-process call that turns a file's bytes into text (at least 3 repeats, up to 250 ms per file, two or three passes for the Rust crates), as a mean and median over the files every compared parser read. `pandoc` and `textutil` are timed as commands and include starting the process. All Rust runners use the same release profile. The last pass also recorded process CPU time, which other programs on the machine cannot inflate: for every Rust crate, this repository included, it is within 3% of the elapsed figure. The other parsers' timings are from the first runs and were not repeated for the direct-model round; only this repository's runner was rebuilt and rerun, in two passes.

## Text documents

### .odt

426 files, 220 scored, 408 read by every parser.

| Parser | Kind | Read | Recall | Precision | F1 | Mean file F1 | Mean ms | Median ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| **rust-office** | this repository | 418/426 | 0.9987 | 0.9990 | 0.9988 | 0.9761 | 0.380 | 0.085 |
| odox-core | Rust | 418/426 | 0.9948 | 0.9953 | 0.9951 | 0.9580 | 0.606 | 0.181 |
| textutil | macOS, command | 426/426 | 0.9924 | 0.9962 | 0.9943 | 0.9606 | 53.4 | 50.9 |
| omniparse | Rust | 418/426 | 0.9951 | 0.9817 | 0.9883 | 0.9478 | 0.075 | 0.026 |
| litchi | Rust | 418/426 | 0.9896 | 0.9842 | 0.9869 | 0.9459 | 0.289 | 0.063 |
| quarb-text-odt | Rust | 418/426 | 0.9811 | 0.9867 | 0.9839 | 0.9501 | 0.224 | 0.060 |
| ebook-rs | Rust | 419/426 | 0.9951 | 0.9672 | 0.9809 | 0.9393 | 0.183 | 0.037 |
| transmutation | Rust | 421/426 | 0.9817 | 0.9760 | 0.9789 | 0.8958 | 0.115 | 0.052 |
| pandoc | Haskell, command | 417/426 | 0.9835 | 0.9666 | 0.9750 | 0.9447 | 41.6 | 34.4 |
| markkit | Rust | 417/426 | 0.9755 | 0.9725 | 0.9740 | 0.9479 | 0.339 | 0.116 |
| officeparser | Node | 418/426 | 0.9953 | 0.9482 | 0.9712 | 0.9367 | 4.958 | 1.126 |
| odfdo | Python | 417/426 | 0.9834 | 0.9401 | 0.9613 | 0.8948 | 1.534 | 0.268 |
| odfpy | Python | 414/426 | 0.9176 | 0.9838 | 0.9495 | 0.9458 | 10.1 | 3.067 |
| tika | Java | 421/426 | 0.9948 | 0.8993 | 0.9446 | 0.9058 | 129.4 | 85.8 |
| anydoc | Rust | 417/426 | 0.9992 | 0.8843 | 0.9382 | 0.9395 | 0.609 | 0.148 |
| kreuzberg | Rust | 421/426 | 0.8333 | 0.9949 | 0.9070 | 0.8959 | 0.253 | 0.089 |
| docx-to-md | Rust | 418/426 | 0.9954 | 0.8311 | 0.9059 | 0.7064 | 0.647 | 0.089 |
| docling | Rust | 419/426 | 0.9856 | 0.7000 | 0.8186 | 0.9170 | 0.588 | 0.132 |
| dotext | Rust | 424/426 | 0.5541 | 0.9887 | 0.7101 | 0.6256 | 0.330 | 0.200 |

`anydoc` has the highest recall, 0.9992 against 0.9987, because it also outputs footnote text, which the reference does not contain; that costs it precision. `docx-to-md`, `docling` and Apache Tika lose precision the same way. Most of what this import still misses is footnote reference numbers, which LibreOffice prints inline, and fields that follow the current date or file name.

### .fodt

79 files, 37 scored, 79 read by every parser.

| Parser | Kind | Read | Recall | Precision | F1 | Mean file F1 | Mean ms | Median ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| **rust-office** | this repository | 79/79 | 0.9997 | 0.9998 | 0.9997 | 0.9961 | 0.039 | 0.032 |
| docling | Rust | 79/79 | 0.9983 | 0.9920 | 0.9952 | 0.9712 | 0.090 | 0.058 |
| quarb-text-odt | Rust | 79/79 | 0.9952 | 0.9936 | 0.9944 | 0.9583 | 0.020 | 0.012 |
| tika | Java | 79/79 | 0.9981 | 0.9885 | 0.9933 | 0.9615 | 8.675 | 0.275 |

## Presentations

### .odp

217 files, 86 scored, 209 read by every parser.

| Parser | Kind | Read | Recall | Precision | F1 | Mean file F1 | Mean ms | Median ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| **rust-office** | this repository | 217/217 | 0.9255 | 0.9469 | 0.9361 | 0.8907 | 0.330 | 0.174 |
| odox-core | Rust | 217/217 | 0.8104 | 0.9696 | 0.8829 | 0.8279 | 1.037 | 0.549 |
| odfpy | Python | 213/217 | 0.8116 | 0.9422 | 0.8720 | 0.7785 | 15.8 | 8.846 |
| docling | Rust | 217/217 | 0.8254 | 0.8982 | 0.8603 | 0.7236 | 0.545 | 0.281 |
| anydoc | Rust | 217/217 | 0.8102 | 0.9077 | 0.8562 | 0.8036 | 0.772 | 0.402 |
| litchi | Rust | 217/217 | 0.8163 | 0.8961 | 0.8543 | 0.8044 | 0.079 | 0.059 |
| dotext | Rust | 217/217 | 0.7711 | 0.9454 | 0.8494 | 0.7673 | 0.236 | 0.204 |
| omniparse | Rust | 213/217 | 0.8197 | 0.8619 | 0.8402 | 0.7423 | 0.044 | 0.033 |
| officeparser | Node | 217/217 | 0.8273 | 0.8180 | 0.8226 | 0.7747 | 5.595 | 2.904 |
| pptx-to-md | Rust | 215/217 | 0.7312 | 0.5552 | 0.6311 | 0.4459 | 0.450 | 0.278 |
| tika | Java | 217/217 | 0.8867 | 0.4512 | 0.5980 | 0.5709 | 190.9 | 99.3 |
| odfdo | Python | 215/217 | 0.5002 | 0.6658 | 0.5712 | 0.4806 | 0.388 | 0.278 |

This import's recall lead comes from three things other parsers do not do together: filling in the master page's footer and slide-number fields on slides that display them, reading embedded text documents and spreadsheets, and reading a chart's title, categories and series names. Apache Tika also descends into embedded documents, but repeats master-page text, which is why its precision is 0.45.

What is still missing is mostly not recoverable from the file or is a matter of order: axis tick labels a chart computes when drawn, text inside binary OLE previews, dates and file names that follow the current day and path, and slide objects whose drawing order differs from the order `pdftotext` reads them in.

### .fodp

22 files, 6 scored, 22 read by every parser.

| Parser | Kind | Read | Recall | Precision | F1 | Mean file F1 | Mean ms | Median ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| tika | Java | 22/22 | 0.9633 | 0.9513 | 0.9573 | 0.9112 | 4.043 | 0.225 |
| **rust-office** | this repository | 22/22 | 0.9633 | 0.9513 | 0.9573 | 0.9112 | 0.064 | 0.008 |
| docling | Rust | 22/22 | 0.9553 | 0.9478 | 0.9515 | 0.7680 | 0.155 | 0.025 |

## Drawings and templates

`.odg`, 10 files, 5 scored. No other Rust crate reads drawings.

| Parser | Kind | Read | Recall | Precision | F1 | Mean file F1 | Mean ms | Median ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| **rust-office** | this repository | 10/10 | 0.9134 | 0.9090 | 0.9112 | 0.8163 | — | — |
| odfpy | Python | 10/10 | 0.8855 | 0.9385 | 0.9112 | 0.7978 | — | — |
| tika | Java | 10/10 | 0.9134 | 0.8844 | 0.8987 | 0.7497 | — | — |
| odfdo | Python | 0/10 | 0.0000 | 0.0000 | 0.0000 | 0.0000 | — | — |

`.ott`, 2 files, both scored:

| Parser | Kind | Read | Recall | Precision | F1 | Mean file F1 | Mean ms | Median ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| **rust-office** | this repository | 2/2 | 0.9837 | 0.9837 | 0.9837 | 0.9837 | 0.324 | 0.324 |
| markkit | Rust | 2/2 | 0.9837 | 0.9837 | 0.9837 | 0.9837 | 0.292 | 0.292 |
| anydoc | Rust | 2/2 | 0.9837 | 0.8115 | 0.8894 | 0.8950 | 0.429 | 0.429 |
| litchi | Rust | 2/2 | 0.9860 | 0.6878 | 0.8103 | 0.8103 | 0.186 | 0.186 |
| tika | Java | 2/2 | 0.9860 | 0.6779 | 0.8034 | 0.8032 | 310.1 | 310.1 |
| docling | Rust | 2/2 | 0.9837 | 0.5553 | 0.7098 | 0.7267 | 0.331 | 0.331 |
| docx-to-md | Rust | 2/2 | 0.9860 | 0.4695 | 0.6361 | 0.6363 | 0.267 | 0.267 |
| odfpy | Python | 1/2 | 0.5105 | 0.6909 | 0.5871 | 0.4063 | 8.362 | 8.362 |
| odfdo | Python | 2/2 | 0.6434 | 0.4678 | 0.5417 | 0.5251 | 0.816 | 0.816 |

The five `.fodg` files have one scored reference, which this import and Apache Tika both match exactly. The single `.odm`, `.otp` and `.otg` files have no scorable text.

## Failures

This import read 756 of 764 files. The 8 it rejected are 4 encrypted files, 3 deliberately invalid packages (a picture stored as the `mimetype` entry, a package without content, and a package holding nothing but `mimetype`) and 1 file that is not a ZIP. Several parsers return output for some of these without reporting an error. LibreOffice produced a reference for only the last one, which it opened as plain text.

## Where the time goes

Sum of the fastest repeat per phase over the 418 `.odt` files this import reads:

| Phase | First run | Second round | Direct model | Share now |
| --- | ---: | ---: | ---: | ---: |
| `rust-odf`: inflate, parse XML, build styles and content | 176 ms | 113 ms | 84 ms | 51% |
| Write the Word package | 94 ms | 60 ms | — | |
| `rust-docx`: parse that package into the document model | 96 ms | 87 ms | — | |
| `rust-docx`: build the document model directly | — | — | 39 ms | 24% |
| `rust-docx`: extract blocks | 46 ms | 41 ms | 41 ms | 25% |
| Total | 412 ms | 301 ms | 164 ms | |

Building the model directly takes 39 ms where writing and re-parsing Word markup took 147 ms. That phase includes the identity hash of the file and the block identifiers. Block extraction is the code the DOCX path uses and was not changed. For the 217 `.odp` files the total went from 145 ms to 88 ms to 72 ms, of which `rust-odf` is 63 ms: a presentation's `styles.xml` is large and the slide adapter is thin.

Two large files account for 54% of the `.odt` total, and every parser is slow on them: a 350-page specification that is almost entirely tables, and a malformed file with 20,000 automatic styles and links nested ten deep. Without them this import's mean is 0.178 ms and its median 0.085 ms, against 0.272 / 0.132 for `docling`, 0.319 / 0.148 for `anydoc`, 0.366 / 0.180 for `odox-core`, 0.199 / 0.116 for `markkit` and 0.121 / 0.063 for `litchi`.

For an ordinary document the time is now mostly reading: building the XML tree is about a fifth, inflating a fifth, reading styles a sixth, and no other step takes more than a tenth. On the table-heavy file half the time is block extraction, which allocates dozens of small strings per table cell; that is shared with the DOCX path and is the largest remaining cost that is not reading. Matching the plain text extractors is not possible while also reading styles and building the model.

## Limits

- LibreOffice is an application reference, not ground truth, and the choice of main-story text as the scope favours parsers that can separate stories.
- Timings were taken on a shared machine with a load average between 10 and 60, which is why the fastest repeat is reported and why CPU time was recorded as a check. Differences of 10% between parsers are within the noise.
- `quarb-text-odt` was not in the CPU-time pass; its figures are from the first two passes.
- The general-purpose parsers were timed in one pass.
- Text order matters to this metric. Slides and drawings have no single reading order, so part of every parser's loss there is ordering, not missing text.
- The corpus is public test data plus conversions. It does not represent production documents.

## Parsers

| Parser | Version | Reads |
| --- | --- | --- |
| anydoc | 0.2.4 | odt, ott, odp |
| docling (Rust port) | 2.82.1 | odt, ott, fodt, odp, fodp |
| docx-to-md | 0.2.0 | odt, ott |
| dotext | 0.1.1 | odt, odp |
| ebook-rs | 0.18.0 | odt |
| kreuzberg | 4.10.4 | odt |
| litchi | 0.0.1 | odt, ott, odp |
| markkit | 0.3.1 | odt, ott |
| odox-core | 0.16.0 | odt, odp |
| omniparse | 0.4.1 | odt, odp |
| pptx-to-md | 2.0.0 | odp |
| quarb-text-odt | 0.61.0 | odt, fodt |
| transmutation | 0.3.5 | odt |
| Apache Tika | 3.3.3 | all |
| officeparser | 8.4.1 | odt, odp |
| odfpy | 1.4.1 | odt, ott, odm, odp, otp, odg, otg |
| odfdo | 3.27.3 | odt, ott, odm, odp, otp, odg, otg |
| pandoc | 3.14 | odt |
| textutil | macOS built-in | odt |

Build notes: `kreuzberg` needs `zip` pinned to 8.6.0, `transmutation` needs `quick-xml`'s `serialize` feature, `dotext` needs `zip`'s `deflate` feature, `litchi` does not compile with only its `odf` feature, and `quarb-text-odt` needs Rust 1.99. `okbase-convert` wraps `anydoc` and was not run separately. `shiva`, `office_oxide` and `undoc` do not read OpenDocument text or presentations.

The [automatic-layout and direct-extraction follow-up](opendocument-layout-extraction-20261113.md) records the next frozen-candidate comparison and its limitations.

# OpenDocument input

`duke-pages` and `duke-slides` read OpenDocument files into the same models they build for Word and PowerPoint files. The bytes decide the format.

## How it is put together

```text
            duke-odf  (package, styles, text, tables, drawing objects)
            /      \
   duke-pages        duke-slides
   DocModel         PresentationDocument
```

- **`duke-odf`** opens the file (ZIP package or flat XML), resolves style inheritance and reads content into owned values, including the sub-documents of embedded charts, text documents and spreadsheets. It knows nothing about Word or PowerPoint. Its XML tree is its own: OpenDocument uses a fixed set of namespaces, so names are resolved to an enum while the tree is built.
- **`duke-pages`** builds its document model (`DocModel`) directly from what `duke-odf` read. OpenDocument and Word are separate paths that meet at the model: no Word package or Word markup is produced on the way, and an OpenDocument file is never converted to DOCX in order to be parsed. Structured blocks, bundles and review run on the model unchanged.
- **`duke-slides`** builds the same `PresentationDocument` the PowerPoint readers produce. Blocks, bundles, export and the viewer model run unchanged.

Because both target the existing models, a viewer or host that already consumes those models needs little new code. The exceptions for text documents are listed under [Text documents](#text-documents).

### Entry points for text documents

| Function | OpenDocument text |
| --- | --- |
| `duke_pages::parse_odt(bytes)` | The model, built directly. |
| `parse_document_model`, `parse_document_model_with_options` | The model, for every supported format: DOCX and `.doc` go through their Word package, OpenDocument does not. |
| `parse_document_blocks`, `parse_document_blocks_with_options`, `parse_document_blocks_with_revisions`, `parse_document_bundle` | Blocks, as for Word files. |
| WebAssembly `build_doc_model_from_bytes` | The model, with an empty package beside it. |
| `parse_document_bytes`, WebAssembly `parse_docx_wasm` and `inspect_document_source_wasm` | An error: these return a Word package and OpenDocument text has none. |

## Supported types

Detection reads the `mimetype` entry, or the `office:mimetype` attribute of a flat XML file. File names and caller-supplied media types are not consulted.

| Media type (`application/vnd.oasis.opendocument.` + ) | Extension | Parser |
| --- | --- | --- |
| `text` | `.odt` | `duke-pages` |
| `text-template` | `.ott` | `duke-pages` |
| `text-master` | `.odm` | `duke-pages` |
| `text-master-template` | `.otm` | `duke-pages` |
| `text-web` | `.oth` | `duke-pages` |
| `text-flat-xml` | `.fodt` | `duke-pages` |
| `presentation` | `.odp` | `duke-slides`, format `odp` |
| `presentation-template` | `.otp` | `duke-slides`, format `odp` |
| `presentation-flat-xml` | `.fodp` | `duke-slides`, format `odp` |
| `graphics` | `.odg` | `duke-slides`, format `odg` |
| `graphics-template` | `.otg` | `duke-slides`, format `odg` |
| `graphics-flat-xml` | `.fodg` | `duke-slides`, format `odg` |

`duke-odf` also recognises spreadsheets (`.ods`, `.ots`, `.fods`) and reads their tables with repeat counts intact, and it reads chart documents (`.odc`, `.otc`) into a title, categories and series. No parser in this repository accepts either as a standalone file. Formula, image and database documents are identified by `sniff` and have no body model yet.

Every `duke-pages` entry point returns an error naming the type when given an OpenDocument file that is not a text document. `duke_slides::detect_format` returns `UnsupportedFormat` for an OpenDocument file that is neither a presentation nor a drawing. Office Open XML and other ZIP formats are never mistaken for OpenDocument: a package whose first entry is `[Content_Types].xml` is rejected without reading the central directory.

Encrypted packages, ZIP64 packages and the pre-OpenDocument OpenOffice.org 1.x formats (`.sxw`, `.sxi`) are rejected.

Only the content has to be well-formed, and a body may hold more than one content element, all of which are read. A `styles.xml` or `meta.xml` that is not well-formed is skipped with a warning, a document type declaration that only names an external DTD is ignored, and an element from outside OpenDocument is read as if only its content were there, as the standard asks.

## Text documents

Imported: paragraphs and headings with named and automatic styles, character formatting, hyperlinks, tabs and line breaks, nested bullet and numbered lists (continuation and explicit start values included), outline numbering of headings, tables with merged cells, header rows, borders and shading, inline and anchored images, text frames, footnotes and endnotes, comments with the text they are anchored to, page size and margins, headers and footers (first-page and even-page variants included), page-style changes as sections, page number and page count fields, formulas with their MathML.

How it appears in the model:

- **Formatting is resolved.** Every paragraph and run carries its final formatting, worked out through the automatic style, its named ancestors and the document defaults. Named paragraph styles are also listed as style definitions with the same inheritance. `Standard` is the default style `Normal`; `Heading 1` to `Heading 9`, and styles with an outline level, are headings, so heading classification in structured blocks works as it does for DOCX.
- **Numbered labels** retain supported custom templates, legal numbering and decimal padding through optional `minimumDigits` metadata. New top-level lists start from their style's starting value; explicit continuations retain their numbering. Aggregate table text includes the existing paragraph labels.
- **Note citations** retain their stored display label in optional `noteReference.label` metadata. Structured text includes the label with UTF-16 inline offsets. The underlying run text remains empty; viewers that generate their own note numbers need to consume this metadata to display custom labels. Empty source citations stay empty.
- **No Word markup.** `source_xml` is absent on every paragraph, table and image. Two things the Word path reads from that markup are stated as data instead: a field's instruction is on its run (`fieldInstruction`), and a comment's anchor is a list of child ranges on the paragraph (`commentRanges`).
- **Images** carry their bytes, content type and the package path they came from. A text frame is an image node whose content is a story of its own (`contentNodes`); no picture of the frame is generated.
- **Sources are named after OpenDocument parts.** Blocks of the body, notes and comments report `content.xml`; headers and footers report `styles.xml/header1`, `styles.xml/footer1` and so on. The document identity is the SHA-256 of the file.
- **Page setup** is the one part of the model that is stored as Word's section properties (`sectionPropertiesXml`), because that is the model's only form for it. The import fills that field with page size, margins and the first-page flag. Nothing in Rust parses it back.

Not imported, each reported in the model's `warnings`:

- Drawing shapes without text are skipped. Shapes with text are imported as text frames without their outline.
- Embedded objects other than formulas (charts, OLE) are imported as their saved preview image, or skipped when the file has none.
- Tracked changes are read as accepted text, and hidden text is left out.
- Bookmarks, image cropping, paragraph line-spacing given as extra leading, multi-column sections and table default cell styles are ignored.

A table row carrying a spreadsheet repeat count (`table:number-rows-repeated`) is shown once, as LibreOffice Writer shows it. A list start value above 32767 is treated as 1, which is also what LibreOffice displays. A page break after a paragraph is stored as a page break before the next one.

Consecutive numbering across list levels is preserved in optional `consecutiveNumbering` and `consecutiveListId` metadata. Structured extraction shares the counter across nested levels and explicit restarts, while independent lists retain separate counters. Bulleted ancestors no longer suppress a descendant's numeric label. A restart on an item containing only sublists does not restart a later visible paragraph.

Numbering style overrides, some continuations of ordinary hierarchical lists and levels beyond the common model's nine-level limit remain incomplete. Lossless round-trip export to OpenDocument is not implemented. Imported OpenDocument features are not guaranteed to round-trip through the generic Word serializer; image paint, custom citation labels and consecutive numbering need separate handling.

## Presentations and drawings

Imported: page size, slides with names and hidden state, slide and master backgrounds, text boxes and placeholders with formatting resolved through the presentation, paragraph and text style chain (per-level outline styles included), bullets and numbering, basic shapes, custom shapes that correspond to a DrawingML preset, paths and polygons, lines and connectors, rotation, groups, images, tables with merged cells, fills (solid, gradient, bitmap), outlines, speaker notes, document metadata.

Master-page decorations appear on every slide with a `master:` id prefix, as inherited nodes do for PowerPoint. Master placeholders and empty slide placeholders are prompts, not content, and are omitted. The master's footer, slide-number and date fields are the exception: a slide whose page style displays them gets them filled in, the footer and a fixed date from the presentation's declarations and the number from the slide's position, placed after the slide's own objects. A date that follows the current day has no stored text and is left out.

Text typed onto a picture becomes a text shape grouped with the picture in the same frame.

Text stored in WMF/EMF records, modern SVM/StarView records and SVG text elements is retained as optional `textContent` metadata on image and object nodes. Chart nodes carry visible labels recovered from their vector preview or the native chart renderer, including generated axis ticks, data labels and chart tables. Structured blocks and bundles expose this text as the object's content and retain its source in `details.textContent`. The image or chart remains the drawable object; no duplicate text shape is added. This is stored-text recovery, not OCR or full vector rendering.

Structured extraction of presentations separates text regions at clear horizontal or vertical gaps and reads complete rectangular grids by row. A bounded tolerance permits edge overlaps of up to half a point, capped at 1% of the object's dimension. Larger ambiguous overlaps and unsupported geometry retain source order. Groups retain their boundaries, and block IDs and source node paths remain stable. Drawing node arrays retain their original order. Drawings keep their existing extraction order; text within a single frame is not reflowed or split into columns.

Shapes carry typed geometry, fill and line, as legacy `.ppt` shapes do; `shapeSvg` is not generated. Not yet imported:

- Custom shapes whose outline needs formula evaluation are drawn as rectangles, with a `shape-geometry-approximated` warning.
- Embedded charts retain native data and use a saved preview or generated artwork for supported chart types. Unsupported artwork retains the chart data.
- An embedded text document or spreadsheet is represented by its preview image when a browser can draw it, with recovered preview text or native text retained in `textContent`. Otherwise its own content is used: the text as a text shape, the spreadsheet's used cells as a table (up to 200 rows and 50 columns). Other embedded objects become their preview image or an `unknown` node; a supported vector preview can supply text, but their binary document payload is not decoded.
- Raster text, arbitrary outlined glyphs, legacy `SVGD` streams and complete vector clipping/occlusion are not decoded. Vector spans use geometric reading order within each object; modern SVM joins explicitly drawn contiguous ranges of the same string within text-shape and paragraph boundaries. Chart legends and table rows can still interleave.
- List-label metadata is retained for rendering, but slide text extraction omits generated labels. Impress numbering differs from the text-document path, including some restart and continuation behavior.
- Audio and video become `unknown` nodes. Animations, transitions and slide comments are not read.
- Image cropping and hatch fills are ignored.

## Limits

`duke-odf::Limits` bounds input size, ZIP entry count, per-entry and total inflated size, XML nodes per part, content nesting depth and content element count. `duke-slides` maps its `ParseLimits` onto them. Table repeat counts are stored as counts by `duke-odf` and expanded by the adapters under their own caps, so a few bytes cannot declare millions of cells.

Vector text recovery additionally limits each input to 32 MiB, recovered text to 1 MiB, records/nodes to 250,000 and nesting to 128 SVG levels, 32 nested SVM drawings or 256 saved drawing states. Recovered object text counts against structured-extraction text budgets. Presentation reading-order inference stops at 64 levels or 4096 siblings and retains source order beyond those limits.

## Validation

November 12, 2026, LibreOffice 26.2 as the converter. These are import-parity checks against the same content parsed from its Office Open XML form, not rendering comparisons.

- 20 Word documents converted to `.odt`: all parsed. Text of paragraph and heading blocks matched the DOCX parse at 98.5% to 100% similarity. Table, cell and list-label counts matched except in one document where the conversion itself merged identical headers and footers. Figures were missing in 7 documents whose figure is a drawing shape or a chart saved without a preview image.
- 30 PowerPoint decks converted to `.odp`: all parsed. Slide counts matched; text similarity was 98.1% to 100% (mean 99.9%). Shape, group, image and table node counts matched; 8 charts and 1 media object became `unknown` nodes.
- A `.fodt` and a `.fodp` produced the same blocks as their packaged forms.
- The text checks above were made on the first version of the text import, which went through a Word package. The direct-model import that replaced it produced the same letters and digits on all 500 readable text files of the benchmark corpus, which include these conversions; the only differences were line breaks at page breaks and after tables that end a cell.
- 350 public test files from the LibreOffice, Apache Tika and ODF Toolkit repositories (173 text, 95 presentation, 16 drawing, 62 spreadsheet, 4 chart and formula): 346 were read by `duke-odf`, and every text, presentation and drawing file among them also converted through its adapter. The other 4 are three encrypted files and one deliberately invalid ZIP, all rejected with the expected error. This is a no-failure check; the content of these files was not compared against a reference.

A comparison with other Rust crates and general-purpose parsers on 764 files, with accuracy and timings, is in [OpenDocument benchmarks](opendocument-benchmarks.md).

The [November 16 presentation text investigation](opendocument-text-accuracy-20261116.md) separates reading-order and reference-export differences from parser defects, fixes slide-scoped fields and foreign-wrapper text loss, and records a fresh baseline/candidate benchmark.

The [November 16 chart and vector text follow-up](opendocument-visual-text-20261116.md) adds object text recovery and measures accuracy and extraction cost against that corrected baseline.

The [November 16 reading-order and SVM follow-up](opendocument-order-svm-20261116.md) adds presentation extraction order and modern StarView text recovery, with a fresh comparison against the object-text parser.

The [continuing F1 and latency log](opendocument-hillclimb-20261116.md) records subsequent measured improvements for text documents and presentations.

## Adopting it downstream

- **Viewers:** accept the new extensions and, for presentations, the `odp` and `odg` format values, then rebuild the WebAssembly bundle against the new crate versions. Presentations need no rendering changes. Text documents have an empty package beside the model, and text frames retain their `contentNodes`. Consume `noteReference.label` and `minimumDigits` to reproduce imported citation and numbered labels. OpenDocument round-trip saving is not supported.
- **Hosts:** route the media types above to the Word or presentation path, and call an entry point that returns a model or blocks, not `parse_document_bytes`. Block and bundle shapes are unchanged, so existing mappers apply. Highlighting that matches server block IDs against the viewer's own parse needs both sides on the same crate version.

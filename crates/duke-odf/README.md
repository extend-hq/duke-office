# duke-odf

Shared OpenDocument readers for the [Duke Office](https://github.com/extend-hq/duke-office) parsers.

`duke-odf` opens an OpenDocument file, either a ZIP package or the single-file flat XML form, and reads it into owned values:

- `sniff(bytes)` identifies the document type from its `mimetype` entry without unpacking the file. Other ZIP formats, including Office Open XML, return `None`.
- `OdfDocument::parse(bytes, &Limits::default())` returns metadata, the style sheet, the body, the images the content refers to and the sub-documents of embedded objects.
- `StyleSheet` resolves style inheritance, keeps automatic styles of `content.xml` and `styles.xml` apart, and exposes list styles, page layouts and master pages.
- The body is text blocks (paragraphs, lists, tables, sections), drawing pages (slides), spreadsheet tables or a chart (title, categories and series).
- `EmbeddedObject::document` reads the sub-document of an embedded chart, text document or spreadsheet the same way.

The crate does not produce a viewer or export model. `duke-pages` builds its document model from text documents and `duke-slides` builds its presentation model from presentations and drawings, each directly from these values, so both parsers and their viewers accept OpenDocument input.

Encrypted packages, ZIP64 packages and the pre-OpenDocument OpenOffice.org 1.x formats are rejected. Tracked changes are read as accepted text.

Licensed under the [MIT License](LICENSE).

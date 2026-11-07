# PowerPoint table grids and merged cells

`SlideNode::Table.rows` preserves the DrawingML source grid, including covered cells inside merged regions. The array indices are zero-based source row and column positions. `columnWidths` and `rowHeights` use EMUs. Grid positions do not advance by an origin cell's span; the following source cells still occupy their own grid slots.

An origin cell retains `colSpan` and `rowSpan` when greater than one. A covered cell retains its own text, formatting, links and source spans, and gains `merge`:

```json
{
  "merge": {
    "horizontal": true,
    "vertical": false,
    "origin": { "row": 0, "column": 0 }
  }
}
```

`horizontal` and `vertical` reflect the source merge flags. `origin`, when present, identifies the top-left origin in the same table. It is omitted for an unresolved or inconsistent continuation. Origin resolution traverses actual cells and retains only the preceding and current origin rows. Large declared spans do not allocate an expanded grid.

The React model retains all cells. The viewer, slide-text collector and visible-text search skip any cell with either merge flag set. Consumers that display or search presentations must apply the same rule. Retained covered text is source data, not additional visible content, and must not be appended to the origin's displayed text.

Table style selection uses the source grid column, fixing column bands and first/last-column selection after horizontal merges. This does not establish complete native table rendering parity: all built-in styles, border conflict rules, text layout and malformed-grid repair remain broader concerns.

## References and measurement scope

Microsoft's [table implementation notes](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/87e635df-37bf-42ef-87b6-fdd8abac88e1) describe the retained source grid and top-left merge dimensions. The [python-pptx merge analysis](https://python-pptx.readthedocs.io/en/latest/dev/analysis/tbl-merge.html) documents the distinction between visible origins and covered cells with hidden retained content.

The benchmark's independent literal-source reference already includes every table cell. Its cell/body-text extraction scores therefore include covered contents. The reference is unchanged by this implementation; higher extraction recall must not be described as higher visible-text or rendered fidelity. Browser probes separately verify that covered content stays hidden.

The source review compares all non-table-row model values and payloads exactly to the frozen preceding build. For changed tables, it independently checks source cell counts, spans, merge flags and origins. It also parses byte-preserving packages with only the merge attributes removed through the preceding parser. The resulting cells provide a metamorphic check of rich text and column styling; they are not an independent application-rendering oracle.

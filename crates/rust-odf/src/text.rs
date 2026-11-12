//! Text content shared by every OpenDocument type: paragraphs, lists and tables.

use std::collections::{BTreeMap, BTreeSet};


use crate::draw::{self, Shape};
use crate::xml::{attr, child, elements, is, owned, text_content, Node, Ns, DC, DRAW, OFFICE, PRESENTATION, TABLE, TEXT, XLINK, XML};
use crate::{Limits, OdfError};

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Paragraph(Paragraph),
    List(List),
    Table(Table),
    /// A `text:section` or the generated body of an index such as a table of contents.
    Section(Section),
    /// A drawing object anchored outside any paragraph.
    Shape(Box<Shape>),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Paragraph {
    /// `text:style-name`, a paragraph-family style.
    pub style: Option<String>,
    /// `text:outline-level` of a `text:h`, starting at 1.
    pub outline_level: Option<u8>,
    pub id: Option<String>,
    pub inlines: Vec<Inline>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    /// Text after OpenDocument white-space collapsing.
    Text(String),
    Span {
        /// A text-family style.
        style: Option<String>,
        children: Vec<Inline>,
    },
    Link {
        href: String,
        children: Vec<Inline>,
    },
    Tab,
    LineBreak,
    SoftPageBreak,
    Note(Note),
    Annotation(Annotation),
    /// Ends the range opened by the annotation with the same name.
    AnnotationEnd(String),
    Bookmark(String),
    /// A field with its saved result, such as `page-number` or `date`.
    Field {
        kind: String,
        text: String,
    },
    Shape(Box<Shape>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub endnote: bool,
    pub citation: String,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Annotation {
    pub name: Option<String>,
    pub author: Option<String>,
    pub date: Option<String>,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct List {
    /// A list style. Nested lists without one inherit their parent's.
    pub style: Option<String>,
    pub id: Option<String>,
    pub continue_numbering: bool,
    pub continue_list: Option<String>,
    pub items: Vec<ListItem>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListItem {
    /// A `text:list-header`: content without a label.
    pub header: bool,
    pub start_value: Option<u32>,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    pub name: Option<String>,
    pub style: Option<String>,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    pub name: Option<String>,
    pub style: Option<String>,
    pub columns: Vec<TableColumn>,
    pub rows: Vec<TableRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableColumn {
    pub style: Option<String>,
    pub default_cell_style: Option<String>,
    /// `table:number-columns-repeated`, not expanded.
    pub repeated: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableRow {
    pub style: Option<String>,
    pub header: bool,
    /// `table:number-rows-repeated`, not expanded.
    pub repeated: u32,
    pub cells: Vec<TableCell>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableCell {
    pub style: Option<String>,
    /// Hidden under a neighbouring cell's span.
    pub covered: bool,
    pub column_span: u32,
    pub row_span: u32,
    /// `table:number-columns-repeated`, not expanded.
    pub repeated: u32,
    /// `office:value-type`, such as `float`, `date` or `string`.
    pub value_type: Option<String>,
    /// The typed value in its lexical form, such as `office:value` or `office:date-value`.
    pub value: Option<String>,
    pub formula: Option<String>,
    pub blocks: Vec<Block>,
}

impl TableCell {
    /// No text and no value.
    pub fn is_empty(&self) -> bool {
        self.value.is_none() && plain_text(&self.blocks).trim().is_empty()
    }
}

impl Table {
    /// The table cut down to the cells in use. A spreadsheet declares far
    /// more rows and columns than it fills, mostly as repeat counts.
    pub fn trimmed(&self) -> Table {
        let mut rows = self.rows.clone();
        for row in &mut rows {
            while row.cells.last().is_some_and(TableCell::is_empty) {
                row.cells.pop();
            }
        }
        while rows.last().is_some_and(|row| row.cells.is_empty()) {
            rows.pop();
        }
        let width = rows
            .iter()
            .map(|row| row.cells.iter().map(|cell| cell.repeated as usize).sum::<usize>())
            .max()
            .unwrap_or(0);
        let mut columns = Vec::with_capacity(width.min(4096));
        'columns: for column in &self.columns {
            for _ in 0..column.repeated {
                if columns.len() >= width.min(4096) {
                    break 'columns;
                }
                columns.push(TableColumn {
                    repeated: 1,
                    ..column.clone()
                });
            }
        }
        Table {
            name: self.name.clone(),
            style: self.style.clone(),
            columns,
            rows,
        }
    }
}

/// State shared while reading content: limits, diagnostics and the binary
/// parts the content refers to.
pub(crate) struct Ctx<'l> {
    pub limits: &'l Limits,
    /// Package paths of images and media referenced by the content.
    pub hrefs: BTreeSet<String>,
    /// Package directories of embedded objects.
    pub objects: BTreeSet<String>,
    /// Images embedded as base64 in flat XML, keyed by a generated reference.
    pub inline: BTreeMap<String, Vec<u8>>,
    pub warnings: BTreeSet<String>,
    pub error: Option<OdfError>,
    depth: usize,
    nodes: usize,
}

impl<'l> Ctx<'l> {
    pub fn new(limits: &'l Limits) -> Self {
        Self {
            limits,
            hrefs: BTreeSet::new(),
            objects: BTreeSet::new(),
            inline: BTreeMap::new(),
            warnings: BTreeSet::new(),
            error: None,
            depth: 0,
            nodes: 0,
        }
    }

    pub fn warn(&mut self, message: &str) {
        self.warnings.insert(message.to_owned());
    }

    /// Enters one nesting level. `false` means a limit was hit and the caller must stop.
    pub fn enter(&mut self) -> bool {
        if self.error.is_some() {
            return false;
        }
        if self.depth >= self.limits.max_depth {
            self.error = Some(OdfError::Limit("content nesting depth".into()));
            return false;
        }
        self.depth += 1;
        true
    }

    pub fn leave(&mut self) {
        self.depth -= 1;
    }

    /// Counts one content node against the budget.
    pub fn count(&mut self) -> bool {
        self.nodes += 1;
        if self.nodes > self.limits.max_content_nodes && self.error.is_none() {
            self.error = Some(OdfError::Limit("content node count".into()));
        }
        self.error.is_none()
    }

    pub fn repeat(&self, node: Node<'_, '_>, name: &str) -> u32 {
        attr(node, TABLE, name)
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(1)
            .max(1)
    }
}

/// Reads the block content of a body, cell, text box, note, header or shape.
pub(crate) fn parse_blocks(parent: Node<'_, '_>, ctx: &mut Ctx<'_>) -> Vec<Block> {
    let mut blocks = Vec::new();
    if !ctx.enter() {
        return blocks;
    }
    for node in elements(parent) {
        if !ctx.count() {
            break;
        }
        let name = node.tag_name().name();
        match node.tag_name().namespace() {
            Some(TEXT) => match name {
                "p" | "h" => blocks.push(Block::Paragraph(parse_paragraph(node, ctx))),
                "list" => blocks.push(Block::List(parse_list(node, ctx))),
                "numbered-paragraph" => blocks.extend(parse_blocks(node, ctx)),
                "section" => blocks.push(Block::Section(Section {
                    name: owned(node, TEXT, "name"),
                    style: owned(node, TEXT, "style-name"),
                    blocks: parse_blocks(node, ctx),
                })),
                "table-of-content" | "illustration-index" | "table-index" | "object-index"
                | "user-index" | "alphabetical-index" | "bibliography" => {
                    if let Some(body) = child(node, TEXT, "index-body") {
                        blocks.push(Block::Section(Section {
                            name: owned(node, TEXT, "name"),
                            style: owned(node, TEXT, "style-name"),
                            blocks: parse_blocks(body, ctx),
                        }));
                    }
                }
                "index-title" => blocks.extend(parse_blocks(node, ctx)),
                "tracked-changes" => ctx.warn("Tracked changes are imported as accepted text"),
                _ => {}
            },
            Some(TABLE) if name == "table" => blocks.push(Block::Table(parse_table(node, ctx))),
            Some(DRAW) | Some(crate::xml::DR3D) => {
                if let Some(shape) = draw::parse_shape(node, ctx) {
                    blocks.push(Block::Shape(Box::new(shape)));
                }
            }
            Some(namespace) if is_foreign(namespace) => blocks.extend(parse_blocks(node, ctx)),
            _ => {}
        }
    }
    ctx.leave();
    blocks
}

/// An element from outside OpenDocument. The standard asks readers to treat
/// such an element as if only its content were there.
fn is_foreign(namespace: Ns) -> bool {
    namespace.is_foreign()
}

fn parse_paragraph(node: Node<'_, '_>, ctx: &mut Ctx<'_>) -> Paragraph {
    let mut inlines = Vec::new();
    let mut after_space = true;
    parse_inlines(node, ctx, &mut inlines, &mut after_space);
    Paragraph {
        style: owned(node, TEXT, "style-name"),
        outline_level: (node.tag_name().name() == "h").then(|| {
            attr(node, TEXT, "outline-level")
                .and_then(|value| value.parse::<u8>().ok())
                .unwrap_or(1)
                .clamp(1, 10)
        }),
        id: owned(node, XML, "id"),
        inlines,
    }
}

/// Collapses white space as OpenDocument defines it: runs become one space
/// and a paragraph's leading white space disappears.
fn push_text(text: &str, out: &mut Vec<Inline>, after_space: &mut bool) {
    let mut collapsed = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, ' ' | '\t' | '\r' | '\n') {
            if !*after_space {
                collapsed.push(' ');
                *after_space = true;
            }
        } else {
            collapsed.push(ch);
            *after_space = false;
        }
    }
    if collapsed.is_empty() {
        return;
    }
    if let Some(Inline::Text(previous)) = out.last_mut() {
        previous.push_str(&collapsed);
    } else {
        out.push(Inline::Text(collapsed));
    }
}

fn push_literal(text: &str, out: &mut Vec<Inline>) {
    if let Some(Inline::Text(previous)) = out.last_mut() {
        previous.push_str(text);
    } else {
        out.push(Inline::Text(text.to_owned()));
    }
}

fn parse_inlines(
    parent: Node<'_, '_>,
    ctx: &mut Ctx<'_>,
    out: &mut Vec<Inline>,
    after_space: &mut bool,
) {
    if !ctx.enter() {
        return;
    }
    for node in parent.children() {
        if node.is_text() {
            push_text(node.text().unwrap_or_default(), out, after_space);
            continue;
        }
        if !node.is_element() || !ctx.count() {
            continue;
        }
        let name = node.tag_name().name();
        match node.tag_name().namespace() {
            Some(TEXT) => match name {
                "span" => {
                    let mut children = Vec::new();
                    parse_inlines(node, ctx, &mut children, after_space);
                    if !children.is_empty() {
                        out.push(Inline::Span {
                            style: owned(node, TEXT, "style-name"),
                            children,
                        });
                    }
                }
                "a" => {
                    let mut children = Vec::new();
                    parse_inlines(node, ctx, &mut children, after_space);
                    out.push(Inline::Link {
                        href: attr(node, XLINK, "href").unwrap_or_default().to_owned(),
                        children,
                    });
                }
                "s" => {
                    let count = attr(node, TEXT, "c")
                        .and_then(|value| value.parse::<usize>().ok())
                        .unwrap_or(1)
                        .clamp(1, ctx.limits.max_repeat);
                    push_literal(&" ".repeat(count), out);
                    *after_space = false;
                }
                "tab" => {
                    out.push(Inline::Tab);
                    *after_space = false;
                }
                "line-break" => {
                    out.push(Inline::LineBreak);
                    *after_space = false;
                }
                "soft-page-break" => out.push(Inline::SoftPageBreak),
                "note" => {
                    out.push(Inline::Note(Note {
                        endnote: attr(node, TEXT, "note-class") == Some("endnote"),
                        citation: child(node, TEXT, "note-citation")
                            .map(text_content)
                            .unwrap_or_default(),
                        blocks: child(node, TEXT, "note-body")
                            .map(|body| parse_blocks(body, ctx))
                            .unwrap_or_default(),
                    }));
                    *after_space = false;
                }
                "bookmark" | "bookmark-start" => {
                    if let Some(name) = owned(node, TEXT, "name") {
                        out.push(Inline::Bookmark(name));
                    }
                }
                "ruby" => {
                    if let Some(base) = child(node, TEXT, "ruby-base") {
                        parse_inlines(base, ctx, out, after_space);
                    }
                }
                "meta" | "meta-field" => parse_inlines(node, ctx, out, after_space),
                "bookmark-end" | "reference-mark" | "reference-mark-start"
                | "reference-mark-end" | "change" | "change-start" | "change-end"
                | "toc-mark" | "toc-mark-start" | "toc-mark-end" | "user-index-mark"
                | "user-index-mark-start" | "user-index-mark-end" | "alphabetical-index-mark"
                | "alphabetical-index-mark-start" | "alphabetical-index-mark-end" => {}
                kind => {
                    let text = text_content(node);
                    *after_space = text.ends_with(' ');
                    out.push(Inline::Field {
                        kind: kind.to_owned(),
                        text,
                    });
                }
            },
            Some(OFFICE) => match name {
                "annotation" => out.push(Inline::Annotation(Annotation {
                    name: owned(node, OFFICE, "name"),
                    author: child(node, DC, "creator").map(text_content),
                    date: child(node, DC, "date").map(text_content),
                    blocks: parse_blocks(node, ctx),
                })),
                "annotation-end" => {
                    if let Some(name) = owned(node, OFFICE, "name") {
                        out.push(Inline::AnnotationEnd(name));
                    }
                }
                _ => {}
            },
            Some(DRAW) | Some(crate::xml::DR3D) => {
                if let Some(shape) = draw::parse_shape(node, ctx) {
                    out.push(Inline::Shape(Box::new(shape)));
                    *after_space = false;
                }
            }
            // Master-page fields a slide fills in: `footer`, `header` and `date-time`.
            Some(PRESENTATION) if matches!(name, "footer" | "header" | "date-time") => {
                out.push(Inline::Field {
                    kind: name.to_owned(),
                    text: text_content(node),
                });
            }
            Some(namespace) if is_foreign(namespace) => parse_inlines(node, ctx, out, after_space),
            _ => {}
        }
    }
    ctx.leave();
}

fn parse_list(node: Node<'_, '_>, ctx: &mut Ctx<'_>) -> List {
    let mut list = List {
        style: owned(node, TEXT, "style-name"),
        id: owned(node, XML, "id"),
        continue_numbering: attr(node, TEXT, "continue-numbering") == Some("true"),
        continue_list: owned(node, TEXT, "continue-list"),
        items: Vec::new(),
    };
    if !ctx.enter() {
        return list;
    }
    for item in elements(node) {
        let header = is(item, TEXT, "list-header");
        if (header || is(item, TEXT, "list-item")) && ctx.count() {
            list.items.push(ListItem {
                header,
                start_value: attr(item, TEXT, "start-value").and_then(|value| value.parse().ok()),
                blocks: parse_blocks(item, ctx),
            });
        }
    }
    ctx.leave();
    list
}

pub(crate) fn parse_table(node: Node<'_, '_>, ctx: &mut Ctx<'_>) -> Table {
    let mut table = Table {
        name: owned(node, TABLE, "name"),
        style: owned(node, TABLE, "style-name"),
        columns: Vec::new(),
        rows: Vec::new(),
    };
    if ctx.enter() {
        table_children(node, ctx, &mut table, false);
        ctx.leave();
    }
    table
}

fn table_children(parent: Node<'_, '_>, ctx: &mut Ctx<'_>, table: &mut Table, header: bool) {
    for node in elements(parent) {
        if node.tag_name().namespace() != Some(TABLE) || !ctx.count() {
            continue;
        }
        match node.tag_name().name() {
            "table-column" => table.columns.push(TableColumn {
                style: owned(node, TABLE, "style-name"),
                default_cell_style: owned(node, TABLE, "default-cell-style-name"),
                repeated: ctx.repeat(node, "number-columns-repeated"),
            }),
            "table-columns" | "table-header-columns" | "table-column-group" | "table-rows"
            | "table-row-group" => table_children(node, ctx, table, header),
            "table-header-rows" => table_children(node, ctx, table, true),
            "table-row" => {
                let mut row = TableRow {
                    style: owned(node, TABLE, "style-name"),
                    header,
                    repeated: ctx.repeat(node, "number-rows-repeated"),
                    cells: Vec::new(),
                };
                for cell in elements(node) {
                    let covered = is(cell, TABLE, "covered-table-cell");
                    if !(covered || is(cell, TABLE, "table-cell")) || !ctx.count() {
                        continue;
                    }
                    let value_type = owned(cell, OFFICE, "value-type");
                    let value = ["value", "date-value", "time-value", "boolean-value", "string-value"]
                        .iter()
                        .find_map(|name| owned(cell, OFFICE, name));
                    row.cells.push(TableCell {
                        style: owned(cell, TABLE, "style-name"),
                        covered,
                        column_span: ctx.repeat(cell, "number-columns-spanned"),
                        row_span: ctx.repeat(cell, "number-rows-spanned"),
                        repeated: ctx.repeat(cell, "number-columns-repeated"),
                        value_type,
                        value,
                        formula: owned(cell, TABLE, "formula"),
                        blocks: parse_blocks(cell, ctx),
                    });
                }
                table.rows.push(row);
            }
            _ => {}
        }
    }
}

/// The plain text of blocks, with paragraphs separated by newlines.
pub fn plain_text(blocks: &[Block]) -> String {
    fn inlines(items: &[Inline], out: &mut String) {
        for item in items {
            match item {
                Inline::Text(text) => out.push_str(text),
                Inline::Span { children, .. } | Inline::Link { children, .. } => {
                    inlines(children, out)
                }
                Inline::Tab => out.push('\t'),
                Inline::LineBreak => out.push('\n'),
                Inline::Field { text, .. } => out.push_str(text),
                _ => {}
            }
        }
    }
    fn blocks_text(blocks: &[Block], out: &mut String) {
        for block in blocks {
            match block {
                Block::Paragraph(paragraph) => {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    inlines(&paragraph.inlines, out);
                }
                Block::List(list) => {
                    for item in &list.items {
                        blocks_text(&item.blocks, out);
                    }
                }
                Block::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            blocks_text(&cell.blocks, out);
                        }
                    }
                }
                Block::Section(section) => blocks_text(&section.blocks, out),
                Block::Shape(shape) => blocks_text(&shape.text, out),
            }
        }
    }
    let mut out = String::new();
    blocks_text(blocks, &mut out);
    out
}

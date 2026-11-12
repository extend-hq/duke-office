//! OpenDocument text import.
//!
//! `rust-odf` reads the file and this module builds the document model from
//! what it read. OpenDocument and Word are separate paths that meet at
//! [`DocModel`]: no Word package or Word markup is produced on the way, so an
//! OpenDocument file is never converted to DOCX in order to be parsed.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};
use std::rc::Rc;

use rust_odf::draw::{Shape, ShapeKind};
use rust_odf::media::{image_content_type, image_extension};
use rust_odf::style::{
    Align, Border, Break, Family, FontSize, LineHeight, ListLabel, ListStyle, MasterPage,
    ParagraphProps, Scope, StyleSheet, TabKind, TextProps, VerticalAlign as CellAlign,
};
use rust_odf::text::{Annotation, Block, Inline, List, Note, Paragraph, Table};
use rust_odf::units::Length;
use rust_odf::{decode_style_name, Body, Limits, OdfDocument, OdfFamily};
use sha2::{Digest, Sha256};

use crate::model::{
    CommentRange, DocModel, DocModelMetadata, DocNode, DocumentCommentDefinition,
    DocumentCompatibilitySettings, DocumentNoteDefinition, DocumentSection, FooterSection,
    HeaderSection, HeadingLevel, ImageFloating, ImageHorizontalAlign, ImageRunNode,
    ImageRunNodeType, ImageVerticalAlign, ImageWrapText, ImageWrapType, NoteReference,
    NoteReferenceKind, NumberingAbstractDefinition, NumberingDefinitionSet,
    NumberingInstanceDefinition, NumberingLevelDefinition, ParagraphAlignment, ParagraphBorderSet,
    ParagraphBorderStyle, ParagraphChildNode, ParagraphIndent, ParagraphLineRule, ParagraphNode,
    ParagraphNodeType, ParagraphNumbering, ParagraphSpacing, ParagraphStyle,
    ParagraphStyleDefinition, ParagraphTabStop, ParagraphTabStopAlignment, ParagraphTabStopLeader,
    SharedParagraphStyle, SharedTextStyle, TableAlignment, TableBorderSet, TableBorderStyle,
    TableBoxSpacing, TableCellContentNode, TableCellNode, TableCellNodeType, TableCellStyle,
    TableCellVerticalAlign, TableNode, TableNodeType, TablePreferredWidth, TableRowHeightRule,
    TableRowNode, TableRowNodeType, TableRowStyle, TableStyle, TextRunNode, TextRunNodeType,
    TextStyle, VerticalAlign,
};

/// Rows or columns one repeated table element may expand to.
const MAX_REPEAT: u32 = 4096;
/// English Metric Units in one CSS pixel.
const EMU_PER_PIXEL: f64 = 9525.0;
/// The part name blocks of the body, notes and comments report as their source.
const CONTENT_PART: &str = "content.xml";

/// Whether the bytes are an OpenDocument file of any type.
pub fn is_odf_format(bytes: &[u8]) -> bool {
    rust_odf::sniff(bytes).is_some()
}

/// Reads an OpenDocument text document, template or master document (`.odt`,
/// `.ott`, `.odm`, `.otm`, `.oth`, `.fodt`) into the document model.
pub fn parse_odt(bytes: &[u8]) -> Result<DocModel, String> {
    let kind = rust_odf::sniff(bytes).ok_or("not an OpenDocument file")?;
    if kind.family != OdfFamily::Text {
        return Err(format!(
            "OpenDocument {:?} files are not text documents",
            kind.family
        ));
    }
    // The identity hash of a large file runs beside the parse.
    #[cfg(not(target_arch = "wasm32"))]
    if bytes.len() >= 64 * 1024 {
        if let Some(pool) = crate::parse::worker_pool() {
            let mut identity = None;
            let model = pool.in_place_scope(|scope| {
                scope.spawn(|_| identity = Some(document_identity(bytes)));
                build_model(bytes)
            });
            let mut model = model?;
            crate::structured::assign_source_ids(&mut model, &identity.expect("joined identity task"));
            return Ok(model);
        }
    }
    let mut model = build_model(bytes)?;
    crate::structured::assign_source_ids(&mut model, &document_identity(bytes));
    Ok(model)
}

fn document_identity(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn build_model(bytes: &[u8]) -> Result<DocModel, String> {
    let mut document = OdfDocument::parse(bytes, &Limits::default()).map_err(|error| error.to_string())?;
    // Text, images and header content move into the model instead of being copied.
    let binaries = std::mem::take(&mut document.binaries);
    let masters = std::mem::take(&mut document.styles.master_pages);
    let body = match std::mem::replace(&mut document.body, Body::Other) {
        Body::Text(blocks) => blocks,
        _ => Vec::new(),
    };
    Ok(Builder::new(&document, binaries, masters).build(body))
}

fn pixels(length: Length) -> f64 {
    length.0 as f64 / EMU_PER_PIXEL
}

fn hex(color: &str) -> String {
    let mut value = String::with_capacity(color.len() + 1);
    value.push('#');
    value.push_str(color);
    value
}

fn heading_level(level: u8) -> Option<HeadingLevel> {
    Some(match level {
        1 => HeadingLevel::One,
        2 => HeadingLevel::Two,
        3 => HeadingLevel::Three,
        4 => HeadingLevel::Four,
        5 => HeadingLevel::Five,
        6 => HeadingLevel::Six,
        7 => HeadingLevel::Seven,
        8 => HeadingLevel::Eight,
        9 => HeadingLevel::Nine,
        _ => return None,
    })
}

/// The level a style name such as `Heading 2` states, when it states one.
fn heading_level_in_name(name: &str) -> Option<u8> {
    let lower = name.to_ascii_lowercase();
    let rest = lower[lower.find("heading")? + "heading".len()..].trim_start();
    let mut digits = rest.bytes().take_while(u8::is_ascii_digit);
    let first = digits.next()?;
    // `Heading 10` is not a level-one heading.
    (digits.next().is_none() && first != b'0').then_some(first - b'0')
}

/// Fills unset properties from `base`, resolving a relative font size against it.
fn overlay(mut props: TextProps, base: &TextProps) -> TextProps {
    if let Some(FontSize::Percent(percent)) = props.size {
        props.size = match base.size {
            Some(FontSize::Points(points)) => Some(FontSize::Points(points * percent / 100.0)),
            _ => None,
        };
    }
    props.inherit(base);
    props
}

fn text_style(props: &TextProps) -> Option<TextStyle> {
    let mut style = TextStyle {
        bold: props.bold,
        italic: props.italic,
        underline: props.underline,
        strike: props.strike,
        all_caps: props.uppercase,
        small_caps: props.small_caps,
        color: props.color.as_deref().map(hex),
        background_color: props.background.as_deref().filter(|color| !color.is_empty()).map(hex),
        font_size_pt: props.points(),
        font_size_cs_pt: props.points(),
        language: props.language.clone(),
        language_east_asia: props.language_asian.clone(),
        language_bidi: props.language_complex.clone(),
        character_spacing_twips: props.letter_spacing.map(Length::twips),
        vertical_align: props.position.and_then(|position| {
            if position > 0.0 {
                Some(VerticalAlign::Superscript)
            } else if position < 0.0 {
                Some(VerticalAlign::Subscript)
            } else {
                None
            }
        }),
        ..TextStyle::default()
    };
    if let Some(font) = &props.font {
        style.font_family = Some(font.clone());
        style.source_font_family = Some(font.clone());
        style.font_family_ascii = Some(font.clone());
        style.font_family_h_ansi = Some(font.clone());
        style.resolved_font_family_ascii = Some(font.clone());
        style.resolved_font_family_h_ansi = Some(font.clone());
    }
    if let Some(font) = &props.font_asian {
        style.font_family_east_asia = Some(font.clone());
        style.resolved_font_family_east_asia = Some(font.clone());
    }
    if let Some(font) = &props.font_complex {
        style.font_family_cs = Some(font.clone());
        style.resolved_font_family_cs = Some(font.clone());
    }
    (style != TextStyle::default()).then_some(style)
}

fn border_kind(border: &Border) -> &'static str {
    match border.style.as_str() {
        "double" => "double",
        "dashed" => "dashed",
        "dotted" => "dotted",
        _ => "single",
    }
}

/// A border width in eighths of a point, within the range Word draws.
fn border_size(border: &Border) -> f64 {
    (border.width.points() * 8.0).round().clamp(2.0, 96.0)
}

fn paragraph_border(border: &Option<Border>) -> Option<ParagraphBorderStyle> {
    let border = border.as_ref().filter(|border| border.is_visible())?;
    Some(ParagraphBorderStyle {
        border_type: border_kind(border).into(),
        color: border.color.as_deref().map(hex),
        size_eighth_pt: Some(border_size(border)),
        space_pt: Some(0.0),
        frame: None,
        shadow: None,
    })
}

fn table_border(border: &Option<Border>) -> Option<TableBorderStyle> {
    let border = border.as_ref().filter(|border| border.is_visible())?;
    Some(TableBorderStyle {
        border_type: border_kind(border).into(),
        color: border.color.as_deref().map(hex),
        size_eighth_pt: Some(border_size(border)),
    })
}

fn alignment(props: &ParagraphProps) -> Option<ParagraphAlignment> {
    let rtl = props.rtl == Some(true);
    Some(match props.align? {
        Align::Left => ParagraphAlignment::Left,
        Align::Right => ParagraphAlignment::Right,
        Align::Start if rtl => ParagraphAlignment::Right,
        Align::Start => ParagraphAlignment::Left,
        Align::End if rtl => ParagraphAlignment::Left,
        Align::End => ParagraphAlignment::Right,
        Align::Center => ParagraphAlignment::Center,
        Align::Justify => ParagraphAlignment::Justify,
    })
}

fn spacing(props: &ParagraphProps) -> Option<ParagraphSpacing> {
    let (line_twips, line_rule) = match props.line_height {
        Some(LineHeight::Percent(percent)) => (
            Some((percent * 2.4).round() as i64),
            Some(ParagraphLineRule::Auto),
        ),
        Some(LineHeight::Exact(height)) => (Some(height.twips()), Some(ParagraphLineRule::Exact)),
        Some(LineHeight::AtLeast(height)) => (Some(height.twips()), Some(ParagraphLineRule::AtLeast)),
        Some(LineHeight::Leading(_)) | None => (None, None),
    };
    let spacing = ParagraphSpacing {
        before_twips: props.margin_top.map(|margin| margin.twips().max(0)),
        after_twips: props.margin_bottom.map(|margin| margin.twips().max(0)),
        before_auto_spacing: None,
        after_auto_spacing: None,
        line_twips,
        line_rule,
    };
    (spacing.before_twips.is_some() || spacing.after_twips.is_some() || spacing.line_twips.is_some())
        .then_some(spacing)
}

fn indent(props: &ParagraphProps) -> Option<ParagraphIndent> {
    let text_indent = props.text_indent.map(Length::twips);
    let indent = ParagraphIndent {
        left_twips: props.margin_left.map(Length::twips),
        right_twips: props.margin_right.map(Length::twips),
        first_line_twips: text_indent.filter(|indent| *indent >= 0),
        hanging_twips: text_indent.filter(|indent| *indent < 0).map(|indent| -indent),
    };
    (indent.left_twips.is_some() || indent.right_twips.is_some() || text_indent.is_some()).then_some(indent)
}

fn tab_stops(props: &ParagraphProps) -> Option<Vec<ParagraphTabStop>> {
    let stops = props.tab_stops.as_ref().filter(|stops| !stops.is_empty())?;
    // OpenDocument measures tab stops from the paragraph's left indent.
    let origin = props.margin_left.map_or(0, Length::twips);
    Some(
        stops
            .iter()
            .map(|stop| ParagraphTabStop {
                alignment: Some(match stop.kind {
                    TabKind::Left => ParagraphTabStopAlignment::Left,
                    TabKind::Center => ParagraphTabStopAlignment::Center,
                    TabKind::Right => ParagraphTabStopAlignment::Right,
                    TabKind::Decimal => ParagraphTabStopAlignment::Decimal,
                }),
                leader: stop.leader.map(|leader| match leader {
                    '-' => ParagraphTabStopLeader::Hyphen,
                    '_' => ParagraphTabStopLeader::Underscore,
                    _ => ParagraphTabStopLeader::Dot,
                }),
                position_twips: Some(origin + stop.position.twips()),
            })
            .collect(),
    )
}

fn paragraph_borders(props: &ParagraphProps) -> Option<ParagraphBorderSet> {
    let borders = ParagraphBorderSet {
        top: paragraph_border(&props.border_top),
        right: paragraph_border(&props.border_right),
        bottom: paragraph_border(&props.border_bottom),
        left: paragraph_border(&props.border_left),
        between: None,
        bar: None,
    };
    (borders.top.is_some() || borders.right.is_some() || borders.bottom.is_some() || borders.left.is_some())
        .then_some(borders)
}

/// Resolved paragraph formatting without what the paragraph's position adds:
/// its list label, outline level or a page break carried over.
fn paragraph_style(props: &ParagraphProps) -> ParagraphStyle {
    ParagraphStyle {
        align: alignment(props),
        spacing: spacing(props),
        indent: indent(props),
        background_color: props.background.as_deref().filter(|color| !color.is_empty()).map(hex),
        borders: paragraph_borders(props),
        tab_stops: tab_stops(props),
        keep_next: props.keep_with_next,
        keep_lines: props.keep_together,
        page_break_before: props.break_before.map(|value| value == Break::Page),
        ..ParagraphStyle::default()
    }
}

/// FNV-1a over the short style names the builder's caches are keyed by,
/// which the default hasher is slow for.
#[derive(Clone, Copy)]
struct NameHasher(u64);

impl Default for NameHasher {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for NameHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

type NameMap<V> = HashMap<String, V, BuildHasherDefault<NameHasher>>;

/// Text formatting in effect at one place in a paragraph.
struct RunFormat {
    /// Index of the formats derived from this one in [`Builder::derived`].
    id: usize,
    /// Fully resolved properties, which spans inside inherit from.
    props: TextProps,
    /// The model's form of `props`, made when text first uses the format: a
    /// span that only wraps other spans never needs it.
    style: OnceCell<Option<SharedTextStyle>>,
    /// A named character style is applied, so a link keeps that look.
    named: bool,
    hidden: bool,
}

impl RunFormat {
    fn style(&self) -> Option<SharedTextStyle> {
        self.style
            .get_or_init(|| text_style(&self.props).map(Into::into))
            .clone()
    }
}

/// What a paragraph style gives every paragraph that uses it, worked out once.
struct ParagraphFormat {
    id: usize,
    style: SharedParagraphStyle,
    run: Rc<RunFormat>,
    /// Indents an automatic style sets itself, which win over a list level's.
    own_left: bool,
    own_text_indent: bool,
    /// The heading level the style implies, 0 for none.
    heading: u8,
    break_after: bool,
    /// The master page the style switches to.
    master: Option<Rc<str>>,
}

/// What a paragraph's position adds to its style.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
struct Variant {
    heading: u8,
    /// `(numId, ilvl)`.
    numbering: Option<(i64, u8)>,
    left: Option<i64>,
    hanging: Option<i64>,
    break_before: bool,
}

/// A named paragraph style as paragraphs refer to it.
struct NamedStyle {
    id: Rc<str>,
    name: Rc<str>,
    heading: u8,
}

/// Where a list's paragraphs take their numbering from.
#[derive(Clone)]
struct ListState {
    num_id: i64,
    abstract_id: i64,
    /// Zero-based.
    level: u8,
    style: Option<String>,
    /// The next paragraph is the first of its item and carries the label.
    label_pending: bool,
    /// Left indent in twips the list level states, for an item's later paragraphs.
    indent: Option<i64>,
    /// Left and hanging indent in twips of a labelled paragraph.
    label_indent: (i64, i64),
}

/// A header or footer of a master page, converted once.
struct Story {
    part_name: String,
    nodes: Vec<DocNode>,
}

struct SectionLayout {
    properties: String,
    headers: Vec<HeaderSection>,
    footers: Vec<FooterSection>,
}

/// Receives the blocks of a story: the body, a note or a table cell.
trait Sink {
    fn paragraph(&mut self, node: ParagraphNode);
    fn table(&mut self, node: TableNode);
}

impl Sink for Vec<DocNode> {
    fn paragraph(&mut self, node: ParagraphNode) {
        self.push(DocNode::Paragraph(node));
    }

    fn table(&mut self, node: TableNode) {
        self.push(DocNode::Table(node));
    }
}

impl Sink for Vec<TableCellContentNode> {
    fn paragraph(&mut self, node: ParagraphNode) {
        self.push(TableCellContentNode::Paragraph(node));
    }

    fn table(&mut self, node: TableNode) {
        self.push(TableCellContentNode::Table(Box::new(node)));
    }
}

fn scope_index(scope: Scope) -> usize {
    match scope {
        Scope::Content => 0,
        Scope::Styles => 1,
    }
}

fn text_run(text: String, run: &RunFormat, link: Option<&str>) -> ParagraphChildNode {
    ParagraphChildNode::Text(TextRunNode {
        r#type: TextRunNodeType::Text,
        text,
        style: run.style(),
        link: link.map(str::to_owned),
        note_reference: None,
        field_instruction: None,
    })
}

fn paragraph_node(
    children: Vec<ParagraphChildNode>,
    style: SharedParagraphStyle,
    mark: Option<SharedTextStyle>,
    comment_ranges: Option<Vec<CommentRange>>,
) -> ParagraphNode {
    ParagraphNode {
        review_content: None,
        block_id: None,
        r#type: ParagraphNodeType::Paragraph,
        children,
        style: Some(style),
        paragraph_mark_style: mark,
        source_paragraph_mark_style: None,
        source_paragraph_mark_formatting: None,
        source_paragraph_mark_properties_xml: None,
        paragraph_mark_deleted: None,
        source_xml: None,
        source_text_patch: None,
        source_run_provenance: None,
        comment_ranges,
    }
}

fn plain_text(nodes: &[DocNode], lines: &mut Vec<String>) {
    fn paragraph(node: &ParagraphNode, lines: &mut Vec<String>) {
        let mut line = String::new();
        for child in &node.children {
            if let ParagraphChildNode::Text(run) = child {
                line.push_str(&run.text);
            }
        }
        lines.push(line);
    }
    fn table(node: &TableNode, lines: &mut Vec<String>) {
        for cell in node.rows.iter().flat_map(|row| &row.cells) {
            for content in &cell.nodes {
                match content {
                    TableCellContentNode::Paragraph(node) => paragraph(node, lines),
                    TableCellContentNode::Table(node) => table(node, lines),
                }
            }
        }
    }
    for node in nodes {
        match node {
            DocNode::Paragraph(node) => paragraph(node, lines),
            DocNode::Table(node) => table(node, lines),
        }
    }
}

struct Builder<'a> {
    document: &'a OdfDocument,
    styles: &'a StyleSheet,
    masters: Vec<MasterPage>,
    /// Named paragraph styles by OpenDocument name.
    named: HashMap<&'a str, NamedStyle>,
    /// The paragraph style of a paragraph that names none.
    default_style: Option<&'a str>,
    /// Paragraph formats by style name, per scope.
    formats: [NameMap<Rc<ParagraphFormat>>; 2],
    format_count: usize,
    variants: HashMap<(usize, Variant), SharedParagraphStyle>,
    /// Formats of spans and links by the format they sit in, then by style name.
    derived: Vec<NameMap<Rc<RunFormat>>>,
    cell_styles: [NameMap<Option<Rc<TableCellStyle>>>; 2],
    note_reference: SharedTextStyle,
    binaries: BTreeMap<String, Vec<u8>>,
    /// Content types of the images already placed, by reference.
    placed: HashMap<String, Option<&'static str>>,
    /// An image is placed more than once, so later uses still need its bytes.
    repeated_images: bool,
    footnotes: Vec<DocumentNoteDefinition>,
    endnotes: Vec<DocumentNoteDefinition>,
    note_count: [i64; 2],
    comments: Vec<DocumentCommentDefinition>,
    /// Comment ids of annotations whose range is still open, by annotation name.
    open_comments: Vec<(String, i64)>,
    abstracts: Vec<NumberingAbstractDefinition>,
    abstract_by_list: HashMap<(usize, String), i64>,
    instances: Vec<NumberingInstanceDefinition>,
    num_by_list_id: HashMap<String, i64>,
    last_num_by_abstract: HashMap<i64, i64>,
    outline_num: Option<i64>,
    stories: HashMap<(usize, u8), Option<Rc<Story>>>,
    story_count: [usize; 2],
    sections: HashMap<String, Rc<SectionLayout>>,
    even_and_odd_headers: bool,
    /// Drawings anchored outside a paragraph, waiting for the next one.
    pending: Vec<ParagraphChildNode>,
    /// The previous paragraph ends its page.
    break_next: bool,
    warnings: BTreeSet<String>,
}

impl<'a> Builder<'a> {
    fn new(document: &'a OdfDocument, binaries: BTreeMap<String, Vec<u8>>, masters: Vec<MasterPage>) -> Self {
        let styles = &document.styles;
        let mut named: HashMap<&'a str, NamedStyle> = HashMap::new();
        let mut used = HashSet::new();
        for style in styles.common_styles().filter(|style| style.family == Family::Paragraph) {
            let display = style.display_name.clone().unwrap_or_else(|| decode_style_name(&style.name));
            let base = if style.name == "Standard" {
                "Normal".to_owned()
            } else {
                let id: String = display.chars().filter(char::is_ascii_alphanumeric).collect();
                if id.is_empty() || id == "Normal" {
                    "Style".to_owned()
                } else {
                    id
                }
            };
            let mut id = base.clone();
            let mut suffix = 1;
            while !used.insert(id.clone()) {
                suffix += 1;
                id = format!("{base}{suffix}");
            }
            named.insert(
                style.name.as_str(),
                NamedStyle {
                    id: id.into(),
                    name: if style.name == "Standard" { "Normal".into() } else { display.into() },
                    heading: 0,
                },
            );
        }
        let headings: Vec<(&str, u8)> = named
            .keys()
            .map(|name| (*name, style_heading(styles, name, &named)))
            .collect();
        for (name, heading) in headings {
            if let Some(style) = named.get_mut(name) {
                style.heading = heading;
            }
        }
        Self {
            document,
            styles,
            masters,
            default_style: named.contains_key("Standard").then_some("Standard"),
            named,
            formats: Default::default(),
            format_count: 0,
            variants: HashMap::new(),
            derived: Vec::new(),
            cell_styles: Default::default(),
            note_reference: TextStyle {
                vertical_align: Some(VerticalAlign::Superscript),
                ..TextStyle::default()
            }
            .into(),
            binaries,
            placed: HashMap::new(),
            repeated_images: false,
            footnotes: Vec::new(),
            endnotes: Vec::new(),
            note_count: [0; 2],
            comments: Vec::new(),
            open_comments: Vec::new(),
            abstracts: Vec::new(),
            abstract_by_list: HashMap::new(),
            instances: Vec::new(),
            num_by_list_id: HashMap::new(),
            last_num_by_abstract: HashMap::new(),
            outline_num: None,
            stories: HashMap::new(),
            story_count: [0; 2],
            sections: HashMap::new(),
            even_and_odd_headers: false,
            pending: Vec::new(),
            break_next: false,
            warnings: document.warnings.iter().cloned().collect(),
        }
    }

    fn warn(&mut self, message: &str) {
        if !self.warnings.contains(message) {
            self.warnings.insert(message.to_owned());
        }
    }

    // ----- formatting -----

    fn run_format(&mut self, props: TextProps, named: bool) -> Rc<RunFormat> {
        self.derived.push(NameMap::default());
        Rc::new(RunFormat {
            id: self.derived.len() - 1,
            style: OnceCell::new(),
            hidden: props.hidden == Some(true),
            props,
            named,
        })
    }

    /// The cached contribution of a paragraph style; `None` is a paragraph that names none.
    fn format(&mut self, scope: Scope, name: Option<&str>) -> Rc<ParagraphFormat> {
        let key = name.unwrap_or_default();
        if let Some(cached) = self.formats[scope_index(scope)].get(key) {
            return Rc::clone(cached);
        }
        let styles = self.styles;
        let name = name.or(self.default_style);
        let style = name.and_then(|name| styles.style(scope, Family::Paragraph, name));
        let (own, nearest) = match style {
            Some(style) if style.automatic => (Some(style), style.parent.as_deref()),
            Some(style) => (None, Some(style.name.as_str())),
            None => (None, None),
        };
        let nearest = nearest.and_then(|name| self.named.get(name));
        let paragraph = styles.resolve_paragraph(scope, Family::Paragraph, name);
        let mut resolved = paragraph_style(&paragraph);
        resolved.style_id = nearest.map(|style| style.id.to_string());
        resolved.style_name = nearest.map(|style| style.name.to_string());
        let heading = nearest.map_or(0, |style| style.heading);
        resolved.heading_level = heading_level(heading);
        let master = name
            .into_iter()
            .flat_map(|name| styles.ancestors(scope, Family::Paragraph, name))
            .find_map(|style| style.master_page.as_deref())
            .filter(|master| !master.is_empty())
            .map(Rc::from);
        let run = self.run_format(styles.resolve_text(scope, Family::Paragraph, name), false);
        self.format_count += 1;
        let format = Rc::new(ParagraphFormat {
            id: self.format_count,
            style: resolved.into(),
            run,
            own_left: own.is_some_and(|style| style.props.paragraph.margin_left.is_some()),
            own_text_indent: own.is_some_and(|style| style.props.paragraph.text_indent.is_some()),
            heading,
            break_after: paragraph.break_after == Some(Break::Page),
            master,
        });
        self.formats[scope_index(scope)].insert(key.to_owned(), Rc::clone(&format));
        format
    }

    fn variant(&mut self, format: &ParagraphFormat, variant: Variant) -> SharedParagraphStyle {
        if variant == Variant::default() {
            return format.style.clone();
        }
        self.variants
            .entry((format.id, variant))
            .or_insert_with(|| {
                let mut style = (*format.style).clone();
                if variant.heading != 0 {
                    style.heading_level = heading_level(variant.heading);
                }
                if let Some((num_id, ilvl)) = variant.numbering {
                    style.numbering = Some(ParagraphNumbering {
                        num_id,
                        ilvl: i64::from(ilvl),
                    });
                }
                if variant.left.is_some() || variant.hanging.is_some() {
                    let mut indent = style.indent.take().unwrap_or(ParagraphIndent {
                        left_twips: None,
                        right_twips: None,
                        first_line_twips: None,
                        hanging_twips: None,
                    });
                    if let Some(left) = variant.left {
                        indent.left_twips = Some(left);
                    }
                    if let Some(hanging) = variant.hanging {
                        indent.hanging_twips = Some(hanging);
                        indent.first_line_twips = None;
                    }
                    style.indent = Some(indent);
                }
                if variant.break_before {
                    style.page_break_before = Some(true);
                }
                style.into()
            })
            .clone()
    }

    /// The format of a span with the given text style inside `outer`.
    fn span_format(&mut self, scope: Scope, name: &str, outer: &Rc<RunFormat>) -> Rc<RunFormat> {
        if let Some(cached) = self.derived[outer.id].get(name) {
            return Rc::clone(cached);
        }
        let styles = self.styles;
        let format = match styles.style(scope, Family::Text, name) {
            Some(style) => {
                let props = overlay(styles.inherited_text(scope, Family::Text, name), &outer.props);
                let named = outer.named || !style.automatic || style.parent.is_some();
                self.run_format(props, named)
            }
            None => Rc::clone(outer),
        };
        self.derived[outer.id].insert(name.to_owned(), Rc::clone(&format));
        format
    }

    /// The format of link text inside `outer`: the document's link style
    /// unless a character style already applies.
    fn link_format(&mut self, outer: &Rc<RunFormat>) -> Rc<RunFormat> {
        const KEY: &str = "\u{1}link";
        if let Some(cached) = self.derived[outer.id].get(KEY) {
            return Rc::clone(cached);
        }
        let styles = self.styles;
        let format = if !outer.named && styles.style(Scope::Styles, Family::Text, "Internet_20_link").is_some() {
            let link = styles.inherited_text(Scope::Styles, Family::Text, "Internet_20_link");
            self.run_format(overlay(link, &outer.props), true)
        } else {
            Rc::clone(outer)
        };
        self.derived[outer.id].insert(KEY.to_owned(), Rc::clone(&format));
        format
    }

    // ----- numbering -----

    fn abstract_num(&mut self, scope: Scope, name: &str) -> Option<i64> {
        if let Some(id) = self
            .abstract_by_list
            .get(&(scope_index(scope), name.to_owned()))
        {
            return Some(*id);
        }
        let list = self.styles.list_style(scope, name)?;
        let id = self.abstracts.len() as i64;
        self.abstracts.push(abstract_definition(id, list, true));
        self.abstract_by_list.insert((scope_index(scope), name.to_owned()), id);
        Some(id)
    }

    fn new_num(&mut self, abstract_id: i64, start: Option<(u8, u32)>) -> i64 {
        let num_id = self.instances.len() as i64 + 1;
        self.instances.push(NumberingInstanceDefinition {
            num_id,
            abstract_num_id: abstract_id,
            level_start_overrides: start
                .map(|(level, start)| HashMap::from([(level.to_string(), i64::from(start))])),
            level_overrides: None,
        });
        self.last_num_by_abstract.insert(abstract_id, num_id);
        num_id
    }

    /// Numbering for a list that is not nested in another list.
    fn top_level_list(&mut self, list: &List, scope: Scope) -> Option<ListState> {
        let name = list.style.as_deref()?;
        let abstract_id = self.abstract_num(scope, name)?;
        let continued = list
            .continue_list
            .as_ref()
            .and_then(|id| self.num_by_list_id.get(id).copied())
            .or_else(|| {
                list.continue_numbering
                    .then(|| self.last_num_by_abstract.get(&abstract_id).copied())
                    .flatten()
            });
        let num_id = continued.unwrap_or_else(|| self.new_num(abstract_id, None));
        if let Some(id) = &list.id {
            self.num_by_list_id.insert(id.clone(), num_id);
        }
        Some(ListState {
            num_id,
            abstract_id,
            level: 0,
            style: Some(name.to_owned()),
            label_pending: false,
            indent: None,
            label_indent: (0, 0),
        })
    }

    fn list<S: Sink>(&mut self, list: List, scope: Scope, parent: Option<&ListState>, out: &mut S) {
        let state = match parent {
            None => self.top_level_list(&list, scope),
            Some(parent) => {
                let mut state = parent.clone();
                state.level = parent.level.saturating_add(1);
                // A nested list naming another style numbers independently.
                if let Some(name) = list.style.as_deref().filter(|name| parent.style.as_deref() != Some(*name)) {
                    if let Some(abstract_id) = self.abstract_num(scope, name) {
                        state.abstract_id = abstract_id;
                        state.num_id = self.new_num(abstract_id, None);
                        state.style = Some(name.to_owned());
                    }
                }
                Some(state)
            }
        };
        let Some(mut state) = state else {
            for item in list.items {
                self.blocks(item.blocks, scope, None, out);
            }
            return;
        };
        let level = state
            .style
            .as_deref()
            .and_then(|name| self.styles.list_style(scope, name))
            .and_then(|style| style.levels.get(&(state.level.saturating_add(1))));
        state.indent = level.and_then(|level| level.margin_left).map(Length::twips);
        state.label_indent = level_indent(
            state.level.min(8),
            level.and_then(|level| level.margin_left),
            level.and_then(|level| level.text_indent),
        );
        for item in list.items {
            if let Some(start) = item.start_value.filter(|start| *start <= 32_767) {
                state.num_id = self.new_num(state.abstract_id, Some((state.level.min(8), start)));
            }
            state.label_pending = !item.header;
            self.blocks(item.blocks, scope, Some(&mut state), out);
        }
    }

    fn outline_numbering(&mut self, level: u8) -> Option<(i64, u8)> {
        let outline = self.styles.outline.as_ref()?;
        let numbered = outline
            .levels
            .get(&level)
            .is_some_and(|level| matches!(&level.label, ListLabel::Number { format, .. } if !format.is_empty()));
        if !numbered {
            return None;
        }
        let num_id = match self.outline_num {
            Some(num_id) => num_id,
            None => {
                let abstract_id = self.abstracts.len() as i64;
                self.abstracts.push(abstract_definition(abstract_id, outline, false));
                let num_id = self.new_num(abstract_id, None);
                self.outline_num = Some(num_id);
                num_id
            }
        };
        Some((num_id, level.saturating_sub(1).min(8)))
    }

    // ----- blocks -----

    /// Converts block content that is a story of its own: a cell, a note, a
    /// comment, a text box, a header or a footer.
    fn story<S: Sink + Default>(&mut self, blocks: Vec<Block>, scope: Scope) -> S {
        let pending = std::mem::take(&mut self.pending);
        let open = std::mem::take(&mut self.open_comments);
        let break_next = std::mem::take(&mut self.break_next);
        let mut out = S::default();
        self.blocks(blocks, scope, None, &mut out);
        self.flush_pending(scope, &mut out);
        self.pending = pending;
        self.open_comments = open;
        self.break_next = break_next;
        out
    }

    /// Gives drawings no paragraph followed a paragraph of their own.
    fn flush_pending<S: Sink>(&mut self, scope: Scope, out: &mut S) {
        if self.pending.is_empty() {
            return;
        }
        let format = self.format(scope, None);
        let children = std::mem::take(&mut self.pending);
        out.paragraph(paragraph_node(children, format.style.clone(), format.run.style(), None));
    }

    fn blocks<S: Sink>(&mut self, blocks: Vec<Block>, scope: Scope, mut list: Option<&mut ListState>, out: &mut S) {
        for block in blocks {
            self.block(block, scope, list.as_deref_mut(), out);
        }
    }

    fn block<S: Sink>(&mut self, block: Block, scope: Scope, list: Option<&mut ListState>, out: &mut S) {
        match block {
            Block::Paragraph(paragraph) => self.paragraph(paragraph, scope, list, out),
            Block::List(nested) => self.list(nested, scope, list.as_deref(), out),
            Block::Table(table) => {
                let table = self.table(table, scope);
                out.table(table);
            }
            Block::Section(section) => self.blocks(section.blocks, scope, list, out),
            // A drawing anchored outside a paragraph attaches to the next one.
            Block::Shape(shape) => {
                let mut pending = std::mem::take(&mut self.pending);
                self.shape(*shape, scope, &mut pending);
                self.pending = pending;
            }
        }
    }

    fn paragraph<S: Sink>(&mut self, paragraph: Paragraph, scope: Scope, list: Option<&mut ListState>, out: &mut S) {
        let format = self.format(scope, paragraph.style.as_deref());
        let outline = paragraph.outline_level.map(|level| level.clamp(1, 9));
        let mut variant = Variant {
            heading: outline.filter(|level| *level != format.heading).unwrap_or(0),
            break_before: std::mem::take(&mut self.break_next),
            ..Variant::default()
        };
        match list {
            Some(state) => {
                if state.label_pending {
                    state.label_pending = false;
                    variant.numbering = Some((state.num_id, state.level.min(8)));
                    if !format.own_left {
                        variant.left = Some(state.label_indent.0);
                    }
                    if !format.own_text_indent {
                        variant.hanging = Some(state.label_indent.1);
                    }
                } else if !format.own_left {
                    variant.left = state.indent;
                }
            }
            None => {
                variant.numbering = paragraph
                    .outline_level
                    .and_then(|level| self.outline_numbering(level));
            }
        }
        let style = self.variant(&format, variant);
        let mut children = std::mem::take(&mut self.pending);
        children.reserve(paragraph.inlines.len());
        let mut ranges = Ranges::open(&self.open_comments);
        self.inlines(paragraph.inlines, scope, &format.run, None, &mut children, &mut ranges);
        if children.is_empty() {
            // An empty paragraph is as tall as its formatting says.
            children.push(text_run(String::new(), &format.run, None));
        }
        if format.break_after {
            self.break_next = true;
        }
        let ranges = ranges.close(children.len());
        out.paragraph(paragraph_node(children, style, format.run.style(), ranges));
    }

    fn inlines(
        &mut self,
        inlines: Vec<Inline>,
        scope: Scope,
        run: &Rc<RunFormat>,
        link: Option<&str>,
        out: &mut Vec<ParagraphChildNode>,
        ranges: &mut Ranges,
    ) {
        for inline in inlines {
            match inline {
                Inline::Text(text) => {
                    if !run.hidden {
                        out.push(text_run(text, run, link));
                    }
                }
                Inline::Span { style, children } => match style {
                    Some(name) => {
                        let inner = self.span_format(scope, &name, run);
                        self.inlines(children, scope, &inner, link, out, ranges);
                    }
                    None => self.inlines(children, scope, run, link, out, ranges),
                },
                Inline::Link { href, children } => {
                    let inner = self.link_format(run);
                    let href = Some(href.as_str()).filter(|href| !href.is_empty());
                    self.inlines(children, scope, &inner, href.or(link), out, ranges);
                }
                Inline::Tab => {
                    if !run.hidden {
                        out.push(text_run("\t".to_owned(), run, link));
                    }
                }
                Inline::LineBreak => {
                    if !run.hidden {
                        out.push(text_run("\n".to_owned(), run, link));
                    }
                }
                Inline::SoftPageBreak | Inline::Bookmark(_) => {}
                Inline::Note(note) => self.note(note, scope, out),
                Inline::Annotation(annotation) => {
                    let name = annotation.name.clone();
                    let id = self.annotation(annotation, scope);
                    ranges.start(id, out.len());
                    match name {
                        // The matching end marker closes the range later.
                        Some(name) => self.open_comments.push((name, id)),
                        None => ranges.end(id, out.len()),
                    }
                }
                Inline::AnnotationEnd(name) => {
                    if let Some(index) = self.open_comments.iter().position(|(open, _)| *open == name) {
                        let (_, id) = self.open_comments.remove(index);
                        ranges.end(id, out.len());
                    }
                }
                Inline::Field { kind, text } => {
                    let instruction = match kind.as_str() {
                        "page-number" => Some("PAGE"),
                        "page-count" => Some("NUMPAGES"),
                        _ => None,
                    };
                    if run.hidden || (instruction.is_none() && text.is_empty()) {
                        continue;
                    }
                    let mut node = text_run(text, run, link);
                    if let ParagraphChildNode::Text(node) = &mut node {
                        node.field_instruction = instruction.map(str::to_owned);
                    }
                    out.push(node);
                }
                Inline::Shape(shape) => self.shape(*shape, scope, out),
            }
        }
    }

    fn note(&mut self, note: Note, scope: Scope, out: &mut Vec<ParagraphChildNode>) {
        let kind = usize::from(note.endnote);
        self.note_count[kind] += 1;
        let id = self.note_count[kind];
        let nodes: Vec<DocNode> = self.story(note.blocks, scope);
        let mut lines = Vec::new();
        plain_text(&nodes, &mut lines);
        let text = lines.join("\n").trim().to_owned();
        if !text.is_empty() {
            let notes = if note.endnote { &mut self.endnotes } else { &mut self.footnotes };
            notes.push(DocumentNoteDefinition {
                id,
                text,
                nodes: Some(nodes),
            });
        }
        out.push(ParagraphChildNode::Text(TextRunNode {
            r#type: TextRunNodeType::Text,
            text: String::new(),
            style: Some(self.note_reference.clone()),
            link: None,
            note_reference: Some(NoteReference {
                kind: if note.endnote { NoteReferenceKind::Endnote } else { NoteReferenceKind::Footnote },
                id,
            }),
            field_instruction: None,
        }));
    }

    /// Stores a comment and returns its id.
    fn annotation(&mut self, annotation: Annotation, scope: Scope) -> i64 {
        let id = self.comments.len() as i64;
        let nodes: Vec<DocNode> = self.story(annotation.blocks, scope);
        let mut lines = Vec::new();
        plain_text(&nodes, &mut lines);
        self.comments.push(DocumentCommentDefinition {
            nodes: Some(nodes),
            id,
            author: annotation.author,
            initials: None,
            date: annotation.date,
            text: lines.join("\n"),
            parent_id: None,
            resolved: None,
            source_xml: None,
            extended_paragraph_id: None,
            source_resolved: None,
            resolution_dirty: None,
            is_new: None,
        });
        id
    }

    // ----- tables -----

    /// Borders, shading, margins and vertical alignment of a cell style.
    /// Tables repeat a few cell styles many times.
    fn cell_style(&mut self, scope: Scope, name: Option<&str>) -> Option<Rc<TableCellStyle>> {
        let key = name.unwrap_or_default();
        if let Some(cached) = self.cell_styles[scope_index(scope)].get(key) {
            return cached.clone();
        }
        let props = self.styles.resolve_table(scope, Family::TableCell, name);
        let borders = TableBorderSet {
            top: table_border(&props.border_top),
            right: table_border(&props.border_right),
            bottom: table_border(&props.border_bottom),
            left: table_border(&props.border_left),
            inside_h: None,
            inside_v: None,
            tl2br: None,
            tr2bl: None,
        };
        let has_borders =
            borders.top.is_some() || borders.right.is_some() || borders.bottom.is_some() || borders.left.is_some();
        let padding = props.padding.map(Length::twips);
        let style = TableCellStyle {
            background_color: props.background.as_deref().filter(|color| !color.is_empty()).map(hex),
            margin_twips: padding.map(|padding| TableBoxSpacing {
                top_twips: Some(padding),
                right_twips: Some(padding),
                bottom_twips: Some(padding),
                left_twips: Some(padding),
            }),
            vertical_align: match props.vertical_align {
                Some(CellAlign::Middle) => Some(TableCellVerticalAlign::Center),
                Some(CellAlign::Bottom) => Some(TableCellVerticalAlign::Bottom),
                Some(CellAlign::Top) | None => None,
            },
            borders: has_borders.then_some(borders),
            ..empty_cell_style()
        };
        let style = (style != empty_cell_style()).then(|| Rc::new(style));
        self.cell_styles[scope_index(scope)].insert(key.to_owned(), style.clone());
        style
    }

    fn table(&mut self, table: Table, scope: Scope) -> TableNode {
        let table_props = self.styles.resolve_table(scope, Family::Table, table.style.as_deref());
        let mut widths: Vec<Option<i64>> = Vec::new();
        for column in &table.columns {
            let width = self
                .styles
                .resolve_table(scope, Family::TableColumn, column.style.as_deref())
                .column_width
                .map(Length::twips);
            for _ in 0..column.repeated.min(MAX_REPEAT) {
                if widths.len() < MAX_REPEAT as usize {
                    widths.push(width);
                }
            }
        }
        let width = table_props.width.map(Length::twips);
        let style = TableStyle {
            style_id: None,
            source_style_id: None,
            width_twips: width,
            preferred_width: Some(match width {
                Some(width) => TablePreferredWidth::Dxa { value: width as f64 },
                None => TablePreferredWidth::Auto,
            }),
            source_width: None,
            alignment: match table_props.align.as_deref() {
                Some("center") => Some(TableAlignment::Center),
                Some("right") => Some(TableAlignment::Right),
                _ => None,
            },
            source_alignment: None,
            source_inherited_alignment: None,
            bidi_visual: None,
            source_bidi_visual: None,
            source_inherited_bidi_visual: None,
            indent_twips: table_props.margin_left.filter(|margin| margin.0 != 0).map(Length::twips),
            layout: None,
            cell_spacing_twips: None,
            cell_margin_twips: None,
            column_widths_twips: widths.iter().copied().collect::<Option<Vec<i64>>>().filter(|widths| !widths.is_empty()),
            collapsed_grid_columns: None,
            borders: None,
            floating: None,
        };

        // Vertical merges still open per grid column: rows left, columns
        // spanned and where the merged cell is.
        let mut merges: HashMap<usize, (u32, u32, usize, usize)> = HashMap::new();
        let mut rows: Vec<TableRowNode> = Vec::with_capacity(table.rows.len().min(MAX_REPEAT as usize));
        for row in table.rows {
            if rows.len() >= MAX_REPEAT as usize * 16 {
                self.warn("A table was truncated at the row limit");
                break;
            }
            // `table:number-rows-repeated` belongs to spreadsheets: LibreOffice
            // Writer shows such a row once in a text document, so it is not expanded.
            let row_props = self.styles.resolve_table(scope, Family::TableRow, row.style.as_deref());
            let (height_twips, height_rule) = match (row_props.row_height, row_props.min_row_height) {
                (Some(height), _) => (Some(height.twips()), Some(TableRowHeightRule::Exact)),
                (None, Some(height)) => (Some(height.twips()), Some(TableRowHeightRule::AtLeast)),
                (None, None) => (None, None),
            };
            let row_style = (height_twips.is_some() || row.header).then(|| TableRowStyle {
                grid_before: None,
                grid_after: None,
                width_before: None,
                width_after: None,
                source_row_geometry: None,
                background_color: None,
                height_twips,
                height_rule,
                cant_split: None,
                is_header: row.header.then_some(true),
            });
            let mut cells: Vec<TableCellNode> = Vec::with_capacity(row.cells.len());
            let mut column = 0usize;
            let mut covered_to_skip = 0u32;
            for mut cell in row.cells {
                let repeat = cell.repeated.clamp(1, MAX_REPEAT);
                for repetition in 0..repeat {
                    if column >= MAX_REPEAT as usize {
                        break;
                    }
                    let span_width = |span: u32| -> Option<i64> {
                        widths
                            .get(column..(column + span as usize).min(widths.len()))
                            .filter(|slice| !slice.is_empty())
                            .and_then(|slice| slice.iter().copied().sum::<Option<i64>>())
                    };
                    if cell.covered {
                        if covered_to_skip > 0 {
                            covered_to_skip -= 1;
                            continue;
                        }
                        let Some((rows_left, span, anchor_row, anchor_cell)) = merges.get(&column).copied() else {
                            column += 1;
                            continue;
                        };
                        // The merged cell above grows by this row.
                        if let Some(anchor) = rows
                            .get_mut(anchor_row)
                            .and_then(|row| row.cells.get_mut(anchor_cell))
                            .and_then(|cell| cell.style.as_mut())
                        {
                            anchor.row_span = Some(anchor.row_span.unwrap_or(1) + 1);
                        }
                        let format = self.format(scope, None);
                        let width = span_width(span);
                        cells.push(TableCellNode {
                            r#type: TableCellNodeType::TableCell,
                            nodes: vec![TableCellContentNode::Paragraph(paragraph_node(
                                vec![text_run(String::new(), &format.run, None)],
                                format.style.clone(),
                                format.run.style(),
                                None,
                            ))],
                            style: Some(TableCellStyle {
                                grid_span: (span > 1).then_some(i64::from(span)),
                                v_merge_continuation: Some(true),
                                width_twips: width,
                                preferred_width: width.map(|width| TablePreferredWidth::Dxa { value: width as f64 }),
                                ..empty_cell_style()
                            }),
                        });
                        if rows_left <= 1 {
                            merges.remove(&column);
                        } else {
                            merges.insert(column, (rows_left - 1, span, anchor_row, anchor_cell));
                        }
                        covered_to_skip = span - 1;
                        column += span as usize;
                        continue;
                    }
                    let span = cell.column_span.clamp(1, MAX_REPEAT);
                    let width = span_width(span);
                    let merged = cell.row_span > 1;
                    let mut style = self.cell_style(scope, cell.style.as_deref()).map(|style| (*style).clone());
                    if span > 1 || merged || width.is_some() {
                        let style = style.get_or_insert_with(empty_cell_style);
                        style.grid_span = (span > 1).then_some(i64::from(span));
                        style.row_span = merged.then_some(1);
                        style.width_twips = width;
                        style.preferred_width = width.map(|width| TablePreferredWidth::Dxa { value: width as f64 });
                    }
                    if merged {
                        merges.insert(column, (cell.row_span - 1, span, rows.len(), cells.len()));
                    } else {
                        merges.remove(&column);
                    }
                    let blocks = if repetition + 1 == repeat {
                        std::mem::take(&mut cell.blocks)
                    } else {
                        cell.blocks.clone()
                    };
                    let mut nodes: Vec<TableCellContentNode> = self.story(blocks, scope);
                    if nodes.is_empty() {
                        let format = self.format(scope, None);
                        nodes.push(TableCellContentNode::Paragraph(paragraph_node(
                            vec![text_run(String::new(), &format.run, None)],
                            format.style.clone(),
                            format.run.style(),
                            None,
                        )));
                    }
                    cells.push(TableCellNode {
                        r#type: TableCellNodeType::TableCell,
                        nodes,
                        style,
                    });
                    covered_to_skip = span - 1;
                    column += span as usize;
                }
            }
            rows.push(TableRowNode {
                r#type: TableRowNodeType::TableRow,
                cells,
                style: row_style,
            });
        }
        TableNode {
            review_rows: None,
            block_id: None,
            r#type: TableNodeType::Table,
            rows,
            style: Some(style),
            source_xml: None,
            source_text_patches: None,
        }
    }

    // ----- drawings -----

    fn shape(&mut self, shape: Shape, scope: Scope, out: &mut Vec<ParagraphChildNode>) {
        let mut shape = shape;
        if let Some(artwork) = rust_office_art::odf::render_shape(&shape, self.styles, scope) {
            let mut image = self.generated_image(&shape, scope, artwork.svg);
            image.paint_bounds = artwork.paint_bounds.map(|b| Box::new(crate::ImagePaintBounds {
                x_px: b.x_px, y_px: b.y_px, width_px: b.width_px, height_px: b.height_px,
                layout_width_px: b.layout_width_px, layout_height_px: b.layout_height_px,
            }));
            if !shape.text.is_empty() {
                image.content_nodes = Some(self.story(std::mem::take(&mut shape.text), scope));
            }
            out.push(ParagraphChildNode::Image(Box::new(image)));
            return;
        }
        match std::mem::take(&mut shape.kind) {
            ShapeKind::Image { href, fallback } => {
                // Prefer the bitmap rendition when the primary image is a vector format.
                let vector = href.to_ascii_lowercase().ends_with(".svg");
                let chosen = fallback.filter(|_| vector).unwrap_or(href);
                self.picture(&shape, chosen, scope, out);
            }
            ShapeKind::Object { href, mut replacement, math } => {
                let chart = href.as_deref().and_then(|h| self.document.objects.get(h))
                    .and_then(|object| object.document(&Limits::default()).ok())
                    .and_then(|document| if let Body::Chart(chart) = document.body { Some(chart) } else { None });
                if let Some(chart) = chart {
                    let placement = shape.placement();
                    let chart_data = rust_office_charts::odf::extract_chart_data(&chart);
                    let rendered = rust_office_charts::odf::render_chart(&chart, pixels(placement.width), pixels(placement.height));
                    if let Some(preview) = replacement.take() {
                        let before = out.len();
                        self.picture(&shape, preview, scope, out);
                        if let Some(ParagraphChildNode::Image(image)) = out.get_mut(before) {
                            image.chart_data = chart_data.map(Box::new);
                            image.chart_part_name = href.map(|h| format!("{h}/content.xml"));
                            image.semantic_kind = Some("chart".into());
                            return;
                        }
                    }
                    if let Some(rendered) = rendered {
                        let mut image = self.generated_image(&shape, scope, rendered.svg);
                        image.chart_data = Some(Box::new(rendered.data));
                        image.chart_part_name = href.map(|h| format!("{h}/content.xml"));
                        image.semantic_kind = Some("chart".into());
                        out.push(ParagraphChildNode::Image(Box::new(image)));
                        return;
                    }
                    let mut image = empty_image();
                    image.alt = shape.description.clone().or(chart.title);
                    image.width_px = Some(pixels(placement.width));
                    image.height_px = Some(pixels(placement.height));
                    image.floating = self.floating(&shape, scope);
                    image.chart_data = chart_data.map(Box::new);
                    image.chart_part_name = href.map(|h| format!("{h}/content.xml"));
                    image.semantic_kind = Some("chart".into());
                    out.push(ParagraphChildNode::Image(Box::new(image)));
                    self.warn("Chart artwork is unavailable; stored chart data is retained");
                    return;
                }
                let math = math.or_else(|| href.as_deref().and_then(|href| self.embedded_math(href)));
                let before = out.len();
                if let Some(replacement) = replacement {
                    self.picture(&shape, replacement, scope, out);
                }
                if let Some(math) = math {
                    // A formula is its MathML, with the preview image when there is one.
                    if out.len() == before {
                        let placement = shape.placement();
                        let mut image = empty_image();
                        image.alt = shape.description.clone().or_else(|| shape.title.clone());
                        image.width_px = Some(pixels(placement.width)).filter(|width| *width > 0.0);
                        image.height_px = Some(pixels(placement.height)).filter(|height| *height > 0.0);
                        image.floating = self.floating(&shape, scope);
                        out.push(ParagraphChildNode::Image(Box::new(image)));
                    }
                    if let Some(ParagraphChildNode::Image(image)) = out.get_mut(before) {
                        image.math_ml = Some(math);
                        image.semantic_kind = Some("formula".into());
                    }
                } else if out.len() > before {
                    self.warn("Embedded objects are imported as their preview image");
                } else if rust_odf::text::plain_text(&shape.text).trim().is_empty() {
                    self.warn("An embedded object without a preview image was skipped");
                } else {
                    self.warn("Drawing shapes are imported as text boxes without their outline");
                    let blocks = std::mem::take(&mut shape.text);
                    let nodes = self.story(blocks, scope);
                    out.push(self.text_box(&shape, scope, nodes));
                }
            }
            ShapeKind::TextBox => {
                let blocks = std::mem::take(&mut shape.text);
                let nodes = self.story(blocks, scope);
                out.push(self.text_box(&shape, scope, nodes));
            }
            ShapeKind::Group(children) => {
                for child in children {
                    self.shape(child, scope, out);
                }
            }
            ShapeKind::Table(table) => {
                let table = self.table(table, scope);
                out.push(self.text_box(&shape, scope, vec![DocNode::Table(table)]));
            }
            _ if rust_odf::text::plain_text(&shape.text).trim().is_empty() => {
                self.warn("Drawing shapes without text are not imported");
            }
            _ => {
                self.warn("Drawing shapes are imported as text boxes without their outline");
                let blocks = std::mem::take(&mut shape.text);
                let nodes = self.story(blocks, scope);
                out.push(self.text_box(&shape, scope, nodes));
            }
        }
    }

    /// The MathML of an embedded object that is a formula document.
    fn embedded_math(&self, href: &str) -> Option<String> {
        let content = self.document.objects.get(href)?.content.as_deref()?;
        // The root element follows the XML declaration and any comments.
        let mut rest = content;
        loop {
            rest = &rest[rest.find('<')?..];
            if !rest.starts_with("<?") && !rest.starts_with("<!") {
                break;
            }
            rest = &rest[rest.find('>')? + 1..];
        }
        let name = rest[1..].split(|ch: char| ch.is_whitespace() || matches!(ch, '>' | '/')).next()?;
        (name == "math" || name.ends_with(":math")).then(|| rest.trim_end().to_owned())
    }

    /// How a drawing that is not inline sits on the page; `None` for an inline one.
    fn floating(&self, shape: &Shape, scope: Scope) -> Option<ImageFloating> {
        if matches!(shape.anchor.as_deref(), None | Some("as-char")) {
            return None;
        }
        let props = self.styles.resolve_graphic(scope, Family::Graphic, shape.style.as_deref());
        let placement = shape.placement();
        let page = shape.anchor.as_deref() == Some("page");
        let horizontal = match props.horizontal_rel.as_deref() {
            Some("page") => "page",
            Some("page-content") => "margin",
            Some("char") => "character",
            Some(_) => "column",
            None if page => "page",
            None => "column",
        };
        let vertical = match props.vertical_rel.as_deref() {
            Some("page") => "page",
            Some("page-content") => "margin",
            Some(_) => "paragraph",
            None if page => "page",
            None => "paragraph",
        };
        let horizontal_align = match props.horizontal_pos.as_deref() {
            Some("left") => Some(ImageHorizontalAlign::Left),
            Some("center") => Some(ImageHorizontalAlign::Center),
            Some("right") => Some(ImageHorizontalAlign::Right),
            _ => None,
        }
        .filter(|_| placement.x.0 == 0);
        let vertical_align = match props.vertical_pos.as_deref() {
            Some("top") => Some(ImageVerticalAlign::Top),
            Some("middle") => Some(ImageVerticalAlign::Center),
            Some("bottom") => Some(ImageVerticalAlign::Bottom),
            _ => None,
        }
        .filter(|_| placement.y.0 == 0);
        let (wrap_type, wrap_text) = match props.wrap.as_deref() {
            Some("none") => (ImageWrapType::TopAndBottom, None),
            Some("run-through") => (ImageWrapType::WrapNone, None),
            Some("left") => (ImageWrapType::Square, Some(ImageWrapText::Left)),
            Some("right") => (ImageWrapType::Square, Some(ImageWrapText::Right)),
            Some("biggest" | "dynamic") => (ImageWrapType::Square, Some(ImageWrapText::Largest)),
            _ => (ImageWrapType::Square, Some(ImageWrapText::BothSides)),
        };
        Some(ImageFloating {
            x_px: horizontal_align.is_none().then(|| pixels(placement.x)),
            y_px: vertical_align.is_none().then(|| pixels(placement.y)),
            horizontal_align,
            vertical_align,
            horizontal_relative_to: Some(horizontal.into()),
            vertical_relative_to: Some(vertical.into()),
            dist_l_px: Some(0.0),
            dist_r_px: Some(0.0),
            dist_t_px: Some(0.0),
            dist_b_px: Some(0.0),
            wrap_type: Some(wrap_type),
            wrap_text,
            behind_document: Some(props.behind_text == Some(true)),
            z_index: Some(i64::from(shape.z_index.unwrap_or(0).max(0)) + 1),
        })
    }

    fn generated_image(&self, shape: &Shape, scope: Scope, svg: String) -> ImageRunNode {
        let placement = shape.placement();
        let mut image = empty_image();
        image.alt = shape.description.clone().or_else(|| shape.title.clone());
        image.width_px = Some(pixels(placement.width));
        image.height_px = Some(pixels(placement.height));
        image.content_type = Some("image/svg+xml".into());
        image.data = Some(svg.into_bytes());
        image.floating = self.floating(shape, scope);
        image
    }

    fn picture(&mut self, shape: &Shape, href: String, scope: Scope, out: &mut Vec<ParagraphChildNode>) {
        // The first use of an image takes its bytes; a repeat is filled in
        // once the model is complete.
        let (content_type, data) = match self.placed.get(&href) {
            Some(content_type) => {
                self.repeated_images |= content_type.is_some();
                (*content_type, None)
            }
            None => {
                let data = self
                    .binaries
                    .remove(&href)
                    .and_then(|bytes| Some((image_content_type(image_extension(&bytes, &href)?), bytes)));
                let content_type = data.as_ref().map(|(content_type, _)| *content_type);
                self.placed.insert(href.clone(), content_type);
                (content_type, data.map(|(_, bytes)| bytes))
            }
        };
        let Some(content_type) = content_type else {
            self.warn("An image that is linked or in an unknown format was skipped");
            return;
        };
        let placement = shape.placement();
        // Fall back to one inch when the frame carries no size.
        let size = |length: Length| if length.0 > 0 { pixels(length) } else { 96.0 };
        let mut image = empty_image();
        image.alt = shape.description.clone().or_else(|| shape.title.clone());
        image.width_px = Some(size(placement.width));
        image.height_px = Some(size(placement.height));
        image.part_name = Some(href);
        image.content_type = Some(content_type.into());
        image.data = data;
        image.floating = self.floating(shape, scope);
        out.push(ParagraphChildNode::Image(Box::new(image)));
    }

    /// A text box: a drawing whose content is a story of its own.
    fn text_box(&mut self, shape: &Shape, scope: Scope, nodes: Vec<DocNode>) -> ParagraphChildNode {
        let placement = shape.placement();
        let min_height = self
            .styles
            .resolve_graphic(scope, Family::Graphic, shape.style.as_deref())
            .min_height
            .map_or(1.0, pixels);
        let mut image = empty_image();
        image.alt = shape.description.clone().or_else(|| shape.title.clone());
        image.width_px = Some(pixels(placement.width).max(1.0));
        image.height_px = Some(pixels(placement.height).max(min_height));
        image.floating = self.floating(shape, scope);
        image.content_nodes = Some(nodes);
        image.semantic_kind = Some("text_box".into());
        image.synthetic_text_box = Some(true);
        ParagraphChildNode::Image(Box::new(image))
    }

    // ----- sections, headers and footers -----

    /// The header or footer `kind` of the master page at `index`, converted on first use.
    fn master_story(&mut self, index: usize, kind: u8) -> Option<Rc<Story>> {
        if let Some(cached) = self.stories.get(&(index, kind)) {
            return cached.clone();
        }
        let master = &mut self.masters[index];
        let blocks = match kind {
            0 => master.header.take(),
            1 => master.header_first.take(),
            2 => master.header_left.take(),
            3 => master.footer.take(),
            4 => master.footer_first.take(),
            _ => master.footer_left.take(),
        };
        let story = blocks.map(|blocks| {
            let footer = usize::from(kind >= 3);
            self.story_count[footer] += 1;
            Rc::new(Story {
                part_name: format!(
                    "styles.xml/{}{}",
                    if footer == 1 { "footer" } else { "header" },
                    self.story_count[footer]
                ),
                nodes: self.story(blocks, Scope::Styles),
            })
        });
        self.stories.insert((index, kind), story.clone());
        story
    }

    /// Page setup and the headers and footers of content laid out with a master page.
    fn section(&mut self, master_name: &str) -> Rc<SectionLayout> {
        if let Some(cached) = self.sections.get(master_name) {
            return Rc::clone(cached);
        }
        let position = |masters: &[MasterPage], name: &str| masters.iter().position(|master| master.name == name);
        let master = position(&self.masters, master_name);
        // A master page that hands over to another one after its first page
        // is a "different first page".
        let following = master.and_then(|index| {
            let master = &self.masters[index];
            master
                .next
                .as_deref()
                .filter(|next| *next != master.name)
                .and_then(|next| position(&self.masters, next))
        });
        let (body, title) = match following {
            Some(following) => (Some(following), master),
            None => (master, None),
        };
        let mut headers = Vec::new();
        let mut footers = Vec::new();
        let mut title_page = false;
        if let Some(body) = body {
            let first_header = title
                .and_then(|title| self.master_story(title, 0))
                .or_else(|| self.master_story(body, 1));
            let first_footer = title
                .and_then(|title| self.master_story(title, 3))
                .or_else(|| self.master_story(body, 4));
            title_page = title.is_some() || first_header.is_some() || first_footer.is_some();
            let references = [
                (false, "default", self.master_story(body, 0)),
                (false, "first", first_header),
                (false, "even", self.master_story(body, 2)),
                (true, "default", self.master_story(body, 3)),
                (true, "first", first_footer),
                (true, "even", self.master_story(body, 5)),
            ];
            for (footer, kind, story) in references {
                let Some(story) = story else {
                    continue;
                };
                self.even_and_odd_headers |= kind == "even";
                if footer {
                    footers.push(FooterSection {
                        part_name: story.part_name.clone(),
                        reference_type: Some(kind.into()),
                        nodes: story.nodes.clone(),
                    });
                } else {
                    headers.push(HeaderSection {
                        part_name: story.part_name.clone(),
                        reference_type: Some(kind.into()),
                        nodes: story.nodes.clone(),
                    });
                }
            }
        }
        let layout = body
            .and_then(|body| self.masters[body].page_layout.as_deref())
            .and_then(|name| self.styles.page_layouts.get(name));
        // A4 portrait with 2 cm margins when the file declares nothing.
        let width = layout.and_then(|layout| layout.width).map_or(11_906, Length::twips);
        let height = layout.and_then(|layout| layout.height).map_or(16_838, Length::twips);
        let margin = |value: Option<Length>| value.map_or(1_134, Length::twips);
        let (top, bottom) = (
            margin(layout.and_then(|layout| layout.margin_top)),
            margin(layout.and_then(|layout| layout.margin_bottom)),
        );
        // Page setup is the one part of the model that is stored as Word's
        // section properties, so it is stated in that form.
        let properties = format!(
            r#"<w:sectPr><w:pgSz w:w="{width}" w:h="{height}"{}/><w:pgMar w:top="{top}" w:right="{}" w:bottom="{bottom}" w:left="{}" w:header="{top}" w:footer="{bottom}" w:gutter="0"/>{}</w:sectPr>"#,
            if width > height { r#" w:orient="landscape""# } else { "" },
            margin(layout.and_then(|layout| layout.margin_right)),
            margin(layout.and_then(|layout| layout.margin_left)),
            if title_page { "<w:titlePg/>" } else { "" },
        );
        let section = Rc::new(SectionLayout {
            properties,
            headers,
            footers,
        });
        self.sections.insert(master_name.to_owned(), Rc::clone(&section));
        section
    }

    /// The master page a block switches to, as a paragraph or table style can request.
    fn master_of(&mut self, block: &Block) -> Option<Rc<str>> {
        match block {
            Block::Paragraph(paragraph) => self.format(Scope::Content, Some(paragraph.style.as_deref()?)).master.clone(),
            Block::Table(table) => self
                .styles
                .ancestors(Scope::Content, Family::Table, table.style.as_deref()?)
                .find_map(|style| style.master_page.as_deref())
                .filter(|master| !master.is_empty())
                .map(Rc::from),
            Block::List(list) => {
                let first = list.items.first()?.blocks.first()?;
                self.master_of(first)
            }
            Block::Section(section) => {
                let first = section.blocks.first()?;
                self.master_of(first)
            }
            Block::Shape(_) => None,
        }
    }

    fn body(&mut self, blocks: Vec<Block>) -> (Vec<DocNode>, Vec<DocumentSection>) {
        let mut master: Rc<str> = self
            .masters
            .iter()
            .find(|master| master.name == "Standard")
            .or(self.masters.first())
            .map(|master| Rc::from(master.name.as_str()))
            .unwrap_or_else(|| Rc::from(""));
        let mut nodes: Vec<DocNode> = Vec::with_capacity(blocks.len());
        let mut sections = Vec::new();
        let mut section_start = 0usize;
        // Sections are transparent here so a page-style switch inside one still splits the body.
        let mut stack = vec![blocks.into_iter()];
        while let Some(current) = stack.last_mut() {
            let Some(block) = current.next() else {
                stack.pop();
                continue;
            };
            let block = match block {
                Block::Section(section) => {
                    stack.push(section.blocks.into_iter());
                    continue;
                }
                block => block,
            };
            if let Some(next) = self.master_of(&block).filter(|next| *next != master) {
                if !nodes.is_empty() {
                    let layout = self.section(&master);
                    sections.push(document_section(section_start, &layout));
                    section_start = nodes.len();
                }
                master = next;
            }
            self.block(block, Scope::Content, None, &mut nodes);
        }
        self.flush_pending(Scope::Content, &mut nodes);
        if nodes.is_empty() {
            let format = self.format(Scope::Content, None);
            nodes.push(DocNode::Paragraph(paragraph_node(
                vec![text_run(String::new(), &format.run, None)],
                format.style.clone(),
                format.run.style(),
                None,
            )));
        }
        let layout = self.section(&master);
        sections.push(document_section(section_start, &layout));
        (nodes, sections)
    }

    // ----- model -----

    fn style_definitions(&mut self) -> Vec<ParagraphStyleDefinition> {
        let styles = self.styles;
        let mut definitions = Vec::with_capacity(self.named.len() + 1);
        if !self.named.contains_key("Standard") {
            let paragraph = styles.resolve_paragraph(Scope::Styles, Family::Paragraph, None);
            let text = styles.resolve_text(Scope::Styles, Family::Paragraph, None);
            definitions.push(style_definition("Normal", "Normal", None, 0, &paragraph, &text, true));
        }
        for style in styles.common_styles().filter(|style| style.family == Family::Paragraph) {
            let Some(named) = self.named.get(style.name.as_str()) else {
                continue;
            };
            let paragraph = styles.resolve_paragraph(Scope::Styles, Family::Paragraph, Some(&style.name));
            let text = styles.resolve_text(Scope::Styles, Family::Paragraph, Some(&style.name));
            let parent = style
                .parent
                .as_deref()
                .and_then(|parent| self.named.get(parent))
                .map(|parent| parent.id.to_string());
            definitions.push(style_definition(
                &named.id,
                &named.name,
                parent,
                named.heading,
                &paragraph,
                &text,
                style.name == "Standard",
            ));
        }
        definitions
    }

    fn build(mut self, body: Vec<Block>) -> DocModel {
        let (nodes, sections) = self.body(body);
        let last = sections.last();
        let section_properties_xml = last.and_then(|section| section.section_properties_xml.clone());
        let header_sections = last.map(|section| section.header_sections.clone()).unwrap_or_default();
        let footer_sections = last.map(|section| section.footer_sections.clone()).unwrap_or_default();
        let paragraph_styles = self.style_definitions();
        let numbering_definitions = (!self.instances.is_empty()).then(|| NumberingDefinitionSet {
            abstracts: std::mem::take(&mut self.abstracts),
            instances: std::mem::take(&mut self.instances),
        });
        let part_names = ["document", "footnotes", "endnotes", "comments"]
            .into_iter()
            .map(|kind| (kind.to_owned(), CONTENT_PART.to_owned()))
            .collect();
        let mut model = DocModel {
            nodes,
            metadata: DocModelMetadata {
                document_id: None,
                source_part_names: Some(part_names),
                source_parts: 1,
                warnings: Vec::new(),
                document_page_count: self.document.metadata.page_count.map(i64::from),
                document_open_tag: None,
                document_background_color: None,
                section_properties_xml,
                sections: Some(sections),
                header_sections,
                footer_sections,
                paragraph_styles,
                default_paragraph_style_id: Some("Normal".into()),
                default_tab_stop_twips: Some(709),
                document_settings_imported: None,
                line_spacing_compatibility_imported: None,
                numbering_definitions,
                compatibility: self.even_and_odd_headers.then(|| DocumentCompatibilitySettings {
                    adjust_line_height_in_table: None,
                    no_leading: None,
                    suppress_top_spacing_wp: None,
                    truncate_font_heights_like_wp6: None,
                    no_extra_line_spacing: None,
                    space_for_ul: None,
                    suppress_bottom_spacing: None,
                    compatibility_mode: None,
                    enable_open_type_features: None,
                    suppress_spacing_before_after_page_break: None,
                    use_printer_metrics: None,
                    use_fixed_html_paragraph_spacing: None,
                    do_not_break_wrapped_tables: None,
                    do_not_break_constrained_forced_table: None,
                    even_and_odd_headers: Some(true),
                }),
                footnotes: Some(std::mem::take(&mut self.footnotes)).filter(|notes| !notes.is_empty()),
                endnotes: Some(std::mem::take(&mut self.endnotes)).filter(|notes| !notes.is_empty()),
                comments: Some(std::mem::take(&mut self.comments)).filter(|comments| !comments.is_empty()),
            },
        };
        if self.repeated_images {
            share_repeated_images(&mut model);
        }
        model.metadata.warnings = self.warnings.into_iter().collect();
        model
    }
}

/// Comment ranges of the paragraph being built.
struct Ranges {
    /// Comments still open, with the child they started at.
    open: Vec<(i64, usize)>,
    closed: Vec<CommentRange>,
}

impl Ranges {
    /// Starts a paragraph inside every comment a previous one left open.
    fn open(comments: &[(String, i64)]) -> Self {
        Self {
            open: comments.iter().map(|(_, id)| (*id, 0)).collect(),
            closed: Vec::new(),
        }
    }

    fn start(&mut self, id: i64, child: usize) {
        self.open.push((id, child));
    }

    fn end(&mut self, id: i64, child: usize) {
        if let Some(index) = self.open.iter().position(|(open, _)| *open == id) {
            let (_, start) = self.open.remove(index);
            self.closed.push(CommentRange {
                comment_id: id,
                child_start: start,
                child_end: child,
                ends_here: true,
            });
        }
    }

    /// Ends the paragraph: comments still open cover the rest of it.
    fn close(mut self, children: usize) -> Option<Vec<CommentRange>> {
        for (id, start) in self.open {
            self.closed.push(CommentRange {
                comment_id: id,
                child_start: start,
                child_end: children,
                ends_here: false,
            });
        }
        (!self.closed.is_empty()).then_some(self.closed)
    }
}

fn document_section(start: usize, layout: &SectionLayout) -> DocumentSection {
    DocumentSection {
        start_node_index: start as i64,
        section_properties_xml: Some(layout.properties.clone()),
        header_sections: layout.headers.clone(),
        footer_sections: layout.footers.clone(),
    }
}

/// The heading level a named paragraph style implies, 0 for none.
fn style_heading(styles: &StyleSheet, name: &str, named: &HashMap<&str, NamedStyle>) -> u8 {
    for style in styles.ancestors(Scope::Styles, Family::Paragraph, name) {
        let level = style
            .default_outline_level
            .filter(|level| *level >= 1)
            .map(|level| level.min(9))
            .or_else(|| heading_level_in_name(&named.get(style.name.as_str())?.name));
        if let Some(level) = level {
            return level;
        }
    }
    0
}

fn style_definition(
    id: &str,
    name: &str,
    based_on_id: Option<String>,
    heading: u8,
    paragraph: &ParagraphProps,
    text: &TextProps,
    is_default: bool,
) -> ParagraphStyleDefinition {
    let style = paragraph_style(paragraph);
    ParagraphStyleDefinition {
        id: id.to_owned(),
        name: name.to_owned(),
        based_on_id,
        next_style_id: None,
        align: style.align,
        snap_to_grid: None,
        source_has_snap_to_grid: None,
        text_alignment: None,
        source_has_text_alignment: None,
        heading_level: heading_level(heading),
        numbering: None,
        spacing: style.spacing,
        indent: style.indent,
        background_color: style.background_color,
        borders: style.borders,
        tab_stops: style.tab_stops,
        contextual_spacing: None,
        keep_next: style.keep_next,
        keep_lines: style.keep_lines,
        widow_control: None,
        page_break_before: style.page_break_before,
        run_style: text_style(text),
        ui_priority: None,
        is_default: is_default.then_some(true),
        is_primary: Some(true),
    }
}

/// Left and hanging indent in twips of a labelled paragraph at a list level.
fn level_indent(ilvl: u8, margin_left: Option<Length>, text_indent: Option<Length>) -> (i64, i64) {
    (
        margin_left.map_or(720 * (i64::from(ilvl) + 1), Length::twips),
        text_indent.map_or(360, |indent| (-indent.twips()).max(0)),
    )
}

/// A list style as a numbering definition. `indent_by_default` gives levels
/// that state no indent the usual list indent; headings numbered by the
/// outline style keep theirs.
fn abstract_definition(id: i64, list: &ListStyle, indent_by_default: bool) -> NumberingAbstractDefinition {
    let mut levels = Vec::with_capacity(list.levels.len().min(9));
    for ilvl in 0u8..9 {
        let Some(level) = list.levels.get(&(ilvl + 1)) else {
            continue;
        };
        let (format, text, start) = match &level.label {
            ListLabel::Bullet(bullet) => ("bullet", bullet.clone(), 1),
            ListLabel::Image(_) => ("bullet", "•".to_owned(), 1),
            ListLabel::Number {
                format,
                start,
                display_levels,
            } => {
                let number_format = match format.as_str() {
                    "" => "none",
                    "a" => "lowerLetter",
                    "A" => "upperLetter",
                    "i" => "lowerRoman",
                    "I" => "upperRoman",
                    _ => "decimal",
                };
                let mut text = level.prefix.clone();
                if !format.is_empty() {
                    let first = (ilvl + 2).saturating_sub((*display_levels).max(1)).max(1);
                    for number in first..=ilvl + 1 {
                        if number > first {
                            text.push('.');
                        }
                        text.push('%');
                        text.push(char::from(b'0' + number));
                    }
                }
                text.push_str(&level.suffix);
                // Lists number up to 32767; LibreOffice shows 1 for a larger start.
                (number_format, text, if *start > 32_767 { 1 } else { *start })
            }
        };
        let bullet = format == "bullet";
        let indent = (indent_by_default || level.margin_left.is_some() || level.text_indent.is_some()).then(|| {
            let (left, hanging) = level_indent(ilvl, level.margin_left, level.text_indent);
            ParagraphIndent {
                left_twips: Some(left),
                right_twips: None,
                first_line_twips: None,
                hanging_twips: Some(hanging),
            }
        });
        levels.push(NumberingLevelDefinition {
            ilvl: i64::from(ilvl),
            start: Some(i64::from(start)),
            format: Some(format.into()),
            text: Some(text),
            suffix: None,
            lvl_restart: None,
            is_lgl: None,
            indent,
            run_style: text_style(&level.text),
            bullet_font_family: level.text.font.clone().filter(|_| bullet),
            bullet_color: level.text.color.as_deref().filter(|_| bullet).map(hex),
            picture_bullet_id: None,
            picture_bullet: None,
        });
    }
    NumberingAbstractDefinition {
        abstract_num_id: id,
        levels,
    }
}

fn empty_cell_style() -> TableCellStyle {
    TableCellStyle {
        background_color: None,
        grid_span: None,
        row_span: None,
        v_merge_continuation: None,
        width_twips: None,
        preferred_width: None,
        source_width: None,
        margin_twips: None,
        vertical_align: None,
        text_direction: None,
        source_text_direction: None,
        no_wrap: None,
        source_no_wrap: None,
        borders: None,
    }
}

fn empty_image() -> ImageRunNode {
    ImageRunNode {
        model_3d: None,
        presentation: None,
        chart_part_name: None,
        chart_data: None,
        paint_bounds: None,
        math_ml: None,
        content_nodes: None,
        semantic_kind: None,
        block_id: None,
        r#type: ImageRunNodeType::Image,
        src: None,
        alt: None,
        width_px: None,
        height_px: None,
        part_name: None,
        content_type: None,
        data: None,
        source_xml: None,
        crop: None,
        css_filter: None,
        css_opacity: None,
        floating: None,
        synthetic_text_box: None,
        text_box_text: None,
    }
}

/// Calls `visit` for every image of the model, text boxes' content included.
fn for_each_image(model: &mut DocModel, visit: &mut dyn FnMut(&mut ImageRunNode)) {
    fn paragraph(node: &mut ParagraphNode, visit: &mut dyn FnMut(&mut ImageRunNode)) {
        for child in &mut node.children {
            if let ParagraphChildNode::Image(image) = child {
                visit(image);
                if let Some(content) = &mut image.content_nodes {
                    nodes(content, visit);
                }
            }
        }
    }
    fn table(node: &mut TableNode, visit: &mut dyn FnMut(&mut ImageRunNode)) {
        for cell in node.rows.iter_mut().flat_map(|row| &mut row.cells) {
            for content in &mut cell.nodes {
                match content {
                    TableCellContentNode::Paragraph(node) => paragraph(node, visit),
                    TableCellContentNode::Table(node) => table(node, visit),
                }
            }
        }
    }
    fn nodes(content: &mut [DocNode], visit: &mut dyn FnMut(&mut ImageRunNode)) {
        for node in content {
            match node {
                DocNode::Paragraph(node) => paragraph(node, visit),
                DocNode::Table(node) => table(node, visit),
            }
        }
    }
    nodes(&mut model.nodes, visit);
    let metadata = &mut model.metadata;
    for header in metadata
        .header_sections
        .iter_mut()
        .chain(metadata.sections.iter_mut().flatten().flat_map(|section| &mut section.header_sections))
    {
        nodes(&mut header.nodes, visit);
    }
    for footer in metadata
        .footer_sections
        .iter_mut()
        .chain(metadata.sections.iter_mut().flatten().flat_map(|section| &mut section.footer_sections))
    {
        nodes(&mut footer.nodes, visit);
    }
    for note in metadata.footnotes.iter_mut().chain(&mut metadata.endnotes).flatten() {
        if let Some(content) = &mut note.nodes {
            nodes(content, visit);
        }
    }
    for comment in metadata.comments.iter_mut().flatten() {
        if let Some(content) = &mut comment.nodes {
            nodes(content, visit);
        }
    }
}

/// Gives every later use of an image the bytes its first use holds.
fn share_repeated_images(model: &mut DocModel) {
    let mut wanted: HashSet<String> = HashSet::new();
    for_each_image(model, &mut |image| {
        if image.data.is_none() && image.content_nodes.is_none() {
            wanted.extend(image.part_name.clone());
        }
    });
    let mut bytes: HashMap<String, Vec<u8>> = HashMap::new();
    for_each_image(model, &mut |image| {
        if let (Some(name), Some(data)) = (&image.part_name, &image.data) {
            if wanted.contains(name) && !bytes.contains_key(name) {
                bytes.insert(name.clone(), data.clone());
            }
        }
    });
    for_each_image(model, &mut |image| {
        if image.data.is_none() {
            image.data = image.part_name.as_ref().and_then(|name| bytes.get(name)).cloned();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_levels_come_from_whole_numbers_in_style_names() {
        assert_eq!(heading_level_in_name("Heading 1"), Some(1));
        assert_eq!(heading_level_in_name("heading9"), Some(9));
        assert_eq!(heading_level_in_name("Subheading 2"), Some(2));
        assert_eq!(heading_level_in_name("Heading 10"), None);
        assert_eq!(heading_level_in_name("Heading"), None);
        assert_eq!(heading_level_in_name("Heading 0"), None);
        assert_eq!(heading_level_in_name("Text body"), None);
    }

    #[test]
    fn relative_font_sizes_resolve_against_the_surrounding_text() {
        let base = TextProps {
            size: Some(FontSize::Points(12.0)),
            bold: Some(true),
            ..TextProps::default()
        };
        let span = TextProps {
            size: Some(FontSize::Percent(50.0)),
            ..TextProps::default()
        };
        let resolved = overlay(span, &base);
        assert_eq!(resolved.size, Some(FontSize::Points(6.0)));
        assert_eq!(resolved.bold, Some(true));
        let style = text_style(&resolved).unwrap();
        assert_eq!(style.font_size_pt, Some(6.0));
        assert!(text_style(&TextProps::default()).is_none());
    }
}

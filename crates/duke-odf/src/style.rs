//! Styles: formatting properties, inheritance, lists, page layouts and master pages.

use std::collections::{BTreeMap, HashMap};


use crate::draw::{self, Shape};
use crate::text::{self, Block, Ctx};
use crate::units::{parse_bool, parse_color, parse_length, parse_number, parse_percent, Length};
use crate::xml::{
    attr, child, elements, is, owned, Node, CHART, DRAW, FO, LO_EXT, NUMBER, OFFICE, PRESENTATION,
    STYLE, SVG, TABLE, TEXT,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Family {
    Text,
    Paragraph,
    Graphic,
    Presentation,
    DrawingPage,
    Table,
    TableColumn,
    TableRow,
    TableCell,
    Section,
    Chart,
    Other,
}

/// The number of [`Family`] values.
const FAMILIES: usize = 12;

impl Family {
    fn index(self) -> usize {
        self as usize
    }

    fn parse(value: &str) -> Self {
        match value {
            "text" => Self::Text,
            "paragraph" => Self::Paragraph,
            "graphic" => Self::Graphic,
            "presentation" => Self::Presentation,
            "drawing-page" => Self::DrawingPage,
            "table" => Self::Table,
            "table-column" => Self::TableColumn,
            "table-row" => Self::TableRow,
            "table-cell" => Self::TableCell,
            "section" => Self::Section,
            "chart" => Self::Chart,
            _ => Self::Other,
        }
    }
}

/// Automatic styles are private to the part that declares them: `P1` in
/// `content.xml` and `P1` in `styles.xml` are different styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// The document body.
    Content,
    /// Master pages, headers and footers.
    Styles,
}

macro_rules! properties {
    ($(#[$meta:meta])* $name:ident { $($(#[$field_meta:meta])* $field:ident : $ty:ty),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Debug, Default, PartialEq)]
        pub struct $name {
            $($(#[$field_meta])* pub $field: Option<$ty>,)*
        }

        impl $name {
            /// Fills every unset property from `base`.
            pub fn inherit(&mut self, base: &Self) {
                $(if self.$field.is_none() {
                    self.$field = base.$field.clone();
                })*
            }

            pub fn is_empty(&self) -> bool {
                $(self.$field.is_none())&&*
            }
        }
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FontSize {
    Points(f64),
    /// Relative to the inherited size.
    Percent(f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    End,
    Left,
    Right,
    Center,
    Justify,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineHeight {
    Percent(f64),
    Exact(Length),
    AtLeast(Length),
    /// Extra space between lines.
    Leading(Length),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Break {
    None,
    Page,
    Column,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKind {
    Left,
    Center,
    Right,
    Decimal,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TabStop {
    pub position: Length,
    pub kind: TabKind,
    pub leader: Option<char>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Border {
    pub width: Length,
    /// `none`, `solid`, `double`, `dashed`, `dotted`, ...
    pub style: String,
    pub color: Option<String>,
}

impl Border {
    pub fn is_visible(&self) -> bool {
        self.style != "none" && self.style != "hidden" && self.width.0 > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fill {
    None,
    Solid,
    Gradient,
    Bitmap,
    Hatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stroke {
    None,
    Solid,
    Dash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerticalAlign {
    Top,
    Middle,
    Bottom,
}

properties! {
    /// `style:text-properties`.
    TextProps {
        /// The resolved family name for Latin text.
        font: String,
        font_asian: String,
        font_complex: String,
        size: FontSize,
        bold: bool,
        italic: bool,
        underline: bool,
        strike: bool,
        /// `RRGGBB`.
        color: String,
        /// `RRGGBB`, or empty for an explicit transparent background.
        background: String,
        uppercase: bool,
        small_caps: bool,
        /// Baseline shift in percent of the font size: positive is superscript.
        position: f64,
        letter_spacing: Length,
        /// A BCP 47 tag such as `en-US`.
        language: String,
        language_asian: String,
        language_complex: String,
        hidden: bool,
    }
}

properties! {
    /// `style:paragraph-properties`.
    ParagraphProps {
        align: Align,
        margin_left: Length,
        margin_right: Length,
        margin_top: Length,
        margin_bottom: Length,
        text_indent: Length,
        line_height: LineHeight,
        break_before: Break,
        break_after: Break,
        keep_with_next: bool,
        keep_together: bool,
        /// `RRGGBB`, or empty for an explicit transparent background.
        background: String,
        rtl: bool,
        tab_stops: Vec<TabStop>,
        border_top: Border,
        border_bottom: Border,
        border_left: Border,
        border_right: Border,
    }
}

properties! {
    /// `style:graphic-properties` and `style:drawing-page-properties`.
    GraphicProps {
        fill: Fill,
        fill_color: String,
        fill_gradient: String,
        fill_image: String,
        /// 0 (transparent) to 1 (opaque).
        opacity: f64,
        stroke: Stroke,
        stroke_color: String,
        stroke_opacity: f64,
        stroke_width: Length,
        stroke_dash: String,
        marker_start: String,
        marker_end: String,
        vertical_align: VerticalAlign,
        padding_left: Length,
        padding_top: Length,
        padding_right: Length,
        padding_bottom: Length,
        auto_grow_height: bool,
        auto_grow_width: bool,
        fit_to_size: bool,
        /// `fo:wrap-option`: whether text wraps inside the shape.
        wrap_text: bool,
        /// `style:wrap`: `none`, `left`, `right`, `parallel`, `dynamic`, `biggest` or `run-through`.
        wrap: String,
        /// `style:run-through="background"`.
        behind_text: bool,
        horizontal_pos: String,
        horizontal_rel: String,
        vertical_pos: String,
        vertical_rel: String,
        /// `RRGGBB`, or empty for an explicit transparent background.
        background: String,
        border: Border,
        min_height: Length,
        /// `fo:clip` insets: top, right, bottom, left.
        clip: [Length; 4],
        background_visible: bool,
        background_objects_visible: bool,
        /// Whether a slide shows its master page's footer, slide number and date fields.
        display_footer: bool,
        display_page_number: bool,
        display_date_time: bool,
        hidden: bool,
    }
}

properties! {
    /// Table, column, row and cell properties.
    TableProps {
        number_style: String,
        width: Length,
        align: String,
        margin_left: Length,
        column_width: Length,
        /// The weight of a `style:rel-column-width` such as `4915*`.
        rel_column_width: f64,
        row_height: Length,
        min_row_height: Length,
        /// `RRGGBB`, or empty for an explicit transparent background.
        background: String,
        border_top: Border,
        border_bottom: Border,
        border_left: Border,
        border_right: Border,
        padding: Length,
        vertical_align: VerticalAlign,
    }
}

/// Every property group a style can carry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Props {
    pub text: TextProps,
    pub paragraph: ParagraphProps,
    pub graphic: GraphicProps,
    pub table: TableProps,
    pub chart: BTreeMap<String, String>,
}

impl Props {
    /// Fills unset properties from `base`, resolving a relative font size against it.
    pub fn inherit(&mut self, base: &Props) {
        self.text.inherit_resolving_size(&base.text);
        self.paragraph.inherit(&base.paragraph);
        self.graphic.inherit(&base.graphic);
        self.table.inherit(&base.table);
        for (name, value) in &base.chart {
            self.chart.entry(name.clone()).or_insert_with(|| value.clone());
        }
    }
}

impl TextProps {
    /// Fills unset properties from `base`, resolving a relative font size against it.
    pub fn inherit_resolving_size(&mut self, base: &TextProps) {
        if let Some(FontSize::Percent(percent)) = self.size {
            match base.size {
                Some(FontSize::Points(points)) => {
                    self.size = Some(FontSize::Points(points * percent / 100.0));
                }
                Some(FontSize::Percent(outer)) => {
                    self.size = Some(FontSize::Percent(outer * percent / 100.0));
                }
                None => {}
            }
        }
        self.inherit(base);
    }

    /// The size in points once inheritance has resolved it.
    pub fn points(&self) -> Option<f64> {
        match self.size {
            Some(FontSize::Points(points)) => Some(points),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub name: String,
    pub display_name: Option<String>,
    pub family: Family,
    pub parent: Option<String>,
    pub list_style: Option<String>,
    /// A paragraph style with a master page starts a new page using it.
    pub master_page: Option<String>,
    pub class: Option<String>,
    pub default_outline_level: Option<u8>,
    /// Declared in `office:automatic-styles`.
    pub automatic: bool,
    /// Properties this style sets itself, before inheritance.
    pub props: Props,
    /// A list style declared inside the style's graphic properties.
    pub list: Option<ListStyle>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ListLabel {
    Bullet(String),
    Number {
        /// `1`, `a`, `A`, `i`, `I`, or empty for no visible number.
        format: String,
        start: u32,
        /// How many levels the label shows, as in `1.2.3`.
        display_levels: u8,
    },
    Image(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListLevel {
    pub label: ListLabel,
    pub prefix: String,
    pub suffix: String,
    /// A text-family style for the label.
    pub text_style: Option<String>,
    pub text: TextProps,
    /// Where the paragraph's text starts.
    pub margin_left: Option<Length>,
    /// Negative for a hanging label.
    pub text_indent: Option<Length>,
    /// Bullet size in percent of the text size.
    pub relative_size: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListStyle {
    pub name: String,
    /// Keyed by level, starting at 1.
    pub levels: BTreeMap<u8, ListLevel>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageLayout {
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub landscape: bool,
    pub margin_top: Option<Length>,
    pub margin_bottom: Option<Length>,
    pub margin_left: Option<Length>,
    pub margin_right: Option<Length>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MasterPage {
    pub name: String,
    pub display_name: Option<String>,
    pub page_layout: Option<String>,
    /// A drawing-page style.
    pub style: Option<String>,
    pub next: Option<String>,
    pub header: Option<Vec<Block>>,
    pub footer: Option<Vec<Block>>,
    pub header_first: Option<Vec<Block>>,
    pub footer_first: Option<Vec<Block>>,
    pub header_left: Option<Vec<Block>>,
    pub footer_left: Option<Vec<Block>>,
    /// Background and placeholder shapes of a presentation or drawing master.
    pub shapes: Vec<Shape>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    /// `linear`, `axial`, `radial`, `ellipsoid`, `square` or `rectangular`.
    pub kind: String,
    pub start_color: Option<String>,
    pub end_color: Option<String>,
    /// Degrees.
    pub angle: f64,
}

/// Named styles of one family by name, in a stable order. A style is large
/// and map nodes shift their entries as they fill, so styles are boxed.
type CommonStyles = BTreeMap<String, Box<Style>>;
/// Automatic styles of one family by name. A document can hold thousands, each looked up by name.
type AutomaticStyles = HashMap<String, Box<Style>>;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StyleSheet {
    /// By [`Family::index`].
    common: [CommonStyles; FAMILIES],
    /// By scope, then by [`Family::index`].
    automatic: [[AutomaticStyles; FAMILIES]; 2],
    defaults: BTreeMap<Family, Props>,
    common_lists: BTreeMap<String, ListStyle>,
    automatic_lists: [BTreeMap<String, ListStyle>; 2],
    /// `text:outline-style`: numbering applied to headings by outline level.
    pub outline: Option<ListStyle>,
    pub page_layouts: BTreeMap<String, PageLayout>,
    pub master_pages: Vec<MasterPage>,
    pub gradients: BTreeMap<String, Gradient>,
    /// Fill image names mapped to their image references.
    pub fill_images: BTreeMap<String, String>,
    /// Font face names mapped to family names.
    pub fonts: BTreeMap<String, String>,
    pub number_formats: BTreeMap<String, String>,
}

fn number_format(node: Node<'_, '_>) -> Option<String> {
    let date = node.tag_name().name() == "date-style";
    if !date
        && !matches!(
            node.tag_name().name(),
            "number-style" | "percentage-style" | "currency-style"
        )
    {
        return None;
    }
    let mut code = String::new();
    for part in elements(node) {
        if part.tag_name().namespace() != Some(NUMBER) {
            continue;
        }
        let long = attr(part, NUMBER, "style") == Some("long");
        match part.tag_name().name() {
            "year" if date => code.push_str(if long { "yyyy" } else { "yy" }),
            "month" if date => code.push_str(if attr(part, NUMBER, "textual") == Some("true") {
                if long {
                    "mmmm"
                } else {
                    "mmm"
                }
            } else if long {
                "mm"
            } else {
                "m"
            }),
            "day" if date => code.push_str(if long { "dd" } else { "d" }),
            "number" if !date => {
                let decimals =
                    attr(part, NUMBER, "decimal-places").and_then(|v| v.parse::<usize>().ok());
                if decimals.is_none() {
                    code.push_str("General");
                    continue;
                }
                code.push_str(if attr(part, NUMBER, "grouping") == Some("true") {
                    "#,##0"
                } else {
                    "0"
                });
                let decimals = decimals.unwrap_or(0).min(30);
                if decimals > 0 {
                    code.push('.');
                    code.extend(std::iter::repeat_n('0', decimals));
                }
            }
            "text" | "currency-symbol" => {
                let text = crate::xml::text_content(part);
                for ch in text.chars() {
                    if !matches!(ch, '/' | '-' | '.' | ' ' | '%') {
                        code.push('\\');
                    }
                    code.push(ch);
                }
            }
            "text-properties" => {}
            _ => return None,
        }
    }
    (!code.is_empty()).then_some(code)
}

fn scope_index(scope: Scope) -> usize {
    match scope {
        Scope::Content => 0,
        Scope::Styles => 1,
    }
}

/// The styles one style inherits from, nearest first; see [`StyleSheet::ancestors`].
pub struct Ancestors<'a> {
    common: &'a CommonStyles,
    next: Option<&'a Style>,
    /// Styles already returned, so a cycle of parents ends.
    seen: [*const Style; ANCESTOR_LIMIT],
    count: usize,
}

/// The longest chain of parent styles followed.
const ANCESTOR_LIMIT: usize = 64;

impl<'a> Iterator for Ancestors<'a> {
    type Item = &'a Style;

    fn next(&mut self) -> Option<&'a Style> {
        let style = self.next.take()?;
        if self.count >= ANCESTOR_LIMIT || self.seen[..self.count].iter().any(|seen| std::ptr::eq(*seen, style)) {
            return None;
        }
        self.seen[self.count] = style;
        self.count += 1;
        self.next = style
            .parent
            .as_deref()
            .and_then(|parent| self.common.get(parent))
            .map(|style| &**style);
        Some(style)
    }
}

impl StyleSheet {
    pub fn style(&self, scope: Scope, family: Family, name: &str) -> Option<&Style> {
        self.automatic[scope_index(scope)][family.index()]
            .get(name)
            .or_else(|| self.common[family.index()].get(name))
            .map(|style| &**style)
    }

    /// The style and its ancestors, nearest first, without collecting them.
    pub fn ancestors(&self, scope: Scope, family: Family, name: &str) -> Ancestors<'_> {
        Ancestors {
            common: &self.common[family.index()],
            next: self.style(scope, family, name),
            seen: [std::ptr::null(); ANCESTOR_LIMIT],
            count: 0,
        }
    }

    /// The style and its ancestors, nearest first.
    pub fn chain(&self, scope: Scope, family: Family, name: &str) -> Vec<&Style> {
        self.ancestors(scope, family, name).collect()
    }

    fn default_props(&self, family: Family) -> Option<&Props> {
        self.defaults.get(&match family {
            Family::Presentation => Family::Graphic,
            family => family,
        })
    }

    /// The family's `style:default-style` properties.
    pub fn defaults(&self, family: Family) -> Props {
        self.default_props(family).cloned().unwrap_or_default()
    }

    /// Properties of a style with every ancestor applied, without the family defaults.
    pub fn inherited(&self, scope: Scope, family: Family, name: &str) -> Props {
        let mut props = Props::default();
        for style in self.ancestors(scope, family, name) {
            props.inherit(&style.props);
        }
        props
    }

    /// Fully resolved properties: the style, its ancestors, then the family defaults.
    pub fn resolve(&self, scope: Scope, family: Family, name: Option<&str>) -> Props {
        let mut props = name
            .map(|name| self.inherited(scope, family, name))
            .unwrap_or_default();
        if let Some(defaults) = self.default_props(family) {
            props.inherit(defaults);
        }
        props
    }

    /// The text properties of [`StyleSheet::inherited`], without resolving the other groups.
    pub fn inherited_text(&self, scope: Scope, family: Family, name: &str) -> TextProps {
        let mut props = TextProps::default();
        for style in self.ancestors(scope, family, name) {
            props.inherit_resolving_size(&style.props.text);
        }
        props
    }

    /// The text properties of [`StyleSheet::resolve`], without resolving the other groups.
    pub fn resolve_text(&self, scope: Scope, family: Family, name: Option<&str>) -> TextProps {
        let mut props = name
            .map(|name| self.inherited_text(scope, family, name))
            .unwrap_or_default();
        if let Some(defaults) = self.default_props(family) {
            props.inherit_resolving_size(&defaults.text);
        }
        props
    }

    /// The paragraph properties of [`StyleSheet::resolve`], without resolving the other groups.
    pub fn resolve_paragraph(&self, scope: Scope, family: Family, name: Option<&str>) -> ParagraphProps {
        let mut props = ParagraphProps::default();
        for style in name.into_iter().flat_map(|name| self.ancestors(scope, family, name)) {
            props.inherit(&style.props.paragraph);
        }
        if let Some(defaults) = self.default_props(family) {
            props.inherit(&defaults.paragraph);
        }
        props
    }

    /// The graphic properties of [`StyleSheet::resolve`], without resolving the other groups.
    pub fn resolve_graphic(&self, scope: Scope, family: Family, name: Option<&str>) -> GraphicProps {
        let mut props = GraphicProps::default();
        for style in name.into_iter().flat_map(|name| self.ancestors(scope, family, name)) {
            props.inherit(&style.props.graphic);
        }
        if let Some(defaults) = self.default_props(family) {
            props.inherit(&defaults.graphic);
        }
        props
    }

    /// The table properties of [`StyleSheet::resolve`], without resolving the other groups.
    pub fn resolve_table(&self, scope: Scope, family: Family, name: Option<&str>) -> TableProps {
        let mut props = TableProps::default();
        for style in name.into_iter().flat_map(|name| self.ancestors(scope, family, name)) {
            props.inherit(&style.props.table);
        }
        if let Some(defaults) = self.default_props(family) {
            props.inherit(&defaults.table);
        }
        props
    }

    /// Named (non-automatic) styles in a stable order.
    pub fn common_styles(&self) -> impl Iterator<Item = &Style> {
        self.common.iter().flat_map(BTreeMap::values).map(|style| &**style)
    }

    pub fn list_style(&self, scope: Scope, name: &str) -> Option<&ListStyle> {
        self.automatic_lists[scope_index(scope)]
            .get(name)
            .or_else(|| self.common_lists.get(name))
    }

    pub fn master_page(&self, name: &str) -> Option<&MasterPage> {
        self.master_pages.iter().find(|page| page.name == name)
    }

    /// The list style a paragraph style, or one of its ancestors, assigns.
    pub fn paragraph_list_style(&self, scope: Scope, name: &str) -> Option<&str> {
        self.ancestors(scope, Family::Paragraph, name)
            .find_map(|style| style.list_style.as_deref())
            .filter(|name| !name.is_empty())
    }

    pub(crate) fn read_fonts(&mut self, root: Node<'_, '_>) {
        let Some(decls) = child(root, OFFICE, "font-face-decls") else {
            return;
        };
        for face in elements(decls).filter(|node| is(*node, STYLE, "font-face")) {
            if let (Some(name), Some(family)) = (owned(face, STYLE, "name"), attr(face, SVG, "font-family")) {
                self.fonts.entry(name).or_insert_with(|| unquote(family));
            }
        }
    }

    /// Reads `office:styles`: named styles, defaults, lists, gradients and fill images.
    pub(crate) fn read_common(&mut self, root: Node<'_, '_>, ctx: &mut Ctx<'_>) {
        let Some(styles) = child(root, OFFICE, "styles") else {
            return;
        };
        for node in elements(styles) {
            let name = node.tag_name().name();
            match node.tag_name().namespace() {
                Some(NUMBER) => {
                    if let (Some(name), Some(code)) =
                        (owned(node, STYLE, "name"), number_format(node))
                    {
                        self.number_formats.insert(name, code);
                    }
                }
                Some(STYLE) if name == "style" => {
                    if let Some(style) = self.read_style(node, false, ctx) {
                        self.common[style.family.index()].insert(style.name.clone(), Box::new(style));
                    }
                }
                Some(STYLE) if name == "default-style" => {
                    let family = Family::parse(attr(node, STYLE, "family").unwrap_or_default());
                    let props = self.read_props(node);
                    self.defaults.insert(family, props);
                }
                Some(TEXT) if name == "list-style" => {
                    let list = self.read_list_style(node, ctx);
                    self.common_lists.insert(list.name.clone(), list);
                }
                Some(TEXT) if name == "outline-style" => {
                    self.outline = Some(self.read_list_style(node, ctx));
                }
                Some(DRAW) if name == "gradient" => {
                    if let Some(name) = owned(node, DRAW, "name") {
                        let color = |name| attr(node, DRAW, name).and_then(parse_color);
                        let angle = attr(node, DRAW, "angle").unwrap_or("0").trim();
                        let legacy = attr(root, OFFICE, "version").is_none_or(|v| v == "1.0" || v == "1.1");
                        let angle = if let Some(value) = angle.strip_suffix("deg") {
                            parse_number(value).unwrap_or(0.0)
                        } else if let Some(value) = angle.strip_suffix("grad") {
                            parse_number(value).unwrap_or(0.0) * 0.9
                        } else if let Some(value) = angle.strip_suffix("rad") {
                            parse_number(value).unwrap_or(0.0).to_degrees()
                        } else {
                            parse_number(angle).unwrap_or(0.0) / if legacy { 10.0 } else { 1.0 }
                        }.rem_euclid(360.0);
                        self.gradients.insert(
                            name,
                            Gradient {
                                kind: attr(node, DRAW, "style").unwrap_or("linear").to_owned(),
                                start_color: color("start-color"),
                                end_color: color("end-color"),
                                angle,
                            },
                        );
                    }
                }
                Some(DRAW) if name == "fill-image" => {
                    if let (Some(name), Some(href)) =
                        (owned(node, DRAW, "name"), draw::image_reference(node, ctx))
                    {
                        self.fill_images.insert(name, href);
                    }
                }
                _ => {}
            }
        }
    }

    /// Reads `office:automatic-styles` of one part, plus its page layouts.
    pub(crate) fn read_automatic(&mut self, root: Node<'_, '_>, scope: Scope, ctx: &mut Ctx<'_>) {
        let Some(styles) = child(root, OFFICE, "automatic-styles") else {
            return;
        };
        for node in elements(styles) {
            let name = node.tag_name().name();
            match node.tag_name().namespace() {
                Some(NUMBER) => {
                    if let (Some(name), Some(code)) =
                        (owned(node, STYLE, "name"), number_format(node))
                    {
                        self.number_formats.insert(name, code);
                    }
                }
                Some(STYLE) if name == "style" => {
                    if let Some(style) = self.read_style(node, true, ctx) {
                        self.automatic[scope_index(scope)][style.family.index()]
                            .insert(style.name.clone(), Box::new(style));
                    }
                }
                Some(TEXT) if name == "list-style" => {
                    let list = self.read_list_style(node, ctx);
                    self.automatic_lists[scope_index(scope)].insert(list.name.clone(), list);
                }
                Some(STYLE) if name == "page-layout" => {
                    if let Some(name) = owned(node, STYLE, "name") {
                        self.page_layouts.insert(name, read_page_layout(node));
                    }
                }
                _ => {}
            }
        }
    }

    pub(crate) fn read_master_pages(&mut self, root: Node<'_, '_>, ctx: &mut Ctx<'_>) {
        let Some(masters) = child(root, OFFICE, "master-styles") else {
            return;
        };
        for node in elements(masters).filter(|node| is(*node, STYLE, "master-page")) {
            let Some(name) = owned(node, STYLE, "name") else {
                continue;
            };
            let mut story = |name| child(node, STYLE, name).map(|story| text::parse_blocks(story, ctx));
            let mut page = MasterPage {
                name,
                display_name: owned(node, STYLE, "display-name"),
                page_layout: owned(node, STYLE, "page-layout-name"),
                style: owned(node, DRAW, "style-name"),
                next: owned(node, STYLE, "next-style-name"),
                header: story("header"),
                footer: story("footer"),
                header_first: story("header-first"),
                footer_first: story("footer-first"),
                header_left: story("header-left"),
                footer_left: story("footer-left"),
                shapes: Vec::new(),
            };
            page.shapes = elements(node)
                .filter(|child| !is(*child, PRESENTATION, "notes"))
                .filter_map(|child| draw::parse_shape(child, ctx))
                .collect();
            self.master_pages.push(page);
        }
    }

    fn read_style(&self, node: Node<'_, '_>, automatic: bool, ctx: &mut Ctx<'_>) -> Option<Style> {
        let name = owned(node, STYLE, "name")?;
        let list = elements(node)
            .filter(|child| is(*child, STYLE, "graphic-properties"))
            .find_map(|properties| child(properties, TEXT, "list-style"))
            .map(|list| self.read_list_style(list, ctx));
        Some(Style {
            name,
            display_name: owned(node, STYLE, "display-name"),
            family: Family::parse(attr(node, STYLE, "family").unwrap_or_default()),
            parent: owned(node, STYLE, "parent-style-name"),
            list_style: attr(node, STYLE, "list-style-name").map(str::to_owned),
            master_page: owned(node, STYLE, "master-page-name"),
            class: owned(node, STYLE, "class"),
            default_outline_level: attr(node, STYLE, "default-outline-level")
                .and_then(|value| value.parse().ok()),
            automatic,
            props: self.read_props(node),
            list,
        })
    }

    fn read_props(&self, node: Node<'_, '_>) -> Props {
        let mut props = Props::default();
        if attr(node, STYLE, "family") == Some("table-cell") {
            props.table.number_style = owned(node, STYLE, "data-style-name");
        }
        if attr(node, STYLE, "family") == Some("chart") {
            if let Some(name) = owned(node, STYLE, "data-style-name") {
                props.chart.insert("number-style-name".into(), name);
            }
        }
        for properties in elements(node).filter(|child| child.tag_name().namespace() == Some(STYLE))
        {
            match properties.tag_name().name() {
                "text-properties" => props.text = self.text_props(properties),
                "paragraph-properties" => props.paragraph = paragraph_props(properties),
                "graphic-properties" | "drawing-page-properties" => {
                    props.graphic = graphic_props(properties);
                }
                "chart-properties" => {
                    for attribute in properties.attributes().iter().filter(|a| {
                        a.namespace == Some(CHART)
                            || a.namespace == Some(LO_EXT)
                                && matches!(
                                    a.name,
                                    "show-horizontal-border"
                                        | "show-vertical-border"
                                        | "show-outline"
                                        | "show-keys"
                                )
                            || a.namespace == Some(TEXT) && a.name == "line-break"
                            || a.namespace == Some(STYLE) && a.name == "rotation-angle"
                    }) {
                        props
                            .chart
                            .insert(attribute.name.to_string(), attribute.value.to_string());
                    }
                }
                "table-properties" | "table-column-properties" | "table-row-properties"
                | "table-cell-properties" => table_props(properties, &mut props.table),
                _ => {}
            }
        }
        props
    }

    fn text_props(&self, node: Node<'_, '_>) -> TextProps {
        let font = |attribute: &str, fallback: Option<&str>| {
            attr(node, STYLE, attribute)
                .map(|name| self.fonts.get(name).cloned().unwrap_or_else(|| name.to_owned()))
                .or_else(|| fallback.and_then(|name| attr(node, FO, name)).map(unquote))
                .filter(|family| !family.is_empty())
        };
        let on_off = |value: Option<&str>, off: &str| value.map(|value| value != off);
        let language = |namespace: crate::xml::Ns, language: &str, country: &str| {
            attr(node, namespace, language)
                .filter(|language| !matches!(*language, "none" | "zxx" | ""))
                .map(|language| {
                    match attr(node, namespace, country).filter(|country| !matches!(*country, "none" | "")) {
                        Some(country) => format!("{language}-{country}"),
                        None => language.to_owned(),
                    }
                })
        };
        TextProps {
            font: font("font-name", Some("font-family")),
            font_asian: font("font-name-asian", None),
            font_complex: font("font-name-complex", None),
            size: attr(node, FO, "font-size").and_then(|value| {
                parse_length(value)
                    .map(|length| FontSize::Points(length.points()))
                    .or_else(|| parse_percent(value).map(FontSize::Percent))
            }),
            bold: attr(node, FO, "font-weight").map(|weight| {
                weight == "bold" || weight.parse::<u32>().is_ok_and(|weight| weight >= 600)
            }),
            italic: attr(node, FO, "font-style").map(|style| style != "normal"),
            underline: on_off(attr(node, STYLE, "text-underline-style"), "none"),
            strike: on_off(attr(node, STYLE, "text-line-through-style"), "none"),
            color: attr(node, FO, "color").and_then(parse_color),
            background: background(node),
            uppercase: attr(node, FO, "text-transform").map(|value| value == "uppercase"),
            small_caps: attr(node, FO, "font-variant").map(|value| value == "small-caps"),
            position: attr(node, STYLE, "text-position").and_then(|value| {
                match value.split_whitespace().next()? {
                    "super" => Some(33.0),
                    "sub" => Some(-33.0),
                    shift => parse_percent(shift),
                }
            }),
            letter_spacing: attr(node, FO, "letter-spacing")
                .map(|value| parse_length(value).unwrap_or_default()),
            language: language(FO, "language", "country"),
            language_asian: language(STYLE, "language-asian", "country-asian"),
            language_complex: language(STYLE, "language-complex", "country-complex"),
            hidden: attr(node, TEXT, "display").map(|value| value == "none"),
        }
    }

    fn read_list_style(&self, node: Node<'_, '_>, ctx: &mut Ctx<'_>) -> ListStyle {
        let mut list = ListStyle {
            name: attr(node, STYLE, "name").unwrap_or_default().to_owned(),
            levels: BTreeMap::new(),
        };
        for level in elements(node).filter(|child| child.tag_name().namespace() == Some(TEXT)) {
            let label = match level.tag_name().name() {
                "list-level-style-bullet" => {
                    ListLabel::Bullet(attr(level, TEXT, "bullet-char").unwrap_or("•").to_owned())
                }
                "list-level-style-number" | "outline-level-style" => ListLabel::Number {
                    format: attr(level, STYLE, "num-format").unwrap_or_default().to_owned(),
                    start: attr(level, TEXT, "start-value")
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(1),
                    display_levels: attr(level, TEXT, "display-levels")
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(1),
                },
                "list-level-style-image" => match draw::image_reference(level, ctx) {
                    Some(href) => ListLabel::Image(href),
                    None => ListLabel::Bullet("•".into()),
                },
                _ => continue,
            };
            let Some(number) = attr(level, TEXT, "level").and_then(|value| value.parse::<u8>().ok()) else {
                continue;
            };
            let properties = child(level, STYLE, "list-level-properties");
            let alignment = properties.and_then(|node| child(node, STYLE, "list-level-label-alignment"));
            let length = |node: Option<Node<'_, '_>>, namespace, name| {
                node.and_then(|node| attr(node, namespace, name)).and_then(parse_length)
            };
            // The older positioning mode gives the label's offset and width
            // instead of the text's margin and a hanging indent.
            let (margin_left, text_indent) = match alignment {
                Some(_) => (length(alignment, FO, "margin-left"), length(alignment, FO, "text-indent")),
                None => {
                    let before = length(properties, TEXT, "space-before").unwrap_or_default();
                    let width = length(properties, TEXT, "min-label-width").unwrap_or_default();
                    (
                        properties.map(|_| Length(before.0 + width.0)),
                        properties.map(|_| Length(-width.0)),
                    )
                }
            };
            list.levels.insert(
                number,
                ListLevel {
                    label,
                    prefix: attr(level, STYLE, "num-prefix").unwrap_or_default().to_owned(),
                    suffix: attr(level, STYLE, "num-suffix").unwrap_or_default().to_owned(),
                    text_style: owned(level, TEXT, "style-name"),
                    text: child(level, STYLE, "text-properties")
                        .map(|properties| self.text_props(properties))
                        .unwrap_or_default(),
                    margin_left,
                    text_indent,
                    relative_size: attr(level, TEXT, "bullet-relative-size").and_then(parse_percent),
                },
            );
        }
        list
    }
}

fn unquote(family: &str) -> String {
    family
        .split(',')
        .next()
        .unwrap_or_default()
        .trim()
        .trim_matches(['\'', '"'])
        .to_owned()
}

/// `fo:background-color`: a color, or an empty string for `transparent`.
fn background(node: Node<'_, '_>) -> Option<String> {
    attr(node, FO, "background-color").map(|value| parse_color(value).unwrap_or_default())
}

fn parse_border(value: &str) -> Border {
    let mut border = Border {
        width: Length(0),
        style: "none".into(),
        color: None,
    };
    for part in value.split_whitespace() {
        if let Some(width) = parse_length(part) {
            border.width = width;
        } else if let Some(color) = parse_color(part) {
            border.color = Some(color);
        } else {
            border.style = part.to_owned();
        }
    }
    border
}

/// `fo:border` and its four sides: top, bottom, left, right.
fn borders(node: Node<'_, '_>) -> [Option<Border>; 4] {
    let all = attr(node, FO, "border").map(parse_border);
    ["border-top", "border-bottom", "border-left", "border-right"]
        .map(|name| attr(node, FO, name).map(parse_border).or_else(|| all.clone()))
}

/// A shorthand length and its four sides: left, right, top, bottom.
fn margins(node: Node<'_, '_>, shorthand: &str) -> [Option<Length>; 4] {
    let all = attr(node, FO, shorthand).and_then(parse_length);
    let names = match shorthand {
        "margin" => ["margin-left", "margin-right", "margin-top", "margin-bottom"],
        "padding" => ["padding-left", "padding-right", "padding-top", "padding-bottom"],
        _ => unreachable!(),
    };
    names.map(|name| attr(node, FO, name).and_then(parse_length).or(all))
}

fn paragraph_props(node: Node<'_, '_>) -> ParagraphProps {
    let [margin_left, margin_right, margin_top, margin_bottom] = margins(node, "margin");
    let [border_top, border_bottom, border_left, border_right] = borders(node);
    let page_break = |name| {
        attr(node, FO, name).map(|value| match value {
            "page" | "even-page" | "odd-page" => Break::Page,
            "column" => Break::Column,
            _ => Break::None,
        })
    };
    let line_height = attr(node, FO, "line-height")
        .and_then(|value| {
            parse_percent(value)
                .map(LineHeight::Percent)
                .or_else(|| parse_length(value).map(LineHeight::Exact))
                .or_else(|| (value == "normal").then_some(LineHeight::Percent(100.0)))
        })
        .or_else(|| {
            attr(node, STYLE, "line-height-at-least")
                .and_then(parse_length)
                .map(LineHeight::AtLeast)
        })
        .or_else(|| {
            attr(node, STYLE, "line-spacing")
                .and_then(parse_length)
                .map(LineHeight::Leading)
        });
    let tab_stops = child(node, STYLE, "tab-stops").map(|stops| {
        elements(stops)
            .filter_map(|stop| {
                Some(TabStop {
                    position: attr(stop, STYLE, "position").and_then(parse_length)?,
                    kind: match attr(stop, STYLE, "type") {
                        Some("center") => TabKind::Center,
                        Some("right") => TabKind::Right,
                        Some("char") => TabKind::Decimal,
                        _ => TabKind::Left,
                    },
                    leader: match attr(stop, STYLE, "leader-style") {
                        None | Some("none") => None,
                        Some(_) => Some(
                            attr(stop, STYLE, "leader-text")
                                .and_then(|text| text.chars().next())
                                .unwrap_or('.'),
                        ),
                    },
                })
            })
            .collect()
    });
    ParagraphProps {
        align: attr(node, FO, "text-align").and_then(|value| match value {
            "start" => Some(Align::Start),
            "end" => Some(Align::End),
            "left" => Some(Align::Left),
            "right" => Some(Align::Right),
            "center" => Some(Align::Center),
            "justify" => Some(Align::Justify),
            _ => None,
        }),
        margin_left,
        margin_right,
        margin_top,
        margin_bottom,
        text_indent: attr(node, FO, "text-indent").and_then(parse_length),
        line_height,
        break_before: page_break("break-before"),
        break_after: page_break("break-after"),
        keep_with_next: attr(node, FO, "keep-with-next").map(|value| value == "always"),
        keep_together: attr(node, FO, "keep-together").map(|value| value == "always"),
        background: background(node),
        rtl: attr(node, STYLE, "writing-mode").and_then(|value| match value {
            "rl-tb" | "rl" => Some(true),
            "lr-tb" | "lr" => Some(false),
            _ => None,
        }),
        tab_stops,
        border_top,
        border_bottom,
        border_left,
        border_right,
    }
}

fn graphic_props(node: Node<'_, '_>) -> GraphicProps {
    let [padding_left, padding_right, padding_top, padding_bottom] = margins(node, "padding");
    let text = |namespace, name| owned(node, namespace, name);
    let flag = |namespace, name| attr(node, namespace, name).and_then(parse_bool);
    let clip = attr(node, FO, "clip").and_then(|value| {
        let inner = value.trim().strip_prefix("rect(")?.strip_suffix(')')?;
        let mut sides = inner
            .split([',', ' '])
            .filter(|part| !part.is_empty())
            .map(|part| parse_length(part).unwrap_or_default());
        Some([sides.next()?, sides.next()?, sides.next()?, sides.next()?])
    });
    GraphicProps {
        fill: attr(node, DRAW, "fill").and_then(|value| match value {
            "none" => Some(Fill::None),
            "solid" => Some(Fill::Solid),
            "gradient" => Some(Fill::Gradient),
            "bitmap" => Some(Fill::Bitmap),
            "hatch" => Some(Fill::Hatch),
            _ => None,
        }),
        fill_color: attr(node, DRAW, "fill-color").and_then(parse_color),
        fill_gradient: text(DRAW, "fill-gradient-name"),
        fill_image: text(DRAW, "fill-image-name"),
        opacity: attr(node, DRAW, "opacity")
            .and_then(parse_percent)
            .map(|percent| (percent / 100.0).clamp(0.0, 1.0)),
        stroke: attr(node, DRAW, "stroke").and_then(|value| match value {
            "none" => Some(Stroke::None),
            "solid" => Some(Stroke::Solid),
            "dash" => Some(Stroke::Dash),
            _ => None,
        }),
        stroke_color: attr(node, SVG, "stroke-color").and_then(parse_color),
        stroke_opacity: attr(node, SVG, "stroke-opacity").and_then(parse_percent).map(|v| (v / 100.0).clamp(0.0, 1.0)),
        stroke_width: attr(node, SVG, "stroke-width").and_then(parse_length),
        stroke_dash: text(DRAW, "stroke-dash"),
        marker_start: text(DRAW, "marker-start"),
        marker_end: text(DRAW, "marker-end"),
        vertical_align: attr(node, DRAW, "textarea-vertical-align").and_then(vertical_align),
        padding_left,
        padding_top,
        padding_right,
        padding_bottom,
        auto_grow_height: flag(DRAW, "auto-grow-height"),
        auto_grow_width: flag(DRAW, "auto-grow-width"),
        fit_to_size: attr(node, DRAW, "fit-to-size").map(|value| value != "false"),
        wrap_text: attr(node, FO, "wrap-option").map(|value| value != "no-wrap"),
        wrap: text(STYLE, "wrap"),
        behind_text: attr(node, STYLE, "run-through").map(|value| value == "background"),
        horizontal_pos: text(STYLE, "horizontal-pos"),
        horizontal_rel: text(STYLE, "horizontal-rel"),
        vertical_pos: text(STYLE, "vertical-pos"),
        vertical_rel: text(STYLE, "vertical-rel"),
        background: background(node),
        border: attr(node, FO, "border").map(parse_border),
        min_height: attr(node, FO, "min-height").and_then(parse_length),
        clip,
        background_visible: flag(PRESENTATION, "background-visible"),
        background_objects_visible: flag(PRESENTATION, "background-objects-visible"),
        display_footer: flag(PRESENTATION, "display-footer"),
        display_page_number: flag(PRESENTATION, "display-page-number"),
        display_date_time: flag(PRESENTATION, "display-date-time"),
        hidden: attr(node, PRESENTATION, "visibility").map(|value| value == "hidden"),
    }
}

fn vertical_align(value: &str) -> Option<VerticalAlign> {
    match value {
        "top" => Some(VerticalAlign::Top),
        "middle" | "center" => Some(VerticalAlign::Middle),
        "bottom" => Some(VerticalAlign::Bottom),
        _ => None,
    }
}

fn table_props(node: Node<'_, '_>, props: &mut TableProps) {
    let [border_top, border_bottom, border_left, border_right] = borders(node);
    let own = TableProps {
        number_style: None,
        width: attr(node, STYLE, "width").and_then(parse_length),
        align: owned(node, TABLE, "align"),
        margin_left: attr(node, FO, "margin-left").and_then(parse_length),
        column_width: attr(node, STYLE, "column-width").and_then(parse_length),
        rel_column_width: attr(node, STYLE, "rel-column-width")
            .and_then(|value| parse_number(value.trim_end_matches('*'))),
        row_height: attr(node, STYLE, "row-height").and_then(parse_length),
        min_row_height: attr(node, STYLE, "min-row-height").and_then(parse_length),
        background: background(node),
        border_top,
        border_bottom,
        border_left,
        border_right,
        padding: attr(node, FO, "padding").and_then(parse_length),
        vertical_align: attr(node, STYLE, "vertical-align").and_then(vertical_align),
    };
    let previous = std::mem::replace(props, own);
    props.inherit(&previous);
}

fn read_page_layout(node: Node<'_, '_>) -> PageLayout {
    let Some(properties) = child(node, STYLE, "page-layout-properties") else {
        return PageLayout::default();
    };
    let [margin_left, margin_right, margin_top, margin_bottom] = margins(properties, "margin");
    PageLayout {
        width: attr(properties, FO, "page-width").and_then(parse_length),
        height: attr(properties, FO, "page-height").and_then(parse_length),
        landscape: attr(properties, STYLE, "print-orientation") == Some("landscape"),
        margin_top,
        margin_bottom,
        margin_left,
        margin_right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_borders() {
        let border = parse_border("0.5pt solid #ff0000");
        assert_eq!(border.width, Length(6350));
        assert_eq!(border.style, "solid");
        assert_eq!(border.color.as_deref(), Some("FF0000"));
        assert!(border.is_visible());
        assert!(!parse_border("none").is_visible());
    }

    #[test]
    fn relative_font_sizes_resolve_against_the_parent() {
        let mut child = Props::default();
        child.text.size = Some(FontSize::Percent(50.0));
        let mut parent = Props::default();
        parent.text.size = Some(FontSize::Points(24.0));
        parent.text.bold = Some(true);
        child.inherit(&parent);
        assert_eq!(child.text.points(), Some(12.0));
        assert_eq!(child.text.bold, Some(true));
    }
}

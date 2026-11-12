//! OpenDocument presentation and drawing adapter.
//!
//! Builds the same [`PresentationDocument`] the PowerPoint readers produce, so
//! blocks, export and the viewer work unchanged. Reading the file itself is
//! `rust-odf`'s job; this module only maps its result onto the slide model.

use std::collections::BTreeMap;

use rust_odf::draw::{normalize_path, polygon_path, Shape, ShapeKind};
use rust_odf::media::{image_content_type, image_extension};
use rust_odf::style::{
    Align, Border, Family, Fill, GraphicProps, LineHeight, ListLabel, ListLevel, ListStyle, Props,
    Scope, Stroke, StyleSheet, TabKind, VerticalAlign,
};
use rust_odf::text::{plain_text, Block, Inline, Table};
use rust_odf::chart::Chart;
use rust_odf::{Body, OdfDocument, OdfError, OdfFamily, Page};

use crate::{
    ColorValue, FillImageMode, FillStyle, GradientStop, LineStyle, NodeMetadata, ParseError,
    ParseLimits, PresentationAsset, PresentationDocument, PresentationFormat, PresentationMetadata,
    PresentationSize, PresentationSlide, PresentationWarning, ShapeGeometry, SlideMaster, SlideNode,
    SlideNote, TableCell, TableCellMerge, TableCellPosition, TextAutoFitMode, TextAutofit,
    TextBullet, TextInsets, TextParagraph, TextRun, TextSpacing, TextSpacingUnit, TextTabAlignment,
    TextTabStop, Transform, VerticalAlignment,
};

/// 13.333 by 7.5 inches, used when a file declares no page size.
const DEFAULT_SIZE: (i64, i64) = (12_192_000, 6_858_000);
/// Rows or columns one repeated table element may expand to.
const MAX_REPEAT: u32 = 1024;

/// The presentation format of an OpenDocument file, `None` for anything else.
/// OpenDocument files that are not presentations or drawings are unsupported.
pub(crate) fn detect(bytes: &[u8]) -> Option<Result<PresentationFormat, ParseError>> {
    Some(match rust_odf::sniff(bytes)?.family {
        OdfFamily::Presentation => Ok(PresentationFormat::Odp),
        OdfFamily::Graphics => Ok(PresentationFormat::Odg),
        _ => Err(ParseError::UnsupportedFormat),
    })
}

pub(crate) fn parse(
    bytes: &[u8],
    limits: &ParseLimits,
    format: PresentationFormat,
) -> Result<PresentationDocument, ParseError> {
    let odf_limits = rust_odf::Limits {
        max_input_bytes: limits.max_input_bytes,
        max_entries: limits.max_entries,
        max_entry_bytes: limits.max_entry_bytes,
        max_total_uncompressed_bytes: limits.max_total_uncompressed_bytes,
        max_xml_nodes: limits.max_xml_nodes,
        max_depth: limits.max_xml_depth,
        max_content_nodes: limits.max_xml_nodes,
        ..rust_odf::Limits::default()
    };
    let document = OdfDocument::parse(bytes, &odf_limits).map_err(|error| match error {
        OdfError::Encrypted => ParseError::Encrypted,
        OdfError::Limit(what) => ParseError::ResourceLimit(what),
        OdfError::NotOdf => ParseError::UnsupportedFormat,
        error => ParseError::Corrupt(error.to_string()),
    })?;
    Ok(Builder::new(&document, odf_limits).build(format))
}

fn color(value: &str, alpha: Option<f64>) -> ColorValue {
    ColorValue {
        value: format!("#{value}"),
        alpha: alpha.filter(|alpha| *alpha < 1.0),
    }
}

fn transform_of(shape: &Shape) -> Transform {
    let placement = shape.placement();
    Transform {
        x: placement.x.0,
        y: placement.y.0,
        width: placement.width.0,
        height: placement.height.0,
        rotation: (placement.rotation.abs() > 1e-6).then_some(placement.rotation),
        flip_horizontal: None,
        flip_vertical: None,
    }
}

fn node_parts(node: &mut SlideNode) -> (&mut String, &Transform, Option<&mut Vec<SlideNode>>) {
    match node {
        SlideNode::Group {
            id,
            transform,
            children,
            ..
        } => (id, transform, Some(children)),
        SlideNode::Shape { id, transform, .. }
        | SlideNode::Image { id, transform, .. }
        | SlideNode::Table { id, transform, .. }
        | SlideNode::Chart { id, transform, .. }
        | SlideNode::Media { id, transform, .. }
        | SlideNode::Unknown { id, transform, .. } => (id, transform, None),
    }
}

/// Marks nodes inherited from a master page, as the PowerPoint reader does.
fn prefix_ids(nodes: &mut [SlideNode], prefix: &str) {
    for node in nodes {
        let (id, _, children) = node_parts(node);
        id.insert_str(0, prefix);
        if let Some(children) = children {
            prefix_ids(children, prefix);
        }
    }
}

fn with_id(mut node: SlideNode, id: String) -> SlideNode {
    *node_parts(&mut node).0 = id;
    node
}

/// A chart's stored data as a chart node. Nothing draws it yet: no chart
/// picture is generated for OpenDocument charts.
fn chart_node(
    chart: &Chart,
    part: Option<String>,
    metadata: Option<Box<NodeMetadata>>,
    name: String,
    transform: Transform,
) -> SlideNode {
    let categories: Vec<serde_json::Value> = chart
        .categories
        .iter()
        .map(|category| serde_json::Value::String(category.clone()))
        .collect();
    SlideNode::Chart {
        metadata,
        id: String::new(),
        name,
        transform,
        chart_type: match chart.class.as_str() {
            "circle" => "pie",
            "ring" => "doughnut",
            "filled-radar" => "radar",
            class => class,
        }
        .to_owned(),
        title: chart.title.clone(),
        series: chart
            .series
            .iter()
            .map(|series| crate::ChartSeries {
                name: series.name.clone(),
                categories: (!categories.is_empty()).then(|| categories.clone()),
                values: series.values.clone(),
                color: None,
            })
            .collect(),
        has_legend: Some(chart.has_legend),
        chart_xml: None,
        chart_part_name: part,
        chart_svg: None,
        chart_style_xml: None,
        chart_colors_xml: None,
    }
}

/// A borderless, unfilled text shape.
fn text_box(
    metadata: Option<Box<NodeMetadata>>,
    id: String,
    name: String,
    transform: Transform,
    paragraphs: Vec<TextParagraph>,
) -> SlideNode {
    SlideNode::Shape {
        metadata,
        id,
        name,
        transform,
        geometry: ShapeGeometry {
            preset: Some("rect".into()),
            path: None,
        },
        shape_svg: None,
        paint_bounds: None,
        source_xml: None,
        fill: Some(FillStyle::None),
        line: None,
        paragraphs,
        vertical_alignment: None,
        text_insets: None,
        autofit: None,
        text_rotation: None,
        text_transform: None,
        vertical_text: None,
        horizontal_overflow: None,
        vertical_overflow: None,
        text_wrap: None,
        column_count: None,
        column_spacing: None,
        right_to_left_columns: None,
        space_first_last_paragraph: None,
    }
}

fn border_line(border: &Border) -> Option<LineStyle> {
    border.is_visible().then(|| LineStyle {
        color: Some(color(border.color.as_deref().unwrap_or("000000"), None)),
        width: Some((border.width.0 as f64 / 9_525.0).max(0.75)),
        dash: match border.style.as_str() {
            "dashed" => Some("dash".into()),
            "dotted" => Some("sysDot".into()),
            _ => None,
        },
        start_arrow: None,
        end_arrow: None,
    })
}

fn bullet(level: &ListLevel) -> Option<TextBullet> {
    let (kind, value, start_at) = match &level.label {
        ListLabel::Bullet(character) if character.trim().is_empty() => return None,
        ListLabel::Bullet(character) => ("character", Some(character.clone()), None),
        ListLabel::Number { format, .. } if format.is_empty() => return None,
        ListLabel::Number { format, start, .. } => {
            let base = match format.as_str() {
                "a" => "alphaLc",
                "A" => "alphaUc",
                "i" => "romanLc",
                "I" => "romanUc",
                _ => "arabic",
            };
            let wrap = match (level.prefix.trim(), level.suffix.trim()) {
                ("(", ")") => "ParenBoth",
                (_, ")") => "ParenR",
                (_, ".") => "Period",
                _ => "Plain",
            };
            ("number", Some(format!("{base}{wrap}")), Some(*start as usize))
        }
        ListLabel::Image(_) => ("picture", None, None),
    };
    Some(TextBullet {
        kind: kind.into(),
        value,
        font_family: level.text.font.clone(),
        font_size_pt: None,
        size_percent: level.relative_size.map(|percent| percent / 100.0),
        start_at,
    })
}

/// What a shape's text inherits before paragraph and span styles apply.
struct TextContext<'s> {
    scope: Scope,
    base: Props,
    /// The name of an outline placeholder's level styles without the level
    /// number, such as `Default-outline`.
    outline: Option<String>,
    /// The list style the shape's own style declares.
    list: Option<&'s ListStyle>,
    slide_number: usize,
    /// Text a slide supplies for its master page's footer and date fields.
    footer: Option<String>,
    date_time: Option<String>,
}

#[derive(Clone, Copy)]
struct ListPosition<'s> {
    style: Option<&'s ListStyle>,
    /// 1 for a top-level list item.
    depth: usize,
}

struct Builder<'a> {
    document: &'a OdfDocument,
    styles: &'a StyleSheet,
    limits: rust_odf::Limits,
    assets: BTreeMap<String, PresentationAsset>,
    asset_ids: BTreeMap<String, Option<String>>,
    warnings: Vec<PresentationWarning>,
    next_node: usize,
    /// Footer and date text of the slide being built.
    footer: Option<String>,
    date_time: Option<String>,
}

impl<'a> Builder<'a> {
    fn new(document: &'a OdfDocument, limits: rust_odf::Limits) -> Self {
        Self {
            document,
            styles: &document.styles,
            limits,
            assets: BTreeMap::new(),
            asset_ids: BTreeMap::new(),
            warnings: Vec::new(),
            next_node: 0,
            footer: None,
            date_time: None,
        }
    }

    fn warn(&mut self, code: &str, message: &str, slide_index: Option<usize>) {
        if self
            .warnings
            .iter()
            .any(|warning| warning.code == code && warning.slide_index == slide_index)
        {
            return;
        }
        let mut warning = PresentationWarning::warning(code, message);
        warning.slide_index = slide_index;
        warning.feature = Some("opendocument".into());
        self.warnings.push(warning);
    }

    fn asset(&mut self, href: &str) -> Option<String> {
        if let Some(id) = self.asset_ids.get(href) {
            return id.clone();
        }
        let id = self.document.binaries.get(href).and_then(|bytes| {
            let extension = image_extension(bytes, href)?;
            self.assets.insert(
                href.to_owned(),
                PresentationAsset {
                    id: href.to_owned(),
                    content_type: image_content_type(extension).to_owned(),
                    byte_length: bytes.len(),
                    file_name: href.rsplit('/').next().map(str::to_owned),
                    data: Some(serde_bytes::ByteBuf::from(bytes.clone())),
                },
            );
            Some(href.to_owned())
        });
        self.asset_ids.insert(href.to_owned(), id.clone());
        id
    }

    fn fill(&mut self, props: &GraphicProps) -> Option<FillStyle> {
        let solid = |props: &GraphicProps| {
            props.fill_color.as_deref().map(|value| FillStyle::Solid {
                color: color(value, props.opacity),
            })
        };
        match props.fill? {
            Fill::None => Some(FillStyle::None),
            Fill::Solid | Fill::Hatch => solid(props),
            Fill::Gradient => {
                let gradient = props
                    .fill_gradient
                    .as_deref()
                    .and_then(|name| self.styles.gradients.get(name));
                let Some((gradient, start, end)) = gradient.and_then(|gradient| {
                    Some((gradient, gradient.start_color.as_deref()?, gradient.end_color.as_deref()?))
                }) else {
                    return solid(props);
                };
                let stop = |position, value| GradientStop {
                    position,
                    color: color(value, props.opacity),
                };
                let stops = if gradient.kind == "axial" {
                    vec![stop(0.0, end), stop(0.5, start), stop(1.0, end)]
                } else {
                    vec![stop(0.0, start), stop(1.0, end)]
                };
                // OpenDocument measures the angle from the top, DrawingML from the left.
                let linear = matches!(gradient.kind.as_str(), "linear" | "axial");
                Some(FillStyle::Gradient {
                    angle: linear.then(|| (90.0 - gradient.angle).rem_euclid(360.0)),
                    stops,
                })
            }
            Fill::Bitmap => {
                let href = props
                    .fill_image
                    .as_deref()
                    .and_then(|name| self.styles.fill_images.get(name))?;
                let asset_id = self.asset(href)?;
                Some(FillStyle::Image {
                    asset_id,
                    mode: FillImageMode::Stretch,
                    crop: None,
                    opacity: props.opacity.filter(|opacity| *opacity < 1.0),
                    effects: None,
                })
            }
        }
    }

    fn line(&self, props: &GraphicProps) -> Option<LineStyle> {
        match props.stroke? {
            Stroke::None => None,
            stroke => Some(LineStyle {
                color: Some(color(props.stroke_color.as_deref().unwrap_or("000000"), None)),
                width: Some(
                    props
                        .stroke_width
                        .map_or(0.75, |width| (width.0 as f64 / 9_525.0).max(0.75)),
                ),
                dash: (stroke == Stroke::Dash).then(|| "dash".to_owned()),
                start_arrow: props.marker_start.as_ref().map(|_| "triangle".to_owned()),
                end_arrow: props.marker_end.as_ref().map(|_| "triangle".to_owned()),
            }),
        }
    }

    // ----- text -----

    fn text_context(&self, shape: &Shape, scope: Scope, slide_number: usize) -> TextContext<'a> {
        let styles = self.styles;
        let (family, name) = match shape.presentation_style.as_deref() {
            Some(name) => (Family::Presentation, Some(name)),
            None => (Family::Graphic, shape.style.as_deref()),
        };
        let chain = name.map(|name| styles.chain(scope, family, name)).unwrap_or_default();
        let mut base = styles.resolve(scope, family, name);
        if let Some(text_style) = shape.text_style.as_deref() {
            let mut props = styles.inherited(scope, Family::Paragraph, text_style);
            props.inherit(&base);
            base = props;
        }
        TextContext {
            scope,
            base,
            outline: (shape.class.as_deref() == Some("outline"))
                .then(|| {
                    chain
                        .iter()
                        .find_map(|style| style.name.strip_suffix("outline1"))
                        .map(|prefix| format!("{prefix}outline"))
                })
                .flatten(),
            list: chain.iter().find_map(|style| style.list.as_ref()),
            slide_number,
            footer: self.footer.clone(),
            date_time: self.date_time.clone(),
        }
    }

    fn run(&self, text: String, props: &Props, link: Option<&str>) -> TextRun {
        let text_props = &props.text;
        TextRun {
            text,
            font_family: text_props.font.clone(),
            east_asian_font_family: text_props.font_asian.clone(),
            complex_script_font_family: text_props.font_complex.clone(),
            font_size_pt: text_props.points(),
            bold: text_props.bold,
            italic: text_props.italic,
            underline: text_props.underline,
            strike: text_props.strike,
            color: text_props.color.as_deref().map(|value| color(value, None)),
            baseline: text_props.position.filter(|position| *position != 0.0),
            language: text_props.language.clone(),
            hyperlink: link.map(str::to_owned),
            character_spacing_pt: text_props.letter_spacing.map(|spacing| spacing.points()),
            ..TextRun::default()
        }
    }

    fn runs(
        &self,
        inlines: &[Inline],
        context: &TextContext<'_>,
        props: &Props,
        link: Option<&str>,
        out: &mut Vec<TextRun>,
    ) {
        for inline in inlines {
            match inline {
                Inline::Text(text) => out.push(self.run(text.clone(), props, link)),
                Inline::Span { style, children } => match style {
                    Some(name) => {
                        let mut inner = self.styles.inherited(context.scope, Family::Text, name);
                        inner.inherit(props);
                        self.runs(children, context, &inner, link, out);
                    }
                    None => self.runs(children, context, props, link, out),
                },
                Inline::Link { href, children } => {
                    let link = Some(href.as_str()).filter(|href| !href.is_empty()).or(link);
                    self.runs(children, context, props, link, out);
                }
                Inline::Tab => out.push(self.run("\t".into(), props, link)),
                Inline::LineBreak => out.push(self.run("\n".into(), props, link)),
                Inline::Field { kind, text } => {
                    // Slide numbers, footers and dates are stored as fields the slide fills in.
                    let text = match kind.as_str() {
                        "page-number" => context.slide_number.to_string(),
                        "footer" => context.footer.clone().unwrap_or_default(),
                        "date-time" => context.date_time.clone().unwrap_or_default(),
                        "header" => String::new(),
                        _ => text.clone(),
                    };
                    if !text.is_empty() {
                        out.push(self.run(text, props, link));
                    }
                }
                _ => {}
            }
        }
    }

    fn paragraphs(
        &self,
        blocks: &[Block],
        context: &TextContext<'_>,
        list: Option<ListPosition<'_>>,
        label_pending: &mut bool,
        out: &mut Vec<TextParagraph>,
    ) {
        let styles = self.styles;
        for block in blocks {
            match block {
                Block::Paragraph(paragraph) => {
                    let depth = list.map_or(0, |list| list.depth);
                    let mut props = paragraph
                        .style
                        .as_deref()
                        .map(|name| styles.inherited(context.scope, Family::Paragraph, name))
                        .unwrap_or_default();
                    match context.outline.as_deref().filter(|_| depth >= 2) {
                        // Deeper outline levels have their own master styles.
                        Some(prefix) => {
                            let mut level = styles.inherited(
                                Scope::Styles,
                                Family::Presentation,
                                &format!("{prefix}{depth}"),
                            );
                            level.inherit(&context.base);
                            props.inherit(&level);
                        }
                        None => props.inherit(&context.base),
                    }
                    let level = list
                        .and_then(|list| list.style)
                        .and_then(|style| style.levels.get(&(depth.min(10) as u8)));
                    let paragraph_props = &props.paragraph;
                    let spacing = |value: Option<rust_odf::units::Length>| {
                        value.map(|value| TextSpacing {
                            value: value.points(),
                            unit: TextSpacingUnit::Points,
                        })
                    };
                    let mut runs = Vec::new();
                    self.runs(&paragraph.inlines, context, &props, None, &mut runs);
                    out.push(TextParagraph {
                        runs,
                        alignment: paragraph_props.align.map(|align| {
                            match align {
                                Align::Start | Align::Left => "left",
                                Align::End | Align::Right => "right",
                                Align::Center => "center",
                                Align::Justify => "justify",
                            }
                            .to_owned()
                        }),
                        level: (depth > 0).then(|| depth - 1),
                        bullet: level.filter(|_| std::mem::take(label_pending)).and_then(bullet),
                        line_spacing: match paragraph_props.line_height {
                            Some(LineHeight::Percent(percent)) => Some(TextSpacing {
                                value: percent / 100.0,
                                unit: TextSpacingUnit::Percent,
                            }),
                            Some(LineHeight::Exact(height) | LineHeight::AtLeast(height)) => {
                                spacing(Some(height))
                            }
                            Some(LineHeight::Leading(_)) | None => None,
                        },
                        space_before: spacing(paragraph_props.margin_top),
                        space_after: spacing(paragraph_props.margin_bottom),
                        rtl: paragraph_props.rtl,
                        margin_left_emu: level
                            .and_then(|level| level.margin_left)
                            .or(paragraph_props.margin_left)
                            .map(|margin| margin.0),
                        indent_emu: level
                            .and_then(|level| level.text_indent)
                            .or(paragraph_props.text_indent)
                            .map(|indent| indent.0),
                        tab_stops: paragraph_props.tab_stops.as_ref().map(|stops| {
                            stops
                                .iter()
                                .map(|stop| TextTabStop {
                                    position_emu: stop.position.0,
                                    alignment: match stop.kind {
                                        TabKind::Left => TextTabAlignment::Left,
                                        TabKind::Center => TextTabAlignment::Center,
                                        TabKind::Right => TextTabAlignment::Right,
                                        TabKind::Decimal => TextTabAlignment::Decimal,
                                    },
                                })
                                .collect()
                        }),
                        ..TextParagraph::default()
                    });
                }
                Block::List(nested) => {
                    let position = ListPosition {
                        style: nested
                            .style
                            .as_deref()
                            .and_then(|name| styles.list_style(context.scope, name))
                            .or(list.and_then(|list| list.style))
                            .or(context.list),
                        depth: list.map_or(1, |list| list.depth + 1),
                    };
                    for item in &nested.items {
                        let mut pending = !item.header;
                        self.paragraphs(&item.blocks, context, Some(position), &mut pending, out);
                    }
                }
                Block::Section(section) => {
                    self.paragraphs(&section.blocks, context, list, label_pending, out);
                }
                Block::Table(_) | Block::Shape(_) => {}
            }
        }
    }

    fn shape_paragraphs(&self, blocks: &[Block], context: &TextContext<'_>) -> Vec<TextParagraph> {
        let mut paragraphs = Vec::new();
        self.paragraphs(blocks, context, None, &mut false, &mut paragraphs);
        paragraphs
    }

    // ----- nodes -----

    fn node(
        &mut self,
        shape: &Shape,
        scope: Scope,
        slide_index: Option<usize>,
    ) -> Option<SlideNode> {
        self.next_node += 1;
        let id = shape
            .id
            .clone()
            .unwrap_or_else(|| format!("shape-{}", self.next_node));
        let name = shape.name.clone().unwrap_or_default();
        let mut transform = transform_of(shape);
        let alt_text = shape.description.clone().or_else(|| shape.title.clone());
        let metadata = (alt_text.is_some() || shape.link.is_some()).then(|| {
            Box::new(NodeMetadata {
                alt_text,
                accessibility_title: shape.title.clone(),
                hyperlink: shape.link.clone(),
                ..NodeMetadata::default()
            })
        });
        let (family, style_name) = match shape.presentation_style.as_deref() {
            Some(name) => (Family::Presentation, Some(name)),
            None => (Family::Graphic, shape.style.as_deref()),
        };
        let props = self.styles.resolve(scope, family, style_name).graphic;

        let image = |builder: &mut Self, href: &str, fallback: Option<&String>| {
            let asset_id = builder.asset(href)?;
            Some((asset_id, fallback.and_then(|href| builder.asset(href))))
        };
        let geometry = match &shape.kind {
            ShapeKind::Image { href, fallback } => {
                let Some((asset_id, fallback_asset_id)) = image(self, href, fallback.as_ref()) else {
                    self.warn(
                        "image-unavailable",
                        "An image that is linked or in an unknown format was skipped",
                        slide_index,
                    );
                    return None;
                };
                let context = self.text_context(shape, scope, slide_index.map_or(0, |index| index + 1));
                let caption = self.shape_paragraphs(&shape.text, &context);
                let captioned = caption.iter().any(|paragraph| !paragraph.runs.is_empty());
                let picture = SlideNode::Image {
                    metadata,
                    model_3d: None,
                    ink: None,
                    id: if captioned { format!("{id}:picture") } else { id.clone() },
                    name: name.clone(),
                    transform: transform.clone(),
                    asset_id,
                    fallback_asset_id,
                    crop: None,
                    opacity: None,
                    preserve_aspect_ratio: false,
                    effects: None,
                    picture_svg: None,
                    fallback_picture: None,
                    paint_bounds: None,
                    source_xml: None,
                };
                if !captioned {
                    return Some(picture);
                }
                // Text typed onto a picture is drawn over it, in the same frame.
                let text = SlideNode::Shape {
                    metadata: None,
                    id: format!("{id}:text"),
                    name: String::new(),
                    transform: transform.clone(),
                    geometry: ShapeGeometry {
                        preset: Some("rect".into()),
                        path: None,
                    },
                    shape_svg: None,
                    paint_bounds: None,
                    source_xml: None,
                    fill: Some(FillStyle::None),
                    line: None,
                    paragraphs: caption,
                    vertical_alignment: props.vertical_align.map(|align| match align {
                        VerticalAlign::Top => VerticalAlignment::Top,
                        VerticalAlign::Middle => VerticalAlignment::Middle,
                        VerticalAlign::Bottom => VerticalAlignment::Bottom,
                    }),
                    text_insets: None,
                    autofit: None,
                    text_rotation: None,
                    text_transform: None,
                    vertical_text: None,
                    horizontal_overflow: None,
                    vertical_overflow: None,
                    text_wrap: None,
                    column_count: None,
                    column_spacing: None,
                    right_to_left_columns: None,
                    space_first_last_paragraph: None,
                };
                return Some(SlideNode::Group {
                    metadata: None,
                    id,
                    name,
                    child_transform: Some(transform.clone()),
                    transform,
                    children: vec![picture, text],
                });
            }
            ShapeKind::Object { href, replacement, .. } => {
                let preview = replacement
                    .as_deref()
                    .and_then(|href| image(self, href, None))
                    .map(|(asset_id, _)| asset_id);
                // A preview a browser can draw stands in for the object.
                // Otherwise the object's own content is used where it can be read.
                let drawable = preview.as_ref().and_then(|id| self.assets.get(id)).is_some_and(|asset| {
                    matches!(
                        asset.content_type.as_str(),
                        "image/png" | "image/jpeg" | "image/gif" | "image/bmp" | "image/svg+xml" | "image/webp"
                    )
                });
                let embedded = href
                    .as_deref()
                    .and_then(|directory| self.document.objects.get(directory))
                    .and_then(|object| object.document(&self.limits).ok());
                let picture = |asset_id: String, id: String, metadata: Option<Box<NodeMetadata>>| SlideNode::Image {
                    metadata,
                    model_3d: None,
                    ink: None,
                    id,
                    name: name.clone(),
                    transform: transform.clone(),
                    asset_id,
                    fallback_asset_id: None,
                    crop: None,
                    opacity: None,
                    preserve_aspect_ratio: false,
                    effects: None,
                    picture_svg: None,
                    fallback_picture: None,
                    paint_bounds: None,
                    source_xml: None,
                };
                let slide_number = slide_index.map_or(0, |index| index + 1);
                match embedded.as_ref().map(|embedded| (embedded, &embedded.body)) {
                    Some((_, Body::Chart(chart))) => {
                        let part = href.as_ref().map(|directory| format!("{directory}/content.xml"));
                        let data = chart_node(chart, part, metadata.clone(), name.clone(), transform.clone());
                        return Some(match preview.filter(|_| drawable) {
                            // The saved picture is what is drawn; the chart node carries the data.
                            Some(asset_id) => SlideNode::Group {
                                metadata: None,
                                children: vec![
                                    picture(asset_id, format!("{id}:preview"), None),
                                    with_id(data, format!("{id}:chart")),
                                ],
                                id,
                                name,
                                child_transform: Some(transform.clone()),
                                transform,
                            },
                            None => with_id(data, id),
                        });
                    }
                    Some((embedded, Body::Text(blocks))) if !drawable => {
                        let reader = Builder::new(embedded, self.limits);
                        let context = TextContext {
                            scope: Scope::Content,
                            base: embedded.styles.resolve(Scope::Content, Family::Paragraph, None),
                            outline: None,
                            list: None,
                            slide_number,
                            footer: None,
                            date_time: None,
                        };
                        let paragraphs = reader.shape_paragraphs(blocks, &context);
                        if paragraphs.iter().any(|paragraph| !paragraph.runs.is_empty()) {
                            return Some(text_box(metadata, id, name, transform, paragraphs));
                        }
                    }
                    Some((embedded, Body::Spreadsheet(tables))) if !drawable => {
                        let used = tables
                            .iter()
                            .map(Table::trimmed)
                            .find(|table| !table.rows.is_empty() && table.rows.len() <= 200 && table.columns.len() <= 50);
                        if let Some(table) = used {
                            let mut reader = Builder::new(embedded, self.limits);
                            return Some(reader.table(&table, Scope::Content, metadata, id, name, transform, slide_index));
                        }
                    }
                    _ => {}
                }
                self.warn(
                    "embedded-object-preview",
                    "An embedded object is imported as its preview image, or not at all when it has none",
                    slide_index,
                );
                return Some(match preview {
                    Some(asset_id) => picture(asset_id, id, metadata),
                    None => SlideNode::Unknown {
                        metadata,
                        id,
                        name,
                        transform,
                        feature: "embedded-object".into(),
                    },
                });
            }
            ShapeKind::Table(table) => {
                return Some(self.table(table, scope, metadata, id, name, transform, slide_index));
            }
            ShapeKind::Group(children) => {
                let children: Vec<SlideNode> = children
                    .iter()
                    .filter_map(|child| self.node(child, scope, slide_index))
                    .collect();
                // Children carry page coordinates, so the group is their bounding box.
                let mut bounds: Option<(i64, i64, i64, i64)> = None;
                for child in &children {
                    let child = match child {
                        SlideNode::Shape { transform, .. }
                        | SlideNode::Image { transform, .. }
                        | SlideNode::Group { transform, .. }
                        | SlideNode::Table { transform, .. }
                        | SlideNode::Chart { transform, .. }
                        | SlideNode::Media { transform, .. }
                        | SlideNode::Unknown { transform, .. } => transform,
                    };
                    let (right, bottom) = (child.x + child.width, child.y + child.height);
                    bounds = Some(match bounds {
                        Some((left, top, old_right, old_bottom)) => (
                            left.min(child.x),
                            top.min(child.y),
                            old_right.max(right),
                            old_bottom.max(bottom),
                        ),
                        None => (child.x, child.y, right, bottom),
                    });
                }
                let (left, top, right, bottom) = bounds.unwrap_or_default();
                let transform = Transform {
                    x: left,
                    y: top,
                    width: right - left,
                    height: bottom - top,
                    ..Transform::default()
                };
                return Some(SlideNode::Group {
                    metadata,
                    id,
                    name,
                    child_transform: Some(transform.clone()),
                    transform,
                    children,
                });
            }
            ShapeKind::Media { .. } => {
                return Some(SlideNode::Unknown {
                    metadata,
                    id,
                    name,
                    transform,
                    feature: "media".into(),
                });
            }
            ShapeKind::Unknown(feature) => {
                return Some(SlideNode::Unknown {
                    metadata,
                    id,
                    name,
                    transform,
                    feature: feature.clone(),
                });
            }
            ShapeKind::TextBox => ShapeGeometry {
                preset: Some("rect".into()),
                path: None,
            },
            ShapeKind::Rect { corner_radius } => ShapeGeometry {
                preset: Some(
                    if corner_radius.is_some_and(|radius| radius.0 > 0) {
                        "roundRect"
                    } else {
                        "rect"
                    }
                    .into(),
                ),
                path: None,
            },
            ShapeKind::Ellipse => ShapeGeometry {
                preset: Some("ellipse".into()),
                path: None,
            },
            ShapeKind::Line(segment) | ShapeKind::Connector(segment) => {
                transform = Transform {
                    x: segment.x1.0.min(segment.x2.0),
                    y: segment.y1.0.min(segment.y2.0),
                    width: (segment.x2.0 - segment.x1.0).abs(),
                    height: (segment.y2.0 - segment.y1.0).abs(),
                    rotation: None,
                    flip_horizontal: (segment.x2.0 < segment.x1.0).then_some(true),
                    flip_vertical: (segment.y2.0 < segment.y1.0).then_some(true),
                };
                ShapeGeometry {
                    preset: Some(
                        if matches!(shape.kind, ShapeKind::Connector(_)) {
                            "straightConnector1"
                        } else {
                            "line"
                        }
                        .into(),
                    ),
                    path: None,
                }
            }
            ShapeKind::Path { d, view_box } => {
                let size = [0.0, 0.0, transform.width as f64, transform.height as f64];
                ShapeGeometry {
                    preset: None,
                    path: normalize_path(d, view_box.unwrap_or(size)),
                }
            }
            ShapeKind::Polygon {
                points,
                view_box,
                closed,
            } => {
                let size = [0.0, 0.0, transform.width as f64, transform.height as f64];
                ShapeGeometry {
                    preset: None,
                    path: polygon_path(points, view_box.unwrap_or(size), *closed),
                }
            }
            ShapeKind::Custom(custom) => {
                if custom.mirror_horizontal {
                    transform.flip_horizontal = Some(true);
                }
                if custom.mirror_vertical {
                    transform.flip_vertical = Some(true);
                }
                match custom.preset() {
                    Some(preset) => ShapeGeometry {
                        preset: Some(preset.to_owned()),
                        path: None,
                    },
                    None => ShapeGeometry {
                        preset: None,
                        path: custom.plain_path(),
                    },
                }
            }
        };
        let geometry = if geometry.preset.is_none() && geometry.path.is_none() {
            self.warn(
                "shape-geometry-approximated",
                "A custom shape outline that needs formula evaluation is drawn as a rectangle",
                slide_index,
            );
            ShapeGeometry {
                preset: Some("rect".into()),
                path: None,
            }
        } else {
            geometry
        };
        let context = self.text_context(shape, scope, slide_index.map_or(0, |index| index + 1));
        let paragraphs = self.shape_paragraphs(&shape.text, &context);
        let padding = [
            props.padding_top,
            props.padding_right,
            props.padding_bottom,
            props.padding_left,
        ];
        Some(SlideNode::Shape {
            metadata,
            id,
            name,
            transform,
            geometry,
            shape_svg: None,
            paint_bounds: None,
            source_xml: None,
            fill: self.fill(&props),
            line: self.line(&props),
            paragraphs,
            vertical_alignment: props.vertical_align.map(|align| match align {
                VerticalAlign::Top => VerticalAlignment::Top,
                VerticalAlign::Middle => VerticalAlignment::Middle,
                VerticalAlign::Bottom => VerticalAlignment::Bottom,
            }),
            text_insets: padding.iter().any(Option::is_some).then(|| {
                let side = |index: usize| padding[index].map_or(0, |padding| padding.0);
                TextInsets {
                    top: side(0),
                    right: side(1),
                    bottom: side(2),
                    left: side(3),
                }
            }),
            autofit: if props.fit_to_size == Some(true) {
                Some(TextAutoFitMode::Normal)
            } else if props.auto_grow_height == Some(true) {
                Some(TextAutoFitMode::Shape)
            } else {
                None
            }
            .map(|mode| TextAutofit {
                mode,
                font_scale: None,
                line_spacing_reduction: None,
            }),
            text_rotation: None,
            text_transform: None,
            vertical_text: None,
            horizontal_overflow: None,
            vertical_overflow: None,
            text_wrap: props
                .wrap_text
                .map(|wrap| if wrap { "square" } else { "none" }.to_owned()),
            column_count: None,
            column_spacing: None,
            right_to_left_columns: None,
            space_first_last_paragraph: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn table(
        &mut self,
        table: &Table,
        scope: Scope,
        metadata: Option<Box<NodeMetadata>>,
        id: String,
        name: String,
        transform: Transform,
        slide_index: Option<usize>,
    ) -> SlideNode {
        let styles = self.styles;
        let mut column_widths: Vec<Option<i64>> = Vec::new();
        for column in &table.columns {
            let width = styles
                .resolve(scope, Family::TableColumn, column.style.as_deref())
                .table
                .column_width
                .map(|width| width.0);
            for _ in 0..column.repeated.min(MAX_REPEAT) {
                if column_widths.len() < MAX_REPEAT as usize {
                    column_widths.push(width);
                }
            }
        }
        let column_count = column_widths.len();
        let mut rows: Vec<Vec<TableCell>> = Vec::new();
        let mut row_heights: Vec<Option<i64>> = Vec::new();
        // Merge origins: (row, column, rows, columns).
        let mut origins: Vec<(usize, usize, usize, usize)> = Vec::new();
        let empty_cell = || TableCell {
            merge: None,
            row_span: None,
            col_span: None,
            fill: None,
            borders: BTreeMap::new(),
            paragraphs: Vec::new(),
            text_insets: None,
            vertical_alignment: None,
            text_rotation: None,
        };
        'rows: for row in &table.rows {
            let row_props = styles.resolve(scope, Family::TableRow, row.style.as_deref()).table;
            for _ in 0..row.repeated.min(MAX_REPEAT) {
                if rows.len() >= MAX_REPEAT as usize {
                    self.warn("table-truncated", "A table was truncated at the row limit", slide_index);
                    break 'rows;
                }
                let row_index = rows.len();
                let mut cells: Vec<TableCell> = Vec::new();
                for cell in &row.cells {
                    for _ in 0..cell.repeated.min(MAX_REPEAT) {
                        let column_index = cells.len();
                        if column_index >= column_count.max(1).min(MAX_REPEAT as usize) && column_count > 0 {
                            break;
                        }
                        if cell.covered {
                            let origin = origins.iter().find(|(row, column, rows, columns)| {
                                (*row..row + rows).contains(&row_index)
                                    && (*column..column + columns).contains(&column_index)
                            });
                            let mut covered = empty_cell();
                            if let Some((row, column, _, _)) = origin {
                                covered.merge = Some(Box::new(TableCellMerge {
                                    horizontal: column_index > *column,
                                    vertical: row_index > *row,
                                    origin: Some(TableCellPosition {
                                        row: *row,
                                        column: *column,
                                    }),
                                }));
                            }
                            cells.push(covered);
                            continue;
                        }
                        let cell_props = styles.resolve(scope, Family::TableCell, cell.style.as_deref());
                        let (row_span, col_span) = (
                            cell.row_span.clamp(1, MAX_REPEAT) as usize,
                            cell.column_span.clamp(1, MAX_REPEAT) as usize,
                        );
                        if row_span > 1 || col_span > 1 {
                            origins.push((row_index, column_index, row_span, col_span));
                        }
                        let table_props = &cell_props.table;
                        let mut borders = BTreeMap::new();
                        for (side, border) in [
                            ("top", &table_props.border_top),
                            ("bottom", &table_props.border_bottom),
                            ("left", &table_props.border_left),
                            ("right", &table_props.border_right),
                        ] {
                            if let Some(line) = border.as_ref().and_then(border_line) {
                                borders.insert(side.to_owned(), line);
                            }
                        }
                        let context = TextContext {
                            scope,
                            base: cell_props.clone(),
                            outline: None,
                            list: None,
                            slide_number: slide_index.map_or(0, |index| index + 1),
                            footer: None,
                            date_time: None,
                        };
                        cells.push(TableCell {
                            merge: None,
                            row_span: (row_span > 1).then_some(row_span),
                            col_span: (col_span > 1).then_some(col_span),
                            fill: table_props
                                .background
                                .as_deref()
                                .filter(|value| !value.is_empty())
                                .map(|value| FillStyle::Solid {
                                    color: color(value, None),
                                }),
                            borders,
                            paragraphs: self.shape_paragraphs(&cell.blocks, &context),
                            text_insets: table_props.padding.map(|padding| TextInsets {
                                top: padding.0,
                                right: padding.0,
                                bottom: padding.0,
                                left: padding.0,
                            }),
                            vertical_alignment: table_props.vertical_align.map(|align| match align {
                                VerticalAlign::Top => VerticalAlignment::Top,
                                VerticalAlign::Middle => VerticalAlignment::Middle,
                                VerticalAlign::Bottom => VerticalAlignment::Bottom,
                            }),
                            text_rotation: None,
                        });
                    }
                }
                rows.push(cells);
                row_heights.push(
                    row_props
                        .row_height
                        .or(row_props.min_row_height)
                        .map(|height| height.0),
                );
            }
        }
        // Every row spans the same grid.
        let width = rows.iter().map(Vec::len).max().unwrap_or(0).max(column_count);
        for cells in &mut rows {
            cells.resize_with(width, empty_cell);
        }
        column_widths.resize(width, None);
        let share = |total: i64, count: usize| if count == 0 { 0 } else { total / count as i64 };
        let default_width = share(transform.width, width);
        let default_height = share(transform.height, rows.len());
        SlideNode::Table {
            metadata,
            id,
            name,
            transform,
            rows,
            column_widths: column_widths
                .into_iter()
                .map(|width| width.unwrap_or(default_width))
                .collect(),
            row_heights: row_heights
                .into_iter()
                .map(|height| height.unwrap_or(default_height))
                .collect(),
        }
    }

    // ----- slides -----

    fn slide(
        &mut self,
        index: usize,
        page: &Page,
        master_nodes: &BTreeMap<String, Vec<SlideNode>>,
    ) -> PresentationSlide {
        let styles = self.styles;
        let page_props = styles
            .resolve(Scope::Content, Family::DrawingPage, page.style.as_deref())
            .graphic;
        let master = page.master_page.as_deref().and_then(|name| styles.master_page(name));
        let master_props = styles
            .resolve(
                Scope::Styles,
                Family::DrawingPage,
                master.and_then(|master| master.style.as_deref()),
            )
            .graphic;
        let own_background = matches!(page_props.fill, Some(fill) if fill != Fill::None);
        let background = if own_background {
            self.fill(&page_props)
        } else {
            self.fill(&master_props)
        }
        .filter(|fill| !matches!(fill, FillStyle::None));

        let mut nodes = Vec::new();
        if page_props.background_objects_visible != Some(false) {
            if let Some(inherited) = master.and_then(|master| master_nodes.get(&master.name)) {
                let mut inherited = inherited.clone();
                prefix_ids(&mut inherited, "master:");
                nodes = inherited;
            }
        }
        for shape in &page.shapes {
            // An untouched placeholder shows only its prompt while editing.
            if shape.placeholder {
                continue;
            }
            nodes.extend(self.node(shape, Scope::Content, Some(index)));
        }
        // The master's footer, slide-number and date fields appear on slides that
        // ask for them. They follow the slide's own objects, as footer
        // placeholders do in a PowerPoint slide, so reading order ends with them.
        self.footer = page.footer_name.as_ref().and_then(|name| self.document.footers.get(name)).cloned();
        self.date_time = page
            .date_time_name
            .as_ref()
            .and_then(|name| self.document.date_times.get(name))
            .cloned();
        for shape in master.map(|master| master.shapes.as_slice()).unwrap_or_default() {
            let shown = match shape.class.as_deref() {
                Some("footer") => page_props.display_footer == Some(true) && self.footer.is_some(),
                Some("page-number") => page_props.display_page_number == Some(true),
                Some("date-time") => page_props.display_date_time == Some(true) && self.date_time.is_some(),
                _ => false,
            };
            if shown {
                let mut field: Vec<SlideNode> = self.node(shape, Scope::Styles, Some(index)).into_iter().collect();
                prefix_ids(&mut field, "master:");
                nodes.extend(field);
            }
        }
        self.footer = None;
        self.date_time = None;
        let notes = page
            .notes
            .iter()
            .filter(|shape| shape.class.as_deref() == Some("notes") && !shape.placeholder)
            .filter_map(|shape| {
                let text = plain_text(&shape.text);
                (!text.trim().is_empty()).then(|| {
                    let context = self.text_context(shape, Scope::Content, index + 1);
                    SlideNote {
                        text,
                        paragraphs: self.shape_paragraphs(&shape.text, &context),
                    }
                })
            })
            .collect();
        PresentationSlide {
            id: format!("slide-{}", index + 1),
            index,
            name: page.name.clone(),
            hidden: page_props.hidden.filter(|hidden| *hidden),
            master_id: page.master_page.clone(),
            layout_id: None,
            background,
            transition: None,
            timing: None,
            nodes,
            notes,
            comments: Vec::new(),
            source_part: Some("content.xml".into()),
            warnings: Vec::new(),
        }
    }

    fn build(mut self, format: PresentationFormat) -> PresentationDocument {
        let document = self.document;
        let styles = self.styles;
        let pages: &[Page] = match &document.body {
            Body::Pages(pages) => pages,
            _ => &[],
        };
        let layout = pages
            .first()
            .and_then(|page| page.master_page.as_deref())
            .and_then(|name| styles.master_page(name))
            .or(styles.master_pages.first())
            .and_then(|master| master.page_layout.as_deref())
            .and_then(|name| styles.page_layouts.get(name));
        let dimension = |value: Option<rust_odf::units::Length>, fallback: i64| {
            value.map(|value| value.0).filter(|value| *value > 0).unwrap_or(fallback) as u64
        };
        let size = PresentationSize {
            width_emu: dimension(layout.and_then(|layout| layout.width), DEFAULT_SIZE.0),
            height_emu: dimension(layout.and_then(|layout| layout.height), DEFAULT_SIZE.1),
        };

        // Decorative master shapes are shown on every slide; placeholders are not.
        let mut master_nodes: BTreeMap<String, Vec<SlideNode>> = BTreeMap::new();
        let mut masters = Vec::new();
        for (index, master) in styles.master_pages.iter().enumerate() {
            let mut nodes: Vec<SlideNode> = master
                .shapes
                .iter()
                .filter(|shape| shape.class.is_none() && !shape.placeholder)
                .filter_map(|shape| self.node(shape, Scope::Styles, None))
                .collect();
            prefix_ids(&mut nodes, &format!("m{}-", index + 1));
            masters.push(SlideMaster {
                id: master.name.clone(),
                name: master.display_name.clone().or_else(|| Some(master.name.clone())),
                theme_id: None,
                transition: None,
                timing: None,
                nodes: nodes.clone(),
            });
            master_nodes.insert(master.name.clone(), nodes);
        }
        let slides: Vec<PresentationSlide> = pages
            .iter()
            .enumerate()
            .map(|(index, page)| self.slide(index, page, &master_nodes))
            .collect();

        let mut warnings = self.warnings;
        for message in &document.warnings {
            let mut warning = PresentationWarning::warning("opendocument-import", message.clone());
            warning.feature = Some("opendocument".into());
            warnings.push(warning);
        }
        let metadata = &document.metadata;
        PresentationDocument {
            document_id: None,
            format,
            size,
            slides,
            masters,
            layouts: Vec::new(),
            themes: Vec::new(),
            assets: self.assets,
            embedded_fonts: Vec::new(),
            warnings,
            metadata: (metadata.title.is_some() || metadata.subject.is_some() || metadata.creator.is_some())
                .then(|| PresentationMetadata {
                    title: metadata.title.clone(),
                    subject: metadata.subject.clone(),
                    creator: metadata.initial_creator.clone().or_else(|| metadata.creator.clone()),
                    company: None,
                }),
        }
    }
}

//! Drawing objects: frames, basic shapes, custom shapes and groups.

use base64::Engine;

use crate::package::internal_path;
use crate::text::{self, Block, Ctx, Table};
use crate::units::{parse_bool, parse_length, parse_number, Length};
use crate::xml::{attr, child, elements, is, owned, text_content, Node, DR3D, DRAW, MATHML, OFFICE, PRESENTATION, SVG, TABLE, TEXT, XLINK, XML};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shape {
    pub kind: ShapeKind,
    pub name: Option<String>,
    pub id: Option<String>,
    /// `draw:style-name`, a graphic-family style.
    pub style: Option<String>,
    /// `presentation:style-name`, a presentation-family style used instead of `style`.
    pub presentation_style: Option<String>,
    /// `draw:text-style-name`, a paragraph-family style for the shape's text.
    pub text_style: Option<String>,
    /// `presentation:class`, such as `title`, `subtitle`, `outline` or `notes`.
    pub class: Option<String>,
    /// An empty presentation placeholder showing only its prompt.
    pub placeholder: bool,
    /// `text:anchor-type`: `as-char`, `char`, `paragraph`, `page` or `frame`.
    pub anchor: Option<String>,
    pub x: Option<Length>,
    pub y: Option<Length>,
    pub width: Option<Length>,
    pub height: Option<Length>,
    /// `draw:transform`, a list such as `rotate (0.5) translate (2cm 3cm)`.
    pub transform: Option<String>,
    pub z_index: Option<i32>,
    pub title: Option<String>,
    pub description: Option<String>,
    /// The target of a `draw:a` wrapped around the shape.
    pub link: Option<String>,
    /// Text inside the shape or text box.
    pub text: Vec<Block>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum ShapeKind {
    #[default]
    TextBox,
    Image {
        /// A package path, a generated key into [`crate::OdfDocument::binaries`], or an external URL.
        href: String,
        /// A second rendition, usually a bitmap behind a vector image.
        fallback: Option<String>,
    },
    /// An embedded or linked object such as a chart, formula or OLE object.
    Object {
        /// The object's package directory.
        href: Option<String>,
        /// The saved preview image.
        replacement: Option<String>,
        /// MathML carried inline instead of in a sub-document.
        math: Option<String>,
    },
    Table(Table),
    Rect {
        corner_radius: Option<Length>,
    },
    Ellipse,
    Line(Segment),
    Connector(Segment),
    Path {
        d: String,
        view_box: Option<[f64; 4]>,
    },
    Polygon {
        points: String,
        view_box: Option<[f64; 4]>,
        closed: bool,
    },
    Custom(CustomGeometry),
    Group(Vec<Shape>),
    Media {
        href: String,
    },
    /// A drawing element that is recognised but not modelled.
    Unknown(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Segment {
    pub x1: Length,
    pub y1: Length,
    pub x2: Length,
    pub y2: Length,
}

/// `draw:enhanced-geometry` of a custom shape, unevaluated.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CustomGeometry {
    /// `draw:type`, such as `rectangle`, `ooxml-roundRect` or `non-primitive`.
    pub shape_type: Option<String>,
    pub path: Option<String>,
    pub view_box: Option<[f64; 4]>,
    pub modifiers: Vec<f64>,
    /// `(name, formula)` pairs in document order.
    pub equations: Vec<(String, String)>,
    pub mirror_horizontal: bool,
    pub mirror_vertical: bool,
}

fn view_box(node: Node<'_, '_>) -> Option<[f64; 4]> {
    let mut values = attr(node, SVG, "viewBox")?
        .split(|ch: char| ch.is_ascii_whitespace() || ch == ',')
        .filter(|part| !part.is_empty())
        .map(parse_number);
    let view_box = [values.next()??, values.next()??, values.next()??, values.next()??];
    Some(view_box)
}

fn length(node: Node<'_, '_>, name: &str) -> Option<Length> {
    attr(node, SVG, name).and_then(parse_length)
}

fn segment(node: Node<'_, '_>) -> Segment {
    Segment {
        x1: length(node, "x1").unwrap_or_default(),
        y1: length(node, "y1").unwrap_or_default(),
        x2: length(node, "x2").unwrap_or_default(),
        y2: length(node, "y2").unwrap_or_default(),
    }
}

/// Records an image or media reference and returns the key adapters use to find its bytes.
pub(crate) fn image_reference(node: Node<'_, '_>, ctx: &mut Ctx<'_>) -> Option<String> {
    if let Some(href) = attr(node, XLINK, "href").filter(|href| !href.is_empty()) {
        return Some(match internal_path(href) {
            Some(path) => {
                ctx.hrefs.insert(path.clone());
                path
            }
            None => href.to_owned(),
        });
    }
    let data = child(node, OFFICE, "binary-data")?;
    let encoded: Vec<u8> = text_content(data)
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if encoded.len() / 4 * 3 > ctx.limits.max_entry_bytes {
        ctx.error = Some(crate::OdfError::Limit("inline binary size".into()));
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).ok()?;
    let key = format!("inline:{}", ctx.inline.len() + 1);
    ctx.inline.insert(key.clone(), bytes);
    Some(key)
}

fn frame_kind(node: Node<'_, '_>, ctx: &mut Ctx<'_>, text: &mut Vec<Block>) -> ShapeKind {
    let mut images = elements(node).filter(|child| is(*child, DRAW, "image"));
    for content in elements(node) {
        let name = content.tag_name().name();
        match content.tag_name().namespace() {
            Some(DRAW) => match name {
                "text-box" => {
                    *text = text::parse_blocks(content, ctx);
                    return ShapeKind::TextBox;
                }
                "image" => {
                    let first = images.next();
                    // A picture can carry text typed onto it.
                    if let Some(image) = first {
                        *text = text::parse_blocks(image, ctx);
                    }
                    let href = first.and_then(|image| image_reference(image, ctx));
                    let fallback = images.next().and_then(|image| image_reference(image, ctx));
                    return match href {
                        Some(href) => ShapeKind::Image { href, fallback },
                        None => ShapeKind::Unknown("image".into()),
                    };
                }
                "object" | "object-ole" => {
                    let mut href = attr(content, XLINK, "href").and_then(internal_path);
                    if let Some(path) = &href {
                        ctx.objects.insert(path.clone());
                    }
                    if href.is_none() {
                        if let Some(document) = child(content, OFFICE, "document") {
                            let key = format!("inline-object:{}", ctx.inline_objects.len() + 1);
                            let source = document.standalone_source();
                            if source.len() > ctx.limits.max_entry_bytes {
                                ctx.error = Some(crate::OdfError::Limit("inline object size".into()));
                            } else {
                                ctx.inline_objects.insert(key.clone(), crate::EmbeddedObject { content: Some(source), styles: None });
                                href = Some(key);
                            }
                        }
                    }
                    let math = child(content, MATHML, "math").map(|math| math.source().to_owned());
                    return ShapeKind::Object {
                        href,
                        replacement: images.next().and_then(|image| image_reference(image, ctx)),
                        math,
                    };
                }
                "plugin" => {
                    return match image_reference(content, ctx) {
                        Some(href) => ShapeKind::Media { href },
                        None => ShapeKind::Unknown("plugin".into()),
                    };
                }
                "applet" | "floating-frame" => return ShapeKind::Unknown(name.to_owned()),
                _ => {}
            },
            Some(TABLE) if name == "table" => {
                return ShapeKind::Table(text::parse_table(content, ctx));
            }
            _ => {}
        }
    }
    ShapeKind::Unknown("frame".into())
}

/// Reads one drawing element. Elements that are not drawing objects, such as
/// a custom shape's geometry, return `None`.
pub(crate) fn parse_shape(node: Node<'_, '_>, ctx: &mut Ctx<'_>) -> Option<Shape> {
    let name = node.tag_name().name();
    if !matches!(node.tag_name().namespace(), Some(DRAW) | Some(DR3D)) {
        return None;
    }
    if node.tag_name().namespace() == Some(DR3D) {
        return (name == "scene").then(|| shape(node, ShapeKind::Unknown("3d-scene".into()), ctx, false));
    }
    if name == "a" {
        let href = owned(node, XLINK, "href");
        let mut inner = elements(node).find_map(|child| parse_shape(child, ctx))?;
        inner.link = href;
        return Some(inner);
    }
    if !ctx.enter() {
        return None;
    }
    let mut text = Vec::new();
    let mut has_text = true;
    let kind = match name {
        "frame" => {
            has_text = false;
            frame_kind(node, ctx, &mut text)
        }
        "rect" | "caption" | "measure" => ShapeKind::Rect {
            corner_radius: attr(node, DRAW, "corner-radius").and_then(parse_length),
        },
        "ellipse" | "circle" => ShapeKind::Ellipse,
        "line" => ShapeKind::Line(segment(node)),
        "connector" => ShapeKind::Connector(segment(node)),
        "path" => ShapeKind::Path {
            d: attr(node, SVG, "d").unwrap_or_default().to_owned(),
            view_box: view_box(node),
        },
        "polygon" | "polyline" => ShapeKind::Polygon {
            points: attr(node, DRAW, "points").unwrap_or_default().to_owned(),
            view_box: view_box(node),
            closed: name == "polygon",
        },
        "custom-shape" => {
            let geometry = child(node, DRAW, "enhanced-geometry");
            ShapeKind::Custom(geometry.map_or_else(CustomGeometry::default, |geometry| {
                let flag = |name| attr(geometry, DRAW, name).and_then(parse_bool) == Some(true);
                CustomGeometry {
                    shape_type: owned(geometry, DRAW, "type"),
                    path: owned(geometry, crate::xml::Ns::DrawExt, "enhanced-path").or_else(|| owned(geometry, DRAW, "enhanced-path")),
                    view_box: view_box(geometry),
                    modifiers: attr(geometry, DRAW, "modifiers")
                        .map(|value| value.split_whitespace().filter_map(parse_number).collect())
                        .unwrap_or_default(),
                    equations: elements(geometry)
                        .filter(|child| is(*child, DRAW, "equation"))
                        .filter_map(|equation| {
                            Some((
                                owned(equation, DRAW, "name")?,
                                attr(equation, DRAW, "formula")?.to_owned(),
                            ))
                        })
                        .collect(),
                    mirror_horizontal: flag("mirror-horizontal"),
                    mirror_vertical: flag("mirror-vertical"),
                }
            }))
        }
        "g" => {
            has_text = false;
            ShapeKind::Group(elements(node).filter_map(|child| parse_shape(child, ctx)).collect())
        }
        "regular-polygon" | "page-thumbnail" | "control" => {
            has_text = false;
            ShapeKind::Unknown(name.to_owned())
        }
        _ => {
            ctx.leave();
            return None;
        }
    };
    let mut result = shape(node, kind, ctx, has_text);
    if !text.is_empty() {
        result.text = text;
    }
    ctx.leave();
    Some(result)
}

fn shape(node: Node<'_, '_>, kind: ShapeKind, ctx: &mut Ctx<'_>, has_text: bool) -> Shape {
    Shape {
        kind,
        name: owned(node, DRAW, "name"),
        id: owned(node, DRAW, "id").or_else(|| owned(node, XML, "id")),
        style: owned(node, DRAW, "style-name"),
        presentation_style: owned(node, PRESENTATION, "style-name"),
        text_style: owned(node, DRAW, "text-style-name"),
        class: owned(node, PRESENTATION, "class"),
        placeholder: attr(node, PRESENTATION, "placeholder").and_then(parse_bool) == Some(true),
        anchor: owned(node, TEXT, "anchor-type"),
        x: length(node, "x"),
        y: length(node, "y"),
        width: length(node, "width"),
        height: length(node, "height"),
        transform: owned(node, DRAW, "transform"),
        z_index: attr(node, DRAW, "z-index").and_then(|value| value.parse().ok()),
        title: child(node, SVG, "title").map(text_content).filter(|text| !text.is_empty()),
        description: child(node, SVG, "desc").map(text_content).filter(|text| !text.is_empty()),
        link: None,
        text: if has_text {
            text::parse_blocks(node, ctx)
        } else {
            Vec::new()
        },
    }
}

/// One decoded step of a `draw:transform` list, in the order written.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransformStep {
    /// Counter-clockwise radians, as OpenDocument stores rotation.
    Rotate(f64),
    Translate(Length, Length),
    Scale(f64, f64),
    SkewX(f64),
    SkewY(f64),
    Matrix([f64; 6]),
}

/// Parses a `draw:transform` value. Unknown functions are skipped.
pub fn parse_transform(value: &str) -> Vec<TransformStep> {
    let mut steps = Vec::new();
    let mut rest = value;
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        let name = rest[..open].trim();
        let arguments: Vec<&str> = rest[open + 1..open + close]
            .split(|ch: char| ch.is_ascii_whitespace() || ch == ',')
            .filter(|part| !part.is_empty())
            .collect();
        let number = |index: usize| arguments.get(index).copied().and_then(parse_number);
        let length = |index: usize| arguments.get(index).copied().and_then(parse_length);
        let step = match name {
            "rotate" => number(0).map(TransformStep::Rotate),
            "translate" => length(0)
                .map(|x| TransformStep::Translate(x, length(1).unwrap_or_default())),
            "scale" => number(0).map(|x| TransformStep::Scale(x, number(1).unwrap_or(x))),
            "skewX" => number(0).map(TransformStep::SkewX),
            "skewY" => number(0).map(TransformStep::SkewY),
            "matrix" if arguments.len() == 6 => {
                let translation = |i| length(i).map(|v| v.0 as f64).or_else(|| number(i).map(|v| v * 360.0));
                (|| Some(TransformStep::Matrix([number(0)?, number(1)?, number(2)?, number(3)?, translation(4)?, translation(5)?])))()
            },
            _ => None,
        };
        steps.extend(step);
        rest = &rest[open + close + 1..];
    }
    steps
}

/// A shape's placement on its page after applying `draw:transform`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Placement {
    pub x: Length,
    pub y: Length,
    pub width: Length,
    pub height: Length,
    /// Clockwise degrees about the shape's centre.
    pub rotation: f64,
}

impl Shape {
    /// Resolves position, size and rotation. A rotated OpenDocument shape is
    /// stored unrotated at the origin with a rotate-then-translate transform;
    /// this returns the equivalent box rotated about its own centre.
    pub fn placement(&self) -> Placement {
        let width = self.width.unwrap_or_default().0 as f64;
        let height = self.height.unwrap_or_default().0 as f64;
        let [a, b, c, d, e, f] = self.affine_transform();
        let scale_x = a.hypot(b);
        let scale_y = c.hypot(d);
        let centre_x = a * width / 2.0 + c * height / 2.0 + e;
        let centre_y = b * width / 2.0 + d * height / 2.0 + f;
        let (width, height) = (width * scale_x, height * scale_y);
        Placement {
            x: Length((centre_x - width / 2.0).round() as i64),
            y: Length((centre_y - height / 2.0).round() as i64),
            width: Length(width.round() as i64),
            height: Length(height.round() as i64),
            rotation: b.atan2(a).to_degrees().rem_euclid(360.0),
        }
    }

    /// Maps untransformed local coordinates to page coordinates, in EMUs.
    pub fn affine_transform(&self) -> [f64; 6] {
        let (mut a, mut b, mut c, mut d) = (1.0f64, 0.0f64, 0.0f64, 1.0f64);
        let mut e = self.x.unwrap_or_default().0 as f64;
        let mut f = self.y.unwrap_or_default().0 as f64;
        for step in self.transform.as_deref().map(parse_transform).unwrap_or_default() {
            let [sa, sb, sc, sd, se, sf] = match step {
                TransformStep::Rotate(angle) => [angle.cos(), -angle.sin(), angle.sin(), angle.cos(), 0.0, 0.0],
                TransformStep::Translate(x, y) => [1.0, 0.0, 0.0, 1.0, x.0 as f64, y.0 as f64],
                TransformStep::Scale(x, y) => [x, 0.0, 0.0, y, 0.0, 0.0],
                TransformStep::SkewX(angle) => [1.0, 0.0, -angle.tan(), 1.0, 0.0, 0.0],
                TransformStep::SkewY(angle) => [1.0, -angle.tan(), 0.0, 1.0, 0.0, 0.0],
                TransformStep::Matrix(matrix) => matrix,
            };
            (a, b, c, d, e, f) = (
                sa * a + sc * b, sb * a + sd * b, sa * c + sc * d,
                sb * c + sd * d, sa * e + sc * f + se, sb * e + sd * f + sf,
            );
        }
        [a, b, c, d, e, f]
    }

}

/// OpenDocument custom shape types and the DrawingML preset each one matches.
const PRESETS: &[(&str, &str)] = &[
    ("rectangle", "rect"),
    ("round-rectangle", "roundRect"),
    ("ellipse", "ellipse"),
    ("isosceles-triangle", "triangle"),
    ("right-triangle", "rtTriangle"),
    ("diamond", "diamond"),
    ("parallelogram", "parallelogram"),
    ("trapezoid", "trapezoid"),
    ("pentagon", "pentagon"),
    ("hexagon", "hexagon"),
    ("octagon", "octagon"),
    ("cross", "plus"),
    ("ring", "donut"),
    ("block-arc", "blockArc"),
    ("star4", "star4"),
    ("star5", "star5"),
    ("star6", "star6"),
    ("star8", "star8"),
    ("star12", "star12"),
    ("star24", "star24"),
    ("right-arrow", "rightArrow"),
    ("left-arrow", "leftArrow"),
    ("up-arrow", "upArrow"),
    ("down-arrow", "downArrow"),
    ("left-right-arrow", "leftRightArrow"),
    ("up-down-arrow", "upDownArrow"),
    ("quad-arrow", "quadArrow"),
    ("circular-arrow", "circularArrow"),
    ("notched-right-arrow", "notchedRightArrow"),
    ("striped-right-arrow", "stripedRightArrow"),
    ("pentagon-right", "homePlate"),
    ("chevron", "chevron"),
    ("can", "can"),
    ("cube", "cube"),
    ("heart", "heart"),
    ("sun", "sun"),
    ("moon", "moon"),
    ("smiley", "smileyFace"),
    ("lightning", "lightningBolt"),
    ("cloud", "cloud"),
    ("frame", "frame"),
    ("bevel", "bevel"),
    ("paper", "foldedCorner"),
    ("forbidden", "noSmoking"),
    ("rectangular-callout", "wedgeRectCallout"),
    ("round-rectangular-callout", "wedgeRoundRectCallout"),
    ("round-callout", "wedgeEllipseCallout"),
    ("cloud-callout", "cloudCallout"),
    ("bracket-pair", "bracketPair"),
    ("brace-pair", "bracePair"),
    ("left-bracket", "leftBracket"),
    ("right-bracket", "rightBracket"),
    ("left-brace", "leftBrace"),
    ("right-brace", "rightBrace"),
    ("horizontal-scroll", "horizontalScroll"),
    ("vertical-scroll", "verticalScroll"),
    ("flowchart-process", "flowChartProcess"),
    ("flowchart-alternate-process", "flowChartAlternateProcess"),
    ("flowchart-decision", "flowChartDecision"),
    ("flowchart-data", "flowChartInputOutput"),
    ("flowchart-predefined-process", "flowChartPredefinedProcess"),
    ("flowchart-internal-storage", "flowChartInternalStorage"),
    ("flowchart-document", "flowChartDocument"),
    ("flowchart-multidocument", "flowChartMultidocument"),
    ("flowchart-terminator", "flowChartTerminator"),
    ("flowchart-preparation", "flowChartPreparation"),
    ("flowchart-manual-input", "flowChartManualInput"),
    ("flowchart-manual-operation", "flowChartManualOperation"),
    ("flowchart-connector", "flowChartConnector"),
    ("flowchart-off-page-connector", "flowChartOffpageConnector"),
    ("flowchart-card", "flowChartPunchedCard"),
    ("flowchart-punched-tape", "flowChartPunchedTape"),
    ("flowchart-summing-junction", "flowChartSummingJunction"),
    ("flowchart-or", "flowChartOr"),
    ("flowchart-collate", "flowChartCollate"),
    ("flowchart-sort", "flowChartSort"),
    ("flowchart-extract", "flowChartExtract"),
    ("flowchart-merge", "flowChartMerge"),
    ("flowchart-stored-data", "flowChartOnlineStorage"),
    ("flowchart-delay", "flowChartDelay"),
    ("flowchart-sequential-access", "flowChartMagneticTape"),
    ("flowchart-magnetic-disk", "flowChartMagneticDisk"),
    ("flowchart-direct-access-storage", "flowChartMagneticDrum"),
    ("flowchart-display", "flowChartDisplay"),
    ("mso-spt202", "rect"),
];

impl CustomGeometry {
    /// The DrawingML preset geometry this custom shape corresponds to.
    ///
    /// LibreOffice names shapes it imported from Office Open XML
    /// `ooxml-<preset>`; its own shape types map through a fixed table.
    pub fn preset(&self) -> Option<&str> {
        let shape_type = self.shape_type.as_deref()?;
        if let Some(preset) = shape_type.strip_prefix("ooxml-") {
            return (!preset.is_empty()).then_some(preset);
        }
        PRESETS
            .iter()
            .find(|(name, _)| *name == shape_type)
            .map(|(_, preset)| *preset)
    }

    /// The outline as a path in the unit square, when `draw:enhanced-path`
    /// uses only literal coordinates with move, line, curve and close
    /// commands. Paths that need equations, modifiers or arcs return `None`.
    pub fn plain_path(&self) -> Option<String> {
        let path = self.path.as_deref()?;
        let mut svg = String::with_capacity(path.len());
        for ch in path.chars() {
            match ch {
                // End of sub-path, no fill, no stroke: irrelevant to the outline.
                'N' | 'F' | 'S' => svg.push(' '),
                'M' | 'L' | 'C' | 'Z' | ' ' | ',' | '.' | '-' | '0'..='9' => svg.push(ch),
                _ => return None,
            }
        }
        normalize_path(&svg, self.view_box.unwrap_or([0.0, 0.0, 21_600.0, 21_600.0]))
    }
}

fn path_number(value: f64, out: &mut String) {
    let rounded = (value * 100_000.0).round() / 100_000.0;
    let text = format!("{rounded}");
    out.push(' ');
    out.push_str(if text == "-0" { "0" } else { &text });
}

/// Rewrites SVG path data as absolute commands scaled into the unit square of
/// its view box, the form the presentation model stores custom outlines in.
pub fn normalize_path(d: &str, view_box: [f64; 4]) -> Option<String> {
    let [min_x, min_y, width, height] = view_box;
    if !(width > 0.0 && height > 0.0) || d.len() > 4 * 1024 * 1024 {
        return None;
    }
    let bytes = d.as_bytes();
    let mut index = 0;
    let number = |index: &mut usize| -> Option<f64> {
        while bytes.get(*index).is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b',') {
            *index += 1;
        }
        let start = *index;
        if matches!(bytes.get(*index), Some(b'-' | b'+')) {
            *index += 1;
        }
        let mut seen_dot = false;
        while let Some(byte) = bytes.get(*index) {
            match byte {
                b'0'..=b'9' => *index += 1,
                b'.' if !seen_dot => {
                    seen_dot = true;
                    *index += 1;
                }
                b'e' | b'E' if bytes.get(*index + 1).is_some_and(|next| next.is_ascii_digit() || matches!(next, b'-' | b'+')) => {
                    *index += 2;
                    while bytes.get(*index).is_some_and(u8::is_ascii_digit) {
                        *index += 1;
                    }
                    break;
                }
                _ => break,
            }
        }
        d[start..*index].parse::<f64>().ok().filter(|value| value.is_finite())
    };
    let mut out = String::new();
    let (mut x, mut y, mut start_x, mut start_y) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    // The previous segment's last control point, for smooth curves.
    let mut control: Option<(char, f64, f64)> = None;
    let mut command = None;
    loop {
        while bytes.get(index).is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b',') {
            index += 1;
        }
        let Some(byte) = bytes.get(index) else {
            break;
        };
        if byte.is_ascii_alphabetic() {
            command = Some(*byte as char);
            index += 1;
            if matches!(command, Some('Z' | 'z')) {
                out.push_str(" Z");
                (x, y) = (start_x, start_y);
                control = None;
                continue;
            }
        }
        let Some(current) = command else {
            return None;
        };
        let relative = current.is_ascii_lowercase();
        let point = |index: &mut usize, from: (f64, f64)| -> Option<(f64, f64)> {
            let (px, py) = (number(index)?, number(index)?);
            Some(if relative { (from.0 + px, from.1 + py) } else { (px, py) })
        };
        let emit = |name: &str, points: &[(f64, f64)], out: &mut String| {
            out.push(' ');
            out.push_str(name);
            for (px, py) in points {
                path_number((px - min_x) / width, out);
                path_number((py - min_y) / height, out);
            }
        };
        match current.to_ascii_uppercase() {
            'M' => {
                (x, y) = point(&mut index, (x, y))?;
                (start_x, start_y) = (x, y);
                emit("M", &[(x, y)], &mut out);
                // Further coordinate pairs are implicit line segments.
                command = Some(if relative { 'l' } else { 'L' });
                control = None;
            }
            'L' => {
                (x, y) = point(&mut index, (x, y))?;
                emit("L", &[(x, y)], &mut out);
                control = None;
            }
            'H' => {
                let value = number(&mut index)?;
                x = if relative { x + value } else { value };
                emit("L", &[(x, y)], &mut out);
                control = None;
            }
            'V' => {
                let value = number(&mut index)?;
                y = if relative { y + value } else { value };
                emit("L", &[(x, y)], &mut out);
                control = None;
            }
            'C' | 'S' => {
                let first = if current.to_ascii_uppercase() == 'C' {
                    point(&mut index, (x, y))?
                } else {
                    match control {
                        Some(('C', cx, cy)) => (2.0 * x - cx, 2.0 * y - cy),
                        _ => (x, y),
                    }
                };
                let second = point(&mut index, (x, y))?;
                let end = point(&mut index, (x, y))?;
                emit("C", &[first, second, end], &mut out);
                control = Some(('C', second.0, second.1));
                (x, y) = end;
            }
            'Q' | 'T' => {
                let first = if current.to_ascii_uppercase() == 'Q' {
                    point(&mut index, (x, y))?
                } else {
                    match control {
                        Some(('Q', cx, cy)) => (2.0 * x - cx, 2.0 * y - cy),
                        _ => (x, y),
                    }
                };
                let end = point(&mut index, (x, y))?;
                emit("Q", &[first, end], &mut out);
                control = Some(('Q', first.0, first.1));
                (x, y) = end;
            }
            'A' => {
                let (rx, ry, rotation) = (number(&mut index)?, number(&mut index)?, number(&mut index)?);
                let (large, sweep) = (number(&mut index)?, number(&mut index)?);
                let end = point(&mut index, (x, y))?;
                out.push_str(" A");
                path_number(rx.abs() / width, &mut out);
                path_number(ry.abs() / height, &mut out);
                path_number(rotation, &mut out);
                path_number(f64::from(u8::from(large != 0.0)), &mut out);
                path_number(f64::from(u8::from(sweep != 0.0)), &mut out);
                path_number((end.0 - min_x) / width, &mut out);
                path_number((end.1 - min_y) / height, &mut out);
                control = None;
                (x, y) = end;
            }
            _ => return None,
        }
    }
    let out = out.trim_start().to_owned();
    (!out.is_empty()).then_some(out)
}

/// The outline of a `draw:polygon` or `draw:polyline` as a unit-square path.
pub fn polygon_path(points: &str, view_box: [f64; 4], closed: bool) -> Option<String> {
    let mut d = String::with_capacity(points.len() + 8);
    for (index, pair) in points.split_whitespace().enumerate() {
        d.push_str(if index == 0 { "M " } else { " L " });
        d.push_str(pair);
    }
    if closed {
        d.push_str(" Z");
    }
    normalize_path(&d, view_box)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_transform_lists() {
        assert_eq!(
            parse_transform("rotate (1.5707963267949) translate (2cm 3cm)"),
            vec![
                TransformStep::Rotate(1.5707963267949),
                TransformStep::Translate(Length(720_000), Length(1_080_000)),
            ]
        );
        assert_eq!(parse_transform("matrix(1 0 0 1 0 0)"), vec![TransformStep::Matrix([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])]);
    }

    #[test]
    fn maps_custom_shapes_to_presets_and_plain_paths() {
        let geometry = |shape_type: &str, path: &str| CustomGeometry {
            shape_type: Some(shape_type.into()),
            path: Some(path.into()),
            ..CustomGeometry::default()
        };
        assert_eq!(geometry("ooxml-roundRect", "").preset(), Some("roundRect"));
        assert_eq!(geometry("flowchart-decision", "").preset(), Some("flowChartDecision"));
        assert_eq!(geometry("non-primitive", "").preset(), None);
        assert_eq!(
            geometry("non-primitive", "M 0 0 L 21600 0 10800 21600 Z N").plain_path().as_deref(),
            Some("M 0 0 L 1 0 L 0.5 1 Z")
        );
        assert_eq!(geometry("non-primitive", "M ?f0 0 L 21600 $1 Z").plain_path(), None);
        assert_eq!(geometry("non-primitive", "M 0 0 A 1 1 2 2 3 3 4 4").plain_path(), None);
    }

    #[test]
    fn normalizes_relative_and_smooth_path_commands() {
        assert_eq!(
            normalize_path("m10 10h80v80h-80z", [0.0, 0.0, 100.0, 100.0]).as_deref(),
            Some("M 0.1 0.1 L 0.9 0.1 L 0.9 0.9 L 0.1 0.9 Z")
        );
        assert_eq!(
            normalize_path("M0,0 C0,50 50,100 100,100 S200,150 200,200", [0.0, 0.0, 200.0, 200.0]).as_deref(),
            Some("M 0 0 C 0 0.25 0.25 0.5 0.5 0.5 C 0.75 0.5 1 0.75 1 1")
        );
        assert_eq!(
            polygon_path("0,0 50,100 100,0", [0.0, 0.0, 100.0, 100.0], true).as_deref(),
            Some("M 0 0 L 0.5 1 L 1 0 Z")
        );
        assert_eq!(normalize_path("M 0 0 L", [0.0, 0.0, 1.0, 1.0]), None);
        assert_eq!(normalize_path("M 0 0", [0.0, 0.0, 0.0, 1.0]), None);
    }

    #[test]
    fn places_plain_and_rotated_shapes() {
        let plain = Shape {
            x: Some(Length(100)),
            y: Some(Length(200)),
            width: Some(Length(400)),
            height: Some(Length(600)),
            ..Shape::default()
        };
        assert_eq!(
            plain.placement(),
            Placement {
                x: Length(100),
                y: Length(200),
                width: Length(400),
                height: Length(600),
                rotation: 0.0,
            }
        );
        // A 4pt by 2pt box turned a quarter turn counter-clockwise about the
        // origin covers x 0..2pt, y -4pt..0, then moves by (10pt, 24pt). Its
        // centre lands at (11pt, 22pt), so the unrotated box starts at (9pt, 21pt).
        let placement = Shape {
            width: Some(Length::from_points(4.0)),
            height: Some(Length::from_points(2.0)),
            transform: Some("rotate (1.5707963267949) translate (10pt 24pt)".into()),
            ..Shape::default()
        }
        .placement();
        assert!((placement.x.0 - Length::from_points(9.0).0).abs() <= 1);
        assert!((placement.y.0 - Length::from_points(21.0).0).abs() <= 1);
        assert_eq!(placement.width, Length::from_points(4.0));
        assert_eq!(placement.height, Length::from_points(2.0));
        assert!((placement.rotation - 270.0).abs() < 1e-6);
    }
}

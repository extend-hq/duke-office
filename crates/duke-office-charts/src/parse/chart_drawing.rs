use super::{art, chart_colors::ChartColors, font_metrics, util::escape_xml_text};
use crate::RenderOptions;
use art::{child, Context};
use roxmltree::{Document, Node, ParsingOptions};
use duke_office_art::PictureContext;
use std::fmt::Write;

const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/chartDrawing";

fn number(node: Option<Node>, name: &str, default: f64) -> f64 {
    node.and_then(|n| n.attribute(name))
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite() && v.abs() <= 1e12)
        .unwrap_or(default)
}

fn enabled(node: Option<Node>, name: &str) -> bool {
    node.and_then(|n| n.attribute(name))
        .is_some_and(|s| matches!(s, "1" | "true" | "on"))
}

fn transform<'a, 'input>(node: Node<'a, 'input>) -> Option<Node<'a, 'input>> {
    child(
        node,
        if node.tag_name().name() == "grpSp" {
            "grpSpPr"
        } else {
            "spPr"
        },
    )
    .and_then(|n| child(n, "xfrm"))
}

fn local_frame(node: Node) -> [f64; 4] {
    let t = transform(node);
    let off = t.and_then(|t| child(t, "off"));
    let ext = t.and_then(|t| child(t, "ext"));
    [
        number(off, "x", 0.0) / 9525.0,
        number(off, "y", 0.0) / 9525.0,
        number(ext, "cx", 0.0) / 9525.0,
        number(ext, "cy", 0.0) / 9525.0,
    ]
}

fn rotation(node: Option<Node>, width: f64, height: f64) -> String {
    let angle = number(node, "rot", 0.0) / 60000.0;
    let sx = if enabled(node, "flipH") { -1 } else { 1 };
    let sy = if enabled(node, "flipV") { -1 } else { 1 };
    format!(
        "translate({} {}) rotate({angle}) scale({sx} {sy}) translate({} {})",
        width / 2.0,
        height / 2.0,
        -width / 2.0,
        -height / 2.0
    )
}

struct Painter<'a> {
    context: Context<'a>,
    font: &'a str,
    next_id: usize,
    remaining: usize,
}

impl Painter<'_> {
    fn draw(&mut self, node: Node, frame: [f64; 4], depth: usize, svg: &mut String) {
        if depth > 32
            || self.remaining == 0
            || !frame
                .into_iter()
                .all(|v| v.is_finite() && v.abs() <= 1_000_000.0)
        {
            return;
        }
        self.remaining -= 1;
        let [x, y, width, height] = frame;
        if width < 0.0 || height < 0.0 {
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        write!(svg, r#"<g transform="translate({x} {y})">"#).unwrap();
        match node.tag_name().name() {
            "grpSp" => {
                let t = transform(node);
                let off = t.and_then(|t| child(t, "chOff"));
                let ext = t.and_then(|t| child(t, "chExt"));
                let cw = number(ext, "cx", width * 9525.0) / 9525.0;
                let ch = number(ext, "cy", height * 9525.0) / 9525.0;
                if cw > 0.0 && ch > 0.0 {
                    write!(
                        svg,
                        r#"<g transform="{} scale({} {}) translate({} {})">"#,
                        rotation(t, width, height),
                        width / cw,
                        height / ch,
                        -number(off, "x", 0.0) / 9525.0,
                        -number(off, "y", 0.0) / 9525.0
                    )
                    .unwrap();
                    for child in node
                        .children()
                        .filter(Node::is_element)
                        .filter(|n| matches!(n.tag_name().name(), "sp" | "cxnSp" | "pic" | "grpSp"))
                    {
                        self.draw(child, local_frame(child), depth + 1, svg);
                    }
                    svg.push_str("</g>");
                }
            }
            "sp" | "cxnSp" => {
                if let Some(properties) = child(node, "spPr") {
                    let mut properties_xml = art::canonical(properties);
                    if child(properties, "prstGeom").is_none()
                        && child(properties, "custGeom").is_none()
                    {
                        if let Some(end) = properties_xml.rfind("</") {
                            let preset = if node.tag_name().name() == "cxnSp" {
                                "line"
                            } else {
                                "rect"
                            };
                            properties_xml.insert_str(end,&format!(r#"<a:prstGeom xmlns:a="{}" prst="{preset}"><a:avLst/></a:prstGeom>"#,art::MAIN));
                        }
                    }
                    let style = child(node, "style").map(art::canonical).unwrap_or_default();
                    if let Some(rendered) = duke_office_art::render_shape(
                        &properties_xml,
                        &style,
                        width,
                        height,
                        &self.context,
                    ) {
                        self.append(&rendered.svg, id, svg);
                    } else {
                        svg.push_str(
                            "<desc>Some chart drawing appearance could not be rendered.</desc>",
                        );
                    }
                }
                if let Some(text) = child(node, "txBody") {
                    write!(
                        svg,
                        r#"<g transform="{}">"#,
                        rotation(transform(node), width, height)
                    )
                    .unwrap();
                    text_body(svg, text, width, height, &self.context, self.font);
                    svg.push_str("</g>");
                }
            }
            "pic" => {
                let rendered = child(node,"blipFill").and_then(|fill| {
                    let blip = child(fill,"blip")?;
                    let id = blip.attributes().find(|a| matches!(a.name(),"embed"|"link") && matches!(a.namespace(),Some(art::REL|"http://purl.oclc.org/ooxml/officeDocument/relationships")))?.value();
                    let source = self.context.resolve_image(id)?;
                    art::picture(fill,child(node,"spPr"),&source,width.max(1.0),height.max(1.0),&self.context)
                });
                if let Some(rendered) = rendered {
                    self.append(&rendered.svg, id, svg);
                } else {
                    svg.push_str("<desc>A chart drawing picture could not be rendered.</desc>");
                }
            }
            _ => svg.push_str("<desc>A chart drawing element is unsupported.</desc>"),
        }
        svg.push_str("</g>");
    }

    fn append(&self, source: &str, id: usize, output: &mut String) {
        if let Some(svg) = art::scoped_svg(source, &format!("cad{id}-")) {
            if let Some(inner) = art::inner_svg(&svg) {
                output.push_str(inner);
            }
        }
    }
}

#[derive(Clone)]
struct Run {
    text: String,
    size: f64,
    face: String,
    fill: String,
    bold: bool,
    italic: bool,
    underline: bool,
}

impl Run {
    fn width(&self) -> f64 {
        font_metrics::width(&self.text, self.size, &self.face, self.bold, self.italic)
            .unwrap_or(self.text.chars().count() as f64 * self.size * 0.52)
    }
}

fn text_body(
    svg: &mut String,
    node: Node,
    width: f64,
    height: f64,
    context: &Context,
    default_face: &str,
) {
    let body = child(node, "bodyPr");
    let inset = |name, default| number(body, name, default * 9525.0) / 9525.0;
    let left = inset("lIns", 9.6);
    let right = inset("rIns", 9.6);
    let top = inset("tIns", 4.8);
    let bottom = inset("bIns", 4.8);
    let available = (width - left - right).max(1.0);
    let wrap = body.and_then(|n| n.attribute("wrap")) != Some("none");
    let mut lines: Vec<(Vec<Run>, String, f64)> = Vec::new();
    for p in node
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "p")
    {
        let props = child(p, "pPr");
        let defaults = props.and_then(|n| child(n, "defRPr"));
        let align = props
            .and_then(|n| n.attribute("algn"))
            .unwrap_or("l")
            .to_owned();
        let mut line = Vec::new();
        let mut advance = 0.0;
        let mut line_height: f64 = 0.0;
        for element in p.children().filter(Node::is_element) {
            if element.tag_name().name() == "br" {
                lines.push((
                    std::mem::take(&mut line),
                    align.clone(),
                    line_height.max(14.0) * 1.2,
                ));
                advance = 0.0;
                line_height = 0.0;
                continue;
            }
            if !matches!(element.tag_name().name(), "r" | "fld") {
                continue;
            }
            let props = child(element, "rPr");
            let attr = |name| {
                props
                    .and_then(|n| n.attribute(name))
                    .or_else(|| defaults.and_then(|n| n.attribute(name)))
            };
            let size = attr("sz")
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|v| v.is_finite())
                .map_or(14.0, |v| (v / 75.0).clamp(1.0, 512.0));
            let face = props
                .and_then(|n| child(n, "latin"))
                .or_else(|| defaults.and_then(|n| child(n, "latin")))
                .and_then(|n| n.attribute("typeface"))
                .filter(|s| !s.starts_with('+'))
                .unwrap_or(default_face)
                .to_owned();
            let fill = props
                .and_then(|n| child(n, "solidFill"))
                .or_else(|| defaults.and_then(|n| child(n, "solidFill")))
                .and_then(|n| context.resolve_color(&art::canonical(n)))
                .map(|c| c.color)
                .unwrap_or_else(|| {
                    context
                        .theme
                        .get("tx1")
                        .cloned()
                        .unwrap_or_else(|| "#000000".into())
                });
            let Some(text) = child(element, "t").and_then(|n| n.text()) else {
                continue;
            };
            let template = Run {
                text: String::new(),
                size,
                face,
                fill,
                bold: matches!(attr("b"), Some("1" | "true")),
                italic: matches!(attr("i"), Some("1" | "true")),
                underline: attr("u").is_some_and(|v| v != "none"),
            };
            for token in text.split_inclusive(char::is_whitespace) {
                let run = Run {
                    text: token.to_owned(),
                    ..template.clone()
                };
                let run_width = run.width();
                if wrap && advance > 0.0 && advance + run_width > available {
                    lines.push((
                        std::mem::take(&mut line),
                        align.clone(),
                        line_height.max(14.0) * 1.2,
                    ));
                    advance = 0.0;
                    line_height = 0.0;
                }
                advance += run_width;
                line_height = line_height.max(size);
                line.push(run);
            }
        }
        lines.push((line, align, line_height.max(14.0) * 1.2));
    }
    let total: f64 = lines.iter().map(|(_, _, h)| h).sum();
    let mut y = top
        + match body.and_then(|n| n.attribute("anchor")) {
            Some("ctr") => (height - top - bottom - total).max(0.0) / 2.0,
            Some("b") => (height - top - bottom - total).max(0.0),
            _ => 0.0,
        };
    let angle = number(body, "rot", 0.0) / 60000.0
        + match body.and_then(|n| n.attribute("vert")) {
            Some("vert") => 90.0,
            Some("vert270") => 270.0,
            _ => 0.0,
        };
    write!(
        svg,
        r#"<g transform="rotate({angle} {} {})">"#,
        width / 2.0,
        height / 2.0
    )
    .unwrap();
    for (line, align, line_height) in lines {
        let line_width: f64 = line.iter().map(Run::width).sum();
        let mut x = left
            + match align.as_str() {
                "ctr" => (available - line_width) / 2.0,
                "r" => available - line_width,
                _ => 0.0,
            };
        let baseline = y + line.iter().map(|r| r.size).fold(14.0, f64::max);
        for run in line {
            write!(svg,r#"<text x="{x}" y="{baseline}" fill="{}" font-size="{}" font-family="{}" font-weight="{}" font-style="{}" text-decoration="{}" xml:space="preserve">{}</text>"#,run.fill,run.size,escape_xml_text(&run.face),if run.bold{"bold"}else{"normal"},if run.italic{"italic"}else{"normal"},if run.underline{"underline"}else{"none"},escape_xml_text(&run.text)).unwrap();
            x += run.width();
        }
        y += line_height;
    }
    svg.push_str("</g>");
    if body.is_some_and(|n| child(n, "prstTxWarp").is_some()) {
        svg.push_str("<desc>Warped chart drawing text uses unwarped text.</desc>");
    }
}

pub(super) fn append(
    svg: &mut String,
    root: Node,
    options: &RenderOptions,
    colors: &ChartColors,
    width: f64,
    height: f64,
) {
    let references: Vec<_> = root
        .children()
        .filter(|n| crate::chart::chart_node(*n, "userShapes"))
        .collect();
    if references.is_empty() {
        return;
    }
    let mut body = String::from(r#"<g data-chart-drawing="true" pointer-events="none">"#);
    let mut next_id = 0;
    for reference in references {
        let drawing = reference
            .attributes()
            .find(|a| {
                a.name() == "id"
                    && matches!(
                        a.namespace(),
                        Some(art::REL | "http://purl.oclc.org/ooxml/officeDocument/relationships")
                    )
            })
            .and_then(|a| options.drawings?.get(a.value()));
        let Some(drawing) = drawing else {
            body.push_str("<desc>A saved chart drawing is unavailable.</desc>");
            continue;
        };
        if drawing.xml.len() > 8 * 1024 * 1024 {
            body.push_str("<desc>A chart drawing exceeds rendering limits.</desc>");
            continue;
        }
        let Ok(document) = Document::parse_with_options(
            &drawing.xml,
            ParsingOptions {
                allow_dtd: false,
                nodes_limit: 50_000,
            },
        ) else {
            body.push_str("<desc>A saved chart drawing is invalid.</desc>");
            continue;
        };
        let context = Context {
            theme: &colors.theme,
            images: Some(&drawing.images),
            theme_xml: options.theme_xml,
            theme_override: options.theme_override_xml,
        };
        let mut painter = Painter {
            context,
            font: options.font_family.unwrap_or("Calibri"),
            next_id,
            remaining: 4096,
        };
        for anchor in document.root_element().children().filter(Node::is_element) {
            if !matches!(
                anchor.tag_name().namespace(),
                Some(DRAWING | "http://purl.oclc.org/ooxml/drawingml/chartDrawing")
            ) {
                continue;
            }
            let point = |parent: &str, name: &str| {
                child(anchor, parent)
                    .and_then(|n| child(n, name))
                    .and_then(|n| n.text())
                    .and_then(|s| s.parse::<f64>().ok())
                    .filter(|n| n.is_finite() && n.abs() < 1e4)
            };
            let Some((x, y)) = point("from", "x").zip(point("from", "y")) else {
                continue;
            };
            let size = match anchor.tag_name().name() {
                "relSizeAnchor" => point("to", "x")
                    .zip(point("to", "y"))
                    .map(|(right, bottom)| ((right - x) * width, (bottom - y) * height)),
                "absSizeAnchor" => Some((
                    number(child(anchor, "ext"), "cx", 0.0) / 9525.0,
                    number(child(anchor, "ext"), "cy", 0.0) / 9525.0,
                )),
                _ => None,
            };
            let Some((w, h)) = size else {
                continue;
            };
            for object in anchor
                .children()
                .filter(Node::is_element)
                .filter(|n| matches!(n.tag_name().name(), "sp" | "cxnSp" | "pic" | "grpSp"))
            {
                painter.draw(object, [x * width, y * height, w, h], 0, &mut body);
            }
        }
        next_id = painter.next_id;
    }
    body.push_str("</g>");
    if let Some(end) = svg.rfind("</svg>") {
        svg.insert_str(end, &body);
    }
}

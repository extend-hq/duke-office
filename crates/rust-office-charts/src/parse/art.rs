use super::chart_colors::color;
use super::util::escape_xml_text;
use super::ThemeColorMap;
use roxmltree::Node;
use rust_office_art::{PictureContext, ResolvedColor, StyleReferenceKind};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;

pub(super) const MAIN: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
pub(super) const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub(super) struct Context<'a> {
    pub theme: &'a ThemeColorMap,
    pub images: Option<&'a HashMap<String, String>>,
    pub theme_xml: Option<&'a str>,
    pub theme_override: Option<&'a str>,
}

impl PictureContext for Context<'_> {
    fn resolve_color(&self, xml: &str) -> Option<ResolvedColor> {
        let wrapped = format!(r#"<root xmlns:a="{MAIN}">{xml}</root>"#);
        let document = roxmltree::Document::parse(&wrapped).ok()?;
        let value = document
            .descendants()
            .find_map(|node| color(node, self.theme))?;
        let opacity = value
            .get(7..9)
            .and_then(|s| u8::from_str_radix(s, 16).ok())
            .map(|a| f64::from(a) / 255.0);
        Some(ResolvedColor {
            color: value.get(..7)?.to_owned(),
            opacity,
        })
    }

    fn resolve_image(&self, id: &str) -> Option<String> {
        safe_image(self.images?.get(id)?)
    }

    fn resolve_style_reference(
        &self,
        kind: StyleReferenceKind,
        index: u32,
        placeholder: &str,
    ) -> Option<String> {
        let (list, ordinal) = match kind {
            StyleReferenceKind::Fill if index >= 1001 => ("bgFillStyleLst", index - 1001),
            StyleReferenceKind::Fill => ("fillStyleLst", index.checked_sub(1)?),
            StyleReferenceKind::Line => ("lnStyleLst", index.checked_sub(1)?),
            StyleReferenceKind::Effect => ("effectStyleLst", index.checked_sub(1)?),
        };
        let resolve = |xml: &str| {
            let doc = roxmltree::Document::parse(xml).ok()?;
            let node = doc
                .descendants()
                .find(|n| n.is_element() && n.tag_name().name() == list)?
                .children()
                .filter(Node::is_element)
                .nth(ordinal as usize)?;
            let node = if kind == StyleReferenceKind::Effect {
                child(node, "effectLst").or_else(|| child(node, "effectDag"))?
            } else {
                node
            };
            let mut xml = canonical(node);
            if let Some(value) = self.resolve_color(placeholder) {
                let alpha = value.opacity.unwrap_or(1.0) * 100_000.0;
                let replacement = format!(
                    r#"<a:srgbClr val="{}"><a:alpha val="{alpha:.0}"/></a:srgbClr>"#,
                    &value.color[1..]
                );
                let doc = roxmltree::Document::parse(&xml).ok()?;
                let mut edits: Vec<_> = doc
                    .descendants()
                    .filter(|n| {
                        n.tag_name().name() == "schemeClr" && n.attribute("val") == Some("phClr")
                    })
                    .map(|n| {
                        let transforms = n
                            .children()
                            .filter(Node::is_element)
                            .map(canonical)
                            .collect::<String>();
                        (
                            n.range(),
                            replacement
                                .replace("</a:srgbClr>", &format!("{transforms}</a:srgbClr>")),
                        )
                    })
                    .collect();
                edits.sort_by_key(|(r, _)| r.start);
                for (range, value) in edits.into_iter().rev() {
                    xml.replace_range(range, &value);
                }
            }
            Some(xml)
        };
        self.theme_override
            .and_then(resolve)
            .or_else(|| self.theme_xml.and_then(resolve))
    }
}

pub(super) fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|n| n.is_element() && n.tag_name().name() == name)
}

pub(super) fn safe_image(source: &str) -> Option<String> {
    let (header, data) = source.split_once(',')?;
    if source.len() > 96 * 1024 * 1024
        || data.is_empty()
        || data.len() % 4 != 0
        || !matches!(
            header,
            "data:image/png;base64"
                | "data:image/jpeg;base64"
                | "data:image/gif;base64"
                | "data:image/webp;base64"
                | "data:image/bmp;base64"
                | "data:image/tiff;base64"
                | "data:image/x-tiff;base64"
        )
        || data
            .trim_end_matches('=')
            .bytes()
            .any(|c| !c.is_ascii_alphanumeric() && !matches!(c, b'+' | b'/'))
        || data.len() - data.trim_end_matches('=').len() > 2
    {
        return None;
    }
    if matches!(
        header,
        "data:image/bmp;base64" | "data:image/tiff;base64" | "data:image/x-tiff;base64"
    ) {
        rust_office_art::normalize_raster(source)
    } else {
        Some(source.to_owned())
    }
}

pub(super) fn canonical(node: Node) -> String {
    let mut namespaces = BTreeMap::new();
    for element in node.descendants().filter(Node::is_element) {
        for uri in element
            .tag_name()
            .namespace()
            .into_iter()
            .chain(element.attributes().filter_map(|a| a.namespace()))
        {
            let prefix = match uri {
                MAIN | "http://purl.oclc.org/ooxml/drawingml/main" => "a".into(),
                REL | "http://purl.oclc.org/ooxml/officeDocument/relationships" => "r".into(),
                "http://www.w3.org/XML/1998/namespace" => "xml".into(),
                _ => format!("ns{}", namespaces.len()),
            };
            namespaces.entry(uri).or_insert(prefix);
        }
    }
    fn write_node(node: Node, namespaces: &BTreeMap<&str, String>, root: bool, out: &mut String) {
        if node.is_text() {
            out.push_str(&escape_xml_text(node.text().unwrap_or_default()));
            return;
        }
        if !node.is_element() {
            return;
        }
        let name = |local: &str, uri: Option<&str>| {
            uri.and_then(|uri| namespaces.get(uri))
                .map_or_else(|| local.to_owned(), |prefix| format!("{prefix}:{local}"))
        };
        let tag = name(node.tag_name().name(), node.tag_name().namespace());
        write!(out, "<{tag}").unwrap();
        if root {
            for (uri, prefix) in namespaces {
                if prefix != "xml" {
                    write!(out, r#" xmlns:{prefix}="{}""#, escape_xml_text(uri)).unwrap();
                }
            }
        }
        for attr in node.attributes() {
            write!(
                out,
                r#" {}="{}""#,
                name(attr.name(), attr.namespace()),
                escape_xml_text(attr.value())
            )
            .unwrap();
        }
        out.push('>');
        for child in node.children() {
            write_node(child, namespaces, false, out);
        }
        write!(out, "</{tag}>").unwrap();
    }
    let mut out = String::new();
    write_node(node, &namespaces, true, &mut out);
    out
}

pub(super) fn scoped_svg(svg: &str, prefix: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(svg).ok()?;
    let ids: HashSet<_> = doc
        .descendants()
        .filter_map(|n| n.attribute("id"))
        .collect();
    let mut edits = Vec::new();
    for node in doc.descendants().filter(Node::is_element) {
        for attr in node.attributes() {
            let value = attr.value();
            let new = if attr.name() == "id" {
                Some(format!("{prefix}{value}"))
            } else if let Some(id) = value.strip_prefix('#').filter(|id| ids.contains(id)) {
                (attr.name() == "href").then(|| format!("#{prefix}{id}"))
            } else {
                value
                    .strip_prefix("url(#")
                    .and_then(|v| v.strip_suffix(')'))
                    .filter(|id| ids.contains(id))
                    .map(|id| format!("url(#{prefix}{id})"))
            };
            if let Some(value) = new {
                edits.push((attr.range_value(), value));
            }
        }
    }
    edits.sort_by_key(|(r, _)| r.start);
    let mut out = svg.to_owned();
    for (range, value) in edits.into_iter().rev() {
        out.replace_range(range, &value);
    }
    Some(
        out.replace("data-office-art-effect=", "data-chart-effect=")
            .replace("data-office-art-overflow=", "data-chart-overflow=")
            .replace("xlink:href=", "href="),
    )
}

pub(super) fn inner_svg(svg: &str) -> Option<&str> {
    Some(&svg[svg.find('>')? + 1..svg.rfind("</svg>")?])
}

pub(super) fn expand_effects(mut svg: String) -> String {
    if !svg.contains("data-chart-overflow=") {
        return svg;
    }
    let Ok(doc) = roxmltree::Document::parse(&svg) else {
        return svg;
    };
    let mut edits = Vec::new();
    let mut insertions = BTreeMap::<usize, String>::new();
    for node in doc
        .descendants()
        .filter(|n| n.attribute("data-chart-overflow") == Some("true"))
    {
        if node.ancestors().any(|n| n.tag_name().name() == "defs") {
            continue;
        }
        let Some(clip) = node
            .ancestors()
            .filter(|n| n.attribute("clip-path").is_some())
            .last()
        else {
            continue;
        };
        let mut ancestors: Vec<_> = node
            .ancestors()
            .skip(1)
            .take_while(|n| n.id() != clip.id())
            .collect();
        ancestors.push(clip);
        let mut body = String::from(r#"<g data-chart-effect="true" pointer-events="none">"#);
        for parent in ancestors.iter().rev() {
            body.push_str("<g");
            for name in [
                "transform",
                "fill",
                "stroke",
                "stroke-width",
                "opacity",
                "fill-opacity",
                "stroke-opacity",
            ] {
                if let Some(value) = parent.attribute(name) {
                    write!(body, r#" {name}="{}""#, escape_xml_text(value)).unwrap();
                }
            }
            body.push('>');
        }
        body.push_str(&svg[node.range()]);
        for _ in &ancestors {
            body.push_str("</g>");
        }
        body.push_str("</g>");
        insertions
            .entry(clip.range().end)
            .or_default()
            .push_str(&body);
        edits.push((node.range(), String::new()));
    }
    edits.extend(insertions.into_iter().map(|(at, value)| (at..at, value)));
    edits.sort_by_key(|(range, _)| range.start);
    for (range, value) in edits.into_iter().rev() {
        svg.replace_range(range, &value);
    }
    svg.replace(" data-chart-overflow=\"true\"", "")
}

pub(super) fn picture(
    fill: Node,
    properties: Option<Node>,
    source: &str,
    width: f64,
    height: f64,
    context: &Context,
) -> Option<rust_office_art::RenderedPicture> {
    let fill_xml = canonical(fill);
    let contents = &fill_xml[fill_xml.find('>')? + 1..fill_xml.rfind("</")?];
    let rectangular = properties
        .and_then(|p| child(p, "prstGeom"))
        .is_some_and(|g| g.attribute("prst") == Some("rect"));
    let props = properties
        .map(|p| {
            p.children()
                .filter(Node::is_element)
                .filter(|n| !(rectangular && n.tag_name().name() == "prstGeom"))
                .map(canonical)
                .collect::<String>()
        })
        .unwrap_or_default();
    let mut attrs = String::new();
    for name in ["dpi", "rotWithShape"] {
        if let Some(value) = fill.attribute(name) {
            write!(attrs, r#" {name}="{}""#, escape_xml_text(value)).unwrap();
        }
    }
    let geometry = if properties.is_some_and(|p| {
        (!rectangular && child(p, "prstGeom").is_some()) || child(p, "custGeom").is_some()
    }) {
        ""
    } else {
        r#"<a:custGeom><a:pathLst><a:path w="1" h="1"><a:moveTo><a:pt x="0" y="0"/></a:moveTo><a:lnTo><a:pt x="1" y="0"/></a:lnTo><a:lnTo><a:pt x="1" y="1"/></a:lnTo><a:lnTo><a:pt x="0" y="1"/></a:lnTo><a:close/></a:path></a:pathLst></a:custGeom>"#
    };
    let xml = format!("<pic:pic><pic:blipFill{attrs}>{contents}</pic:blipFill><pic:spPr>{props}{geometry}</pic:spPr></pic:pic>");
    rust_office_art::render_picture(&xml, source, width, height, context)
}

pub(super) fn geometry_bounds(node: Node, viewport: [f64; 2]) -> Option<[f64; 4]> {
    use kurbo::{BezPath, Shape};
    let num = |name: &str, default: f64| {
        node.attribute(name)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(default)
    };
    let dimension = |name: &str, extent: f64| {
        node.attribute(name).and_then(|v| {
            v.strip_suffix('%')
                .and_then(|s| s.parse::<f64>().ok())
                .map(|v| v * extent / 100.0)
                .or_else(|| v.parse().ok())
        })
    };
    let bounds = match node.tag_name().name() {
        "rect" | "image" => [
            num("x", 0.0),
            num("y", 0.0),
            dimension("width", viewport[0])?,
            dimension("height", viewport[1])?,
        ],
        "circle" | "ellipse" => {
            let rx = num("rx", num("r", 0.0));
            let ry = num("ry", num("r", 0.0));
            [num("cx", 0.0) - rx, num("cy", 0.0) - ry, rx * 2.0, ry * 2.0]
        }
        "path" | "polygon" | "polyline" | "line" => {
            let mut path = BezPath::new();
            if node.tag_name().name() == "path" {
                for segment in svgtypes::SimplifyingPathParser::from(node.attribute("d")?) {
                    use svgtypes::SimplePathSegment::*;
                    match segment.ok()? {
                        MoveTo { x, y } => path.move_to((x, y)),
                        LineTo { x, y } => path.line_to((x, y)),
                        CurveTo {
                            x1,
                            y1,
                            x2,
                            y2,
                            x,
                            y,
                        } => path.curve_to((x1, y1), (x2, y2), (x, y)),
                        Quadratic { x1, y1, x, y } => path.quad_to((x1, y1), (x, y)),
                        ClosePath => path.close_path(),
                    }
                }
            } else if node.tag_name().name() == "line" {
                path.move_to((num("x1", 0.0), num("y1", 0.0)));
                path.line_to((num("x2", 0.0), num("y2", 0.0)));
            } else {
                for (i, (x, y)) in
                    svgtypes::PointsParser::from(node.attribute("points")?).enumerate()
                {
                    if i == 0 {
                        path.move_to((x, y));
                    } else {
                        path.line_to((x, y));
                    }
                }
            }
            let b = path.bounding_box();
            [b.x0, b.y0, b.width(), b.height()]
        }
        _ => return None,
    };
    (bounds.into_iter().all(f64::is_finite) && bounds[2] > 0.0 && bounds[3] > 0.0).then_some(bounds)
}

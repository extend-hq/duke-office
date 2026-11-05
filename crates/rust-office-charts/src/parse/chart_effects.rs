use super::{color, drawing, drawing_child, paint::rgba, ThemeColorMap};
use roxmltree::Node;
use std::cell::Cell;
use std::collections::HashMap;
use std::fmt::Write;

#[derive(Default)]
pub(super) struct Effects {
    next_id: Cell<usize>,
    styles: HashMap<String, Style>,
    #[cfg(feature = "extended-charts")]
    builtin_fill: Option<String>,
}

#[derive(Default)]
struct Style {
    effects: Option<String>,
    reference: Option<String>,
    #[cfg(feature = "extended-charts")]
    material: Option<String>,
}

pub(super) fn fragment(node: Node) -> String {
    let mut xml = String::from("<effectProperties");
    for namespace in node.namespaces() {
        xml.push_str(" xmlns");
        if let Some(prefix) = namespace.name() {
            write!(xml, ":{prefix}").unwrap();
        }
        write!(
            xml,
            r#"="{}""#,
            crate::parse::escape_xml_text(namespace.uri())
        )
        .unwrap();
    }
    xml.push('>');
    xml.push_str(&node.document().input_text()[node.range()]);
    xml.push_str("</effectProperties>");
    xml
}

pub(super) fn properties<'a>(node: Node<'a, 'a>) -> Option<Node<'a, 'a>> {
    let properties =
        crate::chart::child(node, "spPr").or_else(|| crate::chart::cx_child(node, "spPr"))?;
    (drawing_child(properties, "effectLst").is_some()
        || drawing_child(properties, "effectDag").is_some())
    .then_some(properties)
}

fn number(node: Node, name: &str, default: f64) -> f64 {
    node.attribute(name)
        .and_then(|n| n.parse::<f64>().ok())
        .filter(|n| n.is_finite())
        .unwrap_or(default)
}

fn scale(node: Node, name: &str) -> f64 {
    node.attribute(name)
        .and_then(|raw| {
            if let Some(percent) = raw.strip_suffix('%') {
                percent.parse::<f64>().ok().map(|v| v / 100.)
            } else {
                raw.parse::<f64>().ok().map(|v| v / 100_000.)
            }
        })
        .filter(|n| n.is_finite())
        .unwrap_or(1.)
        .clamp(-100., 100.)
}

impl Effects {
    pub(super) fn new(
        style_xml: Option<&str>,
        theme_xml: Option<&str>,
        theme_override: Option<&str>,
        classic_style: Option<u8>,
    ) -> Self {
        const NS: &str = "http://schemas.microsoft.com/office/drawing/2012/chartStyle";
        let mut effects = Self::default();
        #[cfg(feature = "extended-charts")]
        if classic_style.is_some_and(|style| matches!(style % 100, 17..=32 | 41..=48)) {
            let fill = |xml: &str| -> Option<String> {
                let doc = roxmltree::Document::parse(xml).ok()?;
                let list = doc
                    .descendants()
                    .find(|n| drawing(*n) && n.tag_name().name() == "fillStyleLst")?;
                list.children().filter(|n| drawing(*n)).nth(2).map(fragment)
            };
            effects.builtin_fill = theme_override
                .and_then(fill)
                .or_else(|| theme_xml.and_then(fill));
        }
        let theme_styles = |xml: &str| -> Option<Vec<String>> {
            let doc = roxmltree::Document::parse(xml).ok()?;
            let list = doc
                .descendants()
                .find(|n| drawing(*n) && n.tag_name().name() == "effectStyleLst")?;
            Some(
                list.children()
                    .filter(|n| drawing(*n) && n.tag_name().name() == "effectStyle")
                    .map(fragment)
                    .collect(),
            )
        };
        let theme_styles = theme_override
            .and_then(theme_styles)
            .or_else(|| theme_xml.and_then(theme_styles))
            .unwrap_or_default();
        if let Some(style) = classic_style {
            let index = match style % 100 {
                9..=16 => Some(0),
                17..=24 => Some(1),
                25..=32 | 41..=48 => Some(2),
                _ => None,
            };
            if let Some(definition) = index.and_then(|index| theme_styles.get(index)) {
                for entry in ["dataPoint", "dataPoint3D", "dataPointMarker"] {
                    effects.styles.insert(
                        entry.into(),
                        Style {
                            effects: Some(definition.clone()),
                            ..Default::default()
                        },
                    );
                }
            }
        }
        let Some(doc) = style_xml.and_then(|xml| roxmltree::Document::parse(xml).ok()) else {
            return effects;
        };
        for entry in doc
            .root_element()
            .children()
            .filter(|n| n.tag_name().namespace() == Some(NS))
        {
            let properties = entry.children().find(|n| n.has_tag_name((NS, "spPr")));
            let reference = entry.children().find(|n| n.has_tag_name((NS, "effectRef")));
            let explicit = properties.and_then(|n| {
                drawing_child(n, "effectLst").or_else(|| drawing_child(n, "effectDag"))
            });
            let definition = explicit.map(fragment).or_else(|| {
                let index = reference?
                    .attribute("idx")?
                    .parse::<usize>()
                    .ok()?
                    .checked_sub(1)?;
                theme_styles.get(index).cloned()
            });
            #[cfg(feature = "extended-charts")]
            let material = properties
                .and_then(|n| drawing_child(n, "sp3d"))
                .and_then(|n| n.attribute("prstMaterial"))
                .map(str::to_owned);
            effects.styles.insert(
                entry.tag_name().name().to_owned(),
                Style {
                    effects: definition,
                    reference: reference.map(fragment),
                    #[cfg(feature = "extended-charts")]
                    material,
                },
            );
        }
        effects
    }

    #[cfg(feature = "extended-charts")]
    pub(super) fn material(&self, entry: &str) -> Option<&str> {
        self.styles.get(entry)?.material.as_deref()
    }

    pub(super) fn style_effects_xml(&self, entry: &str) -> Option<&str> {
        self.styles.get(entry)?.effects.as_deref()
    }

    #[cfg(feature = "extended-charts")]
    pub(super) fn style_fill_xml(&self, entry: &str) -> Option<&str> {
        matches!(entry, "dataPoint" | "dataPoint3D")
            .then_some(self.builtin_fill.as_deref())
            .flatten()
    }

    pub(super) fn paint_styled(
        &self,
        svg: &mut String,
        properties: Option<Node>,
        theme: &ThemeColorMap,
        entry: &str,
        automatic: &str,
        bounds: [f64; 4],
        body: &str,
    ) {
        if properties.is_some() {
            self.paint(svg, properties, theme, bounds, body);
            self.style_bevel(svg, entry, bounds, body);
            return;
        }
        let Some(style) = self.styles.get(entry) else {
            svg.push_str(body);
            return;
        };
        let Some(doc) = style
            .effects
            .as_deref()
            .and_then(|s| roxmltree::Document::parse(s).ok())
        else {
            svg.push_str(body);
            return;
        };
        let mut theme = theme.clone();
        theme.insert("phclr".into(), automatic.into());
        if let Some(reference) = style
            .reference
            .as_deref()
            .and_then(|s| roxmltree::Document::parse(s).ok())
        {
            let paint = reference.descendants().find_map(|n| {
                if n.tag_name().name() == "styleClr" {
                    super::transformed(automatic, &super::transforms(n))
                } else {
                    color(n, &theme)
                }
            });
            if let Some(paint) = paint {
                theme.insert("phclr".into(), paint);
            }
        }
        let properties = doc
            .descendants()
            .find(|n| drawing_child(*n, "effectLst").is_some());
        self.paint(svg, properties, &theme, bounds, body);
        self.style_bevel(svg, entry, bounds, body);
    }

    fn style_bevel(&self, svg: &mut String, entry: &str, bounds: [f64; 4], body: &str) {
        if entry == "dataPoint3D" {
            return;
        }
        let Some(doc) = self
            .style_effects_xml(entry)
            .and_then(|xml| roxmltree::Document::parse(xml).ok())
        else {
            return;
        };
        let Some(bevel) = doc
            .descendants()
            .find(|node| drawing(*node) && node.tag_name().name() == "bevelT")
        else {
            return;
        };
        let width = (number(bevel, "w", 76200.) / 9525.).clamp(0., 256.);
        let height = (number(bevel, "h", 76200.) / 9525.).clamp(0., 256.);
        if width == 0. || height == 0. || !bounds.into_iter().all(f64::is_finite) {
            return;
        }
        let [x, y, w, h] = bounds;
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        let blur = width / 3.;
        write!(svg, r##"<defs><filter id="ce{id}" filterUnits="userSpaceOnUse" x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" color-interpolation-filters="sRGB"><feMorphology in="SourceAlpha" operator="erode" radius="{width:.3}" result="inside"/><feComposite in="SourceAlpha" in2="inside" operator="out" result="edge"/><feGaussianBlur in="SourceAlpha" stdDeviation="{blur:.3}" result="height"/><feDiffuseLighting in="height" surfaceScale="{height:.3}" diffuseConstant="0.848528" lighting-color="#ffffff" result="diffuse"><feDistantLight azimuth="225" elevation="45"/></feDiffuseLighting><feComposite in="SourceGraphic" in2="diffuse" operator="arithmetic" k1="1" k2="0.4" k3="0" k4="0" result="shaded"/><feSpecularLighting in="height" surfaceScale="{height:.3}" specularConstant="0.8" specularExponent="16" lighting-color="#ffffff" result="shine"><feDistantLight azimuth="225" elevation="45"/></feSpecularLighting><feComposite in="shine" in2="SourceAlpha" operator="arithmetic" k1="0" k2="1" k3="0" k4="-0.22539" result="highlight"/><feComposite in="shaded" in2="highlight" operator="arithmetic" k1="0" k2="1" k3="1" k4="0" result="lit"/><feComposite in="lit" in2="edge" operator="in"/></filter></defs><g data-chart-effect="true" pointer-events="none" filter="url(#ce{id})">{body}</g>"##, x-4., y-4., w+8., h+8.).unwrap();
    }

    pub(super) fn paint(
        &self,
        svg: &mut String,
        properties: Option<Node>,
        theme: &ThemeColorMap,
        bounds: [f64; 4],
        body: &str,
    ) {
        let list = properties.and_then(|p| drawing_child(p, "effectLst"));
        if let Some(properties) = properties {
            if drawing_child(properties, "effectDag").is_some()
                || list.is_some_and(|list| {
                    list.children().any(|n| {
                        drawing(n)
                            && matches!(
                                n.tag_name().name(),
                                "innerShdw" | "reflection" | "softEdge" | "blur"
                            )
                    })
                })
            {
                let [x, y, width, height] = bounds;
                let context = crate::parse::art::Context {
                    theme,
                    images: None,
                    theme_xml: None,
                    theme_override: None,
                };
                let properties = crate::parse::art::canonical(properties);
                let local = format!(r#"<g transform="translate({} {})">{body}</g>"#, -x, -y);
                let rendered = rust_office_art::render_effects(
                    &properties,
                    &local,
                    width.max(1.0),
                    height.max(1.0),
                    &context,
                );
                if let Some(rendered) = rendered {
                    let id = self.next_id.get();
                    self.next_id.set(id + 1);
                    if let Some(scoped) =
                        crate::parse::art::scoped_svg(&rendered.svg, &format!("cae{id}-"))
                    {
                        if let Some(inner) = crate::parse::art::inner_svg(&scoped) {
                            write!(svg, r#"<g transform="translate({x} {y})">{inner}</g>"#)
                                .unwrap();
                            return;
                        }
                    }
                }
                svg.push_str("<desc>Some chart effects could not be rendered.</desc>");
            }
        }
        if let Some(list) = list {
            for effect in list.children().filter(|n| drawing(*n)) {
                self.effect(svg, effect, theme, bounds, body);
            }
        }
        svg.push_str(body);
    }

    fn effect(
        &self,
        svg: &mut String,
        node: Node,
        theme: &ThemeColorMap,
        [x, y, w, h]: [f64; 4],
        body: &str,
    ) {
        let shadow = node.tag_name().name() == "outerShdw";
        if !shadow && node.tag_name().name() != "glow" {
            return;
        }
        let Some([r, g, b, alpha]) = node
            .children()
            .find_map(|n| color(n, theme))
            .as_deref()
            .and_then(rgba)
        else {
            return;
        };
        if alpha == 0 || ![x, y, w, h].into_iter().all(f64::is_finite) {
            return;
        }
        let radius =
            number(node, if shadow { "blurRad" } else { "rad" }, 0.).clamp(0., 9_525_000.) / 9525.;
        let blur = radius / 2.;
        let (sx, sy, kx, ky, dx, dy, ax, ay) = if shadow {
            let direction = number(node, "dir", 0.).to_radians() / 60_000.;
            let distance = number(node, "dist", 0.).clamp(0., 95_250_000.) / 9525.;
            let align = node.attribute("algn").unwrap_or("b");
            let ax = x + w * match align {
                "tl" | "l" | "bl" => 0.,
                "tr" | "r" | "br" => 1.,
                _ => 0.5,
            };
            let ay = y + h * match align {
                "tl" | "t" | "tr" => 0.,
                "bl" | "b" | "br" => 1.,
                _ => 0.5,
            };
            (
                scale(node, "sx"),
                scale(node, "sy"),
                (number(node, "kx", 0.) / 60_000.)
                    .clamp(-89., 89.)
                    .to_radians()
                    .tan(),
                (number(node, "ky", 0.) / 60_000.)
                    .clamp(-89., 89.)
                    .to_radians()
                    .tan(),
                distance * direction.cos(),
                distance * direction.sin(),
                ax,
                ay,
            )
        } else {
            (1., 1., 0., 0., 0., 0., x, y)
        };
        let a = sx;
        let bxy = sx * ky;
        let c = sy * kx;
        let d = sy;
        let e = ax - a * ax - c * ay + dx;
        let f = ay - bxy * ax - d * ay + dy;
        let corners = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)]
            .map(|(px, py)| (a * px + c * py + e, bxy * px + d * py + f));
        let pad = (blur * 3. + 4.).max(4.);
        let left = corners.iter().map(|p| p.0).fold(f64::INFINITY, f64::min) - pad;
        let top = corners.iter().map(|p| p.1).fold(f64::INFINITY, f64::min) - pad;
        let width = corners
            .iter()
            .map(|p| p.0)
            .fold(f64::NEG_INFINITY, f64::max)
            - left
            + pad;
        let height = corners
            .iter()
            .map(|p| p.1)
            .fold(f64::NEG_INFINITY, f64::max)
            - top
            + pad;
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        write!(svg, r##"<defs><filter id="ce{id}" filterUnits="userSpaceOnUse" x="{left:.2}" y="{top:.2}" width="{width:.2}" height="{height:.2}" color-interpolation-filters="sRGB"><feGaussianBlur in="SourceAlpha" stdDeviation="{blur:.3}" result="blur"/><feFlood flood-color="#{r:02x}{g:02x}{b:02x}" flood-opacity="{:.4}"/><feComposite in2="blur" operator="in"/></filter></defs><g data-chart-effect="true" pointer-events="none" filter="url(#ce{id})"><g transform="matrix({a:.6} {bxy:.6} {c:.6} {d:.6} {e:.6} {f:.6})">{body}</g></g>"##, f64::from(alpha) / 255.).unwrap();
    }
}

pub(super) fn line_ends(
    svg: &mut String,
    line: Option<Node>,
    start: (f64, f64),
    end: (f64, f64),
    width: f64,
    paint: &str,
) {
    let Some(line) = line else { return };
    let angle = (end.1 - start.1).atan2(end.0 - start.0).to_degrees();
    for (name, point, rotation) in [("headEnd", start, angle + 180.), ("tailEnd", end, angle)] {
        let Some(node) = drawing_child(line, name) else {
            continue;
        };
        let closed = matches!(node.attribute("type"), Some("diamond" | "oval"));
        let factor = |name| match (node.attribute(name), closed) {
            (Some("sm"), _) => 2.,
            (Some("lg"), true) => 6.,
            (Some("lg"), false) => 5.,
            (_, true) => 4.,
            _ => 3.,
        };
        let length = width * factor("len");
        let breadth = width * factor("w");
        let shape = match node.attribute("type") {
            Some("diamond") => format!(
                r#"<path d="M{} 0L0 {}L{} 0L0 {}Z" fill="{paint}"/>"#,
                -length / 2.,
                -breadth / 2.,
                length / 2.,
                breadth / 2.
            ),
            Some("oval") => format!(
                r#"<ellipse cx="0" cy="0" rx="{}" ry="{}" fill="{paint}"/>"#,
                length / 2.,
                breadth / 2.
            ),
            Some("triangle") => format!(
                r#"<path d="M0 0L{} {}L{} {}Z" fill="{paint}"/>"#,
                -length,
                -breadth / 2.,
                -length,
                breadth / 2.
            ),
            Some("stealth") => format!(
                r#"<path d="M0 0L{} {}L{} 0L{} {}Z" fill="{paint}"/>"#,
                -length,
                -breadth / 2.,
                -length * 0.65,
                -length,
                breadth / 2.
            ),
            Some("arrow") => format!(
                r#"<path d="M{} {}L0 0L{} {}" fill="none" stroke="{paint}" stroke-width="{width}"/>"#,
                -length,
                -breadth / 2.,
                -length,
                breadth / 2.
            ),
            _ => continue,
        };
        write!(
            svg,
            r#"<g transform="translate({:.3} {:.3}) rotate({rotation:.3})">{shape}</g>"#,
            point.0, point.1
        )
        .unwrap();
    }
}

pub(super) fn line_dash(line: Option<Node>, width: f64) -> String {
    let preset = line
        .and_then(|n| drawing_child(n, "prstDash"))
        .and_then(|n| n.attribute("val"));
    let units: &[f64] = match preset {
        Some("dash") => &[4., 3.],
        Some("dot") => &[1., 3.],
        Some("lgDash") => &[8., 3.],
        Some("dashDot") => &[4., 3., 1., 3.],
        Some("lgDashDot") => &[8., 3., 1., 3.],
        Some("lgDashDotDot") => &[8., 3., 1., 3., 1., 3.],
        Some("sysDash") => &[3., 1.],
        Some("sysDot") => &[1., 1.],
        Some("sysDashDot") => &[3., 1., 1., 1.],
        Some("sysDashDotDot") => &[3., 1., 1., 1., 1., 1.],
        _ => return String::new(),
    };
    let cap = if matches!(line.and_then(|n| n.attribute("cap")), Some("rnd" | "sq")) {
        1.
    } else {
        0.
    };
    units
        .iter()
        .enumerate()
        .map(|(index, unit)| {
            format!(
                "{:.3}",
                (unit + if index % 2 == 0 { -cap } else { cap }).max(0.001) * width
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

use std::fmt::Write;

use base64::Engine;

use crate::xml::{
    direct_child_tag, extract_balanced_tag_slices, extract_direct_child_tag_ranges,
    get_attribute_ref,
};

use crate::xml::escape_xml_text;

mod raster;
mod xml;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaintBounds {
    pub x_px: f64,
    pub y_px: f64,
    pub width_px: f64,
    pub height_px: f64,
    pub layout_width_px: f64,
    pub layout_height_px: f64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderedPicture {
    pub svg: String,
    pub paint_bounds: Option<PaintBounds>,
}

#[derive(Clone, Debug)]
pub struct ResolvedColor {
    pub color: String,
    pub opacity: Option<f64>,
}

pub trait PictureContext {
    fn resolve_color(&self, xml: &str) -> Option<ResolvedColor>;
}

pub fn normalize_raster(source: &str) -> Option<String> {
    raster::normalize(source)
}

fn resolve_drawing_color_from_xml(
    xml: Option<&str>,
    context: &impl PictureContext,
) -> Option<ResolvedColor> {
    context.resolve_color(xml?)
}

fn number(xml: &str, name: &str, default: f64) -> Option<f64> {
    match get_attribute_ref(xml, name) {
        Some(value) => value
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite()),
        None => Some(default),
    }
}

fn percentage(xml: &str, name: &str, default: f64) -> Option<f64> {
    let Some(value) = get_attribute_ref(xml, name) else {
        return Some(default);
    };
    let value = value.trim();
    let (value, divisor) = match value.strip_suffix('%') {
        Some(value) => (value, 100.0),
        None => (value, 100_000.0),
    };
    value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .map(|value| value / divisor)
}

fn enabled(xml: &str, name: &str) -> bool {
    get_attribute_ref(xml, name).is_some_and(|value| matches!(value, "1" | "true" | "on"))
}

fn rectangle(xml: Option<&str>) -> Option<[f64; 4]> {
    let xml = xml.unwrap_or("");
    Some([
        percentage(xml, "l", 0.0)?,
        percentage(xml, "t", 0.0)?,
        percentage(xml, "r", 0.0)?,
        percentage(xml, "b", 0.0)?,
    ])
}

fn grayscale(out: &mut String) {
    out.push_str(r##"<feColorMatrix type="matrix" values="0.2125 0.7154 0.0721 0 0 0.2125 0.7154 0.0721 0 0 0.2125 0.7154 0.0721 0 0 0 0 0 1 0"/>"##);
}

fn threshold(out: &mut String, channels: &[&str], threshold: f64) {
    let _ = write!(out, "<feComponentTransfer>");
    for channel in channels {
        let _ = write!(
            out,
            r##"<feFunc{channel} type="linear" slope="1" intercept="{}"/>"##,
            0.5 - threshold
        );
    }
    out.push_str("</feComponentTransfer><feComponentTransfer>");
    for channel in channels {
        let _ = write!(
            out,
            r##"<feFunc{channel} type="discrete" tableValues="0 1"/>"##
        );
    }
    out.push_str("</feComponentTransfer>");
}

fn rgb(xml: &str, colors: &impl PictureContext) -> Option<[f64; 3]> {
    let resolved = resolve_drawing_color_from_xml(Some(xml), colors)?;
    let mut channels = [0.0; 3];
    for (index, channel) in channels.iter_mut().enumerate() {
        *channel = u8::from_str_radix(resolved.color.get(1 + index * 2..3 + index * 2)?, 16).ok()?
            as f64
            / 255.0;
    }
    Some(channels)
}

fn replace_color(out: &mut String, color: [f64; 3]) {
    out.push_str("<feComponentTransfer>");
    for (channel, value) in ["R", "G", "B"].into_iter().zip(color) {
        let _ = write!(
            out,
            r##"<feFunc{channel} type="linear" slope="0" intercept="{value}"/>"##
        );
    }
    out.push_str("</feComponentTransfer>");
}

fn linear_rgb(out: &mut String, slope: f64, intercept: f64) {
    out.push_str("<feComponentTransfer>");
    for channel in ["R", "G", "B"] {
        let _ = write!(
            out,
            r##"<feFunc{channel} type="linear" slope="{slope}" intercept="{intercept}"/>"##
        );
    }
    out.push_str("</feComponentTransfer>");
}

fn blur_radius(xml: &str, name: &str) -> Option<f64> {
    let radius = number(xml, name, 0.0)? / 9525.0;
    (0.0..=10_000.0).contains(&radius).then_some(radius)
}

fn blip_effects(blip: &str) -> Vec<xml::TaggedRange> {
    let mut effects = extract_direct_child_tag_ranges(blip);
    effects.sort_by_key(|effect| {
        !matches!(
            effect.tag_name.as_str(),
            "a:clrChange" | "a:alphaBiLevel" | "a:alphaCeiling" | "a:alphaFloor" | "a:biLevel"
        )
    });
    effects
}

fn blip_filter(blip: &str, colors: &impl PictureContext) -> Option<String> {
    let mut out = String::new();
    for child in blip_effects(blip) {
        let xml = &blip[child.start..child.end];
        match child.tag_name.as_str() {
            "a:extLst" => {}
            "a:grayscl" => grayscale(&mut out),
            "a:biLevel" => {
                grayscale(&mut out);
                threshold(
                    &mut out,
                    &["R", "G", "B"],
                    percentage(xml, "thresh", 0.5)?.clamp(0.0, 1.0),
                );
            }
            "a:alphaBiLevel" => threshold(
                &mut out,
                &["A"],
                percentage(xml, "thresh", 0.5)?.clamp(0.0, 1.0),
            ),
            "a:alphaFloor" => threshold(&mut out, &["A"], 1.0),
            "a:alphaModFix" => {
                let amount = percentage(xml, "amt", 1.0)?.max(0.0);
                let _ = write!(
                    out,
                    r##"<feComponentTransfer><feFuncA type="linear" slope="{amount}" intercept="0"/></feComponentTransfer>"##
                );
            }
            "a:alphaRepl" => {
                let amount = percentage(xml, "a", 1.0)?.clamp(0.0, 1.0);
                let _ = write!(
                    out,
                    r##"<feComponentTransfer><feFuncA type="linear" slope="0" intercept="{amount}"/></feComponentTransfer>"##
                );
            }
            "a:alphaInv" => {
                out.push_str(r##"<feComponentTransfer><feFuncA type="linear" slope="-1" intercept="1"/></feComponentTransfer>"##);
                let color = if extract_direct_child_tag_ranges(xml).is_empty() {
                    [0.0; 3]
                } else {
                    rgb(xml, colors)?
                };
                replace_color(&mut out, color);
            }
            "a:clrRepl" => replace_color(&mut out, rgb(xml, colors)?),
            "a:lum" => {
                let brightness = percentage(xml, "bright", 0.0)?.clamp(-1.0, 1.0);
                let contrast = percentage(xml, "contrast", 0.0)?.clamp(-1.0, 1.0);
                let brightness = (brightness * 0.5) as f32;
                let slope = (if contrast > 0.0 {
                    1.0 / (1.0 - contrast.min(0.99999))
                } else {
                    1.0 + contrast
                }) as f32;
                let intercept = slope * (brightness - 0.5) + brightness + 0.5;
                linear_rgb(&mut out, f64::from(slope), f64::from(intercept));
            }
            "a:blur" => {
                let deviation = blur_radius(xml, "rad")? / 2.0;
                let _ = write!(out, r##"<feGaussianBlur stdDeviation="{deviation}"/>"##);
            }
            "a:duotone" => {
                let children = extract_direct_child_tag_ranges(xml);
                let [dark, light] = children.as_slice() else {
                    return None;
                };
                let dark = rgb(&xml[dark.start..dark.end], colors)?;
                let light = rgb(&xml[light.start..light.end], colors)?;
                grayscale(&mut out);
                out.push_str("<feComponentTransfer>");
                for (index, channel) in ["R", "G", "B"].into_iter().enumerate() {
                    let _ = write!(
                        out,
                        r##"<feFunc{channel} type="table" tableValues="{} {}"/>"##,
                        dark[index], light[index]
                    );
                }
                out.push_str("</feComponentTransfer>");
            }
            _ => return None,
        }
    }
    Some(out)
}

fn adjusted_value(geometry: &str, name: &str, default: f64) -> Option<f64> {
    let Some(adjustments) = direct_child_tag(geometry, "a:avLst") else {
        return Some(default);
    };
    for adjustment in extract_direct_child_tag_ranges(adjustments) {
        let xml = &adjustments[adjustment.start..adjustment.end];
        if get_attribute_ref(xml, "name") == Some(name) {
            let formula = get_attribute_ref(xml, "fmla")?
                .trim()
                .strip_prefix("val ")?;
            return formula
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite());
        }
    }
    Some(default)
}

fn geometry_path(properties: &str, width: f64, height: f64) -> Option<String> {
    if let Some(geometry) = direct_child_tag(properties, "a:custGeom") {
        let paths = direct_child_tag(geometry, "a:pathLst")?;
        let paths = extract_direct_child_tag_ranges(paths);
        if paths.len() != 1 {
            return None;
        }
        let path = extract_balanced_tag_slices(geometry, "a:path")
            .into_iter()
            .next()?;
        let path_width = number(path, "w", 21600.0)?;
        let path_height = number(path, "h", 21600.0)?;
        if path_width <= 0.0 || path_height <= 0.0 {
            return None;
        }
        let mut output = String::new();
        for command in extract_direct_child_tag_ranges(path) {
            let (expected, operation) = match command.tag_name.as_str() {
                "a:moveTo" => (1, 'M'),
                "a:lnTo" => (1, 'L'),
                "a:cubicBezTo" => (3, 'C'),
                "a:quadBezTo" => (2, 'Q'),
                "a:close" => (0, 'Z'),
                _ => return None,
            };
            let command = &path[command.start..command.end];
            let points = extract_direct_child_tag_ranges(command);
            if points.len() != expected {
                return None;
            }
            output.push(operation);
            for point in points {
                if point.tag_name != "a:pt" {
                    return None;
                }
                let point = &command[point.start..point.end];
                let x = get_attribute_ref(point, "x")?
                    .parse::<f64>()
                    .ok()?
                    .mul_add(width / path_width, 0.0);
                let y = get_attribute_ref(point, "y")?
                    .parse::<f64>()
                    .ok()?
                    .mul_add(height / path_height, 0.0);
                if !x.is_finite() || !y.is_finite() {
                    return None;
                }
                let _ = write!(output, "{x} {y} ");
            }
        }
        return (!output.is_empty()).then_some(output);
    }
    let geometry = direct_child_tag(properties, "a:prstGeom").unwrap_or("");
    let preset = get_attribute_ref(geometry, "prst").unwrap_or("rect");
    let x_mid = width / 2.0;
    let y_mid = height / 2.0;
    Some(match preset {
        "rect" => format!("M0 0 H{width} V{height} H0 Z"),
        "ellipse" => format!(
            "M0 {y_mid} A{x_mid} {y_mid} 0 1 0 {width} {y_mid} A{x_mid} {y_mid} 0 1 0 0 {y_mid} Z"
        ),
        "roundRect" => {
            let radius = width.min(height)
                * (adjusted_value(geometry, "adj", 16667.0)? / 100000.0).clamp(0.0, 0.5);
            let right = width - radius;
            let bottom = height - radius;
            format!("M{radius} 0 H{right} A{radius} {radius} 0 0 1 {width} {radius} V{bottom} A{radius} {radius} 0 0 1 {right} {height} H{radius} A{radius} {radius} 0 0 1 0 {bottom} V{radius} A{radius} {radius} 0 0 1 {radius} 0 Z")
        }
        "triangle" => {
            let apex =
                width * (adjusted_value(geometry, "adj", 50000.0)? / 100000.0).clamp(0.0, 1.0);
            format!("M{apex} 0 L{width} {height} L0 {height} Z")
        }
        "diamond" => format!("M{x_mid} 0 L{width} {y_mid} L{x_mid} {height} L0 {y_mid} Z"),
        "rtTriangle" => format!("M0 {height} L{width} {height} L0 0 Z"),
        "flowChartDelay" => format!("M0 0 H{x_mid} A{x_mid} {y_mid} 0 0 1 {x_mid} {height} H0 Z"),
        _ => return None,
    })
}

fn border(properties: &str, colors: &impl PictureContext) -> Option<(String, f64)> {
    let Some(line) = direct_child_tag(properties, "a:ln") else {
        return Some((String::new(), 0.0));
    };
    if direct_child_tag(line, "a:noFill").is_some() {
        return Some((String::new(), 0.0));
    }
    if direct_child_tag(line, "a:gradFill").is_some()
        || direct_child_tag(line, "a:pattFill").is_some()
        || get_attribute_ref(line, "cmpd").is_some_and(|value| value != "sng")
        || get_attribute_ref(line, "algn").is_some_and(|value| value != "ctr")
    {
        return None;
    }
    let width = number(line, "w", 12700.0)?.max(0.0) / 9525.0;
    if width == 0.0 {
        return Some((String::new(), 0.0));
    }
    let color = resolve_drawing_color_from_xml(Some(line), colors)?;
    let cap = match get_attribute_ref(line, "cap").unwrap_or("flat") {
        "rnd" => "round",
        "sq" => "square",
        _ => "butt",
    };
    let join = if direct_child_tag(line, "a:round").is_some() {
        "round"
    } else if direct_child_tag(line, "a:bevel").is_some() {
        "bevel"
    } else {
        "miter"
    };
    let miter = match direct_child_tag(line, "a:miter") {
        Some(xml) => percentage(xml, "lim", 8.0)?,
        None => 8.0,
    }
    .max(1.0);
    let mut attributes = format!(
        r##"fill="none" stroke="{}" stroke-opacity="{}" stroke-width="{}" stroke-linecap="{cap}" stroke-linejoin="{join}" stroke-miterlimit="{miter}""##,
        escape_xml_text(&color.color),
        color.opacity.unwrap_or(1.0),
        width * 2.0,
    );
    if let Some(dash) = direct_child_tag(line, "a:prstDash") {
        let segments: &[f64] = match get_attribute_ref(dash, "val").unwrap_or("solid") {
            "solid" => &[],
            "dot" | "sysDot" => &[1.0, 1.0],
            "dash" | "sysDash" => &[3.0, 1.0],
            "lgDash" => &[8.0, 3.0],
            "dashDot" | "sysDashDot" => &[3.0, 1.0, 1.0, 1.0],
            "lgDashDot" => &[8.0, 3.0, 1.0, 3.0],
            "lgDashDotDot" => &[8.0, 3.0, 1.0, 3.0, 1.0, 3.0],
            "sysDashDotDot" => &[3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
            _ => return None,
        };
        if !segments.is_empty() {
            let values = segments
                .iter()
                .map(|value| (value * width).to_string())
                .collect::<Vec<_>>()
                .join(" ");
            let _ = write!(attributes, r##" stroke-dasharray="{values}""##);
        }
    }
    if let Some(dash) = direct_child_tag(line, "a:custDash") {
        let mut values = Vec::new();
        for stop in extract_direct_child_tag_ranges(dash) {
            let xml = &dash[stop.start..stop.end];
            values.extend([
                percentage(xml, "d", 1.0)?.max(0.0) * width,
                percentage(xml, "sp", 1.0)?.max(0.0) * width,
            ]);
        }
        if !values.is_empty() && values.iter().any(|value| *value > 0.0) {
            let values = values
                .iter()
                .map(f64::to_string)
                .collect::<Vec<_>>()
                .join(" ");
            let _ = write!(attributes, r##" stroke-dasharray="{values}""##);
        }
    }
    let geometry = direct_child_tag(properties, "a:prstGeom")
        .and_then(|geometry| get_attribute_ref(geometry, "prst"))
        .unwrap_or("rect");
    let bounded_corner = direct_child_tag(properties, "a:custGeom").is_none()
        && matches!(geometry, "rect" | "ellipse" | "roundRect");
    Some((
        attributes,
        width
            * if join == "miter" && !bounded_corner {
                miter
            } else {
                1.0
            },
    ))
}

#[derive(Clone, Copy)]
struct Bounds {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl Bounds {
    fn rect(width: f64, height: f64) -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            right: width,
            bottom: height,
        }
    }

    fn expanded(self, radius: f64) -> Self {
        Self {
            left: self.left - radius,
            top: self.top - radius,
            right: self.right + radius,
            bottom: self.bottom + radius,
        }
    }

    fn union(self, other: Self) -> Self {
        Self {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }

    fn transformed(self, matrix: Matrix) -> Self {
        let points = [
            (self.left, self.top),
            (self.right, self.top),
            (self.left, self.bottom),
            (self.right, self.bottom),
        ]
        .map(|(x, y)| matrix.point(x, y));
        Self {
            left: points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min),
            top: points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min),
            right: points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max),
            bottom: points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max),
        }
    }

    fn attributes(self) -> String {
        format!(
            r##"x="{}" y="{}" width="{}" height="{}""##,
            self.left,
            self.top,
            self.right - self.left,
            self.bottom - self.top
        )
    }

    fn valid(self) -> bool {
        [self.left, self.top, self.right, self.bottom]
            .into_iter()
            .all(|value| value.is_finite() && value.abs() <= 100_000_000.0)
            && self.right > self.left
            && self.bottom > self.top
    }
}

#[derive(Clone, Copy)]
struct Matrix([f64; 6]);

impl Matrix {
    fn point(self, x: f64, y: f64) -> (f64, f64) {
        let [a, b, c, d, e, f] = self.0;
        (a * x + c * y + e, b * x + d * y + f)
    }

    fn composed(self, other: Self) -> Self {
        let [a, b, c, d, e, f] = self.0;
        let [g, h, i, j, k, l] = other.0;
        Self([
            a * g + c * h,
            b * g + d * h,
            a * i + c * j,
            b * i + d * j,
            a * k + c * l + e,
            b * k + d * l + f,
        ])
    }

    fn markup(self) -> String {
        let [a, b, c, d, e, f] = self.0;
        format!("matrix({a} {b} {c} {d} {e} {f})")
    }
}

fn alignment(value: &str) -> (f64, f64) {
    (
        match value {
            "tl" | "l" | "bl" => 0.0,
            "tr" | "r" | "br" => 1.0,
            _ => 0.5,
        },
        match value {
            "tl" | "t" | "tr" => 0.0,
            "bl" | "b" | "br" => 1.0,
            _ => 0.5,
        },
    )
}

fn effect_matrix(xml: &str, bounds: Bounds) -> Option<Matrix> {
    let sx = percentage(xml, "sx", 1.0)?;
    let sy = percentage(xml, "sy", 1.0)?;
    let kx = number(xml, "kx", 0.0)? / 60_000.0;
    let ky = number(xml, "ky", 0.0)? / 60_000.0;
    if sx.abs() > 100.0 || sy.abs() > 100.0 || kx.abs() >= 90.0 || ky.abs() >= 90.0 {
        return None;
    }
    let direction = (number(xml, "dir", 0.0)? / 60_000.0).to_radians();
    let distance = number(xml, "dist", 0.0)?.max(0.0) / 9525.0;
    let (ax, ay) = alignment(get_attribute_ref(xml, "algn").unwrap_or("b"));
    let ax = bounds.left + (bounds.right - bounds.left) * ax;
    let ay = bounds.top + (bounds.bottom - bounds.top) * ay;
    let a = sx;
    let b = sx * ky.to_radians().tan();
    let c = sy * kx.to_radians().tan();
    let d = sy;
    Some(Matrix([
        a,
        b,
        c,
        d,
        ax - a * ax - c * ay + distance * direction.cos(),
        ay - b * ax - d * ay + distance * direction.sin(),
    ]))
}

fn color_filter(
    defs: &mut String,
    id: &str,
    bounds: Bounds,
    color_xml: &str,
    colors: &impl PictureContext,
    deviation: f64,
    dilation: f64,
) -> Option<()> {
    let color = resolve_drawing_color_from_xml(Some(color_xml), colors)?;
    let morphology = if dilation > 0.0 {
        format!(
            r##"<feMorphology in="SourceAlpha" operator="dilate" radius="{dilation}" result="silhouette"/>"##
        )
    } else {
        String::new()
    };
    let input = if dilation > 0.0 {
        "silhouette"
    } else {
        "SourceAlpha"
    };
    let _ = write!(
        defs,
        r##"<filter id="{id}" filterUnits="userSpaceOnUse" {} color-interpolation-filters="sRGB">{morphology}<feGaussianBlur in="{input}" stdDeviation="{deviation}" result="blur"/><feFlood flood-color="{}" flood-opacity="{}"/><feComposite in2="blur" operator="in"/></filter>"##,
        bounds.attributes(),
        escape_xml_text(&color.color),
        color.opacity.unwrap_or(1.0)
    );
    Some(())
}

fn apply_shape_effects(
    properties: &str,
    body: &str,
    path: &str,
    width: f64,
    height: f64,
    margin: f64,
    matrix: Matrix,
    colors: &impl PictureContext,
    defs: &mut String,
) -> Option<(String, Bounds)> {
    let local = Bounds::rect(width, height).expanded(margin);
    let mut source_bounds = local;
    let mut source = body.to_string();
    let list = direct_child_tag(properties, "a:effectLst").unwrap_or("");
    let children = extract_direct_child_tag_ranges(list);
    for effect in &children {
        let xml = &list[effect.start..effect.end];
        match effect.tag_name.as_str() {
            "a:blur" => {
                let deviation = blur_radius(xml, "rad")? / 2.0;
                let grow = !get_attribute_ref(xml, "grow")
                    .is_some_and(|value| matches!(value, "0" | "false" | "off"));
                if grow {
                    source_bounds = source_bounds.expanded(3.0 * deviation);
                }
                let _ = write!(
                    defs,
                    r##"<filter id="picture-shape-blur" filterUnits="userSpaceOnUse" {} color-interpolation-filters="sRGB"><feGaussianBlur stdDeviation="{deviation}"/></filter>"##,
                    source_bounds.attributes()
                );
                source = format!(r##"<g filter="url(#picture-shape-blur)">{source}</g>"##);
            }
            "a:softEdge" => {
                let radius = blur_radius(xml, "rad")?;
                let erosion = radius;
                let deviation = radius / 3.0;
                let _ = write!(
                    defs,
                    r##"<filter id="picture-soft-edge" filterUnits="userSpaceOnUse" {} color-interpolation-filters="sRGB"><feComponentTransfer in="SourceAlpha" result="binary"><feFuncA type="linear" slope="255"/></feComponentTransfer><feMorphology in="binary" operator="erode" radius="{erosion}" result="inset"/><feGaussianBlur in="inset" stdDeviation="{deviation}" result="soft"/><feComposite in="SourceGraphic" in2="soft" operator="in"/></filter>"##,
                    source_bounds.attributes()
                );
                source = format!(r##"<g filter="url(#picture-soft-edge)">{source}</g>"##);
            }
            "a:innerShdw" | "a:outerShdw" | "a:glow" | "a:reflection" => {}
            _ => return None,
        }
    }
    let _ = write!(defs, r##"<g id="picture-body">{source}</g>"##);
    let mut bounds = source_bounds.transformed(matrix);
    let mut underlay = String::new();
    let mut overlay = String::new();
    for (index, effect) in children.iter().enumerate() {
        let xml = &list[effect.start..effect.end];
        let id = format!("picture-effect-{index}");
        match effect.tag_name.as_str() {
            "a:glow" => {
                let radius = blur_radius(xml, "rad")?;
                let deviation = radius / 6.0;
                let dilation = radius / 2.0;
                let effect_bounds = bounds.expanded(dilation + 3.0 * deviation);
                color_filter(defs, &id, effect_bounds, xml, colors, deviation, dilation)?;
                let _ = write!(
                    underlay,
                    r##"<g filter="url(#{id})"><path d="{path}" fill="white" transform="{}"/></g>"##,
                    matrix.markup()
                );
                bounds = bounds.union(effect_bounds);
            }
            "a:outerShdw" | "a:reflection" => {
                let deviation = blur_radius(xml, "blurRad")?
                    / if effect.tag_name == "a:outerShdw" {
                        3.0
                    } else {
                        2.0
                    };
                let rotate_with_shape = !get_attribute_ref(xml, "rotWithShape")
                    .is_some_and(|value| matches!(value, "0" | "false" | "off"));
                let transformed = if rotate_with_shape {
                    matrix.composed(effect_matrix(xml, Bounds::rect(width, height))?)
                } else {
                    effect_matrix(xml, Bounds::rect(width, height).transformed(matrix))?
                        .composed(matrix)
                };
                let raw_bounds = source_bounds.transformed(transformed);
                let effect_bounds = raw_bounds.expanded(3.0 * deviation);
                if effect.tag_name == "a:outerShdw" {
                    color_filter(defs, &id, effect_bounds, xml, colors, deviation, 0.0)?;
                    let _ = write!(
                        underlay,
                        r##"<g filter="url(#{id})"><use xlink:href="#picture-body" transform="{}"/></g>"##,
                        transformed.markup()
                    );
                } else {
                    let start_alpha = percentage(xml, "stA", 1.0)?.clamp(0.0, 1.0);
                    let end_alpha = percentage(xml, "endA", 0.0)?.clamp(0.0, 1.0);
                    let start = percentage(xml, "stPos", 0.0)?.clamp(0.0, 1.0);
                    let end = percentage(xml, "endPos", 1.0)?.clamp(start, 1.0);
                    let fade = (number(xml, "fadeDir", 5_400_000.0)? / 60_000.0).to_radians();
                    let dx = fade.cos();
                    let dy = fade.sin();
                    let length = (raw_bounds.right - raw_bounds.left) * dx.abs()
                        + (raw_bounds.bottom - raw_bounds.top) * dy.abs();
                    let center_x = (raw_bounds.left + raw_bounds.right) / 2.0;
                    let center_y = (raw_bounds.top + raw_bounds.bottom) / 2.0;
                    let _ = write!(
                        defs,
                        r##"<linearGradient id="{id}-fade" gradientUnits="userSpaceOnUse" x1="{}" y1="{}" x2="{}" y2="{}"><stop offset="{start}" stop-color="white" stop-opacity="{start_alpha}"/><stop offset="{end}" stop-color="white" stop-opacity="{end_alpha}"/></linearGradient><mask id="{id}-mask" maskUnits="userSpaceOnUse" {}><rect {} fill="url(#{id}-fade)"/></mask><filter id="{id}" filterUnits="userSpaceOnUse" {} color-interpolation-filters="sRGB"><feGaussianBlur stdDeviation="{deviation}"/></filter>"##,
                        center_x - length * dx / 2.0,
                        center_y - length * dy / 2.0,
                        center_x + length * dx / 2.0,
                        center_y + length * dy / 2.0,
                        effect_bounds.attributes(),
                        effect_bounds.attributes(),
                        effect_bounds.attributes()
                    );
                    let _ = write!(
                        underlay,
                        r##"<g mask="url(#{id}-mask)" filter="url(#{id})"><use xlink:href="#picture-body" transform="{}"/></g>"##,
                        transformed.markup()
                    );
                }
                bounds = bounds.union(effect_bounds);
            }
            "a:innerShdw" => {
                let deviation = blur_radius(xml, "blurRad")? / 2.0;
                let direction = (number(xml, "dir", 0.0)? / 60_000.0).to_radians();
                let distance = number(xml, "dist", 0.0)?.max(0.0) / 9525.0;
                let dx = -distance * direction.cos();
                let dy = -distance * direction.sin();
                let color = resolve_drawing_color_from_xml(Some(xml), colors)?;
                let _ = write!(
                    defs,
                    r##"<filter id="{id}" filterUnits="userSpaceOnUse" {} color-interpolation-filters="sRGB"><feOffset in="SourceAlpha" dx="{dx}" dy="{dy}" result="offset"/><feGaussianBlur in="offset" stdDeviation="{deviation}" result="blur"/><feComposite in="SourceAlpha" in2="blur" operator="out" result="edge"/><feFlood flood-color="{}" flood-opacity="{}"/><feComposite in2="edge" operator="in"/></filter>"##,
                    local.attributes(),
                    escape_xml_text(&color.color),
                    color.opacity.unwrap_or(1.0)
                );
                let _ = write!(
                    overlay,
                    r##"<g transform="{}"><use xlink:href="#picture-body" filter="url(#{id})"/></g>"##,
                    matrix.markup()
                );
            }
            _ => {}
        }
    }
    if !bounds.valid() {
        return None;
    }
    Some((
        format!(
            r##"{underlay}<use xlink:href="#picture-body" transform="{}"/>{overlay}"##,
            matrix.markup()
        ),
        bounds,
    ))
}

fn raster_metrics(source: &str) -> Option<(f64, f64, f64, f64)> {
    let (header, payload) = source.split_once(',')?;
    if !header.starts_with("data:image/") || !header.ends_with(";base64") {
        return None;
    }
    let length = payload.len().min(262_144) / 4 * 4;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&payload[..length])
        .ok()?;
    let be_u32 = |offset: usize| -> Option<u32> {
        Some(u32::from_be_bytes(
            bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
        ))
    };
    let le_u32 = |offset: usize| -> Option<u32> {
        Some(u32::from_le_bytes(
            bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
        ))
    };
    let le_u16 = |offset: usize| -> Option<u16> {
        Some(u16::from_le_bytes(
            bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
        ))
    };
    let mut dpi = (96.0, 96.0);
    let (width, height) = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let dimensions = (be_u32(16)? as f64, be_u32(20)? as f64);
        let mut offset = 8usize;
        while offset.checked_add(12)? <= bytes.len() {
            let length = be_u32(offset)? as usize;
            let kind = bytes.get(offset + 4..offset + 8)?;
            if kind == b"pHYs" && length == 9 && bytes.get(offset + 16) == Some(&1) {
                dpi = (
                    be_u32(offset + 8)? as f64 * 0.0254,
                    be_u32(offset + 12)? as f64 * 0.0254,
                );
            }
            if kind == b"IDAT" {
                break;
            }
            offset = offset.checked_add(length)?.checked_add(12)?;
        }
        dimensions
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        (le_u16(6)? as f64, le_u16(8)? as f64)
    } else if bytes.starts_with(b"BM") {
        if le_u32(14)? == 12 {
            (le_u16(18)? as f64, le_u16(20)? as f64)
        } else {
            let x = le_u32(38)? as i32;
            let y = le_u32(42)? as i32;
            if x > 0 && y > 0 {
                dpi = (x as f64 * 0.0254, y as f64 * 0.0254);
            }
            (
                (le_u32(18)? as i32).checked_abs()? as f64,
                (le_u32(22)? as i32).checked_abs()? as f64,
            )
        }
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        let mut offset = 2usize;
        let mut dimensions = None;
        while offset + 4 <= bytes.len() {
            if bytes[offset] != 0xff {
                return None;
            }
            while bytes.get(offset + 1) == Some(&0xff) {
                offset += 1;
            }
            let marker = *bytes.get(offset + 1)?;
            if marker == 0xda || marker == 0xd9 {
                break;
            }
            if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
                offset += 2;
                continue;
            }
            let length =
                u16::from_be_bytes(bytes.get(offset + 2..offset + 4)?.try_into().ok()?) as usize;
            if length < 2 {
                return None;
            }
            let data = bytes.get(offset + 4..offset.checked_add(length)?.checked_add(2)?)?;
            if marker == 0xe0 && data.starts_with(b"JFIF\0") && data.len() >= 12 {
                let x = u16::from_be_bytes(data[8..10].try_into().ok()?) as f64;
                let y = u16::from_be_bytes(data[10..12].try_into().ok()?) as f64;
                if x > 0.0 && y > 0.0 {
                    match data[7] {
                        1 => dpi = (x, y),
                        2 => dpi = (x * 2.54, y * 2.54),
                        _ => {}
                    }
                }
            }
            if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf)
                && data.len() >= 5
            {
                dimensions = Some((
                    u16::from_be_bytes(data[3..5].try_into().ok()?) as f64,
                    u16::from_be_bytes(data[1..3].try_into().ok()?) as f64,
                ));
            }
            offset = offset.checked_add(length)?.checked_add(2)?;
        }
        dimensions?
    } else {
        return None;
    };
    if width <= 0.0 || height <= 0.0 || dpi.0 <= 0.0 || dpi.1 <= 0.0 {
        return None;
    }
    Some((width, height, dpi.0, dpi.1))
}

fn tile_picture(
    fill: &str,
    source: &str,
    intrinsic_source: &str,
    width: f64,
    height: f64,
    crop: [f64; 4],
    filter: &str,
    defs: &mut String,
) -> Option<String> {
    let tile = direct_child_tag(fill, "a:tile")?;
    let (pixel_width, pixel_height, mut dpi_x, mut dpi_y) = raster_metrics(intrinsic_source)?;
    let dpi = number(fill, "dpi", 0.0)?;
    if dpi > 0.0 {
        dpi_x = dpi;
        dpi_y = dpi;
    } else if extract_balanced_tag_slices(fill, "a14:useLocalDpi")
        .into_iter()
        .next()
        .is_some_and(|xml| {
            get_attribute_ref(xml, "val").is_some_and(|value| matches!(value, "0" | "false"))
        })
    {
        dpi_x = 96.0;
        dpi_y = 96.0;
    }
    let sx = percentage(tile, "sx", 1.0)?;
    let sy = percentage(tile, "sy", 1.0)?;
    let image_width = pixel_width * 96.0 / dpi_x * sx;
    let image_height = pixel_height * 96.0 / dpi_y * sy;
    let [left, top, right, bottom] = crop;
    let tile_width = image_width * (1.0 - left - right);
    let tile_height = image_height * (1.0 - top - bottom);
    if [tile_width, tile_height]
        .into_iter()
        .any(|value| !value.is_finite() || !(0.001..=1_000_000.0).contains(&value))
    {
        return None;
    }
    let (align_x, align_y) = alignment(get_attribute_ref(tile, "algn").unwrap_or("tl"));
    let x = (width - tile_width) * align_x + number(tile, "tx", 0.0)? / 9525.0;
    let y = (height - tile_height) * align_y + number(tile, "ty", 0.0)? / 9525.0;
    let flip = get_attribute_ref(tile, "flip").unwrap_or("none");
    let flip_x = matches!(flip, "x" | "xy");
    let flip_y = matches!(flip, "y" | "xy");
    let pattern_width = tile_width * if flip_x { 2.0 } else { 1.0 };
    let pattern_height = tile_height * if flip_y { 2.0 } else { 1.0 };
    let image_x = -left * image_width;
    let image_y = -top * image_height;
    let _ = write!(
        defs,
        r##"<clipPath id="picture-tile-clip"><rect width="{tile_width}" height="{tile_height}"/></clipPath><g id="picture-tile" clip-path="url(#picture-tile-clip)"><image xlink:href="{}" x="{image_x}" y="{image_y}" width="{image_width}" height="{image_height}" preserveAspectRatio="none"{filter}/></g><pattern id="picture-tile-pattern" x="{x}" y="{y}" width="{pattern_width}" height="{pattern_height}" patternUnits="userSpaceOnUse"><use xlink:href="#picture-tile"/>"##,
        escape_xml_text(source)
    );
    if flip_x {
        let _ = write!(
            defs,
            r##"<use xlink:href="#picture-tile" transform="translate({pattern_width} 0) scale(-1 1)"/>"##
        );
    }
    if flip_y {
        let _ = write!(
            defs,
            r##"<use xlink:href="#picture-tile" transform="translate(0 {pattern_height}) scale(1 -1)"/>"##
        );
    }
    if flip_x && flip_y {
        let _ = write!(
            defs,
            r##"<use xlink:href="#picture-tile" transform="translate({pattern_width} {pattern_height}) scale(-1 -1)"/>"##
        );
    }
    defs.push_str("</pattern>");
    Some(r##"fill="url(#picture-tile-pattern)""##.to_string())
}

pub fn render_picture(
    xml: &str,
    source: &str,
    width: f64,
    height: f64,
    context: &impl PictureContext,
) -> Option<RenderedPicture> {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let picture = extract_balanced_tag_slices(xml, "pic:pic")
        .into_iter()
        .next()?;
    let properties = direct_child_tag(picture, "pic:spPr").unwrap_or("");
    let fill = direct_child_tag(picture, "pic:blipFill")?;
    if direct_child_tag(properties, "a:effectDag").is_some()
        || direct_child_tag(properties, "a:scene3d").is_some()
        || direct_child_tag(properties, "a:sp3d").is_some()
    {
        return None;
    }
    let transform = direct_child_tag(properties, "a:xfrm").unwrap_or("");
    let rotation = (number(transform, "rot", 0.0)? / 60000.0).rem_euclid(360.0);
    let flip_h = enabled(transform, "flipH");
    let flip_v = enabled(transform, "flipV");
    if (rotation != 0.0 || flip_h || flip_v)
        && get_attribute_ref(fill, "rotWithShape")
            .is_some_and(|value| matches!(value, "0" | "false" | "off"))
    {
        return None;
    }
    let crop = rectangle(direct_child_tag(fill, "a:srcRect"))?;
    let destination = rectangle(
        direct_child_tag(fill, "a:stretch")
            .and_then(|stretch| direct_child_tag(stretch, "a:fillRect")),
    )?;
    let blip = direct_child_tag(fill, "a:blip")?;
    let intrinsic_source = source;
    let raster_source = if raster::required(blip, source) {
        Some(raster::render(blip, source, context)?)
    } else {
        None
    };
    let source = raster_source.as_deref().unwrap_or(source);
    let filter = if raster_source.is_some() {
        String::new()
    } else {
        blip_filter(blip, context)?
    };
    let (border, margin) = border(properties, context)?;
    let geometry = direct_child_tag(properties, "a:prstGeom")
        .and_then(|geometry| get_attribute_ref(geometry, "prst"));
    let shaped = direct_child_tag(properties, "a:custGeom").is_some()
        || geometry.is_some_and(|geometry| geometry != "rect");
    if !shaped
        && rotation == 0.0
        && !flip_h
        && !flip_v
        && border.is_empty()
        && filter.is_empty()
        && crop == [0.0; 4]
        && destination == [0.0; 4]
        && raster_source.is_none()
        && direct_child_tag(fill, "a:tile").is_none()
        && direct_child_tag(properties, "a:effectLst")
            .is_none_or(|effects| extract_direct_child_tag_ranges(effects).is_empty())
    {
        return None;
    }
    let path = geometry_path(properties, width, height)?;
    let [left, top, right, bottom] = crop;
    let [fill_left, fill_top, fill_right, fill_bottom] = destination;
    let source_width = 1.0 - left - right;
    let source_height = 1.0 - top - bottom;
    let fill_width = width * (1.0 - fill_left - fill_right);
    let fill_height = height * (1.0 - fill_top - fill_bottom);
    if source_width <= 0.0 || source_height <= 0.0 || fill_width <= 0.0 || fill_height <= 0.0 {
        return None;
    }
    let image_width = fill_width / source_width;
    let image_height = fill_height / source_height;
    let image_x = width * fill_left - left * image_width;
    let image_y = height * fill_top - top * image_height;
    if [image_width, image_height, image_x, image_y]
        .iter()
        .any(|value| !value.is_finite() || value.abs() > 100_000_000.0)
    {
        return None;
    }
    let mut defs = format!(
        r##"<clipPath id="picture-clip"><path d="{path}"/></clipPath><clipPath id="picture-fill"><rect x="{}" y="{}" width="{fill_width}" height="{fill_height}"/></clipPath>"##,
        width * fill_left,
        height * fill_top
    );
    let filter_attribute = if filter.is_empty() {
        String::new()
    } else {
        let _ = write!(
            defs,
            r##"<filter id="picture-filter" x="0" y="0" width="100%" height="100%" color-interpolation-filters="sRGB">{filter}</filter>"##
        );
        r##" filter="url(#picture-filter)""##.to_string()
    };
    let border_element = if border.is_empty() {
        String::new()
    } else {
        let bounds = Bounds::rect(width, height).expanded(margin);
        let _ = write!(
            defs,
            r##"<mask id="picture-border-mask" maskUnits="userSpaceOnUse" {}><rect {} fill="white"/><path d="{path}" fill="black"/></mask>"##,
            bounds.attributes(),
            bounds.attributes()
        );
        format!(r##"<path d="{path}" {border} mask="url(#picture-border-mask)"/>"##)
    };
    let fill_element = if direct_child_tag(fill, "a:tile").is_some() {
        let tiled = tile_picture(
            fill,
            source,
            intrinsic_source,
            width,
            height,
            crop,
            &filter_attribute,
            &mut defs,
        )?;
        format!(r##"<path d="{path}" {tiled}/>"##)
    } else {
        format!(
            r##"<g clip-path="url(#picture-clip)"><g clip-path="url(#picture-fill)"><image xlink:href="{}" x="{image_x}" y="{image_y}" width="{image_width}" height="{image_height}" preserveAspectRatio="none"{filter_attribute}/></g></g>"##,
            escape_xml_text(source)
        )
    };
    let (sin, cos) = rotation.to_radians().sin_cos();
    let sx = if flip_h { -1.0 } else { 1.0 };
    let sy = if flip_v { -1.0 } else { 1.0 };
    let [a, b, c, d] = [cos * sx, sin * sx, -sin * sy, cos * sy];
    let matrix = Matrix([
        a,
        b,
        c,
        d,
        width / 2.0 - a * width / 2.0 - c * height / 2.0,
        height / 2.0 - b * width / 2.0 - d * height / 2.0,
    ]);
    let (body, paint) = apply_shape_effects(
        properties,
        &format!("{fill_element}{border_element}"),
        &path,
        width,
        height,
        margin,
        matrix,
        context,
        &mut defs,
    )?;
    let paint_width = paint.right - paint.left;
    let paint_height = paint.bottom - paint.top;
    let paint_x = paint.left;
    let paint_y = paint.top;
    let bounds = ((paint_x.abs()
        + paint_y.abs()
        + (paint_width - width).abs()
        + (paint_height - height).abs())
        > 0.000001)
        .then(|| PaintBounds {
            x_px: paint_x,
            y_px: paint_y,
            width_px: paint_width,
            height_px: paint_height,
            layout_width_px: width,
            layout_height_px: height,
        });
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{paint_width}" height="{paint_height}" viewBox="{paint_x} {paint_y} {paint_width} {paint_height}" preserveAspectRatio="none"><defs>{defs}</defs>{body}</svg>"##,
    );
    Some(RenderedPicture {
        svg,
        paint_bounds: bounds,
    })
}

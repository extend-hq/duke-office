use std::fmt::Write;

use base64::Engine;
use image::ImageEncoder;

#[path = "pattern_data.rs"]
mod pattern_data;
#[path = "scene3d.rs"]
mod scene3d;

use crate::geometry::{geometry_paths, PathFill};
use crate::xml::{escape_xml_text, extract_direct_child_tag_ranges, get_attribute_ref};
use crate::{
    apply_shape_effects, enabled, number, percentage, render_picture, Matrix, PaintBounds,
    PictureContext, RenderedPicture, ResolvedColor,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StyleReferenceKind {
    Fill,
    Line,
    Effect,
}

fn child<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    crate::xml::direct_child_local(xml, name)
}

fn is_empty_element(xml: &str) -> bool {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let empty = match reader.read_event() {
        Ok(Event::Empty(tag)) => tag.attributes().next().is_none(),
        Ok(Event::Start(tag)) if tag.attributes().next().is_none() => {
            matches!(reader.read_event(), Ok(Event::End(end)) if end.name() == tag.name())
        }
        _ => false,
    };
    empty && matches!(reader.read_event(), Ok(Event::Eof))
}

fn local_name(xml: &str) -> Option<&str> {
    xml.trim_start()
        .strip_prefix('<')?
        .split(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>')
        .next()?
        .rsplit(':')
        .next()
}

fn fill_node(xml: &str) -> Option<&str> {
    [
        "noFill",
        "solidFill",
        "gradFill",
        "pattFill",
        "blipFill",
        "grpFill",
    ]
    .into_iter()
    .find_map(|name| child(xml, name))
}

fn referenced_style(
    style: &str,
    kind: StyleReferenceKind,
    context: &impl PictureContext,
) -> Option<Option<String>> {
    let name = match kind {
        StyleReferenceKind::Fill => "fillRef",
        StyleReferenceKind::Line => "lnRef",
        StyleReferenceKind::Effect => "effectRef",
    };
    let Some(reference) = child(style, name) else {
        return Some(None);
    };
    let index = get_attribute_ref(reference, "idx")?.parse::<u32>().ok()?;
    if index == 0 {
        return Some(None);
    }
    Some(Some(
        context.resolve_style_reference(kind, index, reference)?,
    ))
}

fn merge_line(explicit: &str, inherited: &str) -> String {
    let mut merged = String::from("<a:ln");
    for name in ["w", "cap", "cmpd", "algn"] {
        if let Some(value) =
            get_attribute_ref(explicit, name).or_else(|| get_attribute_ref(inherited, name))
        {
            let _ = write!(merged, " {name}=\"{}\"", escape_xml_text(value));
        }
    }
    merged.push('>');
    if let Some(fill) = fill_node(explicit).or_else(|| fill_node(inherited)) {
        merged.push_str(fill);
    }
    for names in [
        &["prstDash", "custDash"][..],
        &["round", "bevel", "miter"][..],
        &["headEnd"][..],
        &["tailEnd"][..],
    ] {
        if let Some(xml) = names
            .iter()
            .find_map(|name| child(explicit, name))
            .or_else(|| names.iter().find_map(|name| child(inherited, name)))
        {
            merged.push_str(xml);
        }
    }
    merged.push_str("</a:ln>");
    merged
}

fn adjusted_color(mut color: ResolvedColor, mode: PathFill) -> Option<ResolvedColor> {
    let Some((slope, intercept)) = color_adjustment(mode) else {
        return Some(color);
    };
    let mut result = String::from("#");
    for index in 0..3 {
        let channel =
            u8::from_str_radix(color.color.get(1 + index * 2..3 + index * 2)?, 16).ok()?;
        let value = (f64::from(channel) * slope + 255.0 * intercept)
            .round()
            .clamp(0.0, 255.0) as u8;
        let _ = write!(result, "{value:02X}");
    }
    color.color = result;
    Some(color)
}

fn color_adjustment(mode: PathFill) -> Option<(f64, f64)> {
    match mode {
        PathFill::Lighten => Some((0.6, 0.4)),
        PathFill::LightenLess => Some((0.8, 0.2)),
        PathFill::Darken => Some((0.6, 0.0)),
        PathFill::DarkenLess => Some((0.8, 0.0)),
        _ => None,
    }
}

fn bitmap_fill(
    xml: &str,
    width: f64,
    height: f64,
    context: &impl PictureContext,
    defs: &mut String,
) -> Option<()> {
    let blip = child(xml, "blip")?;
    let source = get_attribute_ref(blip, "r:embed")
        .and_then(|id| context.resolve_image(id))
        .or_else(|| get_attribute_ref(blip, "r:link").and_then(|id| context.resolve_image(id)))?;
    let mut picture = String::from("<pic:pic><pic:blipFill");
    for name in ["dpi", "rotWithShape"] {
        if let Some(value) = get_attribute_ref(xml, name) {
            let _ = write!(picture, " {name}=\"{}\"", escape_xml_text(value));
        }
    }
    picture.push('>');
    for tag in extract_direct_child_tag_ranges(xml) {
        picture.push_str(&xml[tag.start..tag.end]);
    }
    picture.push_str("</pic:blipFill><pic:spPr><a:custGeom><a:pathLst><a:path w=\"1\" h=\"1\"><a:moveTo><a:pt x=\"0\" y=\"0\"/></a:moveTo><a:lnTo><a:pt x=\"1\" y=\"0\"/></a:lnTo><a:lnTo><a:pt x=\"1\" y=\"1\"/></a:lnTo><a:lnTo><a:pt x=\"0\" y=\"1\"/></a:lnTo><a:close/></a:path></a:pathLst></a:custGeom></pic:spPr></pic:pic>");
    let rendered = render_picture(&picture, &source, width, height, context)?;
    let source = base64::engine::general_purpose::STANDARD.encode(rendered.svg);
    let _ = write!(
        defs,
        r##"<image id="shape-bitmap" x="0" y="0" width="{width}" height="{height}" xlink:href="data:image/svg+xml;base64,{source}" preserveAspectRatio="none"/>"##
    );
    Some(())
}

#[derive(Clone)]
struct Paint {
    value: String,
    opacity: f64,
}

#[derive(Clone, Debug)]
pub struct TextFill {
    pub attributes: String,
    pub definitions: String,
}

pub fn render_text_fill(
    fill_xml: &str,
    id: &str,
    context: &impl PictureContext,
) -> Option<TextFill> {
    if fill_xml.len() > 16 * 1024
        || id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'-' | b'_'))
        || !matches!(local_name(fill_xml)?, "noFill" | "solidFill" | "gradFill")
        || child(fill_xml, "path")
            .is_some_and(|path| get_attribute_ref(path, "path") == Some("rect"))
    {
        return None;
    }
    let mut definitions = String::new();
    let fill = paint(
        Some(fill_xml),
        false,
        id,
        1.0,
        1.0,
        PathFill::Norm,
        context,
        &mut definitions,
    )?;
    definitions = definitions.replace(
        "gradientUnits=\"userSpaceOnUse\"",
        "gradientUnits=\"objectBoundingBox\"",
    );
    Some(TextFill {
        attributes: format!(r#"fill="{}" fill-opacity="{}""#, fill.value, fill.opacity),
        definitions,
    })
}

fn resolved_paint(xml: &str, mode: PathFill, context: &impl PictureContext) -> Option<Paint> {
    let color = adjusted_color(context.resolve_color(xml)?, mode)?;
    let opacity = color.opacity.unwrap_or(1.0);
    if !opacity.is_finite() {
        return None;
    }
    Some(Paint {
        value: escape_xml_text(&color.color),
        opacity: opacity.clamp(0.0, 1.0),
    })
}

fn gradient(
    xml: &str,
    rectangular: bool,
    id: &str,
    width: f64,
    height: f64,
    mode: PathFill,
    context: &impl PictureContext,
    defs: &mut String,
) -> Option<Paint> {
    if child(xml, "tileRect").is_some_and(|tile| {
        ["l", "t", "r", "b"]
            .iter()
            .any(|name| percentage(tile, name, 0.0) != Some(0.0))
    }) {
        return None;
    }
    let stops = child(xml, "gsLst")?;
    let mut colors = Vec::new();
    for stop in extract_direct_child_tag_ranges(stops) {
        if colors.len() >= 256 || stop.tag_name.rsplit(':').next() != Some("gs") {
            return None;
        }
        let stop = &stops[stop.start..stop.end];
        colors.push((
            percentage(stop, "pos", 0.0)?.clamp(0.0, 1.0),
            resolved_paint(stop, mode, context)?,
        ));
    }
    if colors.is_empty() {
        return None;
    }
    colors.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut stop_markup = String::new();
    for (position, paint) in &colors {
        let _ = write!(
            stop_markup,
            r##"<stop offset="{position}" stop-color="{}" stop-opacity="{}"/>"##,
            paint.value, paint.opacity
        );
    }
    if let Some(path) = child(xml, "path") {
        let kind = get_attribute_ref(path, "path").unwrap_or("shape");
        if kind == "rect" || (kind == "shape" && rectangular) {
            return rectangular_gradient(path, id, width, height, &colors, defs);
        }
        if kind != "circle" {
            return None;
        }
        let destination = child(path, "fillToRect").unwrap_or("");
        let left = percentage(destination, "l", 0.0)?;
        let top = percentage(destination, "t", 0.0)?;
        let right = 1.0 - percentage(destination, "r", 0.0)?;
        let bottom = 1.0 - percentage(destination, "b", 0.0)?;
        let cx = (left + right) * width / 2.0;
        let cy = (top + bottom) * height / 2.0;
        if ![cx, cy]
            .into_iter()
            .all(|value| value.is_finite() && value.abs() <= 100_000_000.0)
        {
            return None;
        }
        let radius = libm::hypot(
            cx.abs().max((width - cx).abs()),
            cy.abs().max((height - cy).abs()),
        );
        let _ = write!(
            defs,
            r##"<radialGradient id="{id}" gradientUnits="userSpaceOnUse" cx="{cx}" cy="{cy}" r="{radius}" fx="{cx}" fy="{cy}" color-interpolation="sRGB">{stop_markup}</radialGradient>"##
        );
    } else {
        let linear = child(xml, "lin").unwrap_or("");
        let radians = (number(linear, "ang", 0.0)? / 60_000.0).to_radians();
        let (mut dy, mut dx) = libm::sincos(radians);
        if !get_attribute_ref(linear, "scaled").is_some_and(|v| matches!(v, "0" | "false")) {
            dx /= width.max(f64::EPSILON);
            dy /= height.max(f64::EPSILON);
            let length = libm::hypot(dx, dy);
            if length > 0.0 {
                dx /= length;
                dy /= length;
            }
        }
        let span = width * dx.abs() + height * dy.abs();
        let _ = write!(
            defs,
            r##"<linearGradient id="{id}" gradientUnits="userSpaceOnUse" x1="{}" y1="{}" x2="{}" y2="{}" color-interpolation="sRGB">{stop_markup}</linearGradient>"##,
            width / 2.0 - dx * span / 2.0,
            height / 2.0 - dy * span / 2.0,
            width / 2.0 + dx * span / 2.0,
            height / 2.0 + dy * span / 2.0
        );
    }
    Some(Paint {
        value: format!("url(#{id})"),
        opacity: 1.0,
    })
}

fn rectangular_gradient(
    path: &str,
    id: &str,
    width: f64,
    height: f64,
    stops: &[(f64, Paint)],
    defs: &mut String,
) -> Option<Paint> {
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let target = child(path, "fillToRect").unwrap_or("");
    let left = percentage(target, "l", 0.5)? * width;
    let top = percentage(target, "t", 0.5)? * height;
    let right = (1.0 - percentage(target, "r", 0.5)?) * width;
    let bottom = (1.0 - percentage(target, "b", 0.5)?) * height;
    if left < 0.0 || top < 0.0 || right > width || bottom > height || left > right || top > bottom {
        return None;
    }
    let colors: Vec<_> = stops
        .iter()
        .map(|(position, paint)| Some((*position, rgba(paint)?)))
        .collect::<Option<_>>()?;
    let scale = (300.0_f64 / 96.0)
        .min((4_194_304.0 / (width * height)).sqrt())
        .min(8192.0 / width.max(height));
    let pixel_width = (width * scale).ceil() as u32;
    let pixel_height = (height * scale).ceil() as u32;
    let count = pixel_width.checked_mul(pixel_height)?.checked_mul(4)? as usize;
    if count > 17_000_000 || pixel_width == 0 || pixel_height == 0 {
        return None;
    }
    let distance = |point: f64, first: f64, last: f64, extent: f64| {
        if point < first && first > 0.0 {
            (first - point) / first
        } else if point > last && last < extent {
            (point - last) / (extent - last)
        } else {
            0.0
        }
    };
    let mut pixels = Vec::with_capacity(count);
    for y in 0..pixel_height {
        for x in 0..pixel_width {
            let x = (f64::from(x) + 0.5) / scale;
            let y = (f64::from(y) + 0.5) / scale;
            let position = distance(x, left, right, width)
                .max(distance(y, top, bottom, height))
                .clamp(0.0, 1.0);
            let end = colors.partition_point(|(offset, _)| *offset <= position);
            let (start, end) = if end == 0 {
                (colors[0], colors[0])
            } else if end == colors.len() {
                (colors[end - 1], colors[end - 1])
            } else {
                (colors[end - 1], colors[end])
            };
            let ratio = if end.0 > start.0 {
                (position - start.0) / (end.0 - start.0)
            } else {
                0.0
            };
            pixels.extend(mix_color(start.1, end.1, ratio));
        }
    }
    let source = png_source(&pixels, pixel_width, pixel_height)?;
    let image_width = f64::from(pixel_width) / scale;
    let image_height = f64::from(pixel_height) / scale;
    let _ = write!(
        defs,
        r##"<pattern id="{id}" width="{width}" height="{height}" patternUnits="userSpaceOnUse"><image width="{image_width}" height="{image_height}" xlink:href="{source}" preserveAspectRatio="none"/></pattern>"##
    );
    Some(Paint {
        value: format!("url(#{id})"),
        opacity: 1.0,
    })
}

fn rgba(paint: &Paint) -> Option<[f64; 4]> {
    let value = paint.value.strip_prefix('#')?;
    if value.len() != 6 {
        return None;
    }
    Some([
        f64::from(u8::from_str_radix(&value[0..2], 16).ok()?),
        f64::from(u8::from_str_radix(&value[2..4], 16).ok()?),
        f64::from(u8::from_str_radix(&value[4..6], 16).ok()?),
        paint.opacity,
    ])
}

fn png_source(pixels: &[u8], width: u32, height: u32) -> Option<String> {
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(pixels, width, height, image::ExtendedColorType::Rgba8)
        .ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ))
}

fn mix_color(start: [f64; 4], end: [f64; 4], ratio: f64) -> [u8; 4] {
    let alpha = start[3] * (1.0 - ratio) + end[3] * ratio;
    let mut result = [0u8; 4];
    if alpha > 0.0 {
        for channel in 0..3 {
            result[channel] = ((start[channel] * start[3] * (1.0 - ratio)
                + end[channel] * end[3] * ratio)
                / alpha)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
    result[3] = (alpha * 255.0).round().clamp(0.0, 255.0) as u8;
    result
}

fn pattern(
    xml: &str,
    id: &str,
    mode: PathFill,
    context: &impl PictureContext,
    defs: &mut String,
) -> Option<Paint> {
    let foreground = rgba(&resolved_paint(child(xml, "fgClr")?, mode, context)?)?;
    let background = rgba(&resolved_paint(child(xml, "bgClr")?, mode, context)?)?;
    let coverage = pattern_data::coverage(get_attribute_ref(xml, "prst")?)?;
    let mut pixels = Vec::with_capacity(64 * 64 * 4);
    for y in 0..64 {
        for x in 0..64 {
            let amount = f64::from(coverage[(y / 8) * 8 + x / 8]) / 2.0;
            pixels.extend(mix_color(background, foreground, amount));
        }
    }
    let source = png_source(&pixels, 64, 64)?;
    let period = 32.0 / 3.0;
    let _ = write!(
        defs,
        r##"<pattern id="{id}" x="0" y="0" width="{period}" height="{period}" patternUnits="userSpaceOnUse" data-office-art-pattern-origin="page" data-office-art-pattern-period="{period}"><image width="{period}" height="{period}" preserveAspectRatio="none" xlink:href="{source}"/></pattern>"##
    );
    Some(Paint {
        value: format!("url(#{id})"),
        opacity: 1.0,
    })
}

fn paint(
    xml: Option<&str>,
    rectangular: bool,
    id: &str,
    width: f64,
    height: f64,
    mode: PathFill,
    context: &impl PictureContext,
    defs: &mut String,
) -> Option<Paint> {
    if matches!(mode, PathFill::None) || xml.is_none() {
        return Some(Paint {
            value: "none".into(),
            opacity: 1.0,
        });
    }
    let xml = xml?;
    match local_name(xml)? {
        "noFill" => Some(Paint {
            value: "none".into(),
            opacity: 1.0,
        }),
        "solidFill" => resolved_paint(xml, mode, context),
        "gradFill" => gradient(xml, rectangular, id, width, height, mode, context, defs),
        "pattFill" => pattern(xml, id, mode, context, defs),
        _ => None,
    }
}

struct Stroke {
    attributes: String,
    width: f64,
    margin: f64,
    inset: bool,
    compound: &'static [f64],
    mask_attributes: String,
}

fn marker(xml: &str, id: &str, paint: &Paint, defs: &mut String) -> Option<f64> {
    let kind = get_attribute_ref(xml, "type").unwrap_or("none");
    if kind == "none" {
        return Some(0.0);
    }
    let dimension = |value| -> Option<f64> {
        match value {
            "sm" => Some(2.0),
            "med" => Some(3.0),
            "lg" => Some(5.0),
            _ => None,
        }
    };
    let length = dimension(get_attribute_ref(xml, "len").unwrap_or("med"))?;
    let width = dimension(get_attribute_ref(xml, "w").unwrap_or("med"))?;
    let half = width / 2.0;
    let body = match kind {
        "triangle" => format!(
            r##"<path d="M0 0L{length} {half}L0 {width}Z" fill="{}"/>"##,
            paint.value
        ),
        "stealth" => format!(
            r##"<path d="M0 0L{length} {half}L0 {width}L{} {half}Z" fill="{}"/>"##,
            length / 3.0,
            paint.value
        ),
        "diamond" => format!(
            r##"<path d="M0 {half}L{} 0L{length} {half}L{} {width}Z" fill="{}"/>"##,
            length / 2.0,
            length / 2.0,
            paint.value
        ),
        "oval" => format!(
            r##"<ellipse cx="{}" cy="{half}" rx="{}" ry="{half}" fill="{}"/>"##,
            length / 2.0,
            length / 2.0,
            paint.value
        ),
        "arrow" => format!(
            r##"<path d="M0 0L{length} {half}L0 {width}" fill="none" stroke="{}" stroke-width="1" stroke-linejoin="miter"/>"##,
            paint.value
        ),
        _ => return None,
    };
    let _ = write!(
        defs,
        r##"<marker id="{id}" markerUnits="strokeWidth" markerWidth="{length}" markerHeight="{width}" refX="{length}" refY="{half}" orient="auto-start-reverse" overflow="visible"><g opacity="{}">{body}</g></marker>"##,
        paint.opacity
    );
    Some(length.max(width))
}

fn stroke(
    xml: Option<&str>,
    rectangular: bool,
    width: f64,
    height: f64,
    context: &impl PictureContext,
    defs: &mut String,
) -> Option<Stroke> {
    let Some(xml) = xml else {
        return Some(Stroke {
            attributes: "stroke=\"none\"".into(),
            width: 0.0,
            margin: 0.0,
            inset: false,
            compound: &[],
            mask_attributes: String::new(),
        });
    };
    let line_width = number(xml, "w", 12700.0)?.max(0.0) / 9525.0;
    if line_width > 100_000.0 {
        return None;
    }
    let compound: &'static [f64] = match get_attribute_ref(xml, "cmpd").unwrap_or("sng") {
        "sng" => &[],
        "dbl" => &[1.0, 1.0, 1.0],
        "thickThin" => &[3.0, 1.0, 1.0],
        "thinThick" => &[1.0, 1.0, 3.0],
        "tri" => &[1.0, 1.0, 2.0, 1.0, 1.0],
        _ => return None,
    };
    let inset = match get_attribute_ref(xml, "algn").unwrap_or("ctr") {
        "ctr" => false,
        "in" => true,
        _ => return None,
    };
    let default_fill = "<a:solidFill><a:srgbClr val=\"000000\"/></a:solidFill>";
    let color = paint(
        Some(fill_node(xml).unwrap_or(default_fill)),
        rectangular,
        "shape-stroke",
        width,
        height,
        PathFill::Norm,
        context,
        defs,
    )?;
    let cap = match get_attribute_ref(xml, "cap").unwrap_or("flat") {
        "flat" => "butt",
        "rnd" => "round",
        "sq" => "square",
        _ => return None,
    };
    let join = if child(xml, "miter").is_some() {
        "miter"
    } else if child(xml, "bevel").is_some() {
        "bevel"
    } else {
        "round"
    };
    let limit = percentage(child(xml, "miter").unwrap_or(""), "lim", 8.0)?.clamp(1.0, 100.0);
    let mut attributes = format!(
        r##"stroke="{}" stroke-opacity="{}" stroke-width="{}" stroke-linecap="{cap}" stroke-linejoin="{join}" stroke-miterlimit="{limit}""##,
        color.value,
        color.opacity,
        line_width * if inset { 2.0 } else { 1.0 }
    );
    let mut dash = Vec::new();
    if let Some(preset) = child(xml, "prstDash") {
        let values =
            crate::preset_dash_segments(get_attribute_ref(preset, "val").unwrap_or("solid"))?;
        dash.extend(values.iter().map(|value| value * line_width));
    }
    if let Some(custom) = child(xml, "custDash") {
        dash.clear();
        for stop in extract_direct_child_tag_ranges(custom) {
            if dash.len() >= 512 {
                return None;
            }
            let stop = &custom[stop.start..stop.end];
            dash.extend([
                percentage(stop, "d", 1.0)?.max(0.0) * line_width,
                percentage(stop, "sp", 1.0)?.max(0.0) * line_width,
            ]);
        }
    }
    if !dash.is_empty() && dash.iter().any(|value| *value > 0.0) {
        let _ = write!(
            attributes,
            r##" stroke-dasharray="{}""##,
            dash.iter()
                .map(f64::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
    let mut margin = if inset {
        0.0
    } else {
        line_width / 2.0 * if join == "miter" { limit } else { 1.0 }
    };
    for (name, position) in [("headEnd", "start"), ("tailEnd", "end")] {
        if let Some(endpoint) = child(xml, name) {
            if !compound.is_empty()
                && get_attribute_ref(endpoint, "type").is_some_and(|value| value != "none")
            {
                return None;
            }
            let id = format!("shape-marker-{position}");
            let extent = marker(endpoint, &id, &color, defs)?;
            if extent > 0.0 {
                let _ = write!(attributes, r##" marker-{position}="url(#{id})""##);
                margin = margin.max((extent + 1.0) * line_width);
            }
        }
    }
    if color.value == "none" || line_width == 0.0 {
        margin = 0.0;
    }
    Some(Stroke {
        attributes,
        width: line_width,
        margin,
        inset,
        compound,
        mask_attributes: format!(
            r##"stroke-linecap="{cap}" stroke-linejoin="{join}" stroke-miterlimit="{limit}""##
        ),
    })
}

fn compound_outline(
    path: &str,
    stroke: &Stroke,
    index: usize,
    bounds: [f64; 4],
    defs: &mut String,
    body: &mut String,
) {
    let padding = stroke.margin.max(stroke.width * 2.0) + 1.0;
    let left = bounds[0] - padding;
    let top = bounds[1] - padding;
    let right = bounds[2] + padding;
    let bottom = bounds[3] + padding;
    let mask_width = right - left;
    let mask_height = bottom - top;
    let inside = format!("shape-compound-inside-{index}");
    let outside = format!("shape-compound-outside-{index}");
    let _ = write!(
        defs,
        r##"<clipPath id="{inside}"><path d="{path}"/></clipPath><clipPath id="{outside}"><path clip-rule="evenodd" d="M{left} {top}H{right}V{bottom}H{left}Z {path}"/></clipPath>"##
    );
    let total: f64 = stroke.compound.iter().sum();
    let mut position = if stroke.inset {
        0.0
    } else {
        -stroke.width / 2.0
    };
    for (band, weight) in stroke.compound.iter().enumerate() {
        let end = position + weight / total * stroke.width;
        if band % 2 == 0 {
            for (side, inner, outer, clip) in [
                ("outer", (-end).max(0.0), (-position).max(0.0), &outside),
                ("inner", position.max(0.0), end.max(0.0), &inside),
            ] {
                if outer <= inner {
                    continue;
                }
                let id = format!("shape-compound-{index}-{band}-{side}");
                let _ = write!(
                    defs,
                    r##"<mask id="{id}" maskUnits="userSpaceOnUse" x="{left}" y="{top}" width="{mask_width}" height="{mask_height}" style="mask-type:luminance"><g clip-path="url(#{clip})"><path d="{path}" fill="none" stroke="white" stroke-width="{}" {}/>"##,
                    outer * 2.0,
                    stroke.mask_attributes
                );
                if inner > 0.0 {
                    let _ = write!(
                        defs,
                        r##"<path d="{path}" fill="none" stroke="black" stroke-width="{}" {}/>"##,
                        inner * 2.0,
                        stroke.mask_attributes
                    );
                }
                defs.push_str("</g></mask>");
                let _ = write!(
                    body,
                    r##"<path d="{path}" fill="none" {} mask="url(#{id})"/>"##,
                    stroke.attributes
                );
            }
        }
        position = end;
    }
}

pub fn render_shape(
    properties_xml: &str,
    style_xml: &str,
    width: f64,
    height: f64,
    context: &impl PictureContext,
) -> Option<RenderedPicture> {
    if !width.is_finite()
        || !height.is_finite()
        || width < 0.0
        || height < 0.0
        || width > 1_000_000.0
        || height > 1_000_000.0
        || properties_xml.len() > 4 * 1024 * 1024
        || style_xml.len() > 1024 * 1024
    {
        return None;
    }
    let paths = geometry_paths(properties_xml, width, height)?;
    render_shape_paths(&paths, properties_xml, style_xml, width, height, context)
}

pub fn render_3d_path(
    path: &str,
    properties_xml: &str,
    fill_xml: &str,
    line_xml: Option<&str>,
    width: f64,
    height: f64,
    context: &impl PictureContext,
) -> Option<RenderedPicture> {
    use kurbo::Shape;
    if path.len() > 4 * 1024 * 1024
        || !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
        || width > 1_000_000.0
        || height > 1_000_000.0
        || properties_xml.len() > 4 * 1024 * 1024
        || fill_xml.len() > 1024 * 1024
        || line_xml.is_some_and(|xml| xml.len() > 1024 * 1024)
    {
        return None;
    }
    let bounds = kurbo::BezPath::from_svg(path).ok()?.bounding_box();
    let path = crate::geometry::GeometryPath {
        d: path.to_string(),
        fill: PathFill::Norm,
        stroke: line_xml.is_some(),
        closed: true,
        bounds: [bounds.x0, bounds.y0, bounds.x1, bounds.y1],
    };
    let inner = extract_direct_child_tag_ranges(properties_xml)
        .into_iter()
        .map(|tag| &properties_xml[tag.start..tag.end])
        .collect::<String>();
    let properties = format!(
        "<a:spPr>{inner}{fill_xml}{}</a:spPr>",
        line_xml.unwrap_or("")
    );
    render_shape_paths(&[path], &properties, "", width, height, context)
}

fn render_shape_paths(
    paths: &[crate::geometry::GeometryPath],
    properties_xml: &str,
    style_xml: &str,
    width: f64,
    height: f64,
    context: &impl PictureContext,
) -> Option<RenderedPicture> {
    if paths.is_empty() {
        return None;
    }
    let fill_reference = if fill_node(properties_xml).is_none() {
        referenced_style(style_xml, StyleReferenceKind::Fill, context)?
    } else {
        None
    };
    let fill = fill_node(properties_xml).or(fill_reference.as_deref());
    let inherited_line = if child(style_xml, "lnRef").is_some() {
        referenced_style(style_xml, StyleReferenceKind::Line, context)?
    } else {
        None
    };
    let merged_line = child(properties_xml, "ln")
        .map(|line| merge_line(line, inherited_line.as_deref().unwrap_or("")));
    let line = merged_line.as_deref().or(inherited_line.as_deref());
    let explicit_effects =
        child(properties_xml, "effectLst").or_else(|| child(properties_xml, "effectDag"));
    let effect_reference = if explicit_effects.is_none() {
        referenced_style(style_xml, StyleReferenceKind::Effect, context)?
    } else {
        None
    };
    let effects = explicit_effects
        .or(effect_reference.as_deref())
        .unwrap_or("");
    let effective_properties = format!("<a:spPr>{effects}</a:spPr>");
    let mut defs = String::new();
    let rectangular = child(properties_xml, "prstGeom")
        .is_some_and(|xml| get_attribute_ref(xml, "prst") == Some("rect"));
    let stroke = stroke(line, rectangular, width, height, context, &mut defs)?;
    if !stroke.compound.is_empty() && paths.iter().any(|path| path.stroke && !path.closed) {
        return None;
    }
    let mut body = String::new();
    let mut silhouette = String::new();
    let mut outside = 0.0f64;
    let mut scene_bounds = None;
    let mut bitmap_ready = false;
    let mut fill_cache: Vec<(PathFill, Paint)> = Vec::new();
    let three_dimensional = child(properties_xml, "scene3d").is_some()
        || child(properties_xml, "sp3d").is_some_and(|xml| !is_empty_element(xml))
        || child(properties_xml, "props3d").is_some();
    if three_dimensional {
        let scene = scene3d::render(
            paths,
            properties_xml,
            fill,
            line,
            width,
            height,
            context,
            &mut defs,
        )?;
        body = scene.body;
        silhouette = scene.silhouette;
        outside = scene.outside;
        scene_bounds = Some(scene.bounds);
    } else {
        for (index, path) in paths.iter().enumerate() {
            if !path
                .bounds
                .into_iter()
                .all(|value| value.is_finite() && value.abs() <= 100_000_000.0)
            {
                return None;
            }
            outside = outside
                .max(-path.bounds[0])
                .max(-path.bounds[1])
                .max(path.bounds[2] - width)
                .max(path.bounds[3] - height);
            if !matches!(path.fill, PathFill::None)
                && fill.is_some_and(|xml| local_name(xml) == Some("blipFill"))
            {
                if !bitmap_ready {
                    bitmap_fill(fill?, width, height, context, &mut defs)?;
                    bitmap_ready = true;
                }
                let clip = format!("shape-bitmap-clip-{index}");
                let _ = write!(
                    defs,
                    r##"<clipPath id="{clip}"><path d="{}"/></clipPath>"##,
                    path.d
                );
                let filter = if let Some((slope, intercept)) = color_adjustment(path.fill) {
                    let id = format!("shape-bitmap-shade-{index}");
                    let _ = write!(
                        defs,
                        r##"<filter id="{id}" color-interpolation-filters="sRGB"><feComponentTransfer><feFuncR type="linear" slope="{slope}" intercept="{intercept}"/><feFuncG type="linear" slope="{slope}" intercept="{intercept}"/><feFuncB type="linear" slope="{slope}" intercept="{intercept}"/></feComponentTransfer></filter>"##
                    );
                    format!(r##" filter="url(#{id})""##)
                } else {
                    String::new()
                };
                let _ = write!(
                    body,
                    r##"<g clip-path="url(#{clip})"{filter}><use xlink:href="#shape-bitmap"/></g>"##
                );
            } else {
                let fill = if let Some((_, cached)) =
                    fill_cache.iter().find(|(mode, _)| *mode == path.fill)
                {
                    cached.clone()
                } else {
                    let rendered = paint(
                        fill,
                        rectangular,
                        &format!("shape-fill-{index}"),
                        width,
                        height,
                        path.fill,
                        context,
                        &mut defs,
                    )?;
                    fill_cache.push((path.fill, rendered.clone()));
                    rendered
                };
                if fill.value != "none" {
                    let _ = write!(
                        body,
                        r##"<path d="{}" fill="{}" fill-opacity="{}" stroke="none"/>"##,
                        path.d, fill.value, fill.opacity
                    );
                }
            }
            if path.stroke && stroke.width > 0.0 {
                if !stroke.compound.is_empty() {
                    compound_outline(&path.d, &stroke, index, path.bounds, &mut defs, &mut body);
                    silhouette.push_str(&path.d);
                    silhouette.push(' ');
                    continue;
                }
                let clip = if stroke.inset && path.closed {
                    let id = format!("shape-outline-{index}");
                    let _ = write!(
                        defs,
                        r##"<clipPath id="{id}"><path d="{}"/></clipPath>"##,
                        path.d
                    );
                    format!(r##" clip-path="url(#{id})""##)
                } else {
                    String::new()
                };
                let _ = write!(
                    body,
                    r##"<path d="{}" fill="none" {}{clip}/>"##,
                    path.d, stroke.attributes
                );
            }
            silhouette.push_str(&path.d);
            silhouette.push(' ');
        }
    }
    let transform = child(properties_xml, "xfrm").unwrap_or("");
    let rotation = (number(transform, "rot", 0.0)? / 60000.0).rem_euclid(360.0);
    if (rotation != 0.0 || enabled(transform, "flipH") || enabled(transform, "flipV"))
        && fill.is_some_and(|xml| {
            get_attribute_ref(xml, "rotWithShape").is_some_and(|v| matches!(v, "0" | "false"))
        })
    {
        return None;
    }
    let (sin, cos) = libm::sincos(rotation.to_radians());
    let sx = if enabled(transform, "flipH") {
        -1.0
    } else {
        1.0
    };
    let sy = if enabled(transform, "flipV") {
        -1.0
    } else {
        1.0
    };
    let [a, b, c, d] = [cos * sx, sin * sx, -sin * sy, cos * sy];
    let matrix = Matrix([
        a,
        b,
        c,
        d,
        width / 2.0 - a * width / 2.0 - c * height / 2.0,
        height / 2.0 - b * width / 2.0 - d * height / 2.0,
    ]);
    let (body, mut bounds) = apply_shape_effects(
        &effective_properties,
        &body,
        &silhouette,
        width,
        height,
        outside + stroke.margin,
        matrix,
        context,
        &mut defs,
        true,
    )?;
    if effects.is_empty() {
        if let Some([left, top, right, bottom]) = scene_bounds {
            bounds = crate::Bounds {
                left,
                top,
                right,
                bottom,
            }
            .transformed(matrix);
        }
    }
    if bounds.right - bounds.left < 0.01 {
        bounds.left -= 0.5;
        bounds.right += 0.5;
    }
    if bounds.bottom - bounds.top < 0.01 {
        bounds.top -= 0.5;
        bounds.bottom += 0.5;
    }
    if !bounds.valid() {
        return None;
    }
    bounds.left = bounds.left.floor();
    bounds.top = bounds.top.floor();
    bounds.right = bounds.right.ceil();
    bounds.bottom = bounds.bottom.ceil();
    let paint_width = bounds.right - bounds.left;
    let paint_height = bounds.bottom - bounds.top;
    let expanded = (bounds.left.abs()
        + bounds.top.abs()
        + (paint_width - width).abs()
        + (paint_height - height).abs())
        > 0.000001;
    let paint_bounds = expanded.then(|| PaintBounds {
        x_px: bounds.left,
        y_px: bounds.top,
        width_px: paint_width,
        height_px: paint_height,
        layout_width_px: width.max(1.0),
        layout_height_px: height.max(1.0),
    });
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{paint_width}" height="{paint_height}" viewBox="{} {} {paint_width} {paint_height}" preserveAspectRatio="none"><defs>{defs}</defs>{body}</svg>"##,
        bounds.left, bounds.top
    );
    Some(RenderedPicture { svg, paint_bounds })
}

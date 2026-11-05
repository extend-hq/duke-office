use std::fmt::Write;

use base64::Engine;

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
    extract_direct_child_tag_ranges(xml)
        .into_iter()
        .find(|tag| tag.tag_name.rsplit(':').next() == Some(name))
        .map(|tag| &xml[tag.start..tag.end])
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

struct Paint {
    value: String,
    opacity: f64,
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
        if get_attribute_ref(path, "path").unwrap_or("shape") != "circle" {
            return None;
        }
        let destination = child(path, "fillToRect").unwrap_or("");
        let left = percentage(destination, "l", 0.0)?;
        let top = percentage(destination, "t", 0.0)?;
        let right = 1.0 - percentage(destination, "r", 0.0)?;
        let bottom = 1.0 - percentage(destination, "b", 0.0)?;
        let cx = (left + right) / 2.0;
        let cy = (top + bottom) / 2.0;
        if ![cx, cy]
            .into_iter()
            .all(|value| value.is_finite() && value.abs() <= 100.0)
        {
            return None;
        }
        let _ = write!(
            defs,
            r##"<radialGradient id="{id}" cx="{cx}" cy="{cy}" r="0.5" fx="{cx}" fy="{cy}" color-interpolation="sRGB">{stop_markup}</radialGradient>"##
        );
    } else {
        let linear = child(xml, "lin").unwrap_or("");
        let radians = (number(linear, "ang", 0.0)? / 60_000.0).to_radians();
        let mut dx = radians.cos();
        let mut dy = radians.sin();
        if !get_attribute_ref(linear, "scaled").is_some_and(|v| matches!(v, "0" | "false")) {
            dx *= width;
            dy *= height;
            let length = dx.hypot(dy);
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

fn pattern(
    xml: &str,
    id: &str,
    mode: PathFill,
    context: &impl PictureContext,
    defs: &mut String,
) -> Option<Paint> {
    let foreground = resolved_paint(child(xml, "fgClr")?, mode, context)?;
    let background = resolved_paint(child(xml, "bgClr")?, mode, context)?;
    let preset = get_attribute_ref(xml, "prst")?;
    let mut ink = String::new();
    let mut size = 8.0;
    if let Some(percent) = preset
        .strip_prefix("pct")
        .and_then(|value| value.parse::<u32>().ok())
    {
        if ![5, 10, 20, 25, 30, 40, 50, 60, 70, 75, 80, 90].contains(&percent) {
            return None;
        }
        let bayer = [
            [0, 32, 8, 40, 2, 34, 10, 42],
            [48, 16, 56, 24, 50, 18, 58, 26],
            [12, 44, 4, 36, 14, 46, 6, 38],
            [60, 28, 52, 20, 62, 30, 54, 22],
            [3, 35, 11, 43, 1, 33, 9, 41],
            [51, 19, 59, 27, 49, 17, 57, 25],
            [15, 47, 7, 39, 13, 45, 5, 37],
            [63, 31, 55, 23, 61, 29, 53, 21],
        ];
        for (y, row) in bayer.iter().enumerate() {
            for (x, value) in row.iter().enumerate() {
                if value * 100 < percent * 64 {
                    let _ = write!(ink, r##"<rect x="{x}" y="{y}" width="1" height="1"/>"##);
                }
            }
        }
    } else {
        let (path, weight) = match preset {
            "horz" => ("M0 0H8", 1.0),
            "vert" => ("M0 0V8", 1.0),
            "ltHorz" => {
                size = 4.0;
                ("M0 0H8", 1.0)
            }
            "ltVert" => {
                size = 4.0;
                ("M0 0V8", 1.0)
            }
            "dkHorz" => {
                size = 4.0;
                ("M0 0H8", 2.0)
            }
            "dkVert" => {
                size = 4.0;
                ("M0 0V8", 2.0)
            }
            "narHorz" => {
                size = 2.0;
                ("M0 0H8", 1.0)
            }
            "narVert" => {
                size = 2.0;
                ("M0 0V8", 1.0)
            }
            "cross" => ("M0 0H8 M0 0V8", 1.0),
            "dnDiag" => ("M-1 -1L9 9 M-1 7L1 9 M7 -1L9 1", 1.0),
            "upDiag" => ("M-1 9L9 -1 M-1 1L1 -1 M7 9L9 7", 1.0),
            "ltDnDiag" => {
                size = 4.0;
                ("M-1 -1L9 9 M-1 3L5 9 M3 -1L9 5", 1.0)
            }
            "ltUpDiag" => {
                size = 4.0;
                ("M-1 5L5 -1 M-1 1L1 -1 M3 5L5 3", 1.0)
            }
            "dkDnDiag" | "wdDnDiag" => ("M-2 -2L10 10 M-2 6L2 10 M6 -2L10 2", 3.0),
            "dkUpDiag" | "wdUpDiag" => ("M-2 10L10 -2 M-2 2L2 -2 M6 10L10 6", 3.0),
            "diagCross" => ("M-1 -1L9 9 M-1 9L9 -1", 1.0),
            "dashHorz" => ("M0 0H4 M4 4H8", 1.0),
            "dashVert" => ("M0 0V4 M4 4V8", 1.0),
            "dashDnDiag" => ("M0 0L3 3 M4 4L7 7", 1.0),
            "dashUpDiag" => ("M0 7L3 4 M4 3L7 0", 1.0),
            "smGrid" => {
                size = 4.0;
                ("M0 0H8 M0 0V8", 1.0)
            }
            "lgGrid" => ("M0 0H8 M0 0V8", 1.0),
            "dotGrid" => {
                ink.push_str("<rect width=\"1\" height=\"1\"/>");
                ("", 0.0)
            }
            "smCheck" => {
                size = 4.0;
                ink.push_str("<path d=\"M0 0H2V2H0Z M2 2H4V4H2Z\"/>");
                ("", 0.0)
            }
            "lgCheck" => {
                ink.push_str("<path d=\"M0 0H4V4H0Z M4 4H8V8H4Z\"/>");
                ("", 0.0)
            }
            "horzBrick" => ("M0 0H8 M0 4H8 M0 0V4 M4 4V8", 1.0),
            "diagBrick" => ("M-1 9L9 -1 M-1 1L7 9 M3 -1L5 1", 1.0),
            "openDmnd" | "solidDmnd" => ("M4 0L8 4L4 8L0 4Z", 1.0),
            "zigZag" => ("M0 2L2 0L6 4L8 2 M0 6L2 4L6 8L8 6", 1.0),
            "wave" => ("M0 2Q2 -1 4 2T8 2 M0 6Q2 3 4 6T8 6", 1.0),
            _ => return None,
        };
        if !path.is_empty() {
            let _ = write!(
                ink,
                r##"<path d="{path}" fill="{}" stroke="{}" stroke-width="{weight}"/>"##,
                if preset == "solidDmnd" {
                    foreground.value.as_str()
                } else {
                    "none"
                },
                foreground.value
            );
        }
    }
    let _ = write!(
        defs,
        r##"<pattern id="{id}" width="{size}" height="{size}" patternUnits="userSpaceOnUse"><rect width="{size}" height="{size}" fill="{}" fill-opacity="{}"/><g fill="{}" opacity="{}">{ink}</g></pattern>"##,
        background.value, background.opacity, foreground.value, foreground.opacity
    );
    Some(Paint {
        value: format!("url(#{id})"),
        opacity: 1.0,
    })
}

fn paint(
    xml: Option<&str>,
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
        "gradFill" => gradient(xml, id, width, height, mode, context, defs),
        "pattFill" => pattern(xml, id, mode, context, defs),
        _ => None,
    }
}

struct Stroke {
    attributes: String,
    width: f64,
    margin: f64,
    inset: bool,
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
        });
    };
    let line_width = number(xml, "w", 12700.0)?.max(0.0) / 9525.0;
    if line_width > 100_000.0 || get_attribute_ref(xml, "cmpd").is_some_and(|v| v != "sng") {
        return None;
    }
    let inset = match get_attribute_ref(xml, "algn").unwrap_or("ctr") {
        "ctr" => false,
        "in" => true,
        _ => return None,
    };
    let default_fill = "<a:solidFill><a:srgbClr val=\"000000\"/></a:solidFill>";
    let color = paint(
        Some(fill_node(xml).unwrap_or(default_fill)),
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
    })
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
    if ["scene3d", "sp3d", "effectDag"]
        .into_iter()
        .any(|name| child(properties_xml, name).is_some())
    {
        return None;
    }
    let paths = geometry_paths(properties_xml, width, height)?;
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
    let effect_reference = if child(properties_xml, "effectLst").is_none() {
        referenced_style(style_xml, StyleReferenceKind::Effect, context)?
    } else {
        None
    };
    let effects = child(properties_xml, "effectLst")
        .or(effect_reference.as_deref())
        .unwrap_or("");
    if local_name(effects) == Some("effectDag") {
        return None;
    }
    let effective_properties = format!("<a:spPr>{effects}</a:spPr>");
    let mut defs = String::new();
    let stroke = stroke(line, width, height, context, &mut defs)?;
    let mut body = String::new();
    let mut silhouette = String::new();
    let mut outside = 0.0f64;
    let mut bitmap_ready = false;
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
            let fill = paint(
                fill,
                &format!("shape-fill-{index}"),
                width,
                height,
                path.fill,
                context,
                &mut defs,
            )?;
            if fill.value != "none" {
                let _ = write!(
                    body,
                    r##"<path d="{}" fill="{}" fill-opacity="{}" stroke="none"/>"##,
                    path.d, fill.value, fill.opacity
                );
            }
        }
        if path.stroke && stroke.width > 0.0 {
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
    let transform = child(properties_xml, "xfrm").unwrap_or("");
    let rotation = (number(transform, "rot", 0.0)? / 60000.0).rem_euclid(360.0);
    if (rotation != 0.0 || enabled(transform, "flipH") || enabled(transform, "flipV"))
        && fill.is_some_and(|xml| {
            get_attribute_ref(xml, "rotWithShape").is_some_and(|v| matches!(v, "0" | "false"))
        })
    {
        return None;
    }
    let (sin, cos) = rotation.to_radians().sin_cos();
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

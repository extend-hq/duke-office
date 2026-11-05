use std::fmt::Write;

use crate::model::{TextOutline, TextShadow, TextStyle};
use crate::xml::{extract_balanced_tag_blocks, get_attribute};

use super::context::ThemeColorMap;

fn effect(xml: &str, name: &str) -> Option<String> {
    extract_balanced_tag_blocks(xml, name)
        .into_iter()
        .next()
        .filter(|value| value.len() <= 16 * 1024)
}

fn number(xml: &str, name: &str, default: f64) -> Option<f64> {
    get_attribute(xml, &format!("w14:{name}"))
        .map(|value| value.parse::<f64>().ok().filter(|value| value.is_finite()))
        .unwrap_or(Some(default))
}

fn color(xml: &str, theme: &ThemeColorMap) -> Option<(String, f64)> {
    let normalized = xml.replace("w14:", "a:");
    let normalized = super::re::get(r"(\s)a:")?.replace_all(&normalized, "$1");
    super::colors::resolve_drawing_color_from_xml(Some(&normalized), theme)
        .map(|color| (color.color, color.opacity.unwrap_or(1.0).clamp(0.0, 1.0)))
}

fn shadow(xml: &str, theme: &ThemeColorMap) -> Option<TextShadow> {
    let source = effect(xml, "w14:shadow")?;
    if number(&source, "sx", 100000.0)? != 100000.0
        || number(&source, "sy", 100000.0)? != 100000.0
        || number(&source, "kx", 0.0)? != 0.0
        || number(&source, "ky", 0.0)? != 0.0
    {
        return None;
    }
    let radius = number(&source, "blurRad", 0.0)? / 9525.0;
    let distance = number(&source, "dist", 0.0)? / 9525.0;
    if !(0.0..=1000.0).contains(&radius) || !(0.0..=1000.0).contains(&distance) {
        return None;
    }
    let direction = (number(&source, "dir", 0.0)? / 60000.0)
        .rem_euclid(360.0)
        .to_radians();
    let (color, opacity) = color(&source, theme)?;
    Some(TextShadow {
        source_color: Some(color.clone()),
        color,
        opacity,
        offset_x_px: direction.cos() * distance,
        offset_y_px: direction.sin() * distance,
        blur_radius_px: radius,
        source_xml: Some(source),
    })
}

fn outline(xml: &str, theme: &ThemeColorMap) -> Option<TextOutline> {
    let source = effect(xml, "w14:textOutline")?;
    if get_attribute(&source, "w14:cmpd").is_some_and(|value| value != "sng")
        || get_attribute(&source, "w14:algn").is_some_and(|value| value != "ctr")
        || source.contains("<w14:custDash")
        || source.contains("<w14:gradFill")
        || source.contains("<w14:pattFill")
    {
        return None;
    }
    if super::scan::find_tag_token_ref(&source, "w14:prstDash")
        .and_then(|xml| get_attribute(xml, "w14:val"))
        .is_some_and(|value| value != "solid")
    {
        return None;
    }
    let width = number(&source, "w", 12700.0)? / 9525.0;
    if !(0.0..=100.0).contains(&width) {
        return None;
    }
    let (color, opacity, width) = if source.contains("<w14:noFill") {
        ("#000000".into(), 0.0, 0.0)
    } else {
        let (color, opacity) = color(&source, theme)?;
        (color, opacity, width)
    };
    let line_join = if source.contains("<w14:miter") {
        "miter"
    } else if source.contains("<w14:bevel") {
        "bevel"
    } else {
        "round"
    };
    Some(TextOutline {
        source_color: Some(color.clone()),
        color,
        opacity,
        width_px: width,
        line_join: line_join.into(),
        source_xml: Some(source),
    })
}

pub(super) fn parse_into(style: &mut TextStyle, xml: &str) -> bool {
    let theme = ThemeColorMap::new();
    style.text_shadow = shadow(xml, &theme);
    style.text_outline = outline(xml, &theme);
    style.text_shadow.is_some() || style.text_outline.is_some()
}

pub(crate) fn supports(xml: &str, name: &str) -> bool {
    match name {
        "w14:shadow" => shadow(xml, &ThemeColorMap::new()).is_some(),
        "w14:textOutline" => outline(xml, &ThemeColorMap::new()).is_some(),
        _ => false,
    }
}

pub(super) fn resolve(style: &mut TextStyle, theme: &ThemeColorMap) {
    if let Some(shadow) = &mut style.text_shadow {
        if let Some((color, opacity)) = shadow
            .source_xml
            .as_deref()
            .and_then(|xml| color(xml, theme))
        {
            shadow.source_color = Some(color.clone());
            shadow.color = color;
            shadow.opacity = opacity;
        }
    }
    if let Some(outline) = &mut style.text_outline {
        if let Some((color, opacity)) = outline
            .source_xml
            .as_deref()
            .and_then(|xml| color(xml, theme))
        {
            outline.source_color = Some(color.clone());
            outline.color = color;
            outline.opacity = opacity;
        }
    }
}

pub(super) fn svg_attributes(
    style: Option<&TextStyle>,
    id: &str,
    width: f64,
    height: f64,
    defs: &mut String,
) -> String {
    let Some(style) = style else {
        return String::new();
    };
    let mut attributes = String::new();
    if let Some(outline) = &style.text_outline {
        let _ = write!(
            attributes,
            r##" stroke="{}" stroke-width="{}" stroke-opacity="{}" stroke-linejoin="{}" paint-order="fill stroke""##,
            super::util::escape_xml_text(&outline.color),
            outline.width_px,
            outline.opacity,
            super::util::escape_xml_text(&outline.line_join)
        );
    }
    if let Some(shadow) = &style.text_shadow {
        let margin =
            shadow.offset_x_px.abs().max(shadow.offset_y_px.abs()) + shadow.blur_radius_px + 1.0;
        let _ = write!(
            defs,
            r##"<filter id="{id}" filterUnits="userSpaceOnUse" x="{}" y="{}" width="{}" height="{}" color-interpolation-filters="sRGB"><feGaussianBlur in="SourceAlpha" stdDeviation="{}" result="blur"/><feOffset in="blur" dx="{}" dy="{}" result="offset"/><feFlood flood-color="{}" flood-opacity="{}"/><feComposite in2="offset" operator="in"/><feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge></filter>"##,
            -margin,
            -margin,
            width + margin * 2.0,
            height + margin * 2.0,
            shadow.blur_radius_px / 3.0,
            shadow.offset_x_px,
            shadow.offset_y_px,
            super::util::escape_xml_text(&shadow.color),
            shadow.opacity
        );
        let _ = write!(attributes, r##" filter="url(#{id})""##);
    }
    attributes
}

fn color_xml(color: &str, opacity: f64) -> String {
    let color = color.strip_prefix('#').unwrap_or(color);
    if color.len() != 6
        || !color.bytes().all(|value| value.is_ascii_hexdigit())
        || !opacity.is_finite()
    {
        return String::new();
    }
    format!(
        r#"<w14:srgbClr w14:val="{color}"><w14:alpha w14:val="{}"/></w14:srgbClr>"#,
        (opacity.clamp(0.0, 1.0) * 100000.0).round()
    )
}

pub(crate) fn serialize(style: &TextStyle) -> Vec<String> {
    let mut output = Vec::new();
    if let Some(value) = &style.text_shadow {
        if let Some(source) = &value.source_xml {
            if let Some(mut original) = shadow(source, &ThemeColorMap::new()) {
                original.color = value.source_color.clone().unwrap_or(original.color);
                if original.color == value.color
                    && original.opacity == value.opacity
                    && original.offset_x_px == value.offset_x_px
                    && original.offset_y_px == value.offset_y_px
                    && original.blur_radius_px == value.blur_radius_px
                {
                    output.push(source.clone());
                }
            }
        }
        if output.is_empty()
            && [value.offset_x_px, value.offset_y_px, value.blur_radius_px]
                .iter()
                .all(|v| v.is_finite())
        {
            let color = color_xml(&value.color, value.opacity);
            if !color.is_empty() {
                let distance = value.offset_x_px.hypot(value.offset_y_px) * 9525.0;
                let direction = value
                    .offset_y_px
                    .atan2(value.offset_x_px)
                    .to_degrees()
                    .rem_euclid(360.0)
                    * 60000.0;
                output.push(format!(r#"<w14:shadow w14:blurRad="{}" w14:dist="{}" w14:dir="{}">{color}</w14:shadow>"#, (value.blur_radius_px.max(0.0) * 9525.0).round(), distance.round(), direction.round()));
            }
        }
    }
    if let Some(value) = &style.text_outline {
        let before = output.len();
        if let Some(source) = &value.source_xml {
            if let Some(mut original) = outline(source, &ThemeColorMap::new()) {
                original.color = value.source_color.clone().unwrap_or(original.color);
                if original.color == value.color
                    && original.opacity == value.opacity
                    && original.width_px == value.width_px
                    && original.line_join == value.line_join
                {
                    output.push(source.clone());
                }
            }
        }
        if before == output.len() && value.width_px.is_finite() {
            let color = color_xml(&value.color, value.opacity);
            if !color.is_empty() {
                let join = match value.line_join.as_str() {
                    "bevel" => "<w14:bevel/>",
                    "miter" => "<w14:miter w14:lim=\"800000\"/>",
                    _ => "<w14:round/>",
                };
                output.push(format!(r#"<w14:textOutline w14:w="{}" w14:cmpd="sng" w14:algn="ctr"><w14:solidFill>{color}</w14:solidFill>{join}</w14:textOutline>"#, (value.width_px.max(0.0) * 9525.0).round()));
            }
        }
    }
    output
}

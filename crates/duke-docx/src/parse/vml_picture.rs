use std::borrow::Borrow;

use crate::xml::{
    decode_xml_entities, direct_child_tag, extract_balanced_tag_blocks_in_order,
    extract_balanced_tag_slices, get_attribute_ref, tag_token_end,
};

use super::context::{ParseContext, ParsedStyleSheet};
use super::picture::{render_picture, PicturePaint};

fn fraction(raw: &str) -> Option<f64> {
    let raw = raw.trim();
    let (value, divisor) = if let Some(value) = raw.strip_suffix(['f', 'F']) {
        (value, 65_536.0)
    } else if let Some(value) = raw.strip_suffix('%') {
        (value, 100.0)
    } else {
        (raw, 1.0)
    };
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .map(|value| value / divisor)
}

fn attribute_fraction(xml: &str, name: &str, default: f64) -> Option<f64> {
    get_attribute_ref(xml, name)
        .map(fraction)
        .unwrap_or(Some(default))
}

fn enabled(xml: &str, name: &str) -> Option<bool> {
    match get_attribute_ref(xml, name)
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        None | Some("0" | "f" | "false" | "off") => Some(false),
        Some("1" | "t" | "true" | "on") => Some(true),
        _ => None,
    }
}

fn style_value<'a>(style: &'a str, name: &str) -> Option<&'a str> {
    style
        .split(';')
        .filter_map(|declaration| {
            let (key, value) = declaration.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case(name)
                .then_some(value.trim())
        })
        .next_back()
}

fn rotation(raw: &str) -> Option<f64> {
    let raw = raw.trim().to_ascii_lowercase();
    let (raw, divisor) = if let Some(value) = raw.strip_suffix("fd") {
        (value, 65_536.0)
    } else {
        (raw.strip_suffix("deg").unwrap_or(&raw), 1.0)
    };
    raw.trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .map(|value| (value / divisor).rem_euclid(360.0))
}

pub(super) fn render_vml_picture(
    xml: &str,
    source: &str,
    width: f64,
    height: f64,
    context: &ParseContext<'_, impl Borrow<ParsedStyleSheet>>,
) -> Option<PicturePaint> {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return None;
    }
    if !extract_balanced_tag_slices(xml, "v:group").is_empty()
        || !extract_balanced_tag_slices(xml, "w:txbxContent").is_empty()
    {
        return None;
    }
    let candidates = extract_balanced_tag_blocks_in_order(xml, &["v:shape", "v:rect", "v:image"]);
    let [candidate] = candidates.as_slice() else {
        return None;
    };
    let shape = &xml[candidate.start..candidate.end];
    let opening = &shape[..tag_token_end(shape, 0)?];
    let image = if candidate.tag_name == "v:image" {
        opening
    } else {
        direct_child_tag(shape, "v:imagedata")?
    };
    let image = &image[..tag_token_end(image, 0)?];
    let style = decode_xml_entities(get_attribute_ref(opening, "style").unwrap_or(""));
    let mut angle = style_value(&style, "rotation")
        .map(rotation)
        .unwrap_or(Some(0.0))?;
    let flip = style_value(&style, "flip")
        .unwrap_or("")
        .to_ascii_lowercase();
    let flip = flip.split_ascii_whitespace().collect::<String>();
    let (flip_horizontal, flip_vertical) = match flip.as_str() {
        "" | "none" => (false, false),
        "x" => (true, false),
        "y" => (false, true),
        "xy" | "yx" => (true, true),
        _ => return None,
    };
    if flip_horizontal != flip_vertical {
        angle = -angle;
    }
    let crop = ["cropleft", "croptop", "cropright", "cropbottom"]
        .map(|name| attribute_fraction(image, name, 0.0));
    let [Some(left), Some(top), Some(right), Some(bottom)] = crop else {
        return None;
    };
    let opacity = get_attribute_ref(opening, "opacity")
        .or_else(|| style_value(&style, "opacity"))
        .map(fraction)
        .unwrap_or(Some(1.0))?
        .clamp(0.0, 1.0);
    let grayscale = enabled(image, "grayscale")?;
    let bilevel = enabled(image, "bilevel")?;
    let unrendered_adjustments = [("gain", 1.0), ("blacklevel", 0.0), ("gamma", 1.0)]
        .into_iter()
        .filter_map(|(name, default)| {
            (attribute_fraction(image, name, default) != Some(default)).then_some(name)
        })
        .collect::<Vec<_>>();
    if !unrendered_adjustments.is_empty() {
        context.push_warning(format!(
            "VML picture adjustments retained without rendering: {}",
            unrendered_adjustments.join(", ")
        ));
    }
    if get_attribute_ref(image, "chromakey")
        .is_some_and(|value| !value.trim().is_empty() && !value.trim().eq_ignore_ascii_case("none"))
    {
        context
            .push_warning("VML picture transparency color retained without rendering".to_owned());
    }
    let color_effect = if bilevel {
        r#"<a:biLevel thresh="50000"/>"#
    } else if grayscale {
        "<a:grayscl/>"
    } else {
        ""
    };
    let alpha = if opacity == 1.0 {
        String::new()
    } else {
        format!(r#"<a:alphaModFix amt="{}%"/>"#, opacity * 100.0)
    };
    let drawing = format!(
        r#"<pic:pic><pic:blipFill><a:blip>{color_effect}{alpha}</a:blip><a:srcRect l="{}%" t="{}%" r="{}%" b="{}%"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm rot="{}" flipH="{}" flipV="{}"/><a:prstGeom prst="rect"/><a:ln><a:noFill/></a:ln></pic:spPr></pic:pic>"#,
        left * 100.0,
        top * 100.0,
        right * 100.0,
        bottom * 100.0,
        angle * 60_000.0,
        u8::from(flip_horizontal),
        u8::from(flip_vertical),
    );
    if angle != 0.0
        || flip_horizontal
        || flip_vertical
        || [left, top, right, bottom].iter().any(|value| *value != 0.0)
        || opacity != 1.0
        || grayscale
        || bilevel
    {
        render_picture(&drawing, source, width, height, context)
    } else {
        None
    }
}

use super::{color, drawing, drawing_child, ThemeColorMap};
use crate::parse::art;
use roxmltree::{Node, NodeId};
use std::collections::HashMap;
use std::fmt::Write;

struct Bitmap {
    xml: String,
    source: String,
}

#[derive(Default)]
pub(super) struct PaintServers {
    ids: HashMap<NodeId, usize>,
    markup: String,
    bitmaps: HashMap<usize, Bitmap>,
}

impl PaintServers {
    pub(super) fn new(
        root: Node,
        theme: &ThemeColorMap,
        spreadsheet: bool,
        images: Option<&HashMap<String, String>>,
    ) -> Self {
        let mut paints = Self::default();
        for node in root.descendants().filter(|n| drawing(*n)) {
            let id = paints.ids.len();
            if !matches!(node.tag_name().name(), "gradFill" | "pattFill" | "blipFill") {
                continue;
            }
            let markup = if node.tag_name().name() == "blipFill" {
                let blip = drawing_child(node, "blip");
                let source = blip.and_then(|n| n.attributes().find(|a| matches!(a.name(), "embed" | "link") && matches!(a.namespace(), Some(art::REL | "http://purl.oclc.org/ooxml/officeDocument/relationships"))))
                    .and_then(|a| images?.get(a.value())).and_then(|s| art::safe_image(s));
                let advanced = drawing_child(node, "tile").is_some()
                    || drawing_child(node, "stretch").is_none()
                    || blip.is_some_and(|n| {
                        n.children().any(|n| {
                            drawing(n) && !matches!(n.tag_name().name(), "alphaModFix" | "extLst")
                        })
                    })
                    || source.as_ref().is_some_and(|s| {
                        blip.and_then(|n| n.attribute((art::REL, "embed")))
                            .and_then(|id| images?.get(id))
                            != Some(s)
                    });
                if advanced {
                    if let Some(source) = source {
                        paints.bitmaps.insert(
                            id,
                            Bitmap {
                                xml: art::canonical(node),
                                source,
                            },
                        );
                        Some(format!(
                            r#"<pattern id="cp{id}" width="1" height="1" patternContentUnits="objectBoundingBox"></pattern>"#
                        ))
                    } else {
                        None
                    }
                } else {
                    image_fill(node, &id.to_string(), images)
                }
            } else {
                definition_for_mode(node, theme, &id.to_string(), spreadsheet)
            };
            if let Some(markup) = markup {
                paints.ids.insert(node.id(), id);
                paints.markup.push_str(&markup);
            }
        }
        paints
    }

    pub(super) fn fill(&self, properties: Node, theme: &ThemeColorMap) -> Option<String> {
        properties
            .children()
            .filter(|n| drawing(*n))
            .find_map(|n| match n.tag_name().name() {
                "noFill" => Some("none".to_owned()),
                "solidFill" => n.children().find_map(|c| color(c, theme)),
                "gradFill" | "pattFill" | "blipFill" => {
                    self.ids.get(&n.id()).map(|id| format!("url(#cp{id})"))
                }
                _ => None,
            })
    }

    pub(super) fn defs(&self) -> String {
        if self.markup.is_empty() {
            String::new()
        } else {
            format!("<defs>{}</defs>", self.markup)
        }
    }

    pub(super) fn apply_images(&self, mut svg: String, theme: &ThemeColorMap) -> String {
        if self.bitmaps.is_empty() {
            return svg;
        }
        let Ok(document) = roxmltree::Document::parse(&svg) else {
            return svg;
        };
        let root = document.root_element();
        let viewport = ["width", "height"].map(|name| {
            root.attribute(name)
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.0)
        });
        let context = art::Context {
            theme,
            images: None,
            theme_xml: None,
            theme_override: None,
        };
        let mut edits = Vec::new();
        let mut definitions = String::new();
        let mut cache = HashMap::<(usize, u64, u64), Option<String>>::new();
        for node in root.descendants().filter(Node::is_element) {
            let Some([_, _, width, height]) = art::geometry_bounds(node, viewport) else {
                continue;
            };
            for name in ["fill", "stroke"] {
                let Some(value) = node.ancestors().find_map(|n| n.attribute(name)) else {
                    continue;
                };
                let Some(id) = value
                    .strip_prefix("url(#cp")
                    .and_then(|v| v.strip_suffix(')'))
                    .and_then(|s| s.parse::<usize>().ok())
                else {
                    continue;
                };
                let Some(bitmap) = self.bitmaps.get(&id) else {
                    continue;
                };
                let key = (id, width.to_bits(), height.to_bits());
                let next = cache.len();
                let replacement = cache.entry(key).or_insert_with(|| {
                    let doc = roxmltree::Document::parse(&bitmap.xml).ok()?;
                    let rendered = art::picture(doc.root_element(), None, &bitmap.source, width, height, &context)?;
                    let prefix = format!("caf{next}-");
                    let scoped = art::scoped_svg(&rendered.svg, &prefix)?;
                    let inner = art::inner_svg(&scoped)?;
                    write!(definitions, r#"<pattern id="{prefix}fill" width="1" height="1" patternContentUnits="objectBoundingBox"><svg width="1" height="1" viewBox="0 0 {width} {height}" preserveAspectRatio="none">{inner}</svg></pattern>"#).unwrap();
                    Some(format!("url(#{prefix}fill)"))
                });
                let Some(value) = replacement else {
                    continue;
                };
                if let Some(attr) = node.attributes().find(|a| a.name() == name) {
                    edits.push((attr.range_value(), value.clone()));
                } else {
                    let start = node.range().start + 1 + node.tag_name().name().len();
                    edits.push((start..start, format!(r#" {name}="{value}""#)));
                }
            }
        }
        if cache.values().any(Option::is_none) {
            definitions.push_str("<desc>Some picture fills could not be rendered.</desc>");
        }
        edits.sort_by_key(|(r, _)| r.start);
        for (range, replacement) in edits.into_iter().rev() {
            svg.replace_range(range, &replacement);
        }
        if !definitions.is_empty() {
            if let Some(start) = svg.find('>') {
                svg.insert_str(start + 1, &format!("<defs>{definitions}</defs>"));
            }
        }
        svg
    }
}

fn raster_data_url(value: &str) -> bool {
    let Some((header, data)) = value.split_once(',') else {
        return false;
    };
    if !matches!(
        header,
        "data:image/png;base64"
            | "data:image/jpeg;base64"
            | "data:image/gif;base64"
            | "data:image/webp;base64"
    ) || data.is_empty()
        || data.len() % 4 != 0
    {
        return false;
    }
    let content = data.trim_end_matches('=');
    data.len() - content.len() <= 2
        && content
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/'))
}

fn image_fill(node: Node, id: &str, images: Option<&HashMap<String, String>>) -> Option<String> {
    let blip = drawing_child(node, "blip")?;
    let relationship = blip
        .attributes()
        .find(|attribute| {
            attribute.name() == "embed"
                && matches!(
                    attribute.namespace(),
                    Some(
                        "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
                            | "http://purl.oclc.org/ooxml/officeDocument/relationships"
                    )
                )
        })?
        .value();
    let image = images?
        .get(relationship)
        .filter(|image| raster_data_url(image))?;
    let stretch = drawing_child(node, "stretch")?;
    let offsets = |rectangle: Option<Node>| {
        ["l", "t", "r", "b"].map(|name| {
            rectangle
                .and_then(|n| n.attribute(name))
                .and_then(fraction)
                .unwrap_or(0.)
        })
    };
    let [left, top, right, bottom] = offsets(drawing_child(node, "srcRect"));
    let [fill_left, fill_top, fill_right, fill_bottom] =
        offsets(drawing_child(stretch, "fillRect"));
    let source_width = 1. - left - right;
    let source_height = 1. - top - bottom;
    let fill_width = 1. - fill_left - fill_right;
    let fill_height = 1. - fill_top - fill_bottom;
    if source_width <= 0. || source_height <= 0. || fill_width <= 0. || fill_height <= 0. {
        return None;
    }
    let width = fill_width / source_width;
    let height = fill_height / source_height;
    let x = fill_left - left * width;
    let y = fill_top - top * height;
    if ![x, y, width, height].into_iter().all(f64::is_finite) {
        return None;
    }
    let alpha = drawing_child(blip, "alphaModFix")
        .and_then(|n| n.attribute("amt"))
        .and_then(fraction)
        .unwrap_or(1.)
        .clamp(0., 1.);
    Some(format!(
        r##"<clipPath id="ci{id}"><rect x="{fill_left}" y="{fill_top}" width="{fill_width}" height="{fill_height}"/></clipPath><pattern id="cp{id}" width="1" height="1" patternContentUnits="objectBoundingBox"><g clip-path="url(#ci{id})"><image x="{x}" y="{y}" width="{width}" height="{height}" href="{image}" preserveAspectRatio="none" opacity="{alpha}"/></g></pattern>"##
    ))
}

fn fraction(raw: &str) -> Option<f64> {
    let value = if let Some(v) = raw.strip_suffix('%') {
        v.parse::<f64>().ok()? / 100.0
    } else {
        raw.parse::<f64>().ok()? / 100_000.0
    };
    value.is_finite().then_some(value)
}

fn stops(node: Node, theme: &ThemeColorMap) -> Vec<(f64, String)> {
    let mut values = Vec::new();
    if let Some(list) = drawing_child(node, "gsLst") {
        for stop in list
            .children()
            .filter(|n| drawing(*n) && n.tag_name().name() == "gs")
        {
            if let (Some(value), Some(offset)) = (
                stop.children().find_map(|n| color(n, theme)),
                stop.attribute("pos").and_then(fraction),
            ) {
                values.push((offset.clamp(0., 1.), value));
            }
        }
    }
    if values.is_empty() {
        values.extend([(0., "#000000".into()), (1., "#ffffff".into())]);
    }
    values
}

pub(in crate::parse) fn rgba(hex: &str) -> Option<[u8; 4]> {
    if !hex.starts_with('#') || !matches!(hex.len(), 7 | 9) {
        return None;
    }
    let mut components = [255; 4];
    for (i, pair) in hex[1..].as_bytes().chunks_exact(2).enumerate() {
        components[i] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(components)
}

fn stop_markup(stops: &[(f64, String)]) -> String {
    let mut result = String::new();
    let shaped_ramp = stops.first().is_some_and(|s| s.0 == 0.)
        && stops.last().is_some_and(|s| s.0 == 1.)
        && (stops.len() == 2 || stops.len() == 3 && stops[0].1.eq_ignore_ascii_case(&stops[2].1));
    for (i, (offset, value)) in stops.iter().enumerate() {
        write!(result, r#"<stop offset="{offset}" stop-color="{value}"/>"#).unwrap();
        if !shaped_ramp {
            continue;
        }
        let Some((end_offset, end_value)) = stops.get(i + 1) else {
            continue;
        };
        let (Some(start), Some(end)) = (rgba(value), rgba(end_value)) else {
            continue;
        };
        // The two-color ramp weights the brighter end of each RGB channel.
        for step in 1..16 {
            let t = f64::from(step) / 16.;
            let [r, g, b, a] = std::array::from_fn(|c| {
                let ratio = if c == 3 {
                    t
                } else if start[c] < end[c] {
                    1. - (1. - t).powf(1.875)
                } else {
                    t.powf(1.875)
                };
                (f64::from(start[c]) + (f64::from(end[c]) - f64::from(start[c])) * ratio).round()
                    as u8
            });
            let x = offset + (end_offset - offset) * t;
            write!(
                result,
                r##"<stop offset="{x}" stop-color="#{r:02x}{g:02x}{b:02x}{a:02x}"/>"##
            )
            .unwrap();
        }
    }
    result
}

fn radial_stop_markup(stops: &[(f64, String)]) -> Option<String> {
    let colors: Vec<_> = stops
        .iter()
        .map(|(_, value)| rgba(value))
        .collect::<Option<_>>()?;
    let shaped = stops.first().is_some_and(|s| s.0 == 0.)
        && stops.last().is_some_and(|s| s.0 == 1.)
        && (stops.len() == 2 || stops.len() == 3 && stops[0].1.eq_ignore_ascii_case(&stops[2].1));
    let mut result = String::new();
    for (i, pair) in stops.windows(2).enumerate() {
        let (start, end) = (colors[i], colors[i + 1]);
        let (offset, end_offset) = (pair[0].0, pair[1].0);
        if end_offset <= offset {
            return None;
        }
        let at = |t: f64| -> [u8; 4] {
            std::array::from_fn(|c| {
                let ratio = if !shaped || c == 3 {
                    t
                } else if start[c] < end[c] {
                    1. - (1. - t).powf(1.875)
                } else {
                    t.powf(1.875)
                };
                let value = f64::from(start[c]) + (f64::from(end[c]) - f64::from(start[c])) * ratio;
                if c == 3 {
                    return value.round() as u8;
                }
                // Preserve the radial ramp's linear-light quantization.
                let linear = ((value / 255.).powf(2.2) * 255.).round() / 255.;
                let srgb = if linear <= 0.0031308 {
                    linear * 12.92
                } else {
                    1.055 * linear.powf(1. / 2.4) - 0.055
                };
                (srgb * 255.).round().clamp(0., 255.) as u8
            })
        };
        let mut transitions = Vec::new();
        for c in 0..3 {
            if start[c] == end[c] {
                continue;
            }
            for bucket in 0..255 {
                let threshold = ((f64::from(bucket) + 0.5) / 255.).powf(1. / 2.2) * 255.;
                let ratio =
                    (threshold - f64::from(start[c])) / (f64::from(end[c]) - f64::from(start[c]));
                if !(0. < ratio && ratio < 1.) {
                    continue;
                }
                let t = if !shaped {
                    ratio
                } else if start[c] < end[c] {
                    1. - (1. - ratio).powf(1. / 1.875)
                } else {
                    ratio.powf(1. / 1.875)
                };
                transitions.push(t);
            }
        }
        transitions.sort_by(f64::total_cmp);
        transitions.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
        let mut emit = |t: f64, color: [u8; 4]| {
            let x = offset + (end_offset - offset) * t;
            let [r, g, b, a] = color;
            write!(
                result,
                r##"<stop offset="{x}" stop-color="#{r:02x}{g:02x}{b:02x}{a:02x}"/>"##
            )
            .unwrap();
        };
        emit(0., at(0.));
        for t in transitions {
            emit(t, at((t - 1e-10).max(0.)));
            emit(t, at((t + 1e-10).min(1.)));
        }
        emit(1., at(1.));
    }
    (!result.is_empty()).then_some(result)
}

pub(super) fn definition(node: Node, theme: &ThemeColorMap, id: &str) -> Option<String> {
    definition_for_mode(node, theme, id, false)
}

fn definition_for_mode(
    node: Node,
    theme: &ThemeColorMap,
    id: &str,
    spreadsheet: bool,
) -> Option<String> {
    match node.tag_name().name() {
        "gradFill" => gradient(node, theme, id, spreadsheet),
        "pattFill" => pattern(node, theme, id),
        _ => None,
    }
}

fn gradient(node: Node, theme: &ThemeColorMap, id: &str, spreadsheet: bool) -> Option<String> {
    let values = stops(node, theme);
    let path = drawing_child(node, "path");
    let stops = if spreadsheet && path.is_some_and(|p| p.attribute("path") == Some("circle")) {
        radial_stop_markup(&values).unwrap_or_else(|| stop_markup(&values))
    } else {
        stop_markup(&values)
    };
    if let Some(path) = path {
        let inset = drawing_child(path, "fillToRect");
        let inset_at = |attr| {
            inset
                .and_then(|n| n.attribute(attr).and_then(fraction))
                .unwrap_or(if inset.is_some() { 0. } else { 0.5 })
        };
        let (left, top, right, bottom) = (
            inset_at("l"),
            inset_at("t"),
            1. - inset_at("r"),
            1. - inset_at("b"),
        );
        if path.attribute("path") == Some("circle") {
            let cx = (left + right) / 2.;
            let cy = (top + bottom) / 2.;
            let radius = 0.5_f64.hypot(0.5);
            let inner_radius = radius * (right - left).min(bottom - top).clamp(0., 1.);
            if node.parent_element().and_then(|p| p.parent_element())
                == Some(node.document().root_element())
            {
                let fx = cx * 100.;
                let fy = cy * 100.;
                let outer = radius * 100.;
                let inner = inner_radius * 100.;
                return Some(format!(
                    r#"<radialGradient id="cp{id}" gradientUnits="userSpaceOnUse" cx="50%" cy="50%" r="{outer}%" fx="{fx}%" fy="{fy}%" fr="{inner}%">{stops}</radialGradient>"#
                ));
            }
            return Some(format!(
                r#"<radialGradient id="cp{id}" cx="0.5" cy="0.5" r="{radius}" fx="{cx}" fy="{cy}" fr="{inner_radius}">{stops}</radialGradient>"#
            ));
        }

        // Each side interpolates between the corresponding edges of the focus and outer rectangles.
        let mut markup = format!(r#"<linearGradient id="cs{id}">{stops}</linearGradient>"#);
        let sides = [
            (
                0.,
                top,
                0.,
                0.,
                format!("0,0 1,0 {right},{top} {left},{top}"),
            ),
            (
                right,
                0.,
                1.,
                0.,
                format!("1,0 1,1 {right},{bottom} {right},{top}"),
            ),
            (
                0.,
                bottom,
                0.,
                1.,
                format!("1,1 0,1 {left},{bottom} {right},{bottom}"),
            ),
            (
                left,
                0.,
                0.,
                0.,
                format!("0,1 0,0 {left},{top} {left},{bottom}"),
            ),
        ];
        let mut sides_svg = String::new();
        for (i, (x1, y1, x2, y2, points)) in sides.into_iter().enumerate() {
            write!(markup, r##"<linearGradient id="cs{id}-{i}" href="#cs{id}" gradientUnits="userSpaceOnUse" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}"/>"##).unwrap();
            write!(
                sides_svg,
                r##"<polygon shape-rendering="crispEdges" points="{points}" fill="url(#cs{id}-{i})"/>"##
            )
            .unwrap();
        }
        let first = &values[0].1;
        let focus_w = (right - left).max(0.);
        let focus_h = (bottom - top).max(0.);
        write!(markup, r#"<pattern id="cp{id}" width="1" height="1" patternContentUnits="objectBoundingBox"><rect x="{left}" y="{top}" width="{focus_w}" height="{focus_h}" fill="{first}"/>{sides_svg}</pattern>"#).unwrap();
        return Some(markup);
    }
    let degrees = drawing_child(node, "lin")
        .and_then(|n| n.attribute("ang")?.parse::<i32>().ok())
        .map_or(0., |v| f64::from(v) / 60_000.);
    let (dy, dx) = degrees.to_radians().sin_cos();
    let span = (dx.abs() + dy.abs()) * 0.5;
    let (x1, y1, x2, y2) = (
        0.5 - dx * span,
        0.5 - dy * span,
        0.5 + dx * span,
        0.5 + dy * span,
    );
    Some(format!(
        r#"<linearGradient id="cp{id}" x1="{x1:.6}" y1="{y1:.6}" x2="{x2:.6}" y2="{y2:.6}">{stops}</linearGradient>"#
    ))
}

fn pattern(node: Node, theme: &ThemeColorMap, id: &str) -> Option<String> {
    let preset = node.attribute("prst").unwrap_or("pct5");
    let bits = pattern_bits(preset)?;
    let paint = |name, default: &str| {
        drawing_child(node, name)
            .and_then(|n| n.children().find_map(|c| color(c, theme)))
            .unwrap_or_else(|| default.to_owned())
    };
    let foreground = paint("fgClr", "black");
    let background = paint("bgClr", "white");
    let path = mask_path(bits);
    let fringe = match preset {
        "dnDiag" => Some(0x41a0_5028_140a_0582),
        "upDiag" => Some(0x8205_0a14_2850_a041),
        "diagCross" => Some(0x42a5_5a24_245a_a542),
        _ => None,
    }
    .map(|mask| {
        format!(
            r#"<path fill="{foreground}" fill-opacity="0.5" d="{}"/>"#,
            mask_path(mask)
        )
    })
    .unwrap_or_default();
    // The repeat is eight points wide and tall. The mask uses its eight logical cells.
    Some(format!(
        r#"<pattern id="cp{id}" patternUnits="userSpaceOnUse" width="10.666667" height="10.666667" viewBox="0 0 8 8"><rect width="8" height="8" fill="{background}"/><path fill="{foreground}" d="{path}"/>{fringe}</pattern>"#
    ))
}

fn mask_path(bits: u64) -> String {
    let mut path = String::new();
    for (y, row) in bits.to_be_bytes().into_iter().enumerate() {
        let mut x = 0;
        while x < 8 {
            if row & (0x80 >> x) == 0 {
                x += 1;
                continue;
            }
            let start = x;
            while x < 8 && row & (0x80 >> x) != 0 {
                x += 1;
            }
            let length = x - start;
            write!(path, "M{start} {y}h{length}v1h-{length}z").unwrap();
        }
    }
    path
}

fn pattern_bits(preset: &str) -> Option<u64> {
    Some(match preset {
        "pct5" => 0x8000_0000_0800_0000,
        "pct10" => 0x8000_0800_8000_0800,
        "pct20" => 0x8800_2200_8800_2200,
        "pct25" => 0x8822_8822_8822_8822,
        "pct30" => 0xaa44_aa11_aa44_aa11,
        "pct40" => 0xaa55_aa51_aa55_aa15,
        "pct50" => 0xaa55_aa55_aa55_aa55,
        "pct60" => 0xee55_bb55_ee55_bb55,
        "pct70" => 0x77dd_77dd_77dd_77dd,
        "pct75" => 0x77ff_ddff_77ff_ddff,
        "pct80" => 0xefff_feff_efff_feff,
        "pct90" => 0xffff_fff7_ffff_ff7f,
        "horz" => 0xff00_0000_0000_0000,
        "vert" => 0x8080_8080_8080_8080,
        "ltHorz" => 0xff00_0000_ff00_0000,
        "ltVert" => 0x8888_8888_8888_8888,
        "dkHorz" => 0xffff_0000_ffff_0000,
        "dkVert" => 0xcccc_cccc_cccc_cccc,
        "narHorz" => 0xff00_ff00_ff00_ff00,
        "narVert" => 0x5555_5555_5555_5555,
        "dashHorz" => 0xf000_0000_0f00_0000,
        "dashVert" => 0x8080_8080_0808_0808,
        "cross" => 0xff80_8080_8080_8080,
        "dnDiag" => 0x8040_2010_0804_0201,
        "upDiag" => 0x0102_0408_1020_4080,
        "ltDnDiag" => 0x8844_2211_8844_2211,
        "ltUpDiag" => 0x1122_4488_1122_4488,
        "dkDnDiag" => 0xcc66_3399_cc66_3399,
        "dkUpDiag" => 0x3366_cc99_3366_cc99,
        "wdDnDiag" => 0xc1e0_7038_1c0e_0783,
        "wdUpDiag" => 0x8307_0e1c_3870_e0c1,
        "dashDnDiag" => 0x0000_8844_2211_0000,
        "dashUpDiag" => 0x0000_1122_4488_0000,
        "diagCross" => 0x8142_2418_1824_4281,
        "smCheck" => 0x9966_6699_9966_6699,
        "lgCheck" => 0xf0f0_f0f0_0f0f_0f0f,
        "smGrid" => 0xff88_8888_ff88_8888,
        "lgGrid" => 0xff80_8080_8080_8080,
        "dotGrid" => 0xaa00_8000_8000_8000,
        "smConfetti" => 0x8008_4002_1001_2004,
        "lgConfetti" => 0xb130_031b_d8c0_0c8d,
        "horzBrick" => 0xff80_8080_ff08_0808,
        "diagBrick" => 0x0102_0408_1824_4281,
        "solidDmnd" => 0x1038_7cfe_7c38_1000,
        "openDmnd" => 0x8244_2810_2844_8201,
        "dotDmnd" => 0x8000_2200_0800_2200,
        "plaid" => 0xaa55_aa55_f0f0_f0f0,
        "sphere" => 0x7789_8f8f_7798_f8f8,
        "weave" => 0x8854_2245_8814_2251,
        "divot" => 0x0010_0810_0080_0180,
        "shingle" => 0x0384_4830_0c02_0101,
        "wave" => 0x0018_25c0_0018_25c0,
        "trellis" => 0xff66_ff99_ff66_ff99,
        "zigZag" => 0x8142_2418_8142_2418,
        _ => return None,
    })
}

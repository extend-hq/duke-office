use std::fmt::Write;

pub(super) fn vertical_metrics(family: &str) -> (f64, f64) {
    let primary = family
        .split(',')
        .next()
        .unwrap_or_default()
        .trim()
        .trim_matches(['\'', '"']);
    let (ascent, descent) = match primary.to_ascii_lowercase().as_str() {
        "calibri" | "carlito" => (1950., 550.),
        "times new roman" | "tinos" | "liberation serif" => (1825., 443.),
        "courier new" | "cousine" | "liberation mono" => (1705., 615.),
        "arial" | "arimo" | "liberation sans" => (1854., 434.),
        _ => return (0.8, 0.2),
    };
    (ascent / 2048., descent / 2048.)
}

pub(super) fn metric_family(family: &str) -> &str {
    let primary = family
        .split(',')
        .next()
        .unwrap_or_default()
        .trim()
        .trim_matches(['\'', '"']);
    if primary.eq_ignore_ascii_case("Liberation Sans") {
        "Arimo"
    } else {
        family
    }
}

pub(super) fn constrain_widths(svg: String) -> String {
    let Ok(document) = roxmltree::Document::parse(&svg) else {
        return svg;
    };
    let mut output = String::new();
    let mut end = 0;
    for node in document.descendants().filter(|n| n.has_tag_name("text")) {
        if node.attribute("textLength").is_some() || node.children().any(|n| !n.is_text()) {
            continue;
        }
        let inherited = |name| node.ancestors().find_map(|n| n.attribute(name));
        let family = inherited("font-family").unwrap_or_default();
        let family = metric_family(family);
        let Some(size) = inherited("font-size")
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.)
        else {
            continue;
        };
        let Some(label) = node
            .text()
            .filter(|v| v.is_ascii() && !v.bytes().any(|c| c.is_ascii_control()))
        else {
            continue;
        };
        if inherited("letter-spacing").is_some() || !svg[node.range()].starts_with("<text ") {
            continue;
        }
        let Some((last_index, _)) = label.char_indices().next_back() else {
            continue;
        };
        let bold = matches!(
            inherited("font-weight"),
            Some("bold" | "700" | "800" | "900")
        );
        let italic = inherited("font-style") == Some("italic");
        let width =
            super::font_metrics::width(&label[..last_index], size.round(), family, bold, italic)
                .zip(super::font_metrics::width(
                    &label[last_index..],
                    size,
                    family,
                    bold,
                    italic,
                ))
                .map(|(prefix, last)| prefix + last);
        let Some(width) = width.filter(|v| v.is_finite() && *v > 0.) else {
            continue;
        };
        let start = node.range().start + "<text".len();
        if end == 0 {
            output.reserve(svg.len());
        }
        output.push_str(&svg[end..start]);
        let _ = write!(
            output,
            r#" textLength="{width:.5}" lengthAdjust="spacingAndGlyphs""#
        );
        end = start;
    }
    if end == 0 {
        return svg;
    }
    output.push_str(&svg[end..]);
    output
}

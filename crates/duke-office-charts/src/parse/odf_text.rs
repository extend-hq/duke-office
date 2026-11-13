use std::{fmt::Write, sync::OnceLock};

struct Glyph {
    path: String,
    advance: f64,
}

fn glyph(code: u8, bold: bool, italic: bool) -> Option<&'static Glyph> {
    if !(32..=126).contains(&code) {
        return None;
    }
    static GLYPHS: [OnceLock<Option<Glyph>>; 380] = [const { OnceLock::new() }; 380];
    let style = usize::from(bold) + 2 * usize::from(italic);
    GLYPHS[style * 95 + usize::from(code - 32)]
        .get_or_init(|| {
            let value = duke_office_art::text::outline_text(
                &(code as char).to_string(),
                duke_office_art::text::TextFont {
                    family: "Carlito",
                    size_px: 2048.,
                    bold,
                    italic,
                },
            )?;
            Some(Glyph {
                path: value.path,
                advance: value.advance_width,
            })
        })
        .as_ref()
}

pub(super) struct LegendLayout {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    pub padding_x: f64,
    pub padding_y: f64,
    pub text_gap: f64,
    pub ascent: f64,
    pub line_height: f64,
    pub step: f64,
}

impl LegendLayout {
    pub fn frame(
        &self,
        svg: &mut String,
        legend: Option<roxmltree::Node>,
        colors: &super::chart_colors::ChartColors,
    ) {
        let props = legend.and_then(|n| crate::chart::child(n, "spPr"));
        let fill = props
            .and_then(|n| colors.shape_fill(n))
            .unwrap_or_else(|| "none".into());
        let stroke = props
            .and_then(|n| colors.shape_stroke(n))
            .unwrap_or_else(|| "none".into());
        let width = props
            .and_then(|n| crate::chart::child(n, "ln"))
            .and_then(|n| super::chart_svg::number(n.attribute("w")))
            .unwrap_or(1524.)
            / 9525.;
        let rendering = if width <= 0.16 { "crispEdges" } else { "auto" };
        let width = if width <= 0.16 { 1. } else { width };
        let _ = write!(
            svg,
            r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{fill}" stroke="{stroke}" stroke-width="{width:.2}" shape-rendering="{rendering}"/>"#,
            self.left, self.top, self.width, self.height
        );
    }

    pub fn side(
        width: f64,
        height: f64,
        size: f64,
        family: &str,
        key: f64,
        names: impl Iterator<Item = f64>,
        left: bool,
    ) -> Self {
        let (count, longest) = names.fold((0, 0_f64), |(n, w), v| (n + 1, w.max(v)));
        let padding_x = (96. / 25.4_f64).max(size * 0.33);
        let padding_y = (96. / 25.4_f64).max(size * 0.2);
        let text_gap = (96. / 25.4_f64).max(size * 0.22);
        let (ascent, descent) = vertical_metrics(family);
        let line_height = (ascent + descent) * size;
        let step = line_height + padding_y;
        let w = key + text_gap + longest + padding_x * 2.;
        let h = count as f64 * step + padding_y;
        Self {
            left: if left {
                2.1 * 96. / 25.4
            } else {
                width - 2.1 * 96. / 25.4 - w
            },
            top: (height - h) / 2.,
            width: w,
            height: h,
            padding_x,
            padding_y,
            text_gap,
            ascent: ascent * size,
            line_height,
            step,
        }
    }
}

pub(super) fn label_width(
    label: &str,
    size: f64,
    family: &str,
    bold: bool,
    italic: bool,
) -> Option<f64> {
    let (last_index, _) = label.char_indices().next_back()?;
    let family = metric_family(family);
    Some(
        super::font_metrics::width(&label[..last_index], size.round(), family, bold, italic)?
            + super::font_metrics::width(&label[last_index..], size, family, bold, italic)?,
    )
}

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

#[derive(Clone, Copy, Default)]
struct TextStyle {
    family: Option<&'static str>,
    size: Option<f64>,
    bold: bool,
    italic: bool,
    spaced: bool,
    anchor: i8,
    preserve_text: bool,
}

fn measured_family(family: &str) -> Option<&'static str> {
    let primary = family.split(',').next()?.trim().trim_matches(['\'', '"']);
    [
        (["Calibri", "Carlito", "Carlito"], "Carlito"),
        (["Arial", "Arimo", "Liberation Sans"], "Arimo"),
        (["Times New Roman", "Tinos", "Tinos"], "Tinos"),
        (["Courier New", "Cousine", "Cousine"], "Cousine"),
    ]
    .into_iter()
    .find_map(|(aliases, name)| {
        aliases
            .iter()
            .any(|a| primary.eq_ignore_ascii_case(a))
            .then_some(name)
    })
}

struct TextSpan<'a> {
    start: usize,
    open_end: usize,
    depth: usize,
    style: TextStyle,
    position: Option<(f64, f64)>,
    label: Option<std::borrow::Cow<'a, str>>,
    skip: bool,
}

pub(super) fn constrain_widths(svg: String, portable_polar_text: bool) -> String {
    if !svg.contains("<text ") {
        return svg;
    }
    rewrite_text(&svg, portable_polar_text).unwrap_or(svg)
}

fn rewrite_text(svg: &str, portable_polar_text: bool) -> Option<String> {
    use quick_xml::{events::Event, Reader};
    let mut reader = Reader::from_str(svg);
    let mut stack: Vec<TextStyle> = Vec::new();
    let mut pending: Option<TextSpan<'_>> = None;
    let mut output = String::new();
    let mut definitions = String::new();
    let mut defined = [false; 380];
    let mut prefix = std::borrow::Cow::Borrowed("odf-glyph-");
    while portable_polar_text && svg.contains(prefix.as_ref()) {
        prefix.to_mut().push('_');
    }
    let mut end = 0;
    loop {
        let before = reader.buffer_position() as usize;
        match reader.read_event().ok()? {
            Event::Start(tag) => {
                if let Some(text) = &mut pending {
                    text.skip = true;
                }
                let mut style = stack.last().cloned().unwrap_or_default();
                let is_text = tag.name().as_ref() == b"text";
                let (mut x, mut y) = (None, None);
                let mut skip = false;
                for attribute in tag.attributes() {
                    let attribute = attribute.ok()?;
                    match attribute.key.as_ref() {
                        b"font-family" => {
                            style.family = measured_family(&attribute.unescape_value().ok()?)
                        }
                        b"font-size" => style.size = attribute.unescape_value().ok()?.parse().ok(),
                        b"font-weight" => {
                            style.bold = matches!(
                                attribute.unescape_value().ok()?.as_ref(),
                                "bold" | "700" | "800" | "900"
                            )
                        }
                        b"font-style" => {
                            style.italic = attribute.unescape_value().ok()?.as_ref() == "italic"
                        }
                        b"letter-spacing" => style.spaced = true,
                        b"text-anchor" => {
                            style.anchor = match attribute.unescape_value().ok()?.as_ref() {
                                "middle" => 1,
                                "end" => 2,
                                _ => 0,
                            }
                        }
                        b"dx" | b"dy" | b"rotate" | b"dominant-baseline" | b"style"
                        | b"text-decoration" | b"stroke" | b"word-spacing" | b"font-variant"
                        | b"font-stretch" => style.preserve_text = true,
                        b"textLength" if is_text => skip = true,
                        b"x" if is_text => x = attribute.unescape_value().ok()?.parse::<f64>().ok(),
                        b"y" if is_text => y = attribute.unescape_value().ok()?.parse::<f64>().ok(),
                        _ => {}
                    }
                }
                if is_text && pending.is_none() {
                    pending = Some(TextSpan {
                        start: before,
                        open_end: reader.buffer_position() as usize,
                        depth: stack.len() + 1,
                        style: style.clone(),
                        position: x.zip(y).filter(|(x, y)| x.is_finite() && y.is_finite()),
                        label: None,
                        skip: skip || !svg[before..].starts_with("<text "),
                    });
                }
                stack.push(style);
            }
            Event::Text(text) => {
                if let Some(p) = &mut pending {
                    let label = text.unescape().ok()?;
                    if let Some(previous) = &mut p.label {
                        previous.to_mut().push_str(&label);
                    } else {
                        p.label = Some(label);
                    }
                }
            }
            Event::End(_) => {
                let depth = stack.len();
                stack.pop()?;
                if !pending.as_ref().is_some_and(|p| p.depth == depth) {
                    continue;
                }
                let p = pending.take()?;
                if p.skip || p.style.spaced {
                    continue;
                }
                let Some(label) = p.label.as_deref().filter(|v| {
                    !v.is_empty() && v.is_ascii() && !v.bytes().any(|c| c.is_ascii_control())
                }) else {
                    continue;
                };
                let Some(size) = p.style.size.filter(|v| v.is_finite() && *v > 0.) else {
                    continue;
                };
                let Some(family) = p.style.family else {
                    continue;
                };
                let bold = p.style.bold;
                let italic = p.style.italic;
                let Some(width) = label_width(label, size, family, bold, italic)
                    .filter(|v| v.is_finite() && *v > 0.)
                else {
                    continue;
                };
                if end == 0 {
                    output.reserve(svg.len());
                }
                let primary = family
                    .split(',')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .trim_matches(['\'', '"']);
                if portable_polar_text
                    && label.len() <= 512
                    && (primary.eq_ignore_ascii_case("Calibri")
                        || primary.eq_ignore_ascii_case("Carlito"))
                    && p.position.is_some()
                    && !p.style.preserve_text
                    && label.bytes().all(|c| glyph(c, bold, italic).is_some())
                {
                    let (mut x, y) = p.position?;
                    x -= match p.style.anchor {
                        1 => width / 2.,
                        2 => width,
                        _ => 0.,
                    };
                    let style = usize::from(bold) + 2 * usize::from(italic);
                    output.push_str(&svg[end..p.start]);
                    output.push_str("<g");
                    output.push_str(&svg[p.start + 5..p.open_end]);
                    output.push_str("<title>");
                    output.push_str(&svg[p.open_end..before]);
                    let _ = write!(
                        output,
                        r#"</title><g transform="translate({x:.5} {y:.5}) scale({:.8})">"#,
                        size / 2048.
                    );
                    let mut advance = 0.;
                    for c in label.bytes() {
                        let g = glyph(c, bold, italic)?;
                        let index = style * 95 + usize::from(c - 32);
                        if !defined[index] {
                            let _ = write!(
                                definitions,
                                r#"<path id="{prefix}{index}" d="{}"/>"#,
                                g.path
                            );
                            defined[index] = true;
                        }
                        let _ = write!(
                            output,
                            r##"<use href="#{prefix}{index}" x="{advance:.5}"/>"##
                        );
                        advance += g.advance * size.round() / size;
                    }
                    output.push_str("</g></g>");
                    end = reader.buffer_position() as usize;
                } else {
                    let start = p.start + 5;
                    output.push_str(&svg[end..start]);
                    let _ = write!(
                        output,
                        r#" textLength="{width:.5}" lengthAdjust="spacingAndGlyphs""#
                    );
                    end = start;
                }
            }
            Event::Eof => break,
            Event::Empty(_) | Event::Comment(_) | Event::CData(_) | Event::PI(_) => {
                if let Some(p) = &mut pending {
                    p.skip = true;
                }
            }
            Event::DocType(_) => return None,
            Event::Decl(_) => {}
        }
    }
    if end == 0 || !stack.is_empty() {
        return None;
    }
    output.push_str(&svg[end..]);
    if !definitions.is_empty() {
        let at = output.rfind("</svg>")?;
        definitions.insert_str(0, "<defs>");
        definitions.push_str("</defs>");
        output.insert_str(at, &definitions);
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::constrain_widths;

    #[test]
    fn text_styles_survive_entities_and_unsupported_siblings() {
        let svg = r#"<svg><g font-family="&quot;Carlito&quot;, sans-serif" font-size="13.33" font-weight="bold" font-style="italic"><text x="1" y="20">A&amp;B</text><text x="1" y="40">日本語</text><text x="1" y="60">AB</text></g></svg>"#;
        let result = constrain_widths(svg.into(), false);
        let doc = roxmltree::Document::parse(&result).unwrap();
        let texts: Vec<_> = doc
            .descendants()
            .filter(|n| n.has_tag_name("text"))
            .collect();
        assert_eq!(texts.len(), 3);
        assert_eq!(texts[0].text(), Some("A&B"));
        assert!(texts[0].attribute("textLength").is_some());
        assert!(texts[1].attribute("textLength").is_none());
        assert!(texts[2].attribute("textLength").is_some());
        assert!(result.contains(r#"font-weight="bold" font-style="italic""#));
    }

    #[test]
    fn portable_glyphs_are_reused_and_ids_do_not_collide() {
        let svg = r#"<svg><defs><path id="odf-glyph-0" d="M0 0"/></defs><g font-family="Carlito" font-size="16" transform="rotate(10)"><text x="30" y="20" text-anchor="middle">ABBA</text><text x="30" y="40">AB</text></g></svg>"#;
        let result = constrain_widths(svg.into(), true);
        let doc = roxmltree::Document::parse(&result).unwrap();
        let paths: Vec<_> = doc
            .descendants()
            .filter(|n| n.has_tag_name("path"))
            .collect();
        assert_eq!(paths.len(), 3);
        let uses: Vec<_> = doc
            .descendants()
            .filter(|n| n.has_tag_name("use"))
            .collect();
        assert_eq!(uses.len(), 6);
        for node in uses {
            let id = node.attribute("href").unwrap().strip_prefix('#').unwrap();
            assert_ne!(id, "odf-glyph-0");
            assert!(paths.iter().any(|p| p.attribute("id") == Some(id)));
        }
        assert!(result.contains(r#"transform="rotate(10)""#));
        assert_eq!(
            doc.descendants()
                .filter(|n| n.has_tag_name("title"))
                .filter_map(|n| n.text())
                .collect::<Vec<_>>(),
            ["ABBA", "AB"]
        );
    }

    #[test]
    fn complex_text_and_explicit_spacing_keep_their_rendering() {
        let svg = r#"<svg font-family="Carlito" font-size="16"><text x="0" y="20"><tspan>AB</tspan></text><g letter-spacing="1"><text x="0" y="40">AB</text></g><text x="0" y="60" dx="2">AB</text><text x="0" y="80" textLength="12">AB</text></svg>"#;
        let result = constrain_widths(svg.into(), true);
        let doc = roxmltree::Document::parse(&result).unwrap();
        let texts: Vec<_> = doc
            .descendants()
            .filter(|n| n.has_tag_name("text"))
            .collect();
        assert_eq!(texts.len(), 4);
        assert!(texts[0].attribute("textLength").is_none());
        assert!(texts[1].attribute("textLength").is_none());
        assert_eq!(texts[2].attribute("dx"), Some("2"));
        assert_eq!(texts[3].attribute("textLength"), Some("12"));
        assert!(!result.contains("<use"));
    }

    #[test]
    fn malformed_text_does_not_replace_the_original_svg() {
        let svg = r#"<svg font-family="Carlito" font-size="16"><text x="0" y="20">AB</svg>"#;
        assert_eq!(constrain_widths(svg.into(), true), svg);
    }
}

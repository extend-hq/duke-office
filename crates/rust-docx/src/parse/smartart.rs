use std::{borrow::Borrow, cell::RefCell};

use roxmltree::Node;
use rust_office_art::smartart::{SmartArtDrawingKind, SmartArtDrawingNode};

use super::context::{ParseContext, ParsedStyleSheet};
use super::util::escape_xml_text;

fn child<'a, 'b>(node: Node<'a, 'b>, name: &str) -> Option<Node<'a, 'b>> {
    node.children()
        .find(|node| node.is_element() && node.tag_name().name() == name)
}

fn wrapped(name: &str, node: Node<'_, '_>, xml: &str) -> String {
    let raw = &xml[node.range()];
    let Some(end) = raw.find('>') else {
        return String::new();
    };
    let opening = &raw[..end];
    let attributes = opening
        .find(char::is_whitespace)
        .map(|offset| &opening[offset..])
        .unwrap_or("")
        .trim_end_matches('/');
    let body = if opening.ends_with('/') {
        ""
    } else {
        &raw[end + 1..raw.rfind("</").unwrap_or(end + 1)]
    };
    format!("<{name}{attributes}>{body}</{name}>")
}

fn text_box(
    xml: &str,
    style_xml: &str,
    context: &ParseContext<'_, impl Borrow<ParsedStyleSheet>>,
) -> Option<String> {
    let wrapped_xml = format!(
        r#"<root xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:dsp="http://schemas.microsoft.com/office/drawing/2008/diagram" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">{style_xml}{xml}</root>"#
    );
    let document = roxmltree::Document::parse(&wrapped_xml).ok()?;
    let font_reference = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "fontRef");
    let reference_family = match font_reference.and_then(|node| node.attribute("idx")) {
        Some("major") => context.styles().theme_fonts.major_latin.as_deref(),
        Some("minor") => context.styles().theme_fonts.minor_latin.as_deref(),
        _ => None,
    };
    let mut output = String::new();
    if let Some(body) = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "bodyPr")
    {
        output.push_str(&wrapped("wps:bodyPr", body, &wrapped_xml));
    }
    output.push_str("<wps:txbx><w:txbxContent>");
    for paragraph in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "p")
    {
        output.push_str("<w:p>");
        let properties = child(paragraph, "pPr");
        if let Some(align) = properties.and_then(|node| node.attribute("algn")) {
            let value = match align {
                "ctr" => "center",
                "r" => "right",
                "just" | "dist" => "both",
                _ => "left",
            };
            output.push_str(&format!(r#"<w:pPr><w:jc w:val="{value}"/></w:pPr>"#));
        }
        let defaults = properties.and_then(|node| child(node, "defRPr"));
        for run in paragraph.children().filter(|node| node.is_element()) {
            match run.tag_name().name() {
                "br" => output.push_str("<w:r><w:br/></w:r>"),
                "r" | "fld" => {
                    output.push_str("<w:r><w:rPr>");
                    let direct_style = child(run, "rPr");
                    let style = direct_style.or(defaults).or(font_reference);
                    if let Some(style) = style {
                        let attribute_value = |name| {
                            direct_style
                                .and_then(|node| node.attribute(name))
                                .or_else(|| defaults.and_then(|node| node.attribute(name)))
                        };
                        if let Some(size) =
                            attribute_value("sz").and_then(|value| value.parse::<u32>().ok())
                        {
                            output.push_str(&format!(
                                r#"<w:sz w:val="{}"/>"#,
                                (size as f64 / 50.0).round().max(1.0)
                            ));
                        }
                        for (attribute, tag) in [("b", "w:b"), ("i", "w:i")] {
                            if attribute_value(attribute)
                                .is_some_and(|value| matches!(value, "1" | "true"))
                            {
                                output.push_str(&format!("<{tag}/>"));
                            }
                        }
                        if let Some(font) = child(style, "latin")
                            .or_else(|| defaults.and_then(|node| child(node, "latin")))
                            .and_then(|node| node.attribute("typeface"))
                            .or(reference_family)
                        {
                            let family = match font {
                                "+mj-lt" => context
                                    .styles()
                                    .theme_fonts
                                    .major_latin
                                    .as_deref()
                                    .unwrap_or("Calibri"),
                                "+mn-lt" => context
                                    .styles()
                                    .theme_fonts
                                    .minor_latin
                                    .as_deref()
                                    .unwrap_or("Calibri"),
                                _ => font,
                            };
                            output.push_str(&format!(
                                r#"<w:rFonts w:ascii="{}" w:hAnsi="{}"/>"#,
                                escape_xml_text(family),
                                escape_xml_text(family)
                            ));
                        }
                        if let Some(color) = super::colors::resolve_drawing_color_from_xml(
                            Some(&wrapped_xml[style.range()]),
                            &context.styles().theme_colors,
                        )
                        .or_else(|| {
                            defaults.and_then(|node| {
                                super::colors::resolve_drawing_color_from_xml(
                                    Some(&wrapped_xml[node.range()]),
                                    &context.styles().theme_colors,
                                )
                            })
                        })
                        .or_else(|| {
                            font_reference.and_then(|node| {
                                super::colors::resolve_drawing_color_from_xml(
                                    Some(&wrapped_xml[node.range()]),
                                    &context.styles().theme_colors,
                                )
                            })
                        }) {
                            output.push_str(&format!(
                                r#"<w:color w:val="{}"/>"#,
                                color.color.trim_start_matches('#')
                            ));
                        }
                    }
                    output.push_str("</w:rPr>");
                    for text in run
                        .children()
                        .filter(|node| node.is_element() && node.tag_name().name() == "t")
                    {
                        output.push_str(&format!(
                            r#"<w:t xml:space="preserve">{}</w:t>"#,
                            escape_xml_text(text.text().unwrap_or_default())
                        ));
                    }
                    output.push_str("</w:r>");
                }
                _ => {}
            }
        }
        output.push_str("</w:p>");
    }
    output.push_str("</w:txbxContent></wps:txbx>");
    Some(output)
}

fn word_node(
    node: &SmartArtDrawingNode,
    context: &ParseContext<'_, impl Borrow<ParsedStyleSheet>>,
    remaining: &mut usize,
    depth: usize,
) -> Option<String> {
    if *remaining == 0 || depth > 64 {
        return None;
    }
    *remaining -= 1;
    let xml = &node.source_xml;
    let document = roxmltree::Document::parse(xml).ok()?;
    let root = document.root_element();
    match node.kind {
        SmartArtDrawingKind::Unknown => None,
        SmartArtDrawingKind::Group => {
            let properties = child(root, "grpSpPr")
                .map(|node| wrapped("wpg:grpSpPr", node, xml))
                .unwrap_or_default();
            let mut output = format!("<wpg:grpSp>{properties}");
            for nested in &node.children {
                output.push_str(&word_node(nested, context, remaining, depth + 1)?);
            }
            output.push_str("</wpg:grpSp>");
            Some(output)
        }
        SmartArtDrawingKind::Picture => {
            let mut output = String::from("<pic:pic>");
            for name in ["nvPicPr", "blipFill", "spPr"] {
                if let Some(node) = child(root, name) {
                    output.push_str(&wrapped(&format!("pic:{name}"), node, xml));
                }
            }
            output.push_str("</pic:pic>");
            Some(output)
        }
        SmartArtDrawingKind::Shape | SmartArtDrawingKind::Connector => {
            let text_transform = if let Some(transform) = node.text_transform.as_ref() {
                format!(
                    r#"<wps:txXfrm rot="{}" flipH="{}" flipV="{}"><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></wps:txXfrm>"#,
                    transform.rotation * 60000.0,
                    u8::from(transform.flip_horizontal),
                    u8::from(transform.flip_vertical),
                    transform.x,
                    transform.y,
                    transform.width,
                    transform.height
                )
            } else {
                if child(root, "txXfrm").is_some() {
                    context.push_warning("SmartArt text transform has incomplete or unsupported dimensions; text uses the shape frame");
                }
                String::new()
            };
            let properties = wrapped("wps:spPr", child(root, "spPr")?, xml);
            let style = child(root, "style")
                .map(|node| wrapped("wps:style", node, xml))
                .unwrap_or_default();
            let text = child(root, "txBody")
                .and_then(|_| text_box(xml, &style, context))
                .unwrap_or_default();
            Some(format!(
                "<wps:wsp>{properties}{style}{text_transform}{text}</wps:wsp>"
            ))
        }
    }
}

pub(super) fn render_diagram(
    run: &str,
    width: f64,
    height: f64,
    context: &ParseContext<'_, impl Borrow<ParsedStyleSheet>>,
) -> Option<String> {
    let references = crate::xml::extract_balanced_tag_slices(run, "dgm:relIds")
        .into_iter()
        .next()?;
    let relationship = crate::xml::get_attribute(references, "r:dm")?;
    let data_part = context.relationships.get(&relationship)?;
    let data =
        rust_office_art::smartart::parse_smartart_data(&context.parts.get(data_part)?.content)?;
    let related = super::relationships::parse_relationships_from_parts(context.parts, data_part);
    let drawing_id = data.drawing_relationship_id.as_deref();
    let mut drawing_parts = Vec::new();
    for candidate in [
        drawing_id.and_then(|id| related.get(id)).cloned(),
        drawing_id
            .and_then(|id| context.relationships.get(id))
            .cloned(),
        crate::package::relationship_target_from_parts(context.parts, data_part, "diagramDrawing"),
    ]
    .into_iter()
    .flatten()
    {
        if !drawing_parts.contains(&candidate) {
            drawing_parts.push(candidate);
        }
    }
    for drawing_part in drawing_parts {
        if let Some(drawing) = context
            .parts
            .get(&drawing_part)
            .and_then(|part| rust_office_art::smartart::parse_smartart_drawing(&part.content))
        {
            let child_context = ParseContext {
                relationships: super::relationships::parse_relationships_from_parts(
                    context.parts,
                    &drawing_part,
                ),
                content_types: context.content_types.clone(),
                parts: context.parts,
                binary_assets: context.binary_assets,
                style_sheet: context.styles(),
                warnings: RefCell::new(Vec::new()),
            };
            let (width_emu, height_emu) = crate::xml::extract_balanced_tag_slices(run, "wp:extent")
                .into_iter()
                .find_map(|extent| {
                    let width = crate::xml::parse_integer_attribute(extent, "cx")?;
                    let height = crate::xml::parse_integer_attribute(extent, "cy")?;
                    (width > 0 && height > 0).then_some((width, height))
                })
                .unwrap_or_else(|| {
                    (
                        (width * 9525.0).round() as i64,
                        (height * 9525.0).round() as i64,
                    )
                });
            let properties = drawing.group_properties_xml.as_deref().and_then(|xml| {
                let document = roxmltree::Document::parse(xml).ok()?;
                let transform = child(document.root_element(), "xfrm")?;
                child(transform, "chOff")?;
                child(transform, "chExt")?;
                Some(wrapped("wpg:grpSpPr", document.root_element(), xml))
            }).unwrap_or_else(|| format!(r#"<wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{}" cy="{}"/><a:chOff x="0" y="0"/><a:chExt cx="{}" cy="{}"/></a:xfrm></wpg:grpSpPr>"#, width_emu, height_emu, width_emu, height_emu));
            let mut remaining = 2048;
            let nodes = drawing
                .nodes
                .iter()
                .map(|node| word_node(node, &child_context, &mut remaining, 0))
                .collect::<Option<Vec<_>>>();
            if let Some(nodes) = nodes {
                let group = format!("<wpg:wgp>{properties}{}</wpg:wgp>", nodes.join(""));
                if let Some(svg) = super::shapes::render_standalone_word_shape_svg(
                    &group,
                    Some(width),
                    Some(height),
                    &child_context,
                ) {
                    context
                        .warnings
                        .borrow_mut()
                        .extend(child_context.warnings.into_inner());
                    return Some(svg.replacen("<svg ", r#"<svg data-office-art="true" "#, 1));
                }
            }
        }
    }
    context.push_warning("SmartArt cached drawing is unavailable or unsupported; displaying semantic text without native layout");
    let paragraphs = data
        .points
        .iter()
        .filter(|point| point.kind == "node" && !point.text.is_empty())
        .take(512)
        .map(|point| super::shapes::ParsedTextBoxParagraph {
            text: point.text.clone(),
            style: None,
            align: None,
            images: Vec::new(),
            text_fill: None,
        })
        .collect::<Vec<_>>();
    if paragraphs.is_empty() {
        return None;
    }
    Some(
        super::shapes::render_bounded_text_box_svg(&paragraphs, width, height, None).replacen(
            "<svg ",
            r#"<svg data-office-art="true" "#,
            1,
        ),
    )
}

use super::{
    parse_color, parse_paragraph_style, parse_run_style, text_run, OwnedNode, RunStyle,
    TextParseContext, TextRun, XmlDocument, XmlNode,
};
use crate::{TextField, TextFieldParagraphProperties};
use quick_xml::{events::Event, name::ResolveResult, NsReader};
use std::borrow::Cow;

fn drawing_namespace(namespace: ResolveResult<'_>) -> bool {
    matches!(namespace, ResolveResult::Bound(ns) if matches!(ns.as_ref(),
        b"http://schemas.openxmlformats.org/drawingml/2006/main"
        | b"http://purl.oclc.org/ooxml/drawingml/main"))
}

fn normalized_lines(xml: &str) -> Cow<'_, str> {
    if xml.contains('\r') {
        Cow::Owned(xml.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(xml)
    }
}

fn field_tree(source: &str) -> Option<XmlDocument> {
    struct Frame {
        node: Option<OwnedNode>,
        children: Vec<OwnedNode>,
    }
    let xml = normalized_lines(source);
    let mut reader = NsReader::from_reader(xml.as_bytes());
    let mut stack: Vec<Frame> = Vec::new();
    let mut result = None;
    loop {
        match reader.read_resolved_event().ok()? {
            (namespace, event @ (Event::Start(_) | Event::Empty(_))) => {
                let qualified = drawing_namespace(namespace);
                let empty = matches!(event, Event::Empty(_));
                let start = match event {
                    Event::Start(start) | Event::Empty(start) => start,
                    _ => unreachable!(),
                };
                let name = start.local_name();
                let selected = qualified
                    && ((stack.len() == 1 && name.as_ref() == b"fld")
                        || (stack.len() > 1 && stack.last()?.node.is_some()));
                let node = if selected {
                    let mut attributes = Vec::new();
                    for attribute in start.attributes().with_checks(false).flatten() {
                        let (namespace, local) = reader.resolve_attribute(attribute.key);
                        let accepted = match namespace {
                            ResolveResult::Unbound => !attribute.key.as_ref().contains(&b':')
                                && attribute.key.as_ref() != b"xmlns",
                            ResolveResult::Bound(ns) => name.as_ref() != b"fld" && matches!(ns.as_ref(),
                                b"http://schemas.openxmlformats.org/officeDocument/2006/relationships"
                                | b"http://purl.oclc.org/ooxml/officeDocument/relationships"),
                            ResolveResult::Unknown(_) => false,
                        };
                        if !accepted {
                            continue;
                        }
                        let raw = std::str::from_utf8(&attribute.value).ok()?;
                        let normalized = if raw.contains(['\t', '\n']) {
                            Cow::Owned(raw.replace(['\t', '\n'], " "))
                        } else {
                            Cow::Borrowed(raw)
                        };
                        let value = quick_xml::escape::unescape(&normalized).ok()?;
                        attributes.push((
                            std::str::from_utf8(local.as_ref()).ok()?.to_owned(),
                            value.into_owned(),
                        ));
                    }
                    Some(OwnedNode {
                        name: std::str::from_utf8(name.as_ref()).ok()?.to_owned(),
                        attributes,
                        ..OwnedNode::default()
                    })
                } else {
                    None
                };
                if empty {
                    if let Some(node) = node {
                        if stack.len() == 1 {
                            result = Some(node);
                        } else if let Some(parent) = stack.last_mut() {
                            if parent.node.is_some() {
                                parent.children.push(node);
                            }
                        }
                    }
                } else {
                    stack.push(Frame {
                        node,
                        children: Vec::new(),
                    });
                }
            }
            (_, Event::Text(text)) => {
                if let Some(Frame {
                    node: Some(node), ..
                }) = stack.last_mut()
                {
                    node.text.push_str(&text.unescape().ok()?);
                }
            }
            (_, Event::CData(text)) => {
                if let Some(Frame {
                    node: Some(node), ..
                }) = stack.last_mut()
                {
                    node.text.push_str(std::str::from_utf8(&text).ok()?);
                }
            }
            (_, Event::End(_)) => {
                let Frame { node, children } = stack.pop()?;
                if let Some(mut node) = node {
                    node.children = children;
                    if stack.len() == 1 {
                        result = Some(node);
                    } else if let Some(parent) = stack.last_mut() {
                        if parent.node.is_some() {
                            parent.children.push(node);
                        }
                    }
                }
            }
            (_, Event::Eof) => return result.map(XmlDocument::from_owned),
            _ => {}
        }
    }
}

fn paragraph_properties(
    node: XmlNode<'_>,
    context: &TextParseContext<'_, '_>,
) -> TextFieldParagraphProperties {
    let style = parse_paragraph_style(Some(node));
    TextFieldParagraphProperties {
        alignment: style.alignment,
        level: style.level,
        bullet: style.bullet.flatten(),
        line_spacing: style.line_spacing,
        space_before: style.space_before,
        space_after: style.space_after,
        rtl: style.rtl,
        margin_left_emu: style.margin_left_emu,
        margin_right_emu: node.attr("marR").and_then(|value| value.parse().ok()),
        indent_emu: style.indent_emu,
        default_tab_size_emu: style.default_tab_size_emu,
        tab_stops: style.tab_stops,
        east_asian_line_break: style.east_asian_line_break,
        latin_line_break: style.latin_line_break,
        hanging_punctuation: style.hanging_punctuation,
        font_alignment: node.attr("fontAlgn").map(str::to_owned),
        bullet_none: node.child("buNone").map(|_| true),
        bullet_color: node
            .child("buClr")
            .and_then(|color| parse_color(color, context.colors)),
        bullet_color_follow_text: node.child("buClrTx").map(|_| true),
        bullet_font_follow_text: node.child("buFontTx").map(|_| true),
        bullet_size_follow_text: node.child("buSzTx").map(|_| true),
        bullet_image_asset_id: node
            .child("buBlip")
            .and_then(|node| node.child("blip"))
            .and_then(|node| node.attr("embed"))
            .and_then(|id| context.rels.internal(id))
            .cloned(),
        default_run_properties: node
            .child("defRPr")
            .map(|node| parse_run_style(Some(node), context.colors, context.rels)),
    }
}

pub(super) fn push_run(
    original: XmlNode<'_>,
    default_style: &RunStyle,
    context: &TextParseContext<'_, '_>,
    runs: &mut Vec<TextRun>,
) -> bool {
    let Some(source_xml) = original.source_xml() else {
        return false;
    };
    let Some(document) = field_tree(source_xml) else {
        return true;
    };
    let node = document.root();
    let paragraph_properties = node
        .child("pPr")
        .map(|properties| Box::new(paragraph_properties(properties, context)));
    let style = default_style
        .clone()
        .overlay(parse_run_style(
            node.child("pPr")
                .and_then(|properties| properties.child("defRPr")),
            context.colors,
            context.rels,
        ))
        .overlay(parse_run_style(
            node.child("rPr"),
            context.colors,
            context.rels,
        ));
    let value = node
        .children_named("t")
        .map(XmlNode::text_content)
        .collect();
    let mut run = text_run(value, style);
    run.field = Some(Box::new(TextField {
        id: node.attr("id").map(str::to_owned),
        field_type: node.attr("type").map(str::to_owned),
        has_cached_text: node.child("t").is_some(),
        paragraph_properties,
        source_part: context.source_part.map(str::to_owned),
        source_xml: source_xml.to_owned(),
    }));
    runs.push(run);
    true
}

pub(super) fn canonical_fragment(xml: &str) -> Cow<'_, str> {
    if xml.contains("fld") && xml.contains(['\r', '\n', '\t']) {
        Cow::Owned(
            xml.replace('\r', "&#13;")
                .replace('\n', "&#10;")
                .replace('\t', "&#9;"),
        )
    } else {
        Cow::Borrowed(xml)
    }
}

use super::{
    parse_text_values, parse_xml_tree, HashMap, ParseError, ParseLimits, SlideComment, XmlBudget,
};
use crate::{SlideCommentPosition, SlideCommentPositionUnit};
use quick_xml::{events::Event, name::ResolveResult, NsReader, Reader};
use std::borrow::Cow;

#[path = "pptx_modern_comments.rs"]
mod modern;

fn normalized_xml<'a>(xml: &'a [u8], limits: &ParseLimits) -> Result<Cow<'a, [u8]>, ParseError> {
    let xml = xml.strip_prefix(b"\xef\xbb\xbf").unwrap_or(xml);
    let mut normalized = if xml.contains(&b'\r') {
        let mut result = Vec::with_capacity(xml.len());
        let mut previous_cr = false;
        for &byte in xml {
            if !(byte == b'\n' && previous_cr) {
                result.push(if byte == b'\r' { b'\n' } else { byte });
            }
            previous_cr = byte == b'\r';
        }
        Cow::Owned(result)
    } else {
        Cow::Borrowed(xml)
    };
    let mut ranges = Vec::new();
    let mut reader = Reader::from_reader(normalized.as_ref());
    let mut budget = XmlBudget::default();
    let mut depth = 0usize;
    loop {
        match reader.read_event() {
            Ok(event @ (Event::Start(_) | Event::Empty(_))) => {
                if matches!(event, Event::Start(_)) {
                    if depth >= limits.max_xml_depth {
                        return Err(ParseError::ResourceLimit("XML nesting depth".into()));
                    }
                    depth += 1;
                }
                let start = match &event {
                    Event::Start(start) | Event::Empty(start) => start,
                    _ => unreachable!(),
                };
                budget.consume_node(start, limits)?;
                for attribute in start.attributes().with_checks(false).flatten() {
                    if let Cow::Borrowed(value) = attribute.value {
                        if value.iter().any(|byte| matches!(byte, b'\t' | b'\n')) {
                            let start = value.as_ptr() as usize - normalized.as_ptr() as usize;
                            ranges.push(start..start + value.len());
                        }
                    }
                }
            }
            Ok(Event::End(_)) => depth = depth.saturating_sub(1),
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    if !ranges.is_empty() {
        let bytes = normalized.to_mut();
        for range in ranges {
            for byte in &mut bytes[range] {
                if matches!(byte, b'\t' | b'\n') {
                    *byte = b' ';
                }
            }
        }
    }
    Ok(normalized)
}

#[derive(Default)]
pub(super) struct CommentAuthor {
    name: Option<String>,
    initials: Option<String>,
    user_id: Option<String>,
    provider_id: Option<String>,
    extensions_xml: Option<String>,
}

fn classic_root(xml: &[u8], name: &[u8]) -> bool {
    let mut reader = NsReader::from_reader(xml);
    loop {
        match reader.read_resolved_event() {
            Ok((namespace, Event::Start(start) | Event::Empty(start))) => {
                return start.local_name().as_ref() == name
                    && matches!(namespace, ResolveResult::Bound(ns) if matches!(ns.as_ref(),
                        b"http://schemas.openxmlformats.org/presentationml/2006/main"
                        | b"http://purl.oclc.org/ooxml/presentationml/main"));
            }
            Ok((_, Event::Eof)) | Err(_) => return false,
            _ => {}
        }
    }
}

fn qualified_children(xml: &[u8]) -> Vec<(bool, Vec<bool>)> {
    let mut reader = NsReader::from_reader(xml);
    let mut depth = 0usize;
    let mut result: Vec<(bool, Vec<bool>)> = Vec::new();
    loop {
        match reader.read_resolved_event() {
            Ok((namespace, event @ (Event::Start(_) | Event::Empty(_)))) => {
                let qualified = matches!(namespace, ResolveResult::Bound(ns) if matches!(ns.as_ref(),
                    b"http://schemas.openxmlformats.org/presentationml/2006/main"
                    | b"http://purl.oclc.org/ooxml/presentationml/main"));
                if depth == 1 {
                    result.push((qualified, Vec::new()));
                } else if depth == 2 {
                    if let Some((_, children)) = result.last_mut() {
                        children.push(qualified);
                    }
                }
                if matches!(event, Event::Start(_)) {
                    depth += 1;
                }
            }
            Ok((_, Event::End(_))) => depth = depth.saturating_sub(1),
            Ok((_, Event::Eof)) | Err(_) => return result,
            _ => {}
        }
    }
}

pub(super) fn authors(
    xml: &[u8],
    limits: &ParseLimits,
    context: &str,
) -> Result<HashMap<String, CommentAuthor>, ParseError> {
    if !classic_root(xml, b"cmAuthorLst") {
        return Ok(HashMap::new());
    }
    let normalized = normalized_xml(xml, limits)?;
    let qualified = qualified_children(normalized.as_ref());
    let document = parse_xml_tree(normalized.into_owned(), limits, context)?;
    Ok(document
        .root()
        .children()
        .zip(qualified)
        .filter(|(node, (qualified, _))| *qualified && node.name() == "cmAuthor")
        .filter_map(|(node, _)| {
            Some((
                node.attr("id")?.to_owned(),
                CommentAuthor {
                    name: node.attr("name").map(str::to_owned),
                    initials: node.attr("initials").map(str::to_owned),
                    ..Default::default()
                },
            ))
        })
        .collect())
}

pub(super) fn parse(
    xml: &[u8],
    limits: &ParseLimits,
    context: &str,
    authors: &HashMap<String, CommentAuthor>,
    modern_authors: &HashMap<String, CommentAuthor>,
    rels: &super::Relationships,
) -> Result<Vec<SlideComment>, ParseError> {
    if modern::is_root(xml, b"cmLst") {
        return modern::parse(xml, limits, context, modern_authors, rels);
    }
    if !classic_root(xml, b"cmLst") {
        return Ok(parse_text_values(xml)
            .into_iter()
            .enumerate()
            .map(|(index, text)| SlideComment {
                id: format!("comment-{}", index + 1),
                text,
                paragraphs: Vec::new(),
                replies: Vec::new(),
                modern: None,
                author: None,
                author_id: None,
                author_initials: None,
                source_index: None,
                source_part: None,
                created_at: None,
                modified_at: None,
                position: None,
                x: None,
                y: None,
            })
            .collect());
    }
    let normalized = normalized_xml(xml, limits)?;
    let qualified = qualified_children(normalized.as_ref());
    let document = parse_xml_tree(normalized.into_owned(), limits, context)?;
    Ok(document
        .root()
        .children()
        .zip(qualified)
        .filter(|(node, (qualified, _))| *qualified && node.name() == "cm")
        .enumerate()
        .map(|(index, (node, (_, children)))| {
            let child = |name: &str| {
                node.children()
                    .iter()
                    .zip(&children)
                    .find_map(|(child, qualified)| {
                        (*qualified && child.name() == name).then_some(child)
                    })
            };
            let author_id = node.attr("authorId");
            let author = author_id.and_then(|id| authors.get(id));
            let source_index = node.attr("idx").and_then(|value| value.parse::<u32>().ok());
            let position = child("pos").and_then(|position| {
                Some(SlideCommentPosition {
                    x: position.attr("x")?.parse().ok()?,
                    y: position.attr("y")?.parse().ok()?,
                    unit: SlideCommentPositionUnit::EighthPoint,
                })
            });
            SlideComment {
                id: match (author_id, source_index) {
                    (Some(author), Some(index)) => format!("comment-{author}-{index}"),
                    _ => format!("comment-{}", index + 1),
                },
                text: child("text")
                    .map(|text| text.text().to_owned())
                    .unwrap_or_default(),
                paragraphs: Vec::new(),
                replies: Vec::new(),
                modern: None,
                author: author.and_then(|author| author.name.clone()),
                author_id: author_id.map(str::to_owned),
                author_initials: author.and_then(|author| author.initials.clone()),
                source_index,
                source_part: Some(context.to_owned()),
                created_at: None,
                modified_at: node.attr("dt").map(str::to_owned),
                position,
                x: None,
                y: None,
            }
        })
        .collect())
}

pub(super) fn modern_authors(
    xml: &[u8],
    limits: &ParseLimits,
    context: &str,
) -> Result<HashMap<String, CommentAuthor>, ParseError> {
    modern::authors(xml, limits, context)
}

pub(super) fn is_modern(xml: &[u8]) -> bool {
    modern::is_root(xml, b"cmLst")
}

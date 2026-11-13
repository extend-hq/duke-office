use super::{normalized_xml, CommentAuthor};
use crate::pptx::{
    collect_namespaces, parse_text_paragraphs, scoped_source, HashMap, ParseError, ParseLimits,
    Relationships, RunStyle, SlideComment, TextParseContext, OwnedNode, XmlBudget, XmlDocument,
};
use crate::{
    CommentAnchor, CommentAnchorKind, CommentAnchorPosition, CommentMoniker, CommentTask,
    ModernComment,
};
use quick_xml::{events::Event, name::ResolveResult, NsReader};
use std::collections::BTreeMap;

const MODERN: &str = "http://schemas.microsoft.com/office/powerpoint/2018/8/main";
const DRAWING_COMMAND: &str = "http://schemas.microsoft.com/office/drawing/2013/main/command";
const SLIDE_COMMAND: &str = "http://schemas.microsoft.com/office/powerpoint/2013/main/command";
const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const DRAWING_STRICT: &str = "http://purl.oclc.org/ooxml/drawingml/main";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const RELATIONSHIPS_STRICT: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";

struct Element {
    namespace: String,
    name: String,
    attributes: BTreeMap<String, String>,
    children: Vec<Element>,
    text: String,
    source_xml: Option<String>,
}

impl Element {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(String::as_str)
    }

    fn value(&self, name: &str) -> Option<String> {
        self.attr(name).map(str::to_owned)
    }

    fn child(&self, name: &str) -> Option<&Self> {
        self.children
            .iter()
            .find(|child| child.namespace == MODERN && child.name == name)
    }

    fn drawing_node(&self) -> XmlDocument {
        XmlDocument::from_owned(self.owned_drawing_node())
    }

    fn owned_drawing_node(&self) -> OwnedNode {
        let children = self
            .children
            .iter()
            .filter(|child| matches!(child.namespace.as_str(), DRAWING | DRAWING_STRICT))
            .map(Self::owned_drawing_node)
            .collect();
        let attributes = self
            .attributes
            .iter()
            .filter_map(|(key, value)| {
                let name = if !key.starts_with('{') {
                    key.as_str()
                } else if let Some(name) = key
                    .strip_prefix(&format!("{{{RELATIONSHIPS}}}"))
                    .or_else(|| key.strip_prefix(&format!("{{{RELATIONSHIPS_STRICT}}}")))
                {
                    name
                } else {
                    return None;
                };
                Some((name.to_owned(), value.clone()))
            })
            .collect();
        OwnedNode {
            source_xml: self.source_xml.clone(),
            name: self.name.clone(),
            attributes,
            children,
            text: self.text.clone(),
        }
    }

    fn moniker(&self) -> CommentMoniker {
        CommentMoniker {
            namespace: self.namespace.clone(),
            name: self.name.clone(),
            attributes: self.attributes.clone(),
            children: self.children.iter().map(Self::moniker).collect(),
            text: self.text.clone(),
        }
    }
}

fn namespace(value: ResolveResult<'_>) -> String {
    match value {
        ResolveResult::Bound(ns) => String::from_utf8_lossy(ns.as_ref()).into_owned(),
        ResolveResult::Unbound => String::new(),
        ResolveResult::Unknown(prefix) => format!("unbound:{}", String::from_utf8_lossy(&prefix)),
    }
}

pub(super) fn is_root(xml: &[u8], name: &[u8]) -> bool {
    let mut reader = NsReader::from_reader(xml);
    loop {
        match reader.read_resolved_event() {
            Ok((ns, Event::Start(start) | Event::Empty(start))) => {
                return start.local_name().as_ref() == name
                    && matches!(ns, ResolveResult::Bound(ns) if ns.as_ref() == MODERN.as_bytes());
            }
            Ok((_, Event::Eof)) | Err(_) => return false,
            _ => {}
        }
    }
}

fn tree(xml: &[u8], limits: &ParseLimits, context: &str) -> Result<Element, ParseError> {
    struct Frame {
        node: Element,
        start: usize,
        namespace_count: usize,
    }
    let normalized = normalized_xml(xml, limits)?;
    let xml = normalized.as_ref();
    let mut reader = NsReader::from_reader(xml);
    let mut stack: Vec<Frame> = Vec::new();
    let mut root = None;
    let mut namespaces = crate::pptx::NamespaceDeclarations::default();
    let mut budget = XmlBudget::default();
    let error = |message: String| ParseError::Corrupt(format!("{context}: {message}"));
    loop {
        let start_offset = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(|e| error(e.to_string()))?;
        let completed = match event {
            Event::Start(ref start) | Event::Empty(ref start) => {
                if stack.len() >= limits.max_xml_depth {
                    return Err(ParseError::ResourceLimit("XML nesting depth".into()));
                }
                budget.consume_node(start, limits)?;
                let namespace_count = namespaces.len();
                collect_namespaces(start, stack.len(), &mut namespaces);
                let (ns, name) = reader.resolve_element(start.name());
                let mut node = Element {
                    namespace: namespace(ns),
                    name: String::from_utf8_lossy(name.as_ref()).into_owned(),
                    attributes: BTreeMap::new(),
                    children: Vec::new(),
                    text: String::new(),
                    source_xml: None,
                };
                for attribute in start.attributes() {
                    let attribute = attribute.map_err(|e| error(e.to_string()))?;
                    if attribute.key.as_ref() == b"xmlns"
                        || attribute.key.as_ref().starts_with(b"xmlns:")
                    {
                        continue;
                    }
                    let (ns, name) = reader.resolve_attribute(attribute.key);
                    let ns = namespace(ns);
                    let name = String::from_utf8_lossy(name.as_ref());
                    let key = if ns.is_empty() {
                        name.into_owned()
                    } else {
                        format!("{{{ns}}}{name}")
                    };
                    let value = attribute
                        .decode_and_unescape_value(reader.decoder())
                        .map_err(|e| error(e.to_string()))?;
                    node.attributes.insert(key, value.into_owned());
                }
                let frame = Frame {
                    node,
                    start: start_offset,
                    namespace_count,
                };
                if matches!(event, Event::Start(_)) {
                    stack.push(frame);
                    None
                } else {
                    Some(frame)
                }
            }
            Event::End(_) => Some(
                stack
                    .pop()
                    .ok_or_else(|| error("unmatched closing tag".into()))?,
            ),
            Event::Text(text) => {
                let text = text.unescape().map_err(|e| error(e.to_string()))?;
                budget.consume_text(text.len(), limits)?;
                if let Some(frame) = stack.last_mut() {
                    frame.node.text.push_str(&text);
                } else if !text.trim().is_empty() {
                    return Err(error("text outside root".into()));
                }
                None
            }
            Event::CData(text) => {
                budget.consume_text(text.len(), limits)?;
                let text = std::str::from_utf8(&text).map_err(|e| error(e.to_string()))?;
                stack
                    .last_mut()
                    .ok_or_else(|| error("CDATA outside root".into()))?
                    .node
                    .text
                    .push_str(text);
                None
            }
            Event::Eof => break,
            _ => None,
        };
        if let Some(mut frame) = completed {
            let retain = (frame.node.namespace == MODERN && frame.node.name == "extLst")
                || (matches!(frame.node.namespace.as_str(), DRAWING | DRAWING_STRICT)
                    && frame.node.name == "fld");
            if retain {
                let wrapper = if frame.node.name == "fld" {
                    "textFieldSource"
                } else {
                    "commentExtensionSource"
                };
                let source = scoped_source(
                    &xml[frame.start..reader.buffer_position() as usize],
                    &mut namespaces,
                    wrapper,
                );
                frame.node.source_xml = Some(budget.retain_source_xml(&source, limits, context)?);
            }
            namespaces.truncate(frame.namespace_count);
            if let Some(parent) = stack.last_mut() {
                parent.node.children.push(frame.node);
            } else if root.replace(frame.node).is_some() {
                return Err(error("multiple XML roots".into()));
            }
        }
    }
    if !stack.is_empty() {
        return Err(error("unclosed XML element".into()));
    }
    root.ok_or_else(|| error("missing XML root".into()))
}

pub(super) fn authors(
    xml: &[u8],
    limits: &ParseLimits,
    context: &str,
) -> Result<HashMap<String, CommentAuthor>, ParseError> {
    if !is_root(xml, b"authorLst") {
        return Ok(HashMap::new());
    }
    let root = tree(xml, limits, context)?;
    Ok(root
        .children
        .iter()
        .filter(|node| node.namespace == MODERN && node.name == "author")
        .filter_map(|node| {
            Some((
                node.attr("id")?.trim().to_owned(),
                CommentAuthor {
                    name: node.value("name"),
                    initials: node.value("initials"),
                    user_id: node.value("userId"),
                    provider_id: node.value("providerId"),
                    extensions_xml: node
                        .child("extLst")
                        .and_then(|node| node.source_xml.clone()),
                },
            ))
        })
        .collect())
}

fn completion_fraction(value: &str) -> Option<f64> {
    let value = value.trim().strip_suffix('%')?.parse::<f64>().ok()? / 100.0;
    (value.is_finite() && (0.0..=1.0).contains(&value)).then_some(value)
}

fn coordinate_emu(value: &str) -> Option<f64> {
    let value = value.trim();
    if let Ok(integer) = value.parse::<i64>() {
        return (-27273042329600..=27273042316900)
            .contains(&integer)
            .then_some(integer as f64);
    }
    let (number, scale) = [
        ("mm", 36000.0),
        ("cm", 360000.0),
        ("in", 914400.0),
        ("pt", 12700.0),
        ("pc", 152400.0),
        ("pi", 152400.0),
    ]
    .into_iter()
    .find_map(|(unit, scale)| value.strip_suffix(unit).map(|number| (number, scale)))?;
    let result = number.parse::<f64>().ok()? * scale;
    result.is_finite().then_some(result)
}

fn comment(
    node: &Element,
    index: usize,
    context: &str,
    authors: &HashMap<String, CommentAuthor>,
    rels: &Relationships,
    reply: bool,
) -> SlideComment {
    let author_id = node.attr("authorId");
    let author = author_id.and_then(|id| authors.get(id.trim()));
    let body = node.child("txBody").map(Element::drawing_node);
    let text_context = TextParseContext {
        source_part: Some(context),
        presentation_text_style: None,
        shape_default_style: RunStyle::default(),
        colors: None,
        rels,
    };
    let paragraphs = parse_text_paragraphs(body.as_ref().map(XmlDocument::root), None, None, None, &text_context);
    let text = paragraphs
        .iter()
        .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    let anchors = if reply {
        Vec::new()
    } else {
        node.children
            .iter()
            .filter_map(|anchor| {
                let kind = match (anchor.namespace.as_str(), anchor.name.as_str()) {
                    (SLIDE_COMMAND, "sldMkLst") => CommentAnchorKind::Slide,
                    (DRAWING_COMMAND, "deMkLst") => CommentAnchorKind::DrawingElement,
                    (DRAWING_COMMAND, "txMkLst") => CommentAnchorKind::TextRange,
                    (MODERN, "unknownAnchor") => CommentAnchorKind::Unknown,
                    _ => return None,
                };
                Some(CommentAnchor {
                    kind,
                    monikers: anchor.children.iter().map(Element::moniker).collect(),
                })
            })
            .collect()
    };
    let position_emu = if reply {
        None
    } else {
        node.child("pos").and_then(|node| {
            Some(CommentAnchorPosition {
                x: coordinate_emu(node.attr("x")?)?,
                y: coordinate_emu(node.attr("y")?)?,
                source_x: node.attr("x")?.to_owned(),
                source_y: node.attr("y")?.to_owned(),
            })
        })
    };
    let replies = if reply {
        Vec::new()
    } else {
        node.child("replyLst")
            .map(|list| {
                list.children
                    .iter()
                    .filter(|node| node.namespace == MODERN && node.name == "reply")
                    .enumerate()
                    .map(|(index, node)| comment(node, index, context, authors, rels, true))
                    .collect()
            })
            .unwrap_or_default()
    };
    let task = (!reply).then(|| {
        let completion = node.attr("complete").unwrap_or("0%").to_owned();
        CommentTask {
            start_date: node.value("startDate"),
            due_date: node.value("dueDate"),
            assigned_to: node
                .attr("assignedTo")
                .unwrap_or_default()
                .split_ascii_whitespace()
                .map(str::to_owned)
                .collect(),
            title: node.attr("title").unwrap_or_default().to_owned(),
            completion_fraction: completion_fraction(&completion),
            completion,
        }
    });
    SlideComment {
        id: node.value("id").unwrap_or_else(|| {
            format!("{}-{}", if reply { "reply" } else { "comment" }, index + 1)
        }),
        text,
        paragraphs,
        replies,
        modern: Some(Box::new(ModernComment {
            status: node
                .attr("status")
                .unwrap_or("active")
                .split_ascii_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            author_user_id: author.and_then(|a| a.user_id.clone()),
            author_provider_id: author.and_then(|a| a.provider_id.clone()),
            author_extensions_xml: author.and_then(|a| a.extensions_xml.clone()),
            anchors,
            position_emu,
            task,
            extensions_xml: node
                .child("extLst")
                .and_then(|node| node.source_xml.clone()),
        })),
        author: author.and_then(|a| a.name.clone()),
        author_id: author_id.map(str::to_owned),
        author_initials: author.and_then(|a| a.initials.clone()),
        source_index: None,
        source_part: Some(context.to_owned()),
        created_at: node.value("created"),
        modified_at: None,
        position: None,
        x: None,
        y: None,
    }
}

pub(super) fn parse(
    xml: &[u8],
    limits: &ParseLimits,
    context: &str,
    authors: &HashMap<String, CommentAuthor>,
    rels: &Relationships,
) -> Result<Vec<SlideComment>, ParseError> {
    let root = tree(xml, limits, context)?;
    Ok(root
        .children
        .iter()
        .filter(|node| node.namespace == MODERN && node.name == "cm")
        .enumerate()
        .map(|(index, node)| comment(node, index, context, authors, rels, false))
        .collect())
}

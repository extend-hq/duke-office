use quick_xml::{events::{BytesStart, Event}, name::ResolveResult, NsReader};

use crate::{ConnectorConnections, ConnectorEndpoint, ObjectEditing, ObjectEditingKind as Kind, ObjectLocks};
use std::collections::VecDeque;

const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const STRICT_DRAWING: &str = "http://purl.oclc.org/ooxml/drawingml/main";
const DRAWING_2010: &str = "http://schemas.microsoft.com/office/drawing/2010/main";

const MAX_CACHE_BYTES: usize = 256 * 1024;
const MAX_CACHE_ENTRIES: usize = 8;

#[derive(Default)]
pub(super) struct Cache {
    entries: VecDeque<Box<ObjectEditing>>,
    bytes: usize,
}

impl Cache {
    fn cost(value: &ObjectEditing) -> usize {
        value.source_xml.capacity().saturating_add(std::mem::size_of::<ObjectEditing>())
    }

    pub(super) fn parse(&mut self, xml: &str) -> Option<Box<ObjectEditing>> {
        if let Some(index) = self.entries.iter().position(|value| value.source_xml == xml) {
            let value = self.entries.remove(index)?;
            let result = value.clone();
            self.entries.push_front(value);
            return Some(result);
        }
        let value = parse(xml)?;
        let cost = Self::cost(&value);
        if cost <= MAX_CACHE_BYTES {
            while self.entries.len() >= MAX_CACHE_ENTRIES || self.bytes + cost > MAX_CACHE_BYTES {
                let Some(removed) = self.entries.pop_back() else { break; };
                self.bytes -= Self::cost(&removed);
            }
            self.entries.push_front(value.clone());
            self.bytes += cost;
        }
        Some(value)
    }
}

fn host_namespace(value: &str) -> Option<&'static str> {
    match value {
        "http://schemas.openxmlformats.org/presentationml/2006/main" => Some("http://schemas.openxmlformats.org/presentationml/2006/main"),
        "http://purl.oclc.org/ooxml/presentationml/main" => Some("http://purl.oclc.org/ooxml/presentationml/main"),
        DRAWING => Some(DRAWING),
        STRICT_DRAWING => Some(STRICT_DRAWING),
        "http://schemas.microsoft.com/office/drawing/2008/diagram" => Some("http://schemas.microsoft.com/office/drawing/2008/diagram"),
        "http://schemas.microsoft.com/office/powerpoint/2010/main" => Some("http://schemas.microsoft.com/office/powerpoint/2010/main"),
        _ => None,
    }
}

fn names(kind: Kind) -> (&'static str, &'static str, &'static str) {
    match kind {
        Kind::Shape => ("nvSpPr", "cNvSpPr", "spLocks"),
        Kind::Connector => ("nvCxnSpPr", "cNvCxnSpPr", "cxnSpLocks"),
        Kind::Picture => ("nvPicPr", "cNvPicPr", "picLocks"),
        Kind::Group => ("nvGrpSpPr", "cNvGrpSpPr", "grpSpLocks"),
        Kind::GraphicFrame => ("nvGraphicFramePr", "cNvGraphicFramePr", "graphicFrameLocks"),
        Kind::ContentPart => ("nvContentPartPr", "cNvContentPartPr", "cpLocks"),
    }
}

#[derive(Clone, Copy, Default)]
pub(super) enum Scope {
    #[default]
    None,
    Object(Kind, &'static str),
    Nonvisual(Kind, &'static str),
    Properties(Kind),
}

impl Scope {
    pub(super) fn next(self, start: &BytesStart<'_>, namespaces: &[(usize, String, String)]) -> Self {
        let qualified = start.name();
        let bytes = qualified.as_ref();
        let colon = bytes.iter().position(|byte| *byte == b':');
        let prefix = colon.map(|index| &bytes[..index]).unwrap_or_default();
        let name = colon.map(|index| &bytes[index + 1..]).unwrap_or(bytes);
        let kind = match name {
            b"sp" => Some(Kind::Shape), b"cxnSp" => Some(Kind::Connector),
            b"pic" => Some(Kind::Picture), b"grpSp" | b"spTree" => Some(Kind::Group),
            b"graphicFrame" => Some(Kind::GraphicFrame), b"contentPart" => Some(Kind::ContentPart),
            _ => None,
        };
        if kind.is_none() && !matches!(self,
            Self::Object(kind, _) if name == names(kind).0.as_bytes())
            && !matches!(self, Self::Nonvisual(kind, _) if name == names(kind).1.as_bytes()) {
            return Self::None;
        }
        let Some(namespace) = namespaces.iter().rev()
            .find(|(_, candidate, _)| candidate.as_bytes() == prefix)
            .and_then(|(_, _, uri)| host_namespace(uri)) else { return Self::None; };
        if let Some(kind) = kind { return Self::Object(kind, namespace); }
        match self {
            Self::Object(kind, parent) if parent == namespace && name == names(kind).0.as_bytes() => Self::Nonvisual(kind, namespace),
            Self::Nonvisual(kind, parent) if parent == namespace && name == names(kind).1.as_bytes() => Self::Properties(kind),
            _ => Self::None,
        }
    }

    pub(super) fn is_properties(self) -> bool { matches!(self, Self::Properties(_)) }

    pub(super) fn retain(self, has_children: bool, has_attribute: impl Fn(&str) -> bool) -> bool {
        let Self::Properties(kind) = self else { return false; };
        kind == Kind::Connector || has_children
            || has_attribute("txBox") || has_attribute("preferRelativeResize")
            || has_attribute("isComment")
    }
}

fn boolean(value: &str) -> Option<bool> {
    match value.trim_matches([' ', '\t', '\r', '\n']) {
        "true" | "1" => Some(true), "false" | "0" => Some(false), _ => None,
    }
}

fn uint(value: &str) -> Option<u32> {
    let value = value.trim_matches([' ', '\t', '\r', '\n']);
    let negative = value.starts_with('-');
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) { return None; }
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() { return Some(0); }
    if negative { return None; }
    digits.parse().ok()
}

fn attributes(start: &BytesStart<'_>, mut read: impl FnMut(&[u8], &str)) {
    for attribute in start.attributes().with_checks(false).flatten() {
        if attribute.key.as_ref().contains(&b':') { continue; }
        if let Ok(value) = attribute.unescape_value() { read(attribute.key.as_ref(), &value); }
    }
}

fn locks(start: &BytesStart<'_>, kind: Kind) -> ObjectLocks {
    let mut result = ObjectLocks::default();
    let common = !matches!(kind, Kind::Group | Kind::GraphicFrame);
    attributes(start, |name, value| {
        let target = match name {
            b"noGrp" => &mut result.no_grouping,
            b"noSelect" => &mut result.no_selection,
            b"noChangeAspect" => &mut result.no_change_aspect,
            b"noMove" => &mut result.no_move,
            b"noResize" => &mut result.no_resize,
            b"noRot" if kind != Kind::GraphicFrame => &mut result.no_rotation,
            b"noEditPoints" if common => &mut result.no_edit_points,
            b"noAdjustHandles" if common => &mut result.no_adjust_handles,
            b"noChangeArrowheads" if common => &mut result.no_change_arrowheads,
            b"noChangeShapeType" if common => &mut result.no_change_shape_type,
            b"noTextEdit" if kind == Kind::Shape => &mut result.no_text_edit,
            b"noCrop" if kind == Kind::Picture => &mut result.no_crop,
            b"noUngrp" if kind == Kind::Group => &mut result.no_ungrouping,
            b"noDrilldown" if kind == Kind::GraphicFrame => &mut result.no_drilldown,
            _ => return,
        };
        *target = boolean(value);
    });
    result
}

fn endpoint(start: &BytesStart<'_>) -> Option<ConnectorEndpoint> {
    let (mut shape_id, mut connection_site) = (None, None);
    attributes(start, |name, value| match name {
        b"id" => shape_id = uint(value), b"idx" => connection_site = uint(value), _ => {},
    });
    Some(ConnectorEndpoint { shape_id: shape_id?, connection_site: connection_site? })
}

pub(super) fn parse(xml: &str) -> Option<Box<ObjectEditing>> {
    let mut reader = NsReader::from_str(xml);
    let mut depth = 0usize;
    let mut result: Option<ObjectEditing> = None;
    let mut meaningful = false;
    loop {
        let (namespace, event) = reader.read_resolved_event().ok()?;
        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(start) | Event::Empty(start) => {
                let local = start.local_name();
                let name = local.as_ref();
                let uri = match &namespace { ResolveResult::Bound(uri) => std::str::from_utf8(uri.as_ref()).ok(), _ => None };
                if depth == 1 && uri.and_then(host_namespace).is_some() {
                    let kind = [Kind::Shape, Kind::Connector, Kind::Picture, Kind::Group, Kind::GraphicFrame, Kind::ContentPart]
                        .into_iter().find(|&kind| names(kind).1.as_bytes() == name)?;
                    let mut value = ObjectEditing { kind, locks: None,
                        connector: (kind == Kind::Connector).then(ConnectorConnections::default),
                        text_box: None, prefer_relative_resize: None, is_comment: None, source_xml: xml.to_owned() };
                    meaningful |= kind == Kind::Connector;
                    attributes(&start, |name, raw| {
                        let target = match name {
                            b"txBox" if kind == Kind::Shape => &mut value.text_box,
                            b"preferRelativeResize" if kind == Kind::Picture => &mut value.prefer_relative_resize,
                            b"isComment" if kind == Kind::ContentPart => &mut value.is_comment,
                            _ => return,
                        };
                        *target = boolean(raw);
                        meaningful |= target.is_some();
                    });
                    result = Some(value);
                } else if depth == 2 {
                    if let Some(value) = result.as_mut() {
                        let drawing = matches!(uri, Some(DRAWING | STRICT_DRAWING));
                        let content = value.kind == Kind::ContentPart && uri == Some(DRAWING_2010);
                        if (drawing || content) && name == names(value.kind).2.as_bytes() {
                            value.locks = Some(locks(&start, value.kind));
                            meaningful = true;
                        } else if drawing && name == b"extLst" {
                            meaningful = true;
                        } else if drawing {
                            if let Some(connector) = value.connector.as_mut() {
                                match name { b"stCxn" => connector.start = endpoint(&start),
                                    b"endCxn" => connector.end = endpoint(&start), _ => {} }
                            }
                        }
                    }
                }
                if !empty { depth += 1; }
            }
            Event::End(_) => depth = depth.checked_sub(1)?,
            Event::Eof => break,
            _ => {},
        }
    }
    meaningful.then_some(result?).map(Box::new)
}

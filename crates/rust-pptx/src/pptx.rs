#[path = "pptx_comments.rs"]
mod comments;
#[path = "pptx_fields.rs"]
mod fields;
#[path = "pptx_paint.rs"]
mod paint;
#[path = "pptx_timing.rs"]
mod timing;

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    fmt::Write as _,
    io::{Cursor, Read},
    rc::Rc,
    sync::OnceLock,
};

#[cfg(all(target_arch = "wasm32", not(target_feature = "simd128")))]
use base64::Engine;
use compact_str::CompactString;
use quick_xml::{
    events::{BytesStart, Event},
    name::ResolveResult,
    NsReader, Reader,
};
use zip::ZipArchive;

use crate::{
    ChartSeries, ColorValue, FillStyle, GradientStop, LineStyle, ParseError, ParseLimits,
    ParseOptions, PresentationAsset, PresentationDocument, PresentationEmbeddedFont,
    PresentationFormat, PresentationMetadata, PresentationSize, PresentationSlide,
    PresentationWarning, ShapeGeometry, SlideComment, SlideNode, SlideNote, TableCell,
    TextParagraph, TextRun, Transform, VerticalAlignment,
};

const DEFAULT_WIDTH: u64 = 9_144_000;
const DEFAULT_HEIGHT: u64 = 6_858_000;

#[path = "pptx_editing.rs"]
mod editing;
#[path = "pptx_xml.rs"]
mod xml_tree;
use xml_tree::{OwnedNode, RawAttribute, RawNode, StrRef, XmlDocument, XmlNode, NONE};

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}

fn attr(start: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    start
        .attributes()
        .with_checks(false)
        .flatten()
        .find_map(|value| {
            (local_name(value.key.as_ref()) == name)
                .then(|| value.unescape_value().ok().map(|value| value.into_owned()))
                .flatten()
        })
}

fn normalize_target(base: &str, target: &str) -> String {
    if target.contains("://")
        || target.starts_with("mailto:")
        || target.starts_with("tel:")
        || target.starts_with('#')
    {
        return target.to_owned();
    }
    // OPC relationship targets may be package-absolute.
    let (base, target) = match target.strip_prefix('/') {
        Some(absolute) => ("", absolute),
        None => (base, target),
    };
    let mut parts: Vec<&str> = base.split('/').collect();
    parts.pop();
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            value => parts.push(value),
        }
    }
    parts.join("/")
}

fn read_entry(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    name: &str,
    limits: &ParseLimits,
) -> Result<Option<Vec<u8>>, ParseError> {
    let Ok(mut entry) = archive.by_name(name) else {
        return Ok(None);
    };
    if entry.size() as usize > limits.max_entry_bytes {
        return Err(ParseError::ResourceLimit(format!(
            "entry {name} exceeds size limit"
        )));
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .read_to_end(&mut bytes)
        .map_err(|error| ParseError::Corrupt(error.to_string()))?;
    Ok(Some(bytes))
}

const CHART_STYLE_REL_TYPE: &str =
    "http://schemas.microsoft.com/office/2011/relationships/chartStyle";
const CHART_COLOR_STYLE_REL_TYPE: &str =
    "http://schemas.microsoft.com/office/2011/relationships/chartColorStyle";

fn office_relationship(kind: &str, name: &str) -> bool {
    kind.strip_prefix("http://schemas.openxmlformats.org/officeDocument/2006/relationships/")
        .or_else(|| kind.strip_prefix("http://purl.oclc.org/ooxml/officeDocument/relationships/"))
        == Some(name)
}

fn is_chart_relationship_type(relationship_type: &str) -> bool {
    matches!(
        relationship_type.rsplit('/').next(),
        Some("chart" | "chartEx")
    )
}

fn is_diagram_relationship_type(relationship_type: &str) -> bool {
    relationship_type
        .rsplit('/')
        .next()
        .is_some_and(|name| name.starts_with("diagram"))
}

/// Companion Microsoft chart style parts resolved from a chart part's rels.
#[derive(Debug, Clone, Default)]
struct ChartCompanionParts {
    style_xml: Option<String>,
    colors_xml: Option<String>,
    theme_override_xml: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct Relationship {
    target: String,
    kind: CompactString,
    external: bool,
    order: usize,
}

#[derive(Debug, Clone, Default)]
struct Relationships {
    entries: HashMap<String, Relationship>,
    /// Shape-paint cache key for this part, built on first use; `entries` is final by then.
    paint_key: OnceLock<paint::RelationshipKey>,
}

impl Relationships {
    fn paint_key(&self) -> &paint::RelationshipKey {
        self.paint_key
            .get_or_init(|| paint::RelationshipKey::new(self.iter()))
    }

    fn get(&self, id: &str) -> Option<&String> {
        self.entries.get(id).map(|entry| &entry.target)
    }

    fn contains_key(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }

    fn is_external(&self, id: &str) -> bool {
        self.entries.get(id).is_some_and(|entry| entry.external)
    }

    fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.entries.iter().map(|(id, entry)| (id, &entry.target))
    }

    fn values(&self) -> impl Iterator<Item = &String> {
        self.entries
            .values()
            .filter(|entry| !entry.external)
            .map(|entry| &entry.target)
    }

    fn typed(&self) -> impl Iterator<Item = (&str, &String)> {
        self.entries
            .values()
            .filter(|entry| !entry.external && !entry.kind.is_empty())
            .map(|entry| (entry.kind.as_str(), &entry.target))
    }

    fn paths<'a>(
        &'a self,
        kind: &'a str,
        legacy_prefix: &'a str,
    ) -> impl Iterator<Item = &'a String> {
        self.entries
            .values()
            .filter(move |entry| {
                !entry.external
                    && (office_relationship(&entry.kind, kind)
                        || (entry.kind.is_empty() && entry.target.starts_with(legacy_prefix)))
            })
            .map(|entry| &entry.target)
    }

    fn related(&self, kind: &str, legacy_prefix: &str) -> Option<&String> {
        self.entries
            .values()
            .filter(|entry| !entry.external && office_relationship(&entry.kind, kind))
            .min_by_key(|entry| entry.order)
            .map(|entry| &entry.target)
            .or_else(|| {
                self.entries
                    .values()
                    .filter(|entry| {
                        !entry.external
                            && entry.kind.is_empty()
                            && entry.target.starts_with(legacy_prefix)
                    })
                    .min_by_key(|entry| entry.order)
                    .map(|entry| &entry.target)
            })
    }

    fn asset_paths(&self) -> impl Iterator<Item = &String> {
        self.entries
            .values()
            .filter(|entry| {
                !entry.external
                    && (entry.target.starts_with("ppt/media/")
                        || ["image", "audio", "video"]
                            .iter()
                            .any(|kind| office_relationship(&entry.kind, kind)))
            })
            .map(|entry| &entry.target)
    }

    fn internal(&self, id: &str) -> Option<&String> {
        self.entries
            .get(id)
            .filter(|entry| !entry.external)
            .map(|entry| &entry.target)
    }
}

fn relationships(xml: &[u8], base: &str, max_depth: usize) -> Result<Relationships, ParseError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut result = Relationships::default();
    let mut depth = 0usize;
    loop {
        match reader.read_event() {
            Ok(event @ (Event::Start(_) | Event::Empty(_))) => {
                if matches!(event, Event::Start(_)) {
                    depth += 1;
                    if depth > max_depth {
                        return Err(ParseError::ResourceLimit("XML nesting depth".into()));
                    }
                }
                let start = match event {
                    Event::Start(start) | Event::Empty(start) => start,
                    _ => unreachable!(),
                };
                if local_name(start.name().as_ref()) == b"Relationship" {
                    let (mut id, mut target, mut kind, mut external) = (None, None, None, false);
                    for attribute in start.attributes().with_checks(false).flatten() {
                        let slot = match local_name(attribute.key.as_ref()) {
                            b"Id" => &mut id,
                            b"Target" => &mut target,
                            b"Type" => &mut kind,
                            b"TargetMode" => {
                                external =
                                    attribute.unescape_value().ok().as_deref() == Some("External");
                                continue;
                            }
                            _ => continue,
                        };
                        if slot.is_none() {
                            *slot = attribute.unescape_value().ok();
                        }
                    }
                    if let (Some(id), Some(target)) = (id, target) {
                        let entry = Relationship {
                            target: if external {
                                target.into_owned()
                            } else {
                                normalize_target(base, &target)
                            },
                            kind: kind.as_deref().unwrap_or_default().into(),
                            external,
                            order: result.entries.len(),
                        };
                        result.entries.insert(id.into_owned(), entry);
                    }
                }
            }
            Ok(Event::End(_)) => depth = depth.saturating_sub(1),
            Ok(Event::Eof) => break,
            Err(error) => return Err(ParseError::Corrupt(format!("relationship XML: {error}"))),
            _ => {}
        }
    }
    Ok(result)
}

fn parse_presentation_xml(
    xml: &[u8],
    rels: &Relationships,
    limits: &ParseLimits,
) -> Result<(PresentationSize, Vec<String>), ParseError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut size = PresentationSize {
        width_emu: DEFAULT_WIDTH,
        height_emu: DEFAULT_HEIGHT,
    };
    let mut slides = Vec::new();
    let mut depth = 0usize;
    loop {
        match reader.read_event() {
            Ok(Event::Start(start)) => {
                depth += 1;
                if depth > limits.max_xml_depth {
                    return Err(ParseError::ResourceLimit("XML nesting depth".into()));
                }
                match local_name(start.name().as_ref()) {
                    b"sldSz" => {
                        size.width_emu = attr(&start, b"cx")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(DEFAULT_WIDTH);
                        size.height_emu = attr(&start, b"cy")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(DEFAULT_HEIGHT);
                    }
                    b"sldId" => {
                        if let Some(id) =
                            attr(&start, b"id")
                                .filter(|value| rels.contains_key(value))
                                .or_else(|| {
                                    start.attributes().with_checks(false).flatten().find_map(
                                        |value| {
                                            let candidate =
                                                String::from_utf8_lossy(value.value.as_ref())
                                                    .into_owned();
                                            (local_name(value.key.as_ref()) == b"id"
                                                && rels.contains_key(&candidate))
                                            .then_some(candidate)
                                        },
                                    )
                                })
                        {
                            if let Some(target) = rels.internal(&id) {
                                slides.push(target.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(start)) => match local_name(start.name().as_ref()) {
                b"sldSz" => {
                    size.width_emu = attr(&start, b"cx")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(DEFAULT_WIDTH);
                    size.height_emu = attr(&start, b"cy")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(DEFAULT_HEIGHT);
                }
                b"sldId" => {
                    if let Some(id) = attr(&start, b"id")
                        .filter(|value| rels.contains_key(value))
                        .or_else(|| {
                            start
                                .attributes()
                                .with_checks(false)
                                .flatten()
                                .find_map(|value| {
                                    let candidate =
                                        String::from_utf8_lossy(value.value.as_ref()).into_owned();
                                    (local_name(value.key.as_ref()) == b"id"
                                        && rels.contains_key(&candidate))
                                    .then_some(candidate)
                                })
                        })
                    {
                        if let Some(target) = rels.internal(&id) {
                            slides.push(target.clone());
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::End(_)) => depth = depth.saturating_sub(1),
            Ok(Event::Eof) => break,
            Err(error) => return Err(ParseError::Corrupt(format!("presentation XML: {error}"))),
            _ => {}
        }
    }
    Ok((size, slides))
}

#[derive(Debug)]
struct EmbeddedFontReference {
    family: String,
    variants: Vec<(String, String, String)>,
}

fn parse_embedded_font_references(xml: &[u8]) -> Vec<EmbeddedFontReference> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut current: Option<EmbeddedFontReference> = None;
    let mut fonts = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(start)) => match local_name(start.name().as_ref()) {
                b"embeddedFont" => {
                    current = Some(EmbeddedFontReference {
                        family: String::new(),
                        variants: Vec::new(),
                    });
                }
                b"font" => {
                    if let Some(font) = current.as_mut() {
                        font.family = attr(&start, b"typeface").unwrap_or_default();
                    }
                }
                b"regular" | b"bold" | b"italic" | b"boldItalic" => {
                    if let (Some(font), Some(rel_id)) = (current.as_mut(), attr(&start, b"id")) {
                        let (style, weight) = match local_name(start.name().as_ref()) {
                            b"bold" => ("normal", "700"),
                            b"italic" => ("italic", "400"),
                            b"boldItalic" => ("italic", "700"),
                            _ => ("normal", "400"),
                        };
                        font.variants
                            .push((rel_id, style.to_owned(), weight.to_owned()));
                    }
                }
                _ => {}
            },
            Ok(Event::Empty(start)) => match local_name(start.name().as_ref()) {
                b"font" => {
                    if let Some(font) = current.as_mut() {
                        font.family = attr(&start, b"typeface").unwrap_or_default();
                    }
                }
                b"regular" | b"bold" | b"italic" | b"boldItalic" => {
                    if let (Some(font), Some(rel_id)) = (current.as_mut(), attr(&start, b"id")) {
                        let (style, weight) = match local_name(start.name().as_ref()) {
                            b"bold" => ("normal", "700"),
                            b"italic" => ("italic", "400"),
                            b"boldItalic" => ("italic", "700"),
                            _ => ("normal", "400"),
                        };
                        font.variants
                            .push((rel_id, style.to_owned(), weight.to_owned()));
                    }
                }
                _ => {}
            },
            Ok(Event::End(end)) if local_name(end.name().as_ref()) == b"embeddedFont" => {
                if let Some(font) = current.take().filter(|font| !font.family.is_empty()) {
                    fonts.push(font);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    fonts
}

fn font_magic(data: &[u8]) -> Option<&'static str> {
    match data.get(..4) {
        Some(b"OTTO") => Some("font/otf"),
        Some(b"\x00\x01\x00\x00") | Some(b"true") | Some(b"typ1") => Some("font/ttf"),
        Some(b"wOFF") => Some("font/woff"),
        Some(b"wOF2") => Some("font/woff2"),
        _ => None,
    }
}

fn deobfuscate_embedded_font(path: &str, data: &mut [u8]) -> bool {
    if font_magic(data).is_some() {
        return true;
    }
    let stem = path
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .split('.')
        .next()
        .unwrap_or_default();
    let hex: String = stem
        .chars()
        .filter(|value| value.is_ascii_hexdigit())
        .collect();
    if hex.len() != 32 || data.len() < 32 {
        return false;
    }
    let mut key = Vec::with_capacity(16);
    for index in (0..32).step_by(2) {
        let Ok(value) = u8::from_str_radix(&hex[index..index + 2], 16) else {
            return false;
        };
        key.push(value);
    }
    key.reverse();
    for index in 0..32 {
        data[index] ^= key[index % 16];
    }
    font_magic(data).is_some()
}

#[derive(Default)]
struct XmlBudget {
    nodes: usize,
    attributes: usize,
    text_bytes: usize,
    attribute_bytes: usize,
    source_xml_bytes: usize,
}

impl XmlBudget {
    fn consume_node(
        &mut self,
        start: &BytesStart<'_>,
        limits: &ParseLimits,
    ) -> Result<(), ParseError> {
        self.consume_node_count(limits)?;
        for attribute in start.attributes().with_checks(false).flatten() {
            self.consume_attribute(&attribute);
        }
        self.check_attributes(limits)
    }

    fn consume_node_count(&mut self, limits: &ParseLimits) -> Result<(), ParseError> {
        self.nodes = self.nodes.saturating_add(1);
        if self.nodes > limits.max_xml_nodes {
            return Err(ParseError::ResourceLimit("XML node count".into()));
        }
        Ok(())
    }

    fn consume_attribute(&mut self, attribute: &quick_xml::events::attributes::Attribute<'_>) {
        self.attributes = self.attributes.saturating_add(1);
        self.attribute_bytes = self
            .attribute_bytes
            .saturating_add(attribute.key.as_ref().len())
            .saturating_add(attribute.value.as_ref().len());
    }

    fn check_attributes(&self, limits: &ParseLimits) -> Result<(), ParseError> {
        if self.attributes > limits.max_xml_attributes {
            return Err(ParseError::ResourceLimit("XML attribute count".into()));
        }
        if self.attribute_bytes > limits.max_xml_attribute_bytes {
            return Err(ParseError::ResourceLimit("XML attribute bytes".into()));
        }
        Ok(())
    }

    fn consume_text(&mut self, bytes: usize, limits: &ParseLimits) -> Result<(), ParseError> {
        self.text_bytes = self.text_bytes.saturating_add(bytes);
        if self.text_bytes.saturating_add(self.source_xml_bytes) > limits.max_xml_text_bytes {
            return Err(ParseError::ResourceLimit("XML text bytes".into()));
        }
        Ok(())
    }

    fn retain_source_xml(
        &mut self,
        source: &[u8],
        limits: &ParseLimits,
        context: &str,
    ) -> Result<String, ParseError> {
        self.account_source_xml(source.len(), limits)?;
        let source = std::str::from_utf8(source)
            .map_err(|error| ParseError::Corrupt(format!("{context}: {error}")))?;
        Ok(source.to_owned())
    }

    fn account_source_xml(&mut self, bytes: usize, limits: &ParseLimits) -> Result<(), ParseError> {
        self.source_xml_bytes = self.source_xml_bytes.saturating_add(bytes);
        if self.text_bytes.saturating_add(self.source_xml_bytes) > limits.max_xml_text_bytes {
            return Err(ParseError::ResourceLimit(
                "XML retained source bytes".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Default)]
struct NamespaceDeclarations {
    values: Vec<(usize, String, String)>,
    encoded: Option<Vec<u8>>,
}

impl NamespaceDeclarations {
    fn len(&self) -> usize {
        self.values.len()
    }

    fn push(&mut self, value: (usize, String, String)) {
        self.values.push(value);
        self.encoded = None;
    }

    fn truncate(&mut self, length: usize) {
        if length < self.values.len() {
            self.values.truncate(length);
            self.encoded = None;
        }
    }

    fn encoded(&mut self) -> &[u8] {
        let values = &self.values;
        self.encoded.get_or_insert_with(|| {
            let active: BTreeMap<_, _> = values
                .iter()
                .map(|(_, prefix, uri)| (prefix, uri))
                .collect();
            let mut result = String::new();
            for (prefix, uri) in active {
                if prefix.is_empty() {
                    result.push_str(" xmlns=\"");
                } else {
                    let _ = write!(result, " xmlns:{prefix}=\"");
                }
                result.push_str(&quick_xml::escape::escape(uri));
                result.push('"');
            }
            result.into_bytes()
        })
    }
}

/// Whether raw tag bytes contain `xmlns`, without building a substring searcher per tag.
fn mentions_xmlns(tag: &[u8]) -> bool {
    let mut rest = tag;
    while let Some(position) = memchr::memchr(b'x', rest) {
        rest = &rest[position..];
        if rest.starts_with(b"xmlns") {
            return true;
        }
        rest = &rest[1..];
    }
    false
}

fn collect_namespaces(
    start: &BytesStart<'_>,
    depth: usize,
    namespaces: &mut NamespaceDeclarations,
) {
    if !mentions_xmlns(start.as_ref()) {
        return;
    }
    for attribute in start.attributes().with_checks(false).flatten() {
        let key = attribute.key.as_ref();
        let prefix = if key == b"xmlns" {
            Some(&b""[..])
        } else {
            key.strip_prefix(b"xmlns:")
        };
        if let (Some(prefix), Ok(value)) = (prefix, attribute.unescape_value()) {
            namespaces.push((
                depth,
                String::from_utf8_lossy(prefix).into_owned(),
                value.into_owned(),
            ));
        }
    }
}

fn scoped_source(source: &[u8], namespaces: &mut NamespaceDeclarations, wrapper_name: &str) -> Vec<u8> {
    let declarations = namespaces.encoded();
    let mut result = Vec::with_capacity(source.len() + declarations.len() + wrapper_name.len() * 2 + 5);
    result.push(b'<');
    result.extend_from_slice(wrapper_name.as_bytes());
    result.extend_from_slice(declarations);
    result.push(b'>');
    result.extend_from_slice(source);
    result.extend_from_slice(b"</");
    result.extend_from_slice(wrapper_name.as_bytes());
    result.push(b'>');
    result
}

#[derive(Clone, Copy, PartialEq)]
enum ImageExtensionScope {
    None,
    Blip,
    List,
    Extension,
}

fn image_extension_scope(
    start: &BytesStart<'_>,
    parent: ImageExtensionScope,
    namespaces: &[(usize, String, String)],
) -> ImageExtensionScope {
    let next = match (local_name(start.name().as_ref()), parent) {
        (b"blip", _) => ImageExtensionScope::Blip,
        (b"extLst", ImageExtensionScope::Blip) => ImageExtensionScope::List,
        (b"ext", ImageExtensionScope::List) => ImageExtensionScope::Extension,
        _ => return ImageExtensionScope::None,
    };
    let name = start.name();
    let prefix = name
        .as_ref()
        .iter()
        .position(|byte| *byte == b':')
        .map(|end| &name.as_ref()[..end])
        .unwrap_or_default();
    let namespace = namespaces
        .iter()
        .rev()
        .find(|(_, candidate, _)| candidate.as_bytes() == prefix);
    if namespace.is_some_and(|(_, _, uri)| {
        matches!(
            uri.as_str(),
            "http://schemas.openxmlformats.org/drawingml/2006/main"
                | "http://purl.oclc.org/ooxml/drawingml/main"
        )
    }) {
        next
    } else {
        ImageExtensionScope::None
    }
}

/// Builds the flat node tables for one part while the part bytes are borrowed.
struct TreeBuilder<'x> {
    xml: &'x [u8],
    borrowable: bool,
    decoder: quick_xml::encoding::Decoder,
    context: &'x str,
    nodes: Vec<RawNode>,
    attributes: Vec<RawAttribute>,
    strings: Vec<String>,
}

impl<'x> TreeBuilder<'x> {
    fn buffer_ref(&self, bytes: &[u8]) -> Option<StrRef> {
        if !self.borrowable {
            return None;
        }
        let start = (bytes.as_ptr() as usize).checked_sub(self.xml.as_ptr() as usize)?;
        let end = start.checked_add(bytes.len())?;
        (end <= self.xml.len()).then(|| StrRef::Buffer {
            start: start as u32,
            len: bytes.len() as u32,
        })
    }

    fn intern(&mut self, value: String) -> StrRef {
        self.strings.push(value);
        StrRef::Owned(self.strings.len() as u32 - 1)
    }

    /// Stores raw bytes that are known to be valid UTF-8 when borrowable.
    fn name_ref(&mut self, bytes: &[u8]) -> StrRef {
        match self.buffer_ref(bytes) {
            Some(value) => value,
            None => self.intern(String::from_utf8_lossy(bytes).into_owned()),
        }
    }

    fn resolve(&self, value: StrRef) -> &str {
        match value {
            StrRef::None => "",
            StrRef::Buffer { start, len } => {
                let bytes = &self.xml[start as usize..start as usize + len as usize];
                debug_assert!(std::str::from_utf8(bytes).is_ok());
                // SAFETY: see `XmlDocument::resolve`; the builder only creates
                // buffer references for borrowable (UTF-8 validated) parts at
                // ASCII delimiter boundaries.
                unsafe { std::str::from_utf8_unchecked(bytes) }
            }
            StrRef::Owned(index) => self.strings[index as usize].as_str(),
        }
    }

    fn push_start(
        &mut self,
        start: &BytesStart<'_>,
        budget: &mut XmlBudget,
        limits: &ParseLimits,
    ) -> Result<u32, ParseError> {
        budget.consume_node_count(limits)?;
        // Raw tag bytes bound both attribute count and total raw attribute bytes.
        // Near a limit, reject before decoding or allocating attribute values.
        let preflight = start.len() > limits.max_xml_attributes.saturating_sub(budget.attributes)
            || start.len()
                > limits
                    .max_xml_attribute_bytes
                    .saturating_sub(budget.attribute_bytes);
        if preflight {
            for attribute in start.attributes().with_checks(false).flatten() {
                budget.consume_attribute(&attribute);
            }
            budget.check_attributes(limits)?;
        }
        let attributes_start = self.attributes.len() as u32;
        for attribute in start.attributes().with_checks(false).flatten() {
            if !preflight {
                budget.consume_attribute(&attribute);
            }
            let key = attribute.key.as_ref();
            let local = local_name(key);
            let prefixed = key.len() != local.len();
            let name = self.name_ref(local);
            let raw = attribute.value.as_ref();
            let value = if self.borrowable && memchr::memchr(b'&', raw).is_none() {
                self.name_ref(raw)
            } else {
                let decoded = attribute
                    .decode_and_unescape_value(self.decoder)
                    .map_err(|error| ParseError::Corrupt(format!("{}: {error}", self.context)))?;
                self.intern(decoded.into_owned())
            };
            self.attributes.push(RawAttribute {
                name,
                value,
                prefixed,
            });
        }
        let attributes_len = self.attributes.len() as u32 - attributes_start;
        let qualified = start.name();
        let local = local_name(qualified.as_ref());
        let name = self.name_ref(local);
        let index = self.nodes.len() as u32;
        self.nodes.push(RawNode::new(
            name,
            attributes_start,
            attributes_len,
            XmlNode::name_bit(local),
        ));
        Ok(index)
    }

    fn link_child(&mut self, parent: Option<&mut (u32, u32)>, index: u32) {
        if let Some((parent_index, last_child)) = parent {
            if *last_child == NONE {
                self.nodes[*parent_index as usize].first_child = index;
            } else {
                self.nodes[*last_child as usize].next_sibling = index;
            }
            *last_child = index;
        }
    }

    fn has_attribute(&self, index: u32, name: &str) -> bool {
        let node = &self.nodes[index as usize];
        let range = node.attributes_start as usize
            ..(node.attributes_start + node.attributes_len) as usize;
        self.attributes[range]
            .iter()
            .any(|attribute| self.resolve(attribute.name) == name)
    }

    /// Appends raw part bytes (valid UTF-8 in a borrowable part) to a node's text.
    fn append_raw_text(&mut self, index: u32, raw: &[u8]) {
        match self.nodes[index as usize].text {
            StrRef::None => {
                let value = self.name_ref(raw);
                self.nodes[index as usize].text = value;
            }
            existing => {
                let piece = String::from_utf8_lossy(raw).into_owned();
                self.append_owned_text(index, existing, &piece);
            }
        }
    }

    fn append_text(&mut self, index: u32, piece: &str) {
        match self.nodes[index as usize].text {
            StrRef::None => {
                let value = self.intern(piece.to_owned());
                self.nodes[index as usize].text = value;
            }
            existing => self.append_owned_text(index, existing, piece),
        }
    }

    fn append_owned_text(&mut self, index: u32, existing: StrRef, piece: &str) {
        if let StrRef::Owned(string) = existing {
            self.strings[string as usize].push_str(piece);
        } else {
            let mut value = self.resolve(existing).to_owned();
            value.push_str(piece);
            let value = self.intern(value);
            self.nodes[index as usize].text = value;
        }
    }

    fn retain_source(
        &mut self,
        index: u32,
        source: &[u8],
        qualified: Option<Vec<u8>>,
        budget: &mut XmlBudget,
        limits: &ParseLimits,
    ) -> Result<(), ParseError> {
        let value = match qualified {
            Some(bytes) => {
                budget.account_source_xml(bytes.len(), limits)?;
                let text = String::from_utf8(bytes).map_err(|error| {
                    ParseError::Corrupt(format!("{}: {}", self.context, error.utf8_error()))
                })?;
                self.intern(text)
            }
            None => {
                budget.account_source_xml(source.len(), limits)?;
                match self.buffer_ref(source) {
                    Some(value) => value,
                    None => {
                        let text = std::str::from_utf8(source).map_err(|error| {
                            ParseError::Corrupt(format!("{}: {error}", self.context))
                        })?;
                        self.intern(text.to_owned())
                    }
                }
            }
        };
        self.nodes[index as usize].source_xml = value;
        Ok(())
    }
}

fn qualified_source(
    name: &str,
    is_properties: bool,
    source: &[u8],
    namespaces: &mut NamespaceDeclarations,
) -> Option<Vec<u8>> {
    let wrapper = if is_properties {
        "nonvisualSource"
    } else if matches!(name, "oMath" | "oMathPara") {
        "officeMathSource"
    } else if name == "fld" {
        "textFieldSource"
    } else if name == "svgBlip" {
        "imageExtensionSource"
    } else if matches!(name, "transition" | "timing" | "AlternateContent") {
        "animationSource"
    } else {
        return None;
    };
    Some(scoped_source(source, namespaces, wrapper))
}

fn parse_xml_tree(
    xml: Vec<u8>,
    limits: &ParseLimits,
    context: &str,
) -> Result<XmlDocument, ParseError> {
    struct Frame {
        link: (u32, u32),
        image_extension_scope: ImageExtensionScope,
        editing_scope: editing::Scope,
    }

    let buffer = xml;
    let base = if buffer.starts_with(b"\xef\xbb\xbf") { 3 } else { 0 };
    let xml: &[u8] = &buffer[base..];
    let xml_text = std::str::from_utf8(xml).ok();
    let borrowable = xml.len() < u32::MAX as usize && xml_text.is_some();
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut builder = TreeBuilder {
        xml,
        borrowable,
        decoder: reader.decoder(),
        context,
        nodes: Vec::new(),
        attributes: Vec::new(),
        strings: Vec::new(),
    };
    let mut stack = Vec::<Frame>::new();
    let mut source_starts = Vec::new();
    let mut root = NONE;
    let mut budget = XmlBudget::default();
    let track_image_extensions = memchr::memmem::find(xml, b"svgBlip").is_some();
    let track_editing = memchr::memmem::find(xml, b"cNv").is_some();
    // For these ASCII needles a byte search over valid UTF-8 equals `str::contains`.
    let track_existing_namespaces = (xml_text.is_some()
        && [&b"oMath"[..], b"transition", b"timing", b"fld"]
            .into_iter()
            .any(|needle| memchr::memmem::find(xml, needle).is_some()))
        || track_image_extensions;
    let track_namespaces = track_existing_namespaces || track_editing;
    let mut namespaces = NamespaceDeclarations::default();
    let mut namespace_scopes = Vec::new();
    loop {
        let source_start = reader.buffer_position() as usize;
        match reader.read_event() {
            Ok(Event::Start(start)) => {
                if stack.len() >= limits.max_xml_depth {
                    return Err(ParseError::ResourceLimit("XML nesting depth".into()));
                }
                let index = builder.push_start(&start, &mut budget, limits)?;
                if track_namespaces {
                    namespace_scopes.push(namespaces.len());
                    collect_namespaces(&start, stack.len(), &mut namespaces);
                }
                let editing_scope = if track_editing {
                    stack.last().map(|frame| frame.editing_scope).unwrap_or_default()
                        .next(&start, &namespaces.values)
                } else { editing::Scope::None };
                let parent_scope = stack
                    .last()
                    .map(|frame| frame.image_extension_scope)
                    .unwrap_or(ImageExtensionScope::None);
                let image_extension_scope = if track_image_extensions {
                    image_extension_scope(&start, parent_scope, &namespaces.values)
                } else {
                    ImageExtensionScope::None
                };
                let qualified = start.name();
                let local = local_name(qualified.as_ref());
                source_starts.push(
                    (editing_scope.is_properties() || matches!(
                        local,
                        b"pic"
                            | b"sp"
                            | b"cxnSp"
                            | b"fmtScheme"
                            | b"model3d"
                            | b"oleObj"
                            | b"contentPart"
                            | b"control"
                            | b"oMath"
                            | b"oMathPara"
                            | b"transition"
                            | b"timing"
                            | b"fld"
                    ) || (parent_scope == ImageExtensionScope::Extension
                        && local == b"svgBlip")
                        || (track_existing_namespaces
                            && local == b"AlternateContent"
                            && stack.len() == 1))
                        .then_some(source_start),
                );
                builder.link_child(stack.last_mut().map(|frame| &mut frame.link), index);
                stack.push(Frame {
                    link: (index, NONE),
                    image_extension_scope,
                    editing_scope,
                });
            }
            Ok(Event::Empty(start)) => {
                let index = builder.push_start(&start, &mut budget, limits)?;
                let namespace_count = namespaces.len();
                if track_namespaces {
                    collect_namespaces(&start, stack.len(), &mut namespaces);
                }
                let editing_scope = if track_editing {
                    stack.last().map(|frame| frame.editing_scope).unwrap_or_default()
                        .next(&start, &namespaces.values)
                } else { editing::Scope::None };
                let qualified = start.name();
                let local = local_name(qualified.as_ref());
                if editing_scope.retain(false, |name| builder.has_attribute(index, name)) || matches!(
                    local,
                    b"pic"
                        | b"sp"
                        | b"cxnSp"
                        | b"fmtScheme"
                        | b"model3d"
                        | b"oleObj"
                        | b"contentPart"
                        | b"control"
                        | b"oMath"
                        | b"oMathPara"
                        | b"transition"
                        | b"timing"
                        | b"fld"
                ) || (local == b"svgBlip"
                    && stack.last().is_some_and(|frame| {
                        frame.image_extension_scope == ImageExtensionScope::Extension
                    }))
                {
                    let source = &xml[source_start..reader.buffer_position() as usize];
                    let name = builder.resolve(builder.nodes[index as usize].name).to_owned();
                    let qualified = qualified_source(
                        &name,
                        editing_scope.is_properties(),
                        source,
                        &mut namespaces,
                    );
                    builder.retain_source(index, source, qualified, &mut budget, limits)?;
                }
                if track_namespaces {
                    namespaces.truncate(namespace_count);
                }
                builder.link_child(stack.last_mut().map(|frame| &mut frame.link), index);
                if let Some(parent) = stack.last() {
                    let bits = builder.nodes[index as usize].subtree_names;
                    builder.nodes[parent.link.0 as usize].subtree_names |= bits;
                } else if root != NONE {
                    return Err(ParseError::Corrupt(format!(
                        "{context} contains multiple XML roots"
                    )));
                } else {
                    root = index;
                }
            }
            Ok(Event::Text(text)) => {
                if let Some(frame) = stack.last() {
                    let index = frame.link.0;
                    let raw = text.as_ref();
                    if builder.borrowable && memchr::memchr(b'&', raw).is_none() {
                        budget.consume_text(raw.len(), limits)?;
                        builder.append_raw_text(index, raw);
                    } else {
                        let value = text
                            .unescape()
                            .map_err(|error| ParseError::Corrupt(format!("{context}: {error}")))?;
                        budget.consume_text(value.len(), limits)?;
                        builder.append_text(index, &value);
                    }
                }
            }
            Ok(Event::CData(text)) => {
                if let Some(frame) = stack.last() {
                    let index = frame.link.0;
                    budget.consume_text(text.len(), limits)?;
                    if builder.borrowable {
                        builder.append_raw_text(index, text.as_ref());
                    } else {
                        builder.append_text(index, &String::from_utf8_lossy(text.as_ref()));
                    }
                }
            }
            Ok(Event::End(_)) => {
                let Frame { link: (index, last_child), editing_scope, .. } = stack.pop().ok_or_else(|| {
                    ParseError::Corrupt(format!("{context} contains an unmatched closing tag"))
                })?;
                let has_children = last_child != NONE;
                if let Some(start) = source_starts.pop().flatten().filter(|_| {
                    !editing_scope.is_properties()
                        || editing_scope.retain(has_children, |name| builder.has_attribute(index, name))
                }) {
                    let source = &xml[start..reader.buffer_position() as usize];
                    let name = builder.resolve(builder.nodes[index as usize].name).to_owned();
                    let qualified = qualified_source(
                        &name,
                        editing_scope.is_properties(),
                        source,
                        &mut namespaces,
                    );
                    builder.retain_source(index, source, qualified, &mut budget, limits)?;
                }
                if track_namespaces {
                    namespaces.truncate(namespace_scopes.pop().unwrap_or(0));
                }
                if let Some(parent) = stack.last() {
                    let bits = builder.nodes[index as usize].subtree_names;
                    builder.nodes[parent.link.0 as usize].subtree_names |= bits;
                } else if root != NONE {
                    return Err(ParseError::Corrupt(format!(
                        "{context} contains multiple XML roots"
                    )));
                } else {
                    root = index;
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(ParseError::Corrupt(format!("{context}: {error}"))),
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(ParseError::Corrupt(format!(
            "{context} contains unclosed XML elements"
        )));
    }
    if root == NONE {
        return Err(ParseError::Corrupt(format!("{context} has no XML root")));
    }
    let TreeBuilder {
        nodes,
        attributes,
        strings,
        ..
    } = builder;
    drop(reader);
    Ok(XmlDocument {
        buffer,
        base,
        nodes,
        attributes,
        strings,
        root,
    })
}

fn bool_value(value: &str) -> Option<bool> {
    match value {
        "1" => Some(true),
        "0" => Some(false),
        _ if value.eq_ignore_ascii_case("true")
            || value.eq_ignore_ascii_case("t")
            || value.eq_ignore_ascii_case("on") => Some(true),
        _ if value.eq_ignore_ascii_case("false")
            || value.eq_ignore_ascii_case("f")
            || value.eq_ignore_ascii_case("off") => Some(false),
        _ => None,
    }
}

fn parse_transform(xfrm: Option<XmlNode<'_>>) -> Transform {
    let Some(xfrm) = xfrm else {
        return Transform::default();
    };
    let off = xfrm.child("off");
    let ext = xfrm.child("ext");
    Transform {
        x: off
            .and_then(|node| node.attr("x"))
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
        y: off
            .and_then(|node| node.attr("y"))
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
        width: ext
            .and_then(|node| node.attr("cx"))
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
        height: ext
            .and_then(|node| node.attr("cy"))
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
        rotation: xfrm
            .attr("rot")
            .and_then(|value| value.parse::<f64>().ok())
            .map(|value| value / 60_000.0),
        flip_horizontal: xfrm.attr("flipH").and_then(bool_value),
        flip_vertical: xfrm.attr("flipV").and_then(bool_value),
    }
}

fn parse_child_transform(xfrm: Option<XmlNode<'_>>) -> Option<Transform> {
    let xfrm = xfrm?;
    let off = xfrm.child("chOff")?;
    let ext = xfrm.child("chExt")?;
    Some(Transform {
        x: off
            .attr("x")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
        y: off
            .attr("y")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
        width: ext
            .attr("cx")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
        height: ext
            .attr("cy")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
        ..Default::default()
    })
}

fn node_identity(node: XmlNode<'_>, slide_index: usize, node_index: usize) -> (String, String) {
    let properties = node.descendant("cNvPr");
    (
        properties
            .and_then(|value| value.attr("id"))
            .map(str::to_owned)
            .unwrap_or_else(|| format!("node-{slide_index}-{node_index}")),
        properties
            .and_then(|value| value.attr("name"))
            .unwrap_or_default()
            .to_owned(),
    )
}

fn nonvisual_properties(node: XmlNode<'_>) -> Option<XmlNode<'_>> {
    node.children().iter().find(|child| {
        matches!(
            child.name(),
            "nvSpPr"
                | "nvPicPr"
                | "nvGrpSpPr"
                | "nvGraphicFramePr"
                | "nvCxnSpPr"
                | "nvContentPartPr"
        )
    })
}

fn object_action(node: XmlNode<'_>, rels: &Relationships) -> crate::ObjectAction {
    let string = |name| node.attr(name).map(str::to_owned);
    let sound = node.child("snd");
    crate::ObjectAction {
        relationship_id: string("id"),
        target: node.attr("id").and_then(|id| rels.get(id)).cloned(),
        external: node
            .attr("id")
            .filter(|id| rels.contains_key(*id))
            .map(|id| rels.is_external(id)),
        action: string("action"),
        tooltip: string("tooltip"),
        target_frame: string("tgtFrame"),
        invalid_url: string("invalidUrl"),
        history: node.attr("history").and_then(bool_value),
        highlight_click: node.attr("highlightClick").and_then(bool_value),
        end_sound: node.attr("endSnd").and_then(bool_value),
        sound_asset_id: sound
            .and_then(|sound| sound.attr("embed"))
            .and_then(|id| rels.internal(id))
            .cloned(),
        sound_name: sound
            .and_then(|sound| sound.attr("name"))
            .map(str::to_owned),
        sound_builtin: sound
            .and_then(|sound| sound.attr("builtIn"))
            .and_then(bool_value),
    }
}

fn node_metadata(
    node: XmlNode<'_>,
    context: &NodeParseContext<'_, '_>,
) -> Option<crate::NodeMetadata> {
    let properties = nonvisual_properties(node)?.child("cNvPr")?;
    let click_action = properties
        .child("hlinkClick")
        .map(|action| Box::new(object_action(action, context.rels)));
    let description = properties.attr("descr").map(str::to_owned);
    let title = properties.attr("title").map(str::to_owned);
    Some(crate::NodeMetadata {
        editing: nonvisual_properties(node).and_then(|properties| {
            properties.children().iter().filter_map(|child| child.source_xml())
                .find_map(|xml| context.editing_cache.borrow_mut().parse(xml))
        }),
        source_id: properties.attr("id").map(str::to_owned),
        hidden: properties.attr("hidden").and_then(bool_value),
        alt_text: description
            .as_ref()
            .filter(|text| !text.is_empty())
            .or_else(|| title.as_ref().filter(|text| !text.is_empty()))
            .cloned(),
        description,
        accessibility_title: title,
        decorative: properties
            .child("extLst")
            .and_then(|extensions| extensions.descendant("decorative"))
            .and_then(|decorative| decorative.attr("val"))
            .and_then(bool_value),
        hyperlink: click_action
            .as_ref()
            .and_then(|action| action.target.clone()),
        click_action,
        hover_action: properties
            .child("hlinkHover")
            .map(|action| Box::new(object_action(action, context.rels))),
        source_part: context.source_part.map(str::to_owned),
    })
}

fn color_element(node: XmlNode<'_>) -> Option<XmlNode<'_>> {
    if matches!(
        node.name(),
        "srgbClr" | "scrgbClr" | "hslClr" | "schemeClr" | "sysClr" | "prstClr"
    ) {
        return Some(node);
    }
    node.children().iter().find_map(color_element)
}

#[derive(Debug, Clone)]
struct ThemeDataInternal {
    id: String,
    name: Option<String>,
    colors: BTreeMap<String, String>,
    major_fonts: BTreeMap<String, String>,
    minor_fonts: BTreeMap<String, String>,
    rels: Relationships,
    document: XmlDocument,
    fill_styles: Vec<u32>,
    background_fill_styles: Vec<u32>,
    line_styles: Vec<u32>,
    effect_styles: Vec<u32>,
    format_scheme_xml: Option<String>,
}

impl ThemeDataInternal {
    fn style(&self, indices: &[u32], index: usize) -> Option<XmlNode<'_>> {
        indices.get(index).map(|node| self.document.node(*node))
    }

    fn fill_style(&self, index: usize) -> Option<XmlNode<'_>> {
        self.style(&self.fill_styles, index)
    }

    fn background_fill_style(&self, index: usize) -> Option<XmlNode<'_>> {
        self.style(&self.background_fill_styles, index)
    }

    fn line_style(&self, index: usize) -> Option<XmlNode<'_>> {
        self.style(&self.line_styles, index)
    }

    fn effect_style(&self, index: usize) -> Option<XmlNode<'_>> {
        self.style(&self.effect_styles, index)
    }

    fn public(&self) -> crate::PresentationTheme {
        crate::PresentationTheme {
            id: self.id.clone(),
            name: self.name.clone(),
            format_scheme_xml: self.format_scheme_xml.clone(),
            colors: self.colors.clone(),
            major_fonts: self.major_fonts.clone(),
            minor_fonts: self.minor_fonts.clone(),
        }
    }

    fn resolve_font(&self, typeface: &str) -> Option<String> {
        let fonts = if typeface.starts_with("+mj-") {
            &self.major_fonts
        } else if typeface.starts_with("+mn-") {
            &self.minor_fonts
        } else {
            return Some(typeface.to_owned());
        };
        let key = if typeface.ends_with("-ea") {
            "eastAsia"
        } else if typeface.ends_with("-cs") {
            "complexScript"
        } else {
            "latin"
        };
        fonts.get(key).cloned().filter(|font| !font.is_empty())
    }
}

#[derive(Debug)]
struct ParseDiagnostics {
    slide_index: Option<usize>,
    part_name: String,
    warnings: RefCell<Vec<PresentationWarning>>,
}

impl ParseDiagnostics {
    fn new(slide_index: Option<usize>, part_name: impl Into<String>) -> Self {
        Self {
            slide_index,
            part_name: part_name.into(),
            warnings: RefCell::new(Vec::new()),
        }
    }

    fn warn(&self, code: &str, message: impl Into<String>, feature: Option<&str>) {
        let mut warning = PresentationWarning::warning(code, message);
        warning.slide_index = self.slide_index;
        warning.part_name = Some(self.part_name.clone());
        warning.feature = feature.map(str::to_owned);
        self.warnings.borrow_mut().push(warning);
    }

    fn take(self) -> Vec<PresentationWarning> {
        self.warnings.into_inner()
    }
}

struct ColorContext<'a> {
    theme: Option<&'a ThemeDataInternal>,
    color_map: &'a BTreeMap<String, String>,
    diagnostics: &'a ParseDiagnostics,
    paint_key: Rc<paint::ColorKey>,
}

fn preset_color(value: &str) -> Option<&'static str> {
    match value {
        "black" => Some("#000000"),
        "white" => Some("#FFFFFF"),
        "red" => Some("#FF0000"),
        "green" => Some("#008000"),
        "blue" => Some("#0000FF"),
        "yellow" => Some("#FFFF00"),
        "cyan" | "aqua" => Some("#00FFFF"),
        "magenta" | "fuchsia" => Some("#FF00FF"),
        "gray" | "grey" => Some("#808080"),
        "silver" => Some("#C0C0C0"),
        "maroon" | "darkRed" => Some("#800000"),
        "olive" | "darkYellow" => Some("#808000"),
        "lime" => Some("#00FF00"),
        "teal" | "darkCyan" => Some("#008080"),
        "navy" | "darkBlue" => Some("#000080"),
        "purple" | "darkMagenta" => Some("#800080"),
        "darkGreen" => Some("#006400"),
        "orange" => Some("#FFA500"),
        "gold" => Some("#FFD700"),
        "brown" => Some("#A52A2A"),
        "pink" => Some("#FFC0CB"),
        "violet" => Some("#EE82EE"),
        "lightGray" | "lightGrey" => Some("#D3D3D3"),
        _ => None,
    }
}

fn normalized_hex(value: &str) -> Option<String> {
    let value = value.strip_prefix('#').unwrap_or(value);
    (value.len() == 6 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| format!("#{}", value.to_ascii_uppercase()))
}

fn rgb_from_hex(value: &str) -> Option<[f64; 3]> {
    let value = value.strip_prefix('#').unwrap_or(value);
    if value.len() != 6 {
        return None;
    }
    Some([
        u8::from_str_radix(&value[0..2], 16).ok()? as f64,
        u8::from_str_radix(&value[2..4], 16).ok()? as f64,
        u8::from_str_radix(&value[4..6], 16).ok()? as f64,
    ])
}

fn percentage(value: &str) -> Option<f64> {
    let value = value.trim();
    if let Some(value) = value.strip_suffix('%') {
        return value.trim().parse::<f64>().ok().map(|value| value / 100.0);
    }
    value.parse::<f64>().ok().map(|value| value / 100_000.0)
}

fn rgb_to_hsl(rgb: [f64; 3]) -> [f64; 3] {
    let [red, green, blue] = rgb.map(|channel| channel / 255.0);
    let maximum = red.max(green).max(blue);
    let minimum = red.min(green).min(blue);
    let luminance = (maximum + minimum) / 2.0;
    if (maximum - minimum).abs() < f64::EPSILON {
        return [0.0, 0.0, luminance];
    }
    let delta = maximum - minimum;
    let saturation = if luminance > 0.5 {
        delta / (2.0 - maximum - minimum)
    } else {
        delta / (maximum + minimum)
    };
    let hue = if (maximum - red).abs() < f64::EPSILON {
        (green - blue) / delta + if green < blue { 6.0 } else { 0.0 }
    } else if (maximum - green).abs() < f64::EPSILON {
        (blue - red) / delta + 2.0
    } else {
        (red - green) / delta + 4.0
    } / 6.0;
    [hue, saturation, luminance]
}

fn hue_channel(p: f64, q: f64, mut value: f64) -> f64 {
    if value < 0.0 {
        value += 1.0;
    }
    if value > 1.0 {
        value -= 1.0;
    }
    if value < 1.0 / 6.0 {
        p + (q - p) * 6.0 * value
    } else if value < 0.5 {
        q
    } else if value < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - value) * 6.0
    } else {
        p
    }
}

fn hsl_to_rgb([hue, saturation, luminance]: [f64; 3]) -> [f64; 3] {
    if saturation.abs() < f64::EPSILON {
        return [luminance * 255.0; 3];
    }
    let q = if luminance < 0.5 {
        luminance * (1.0 + saturation)
    } else {
        luminance + saturation - luminance * saturation
    };
    let p = 2.0 * luminance - q;
    [
        hue_channel(p, q, hue + 1.0 / 3.0) * 255.0,
        hue_channel(p, q, hue) * 255.0,
        hue_channel(p, q, hue - 1.0 / 3.0) * 255.0,
    ]
}

fn angle_degrees(value: &str) -> Option<f64> {
    value.parse::<f64>().ok().map(|value| value / 60_000.0)
}

fn srgb_channel_to_linear(channel: f64) -> f64 {
    let value = (channel / 255.0).clamp(0.0, 1.0);
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_channel_to_srgb(channel: f64) -> f64 {
    let value = channel.clamp(0.0, 1.0);
    let encoded = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    encoded * 255.0
}

fn transformed_color(value: &str, color: XmlNode<'_>) -> Option<String> {
    let mut rgb = rgb_from_hex(value)?;
    for transform in color.children() {
        let amount = transform.attr("val").and_then(percentage);
        match (transform.name(), amount) {
            ("tint", Some(amount)) => {
                for channel in &mut rgb {
                    let linear = srgb_channel_to_linear(*channel);
                    *channel = linear_channel_to_srgb(linear * amount + (1.0 - amount));
                }
            }
            ("shade", Some(amount)) => {
                for channel in &mut rgb {
                    *channel = linear_channel_to_srgb(srgb_channel_to_linear(*channel) * amount);
                }
            }
            ("lum", Some(amount)) => {
                let mut hsl = rgb_to_hsl(rgb);
                hsl[2] = amount.clamp(0.0, 1.0);
                rgb = hsl_to_rgb(hsl);
            }
            ("lumMod", Some(amount)) => {
                let mut hsl = rgb_to_hsl(rgb);
                hsl[2] = (hsl[2] * amount).clamp(0.0, 1.0);
                rgb = hsl_to_rgb(hsl);
            }
            ("lumOff", Some(amount)) => {
                let mut hsl = rgb_to_hsl(rgb);
                hsl[2] = (hsl[2] + amount).clamp(0.0, 1.0);
                rgb = hsl_to_rgb(hsl);
            }
            ("sat", Some(amount)) => {
                let mut hsl = rgb_to_hsl(rgb);
                hsl[1] = amount.clamp(0.0, 1.0);
                rgb = hsl_to_rgb(hsl);
            }
            ("satMod", Some(amount)) => {
                let mut hsl = rgb_to_hsl(rgb);
                hsl[1] = (hsl[1] * amount).clamp(0.0, 1.0);
                rgb = hsl_to_rgb(hsl);
            }
            ("satOff", Some(amount)) => {
                let mut hsl = rgb_to_hsl(rgb);
                hsl[1] = (hsl[1] + amount).clamp(0.0, 1.0);
                rgb = hsl_to_rgb(hsl);
            }
            ("hueMod", Some(amount)) => {
                let mut hsl = rgb_to_hsl(rgb);
                hsl[0] = (hsl[0] * amount).rem_euclid(1.0);
                rgb = hsl_to_rgb(hsl);
            }
            ("red", Some(amount)) => rgb[0] = 255.0 * amount,
            ("green", Some(amount)) => rgb[1] = 255.0 * amount,
            ("blue", Some(amount)) => rgb[2] = 255.0 * amount,
            ("redMod", Some(amount)) => rgb[0] *= amount,
            ("greenMod", Some(amount)) => rgb[1] *= amount,
            ("blueMod", Some(amount)) => rgb[2] *= amount,
            ("redOff", Some(amount)) => rgb[0] += 255.0 * amount,
            ("greenOff", Some(amount)) => rgb[1] += 255.0 * amount,
            ("blueOff", Some(amount)) => rgb[2] += 255.0 * amount,
            ("hue", _) => {
                if let Some(value) = transform.attr("val").and_then(angle_degrees) {
                    let mut hsl = rgb_to_hsl(rgb);
                    hsl[0] = (value / 360.0).rem_euclid(1.0);
                    rgb = hsl_to_rgb(hsl);
                }
            }
            ("hueOff", _) => {
                if let Some(value) = transform.attr("val").and_then(angle_degrees) {
                    let mut hsl = rgb_to_hsl(rgb);
                    hsl[0] = (hsl[0] + value / 360.0).rem_euclid(1.0);
                    rgb = hsl_to_rgb(hsl);
                }
            }
            ("comp", _) => {
                let mut hsl = rgb_to_hsl(rgb);
                hsl[0] = (hsl[0] + 0.5).rem_euclid(1.0);
                rgb = hsl_to_rgb(hsl);
            }
            ("inv", _) => {
                rgb = rgb.map(|channel| 255.0 - channel);
            }
            ("gray", _) => {
                let gray = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
                rgb = [gray; 3];
            }
            _ => {}
        }
    }
    Some(format!(
        "#{:02X}{:02X}{:02X}",
        rgb[0].round().clamp(0.0, 255.0) as u8,
        rgb[1].round().clamp(0.0, 255.0) as u8,
        rgb[2].round().clamp(0.0, 255.0) as u8
    ))
}

fn parse_color_with_placeholder(
    node: XmlNode<'_>,
    context: Option<&ColorContext<'_>>,
    placeholder: Option<&ColorValue>,
) -> Option<ColorValue> {
    let color = color_element(node)?;
    let inherited_alpha = (color.name() == "schemeClr" && color.attr("val") == Some("phClr"))
        .then(|| placeholder.and_then(|value| value.alpha))
        .flatten()
        .unwrap_or(1.0);
    let base_value = match color.name() {
        "srgbClr" => normalized_hex(color.attr("val")?),
        "scrgbClr" => {
            let channel = |name: &str| {
                color
                    .attr(name)
                    .and_then(|value| value.parse::<f64>().ok())
                    .map(|value| (value * 255.0 / 100_000.0).round().clamp(0.0, 255.0) as u8)
                    .unwrap_or(0)
            };
            Some(format!(
                "#{:02X}{:02X}{:02X}",
                channel("r"),
                channel("g"),
                channel("b")
            ))
        }
        "hslClr" => {
            let hue = color.attr("hue").and_then(angle_degrees).unwrap_or(0.0) / 360.0;
            let saturation = color.attr("sat").and_then(percentage).unwrap_or(0.0);
            let luminance = color.attr("lum").and_then(percentage).unwrap_or(0.0);
            let rgb = hsl_to_rgb([hue, saturation, luminance]);
            Some(format!(
                "#{:02X}{:02X}{:02X}",
                rgb[0].round().clamp(0.0, 255.0) as u8,
                rgb[1].round().clamp(0.0, 255.0) as u8,
                rgb[2].round().clamp(0.0, 255.0) as u8
            ))
        }
        "schemeClr" => {
            let requested = color.attr("val")?;
            if requested == "phClr" {
                placeholder.map(|value| value.value.clone())
            } else {
                let context = context?;
                let mapped = context
                    .color_map
                    .get(requested)
                    .map(String::as_str)
                    .unwrap_or(requested);
                let resolved = context
                    .theme
                    .and_then(|theme| theme.colors.get(mapped))
                    .cloned();
                if resolved.is_none() {
                    context.diagnostics.warn(
                        "degraded-rendering",
                        format!("Theme color {requested} could not be resolved."),
                        Some("theme-color"),
                    );
                }
                resolved
            }
        }
        "sysClr" => color
            .attr("lastClr")
            .and_then(normalized_hex)
            .or_else(|| color.attr("val").and_then(preset_color).map(str::to_owned)),
        "prstClr" => {
            let preset = color.attr("val")?;
            let resolved = preset_color(preset).map(str::to_owned);
            if resolved.is_none() {
                if let Some(context) = context {
                    context.diagnostics.warn(
                        "degraded-rendering",
                        "An unsupported preset color was ignored.",
                        Some("preset-color"),
                    );
                }
            }
            resolved
        }
        _ => None,
    }?;
    let value = transformed_color(&base_value, color)?;
    let mut alpha = color
        .child("alpha")
        .and_then(|node| node.attr("val"))
        .and_then(|value| value.parse::<f64>().ok())
        .map(|value| (value / 100_000.0).clamp(0.0, 1.0))
        .unwrap_or(inherited_alpha);
    if let Some(value) = color
        .child("alphaMod")
        .and_then(|node| node.attr("val"))
        .and_then(|value| value.parse::<f64>().ok())
    {
        alpha *= value / 100_000.0;
    }
    if let Some(value) = color
        .child("alphaOff")
        .and_then(|node| node.attr("val"))
        .and_then(|value| value.parse::<f64>().ok())
    {
        alpha += value / 100_000.0;
    }
    let alpha = (alpha < 1.0).then(|| alpha.clamp(0.0, 1.0));
    Some(ColorValue { value, alpha })
}

fn parse_color(node: XmlNode<'_>, context: Option<&ColorContext<'_>>) -> Option<ColorValue> {
    parse_color_with_placeholder(node, context, None)
}

fn parse_theme(
    xml: Vec<u8>,
    path: &str,
    rels: Relationships,
    limits: &ParseLimits,
) -> Result<ThemeDataInternal, ParseError> {
    let document = parse_xml_tree(xml, limits, path)?;
    let root = document.root();
    let color_scheme = root.descendant("clrScheme");
    let mut colors = BTreeMap::new();
    if let Some(color_scheme) = color_scheme {
        for entry in color_scheme.children() {
            if let Some(color) = parse_color(entry, None) {
                colors.insert(entry.name().to_string(), color.value);
            }
        }
    }
    let parse_fonts = |node: Option<XmlNode<'_>>| {
        let mut fonts = BTreeMap::new();
        if let Some(node) = node {
            for (element, key) in [
                ("latin", "latin"),
                ("ea", "eastAsia"),
                ("cs", "complexScript"),
            ] {
                if let Some(typeface) = node.child(element).and_then(|font| font.attr("typeface")) {
                    if !typeface.is_empty() {
                        fonts.insert(key.to_owned(), typeface.to_owned());
                    }
                }
            }
        }
        fonts
    };
    let font_scheme = root.descendant("fontScheme");
    let format_scheme = root.descendant("fmtScheme");
    let name = root
        .attr("name")
        .or_else(|| color_scheme.and_then(|scheme| scheme.attr("name")))
        .map(str::to_owned);
    let major_fonts = parse_fonts(font_scheme.and_then(|scheme| scheme.child("majorFont")));
    let minor_fonts = parse_fonts(font_scheme.and_then(|scheme| scheme.child("minorFont")));
    let format_scheme_xml = format_scheme
        .and_then(|node| node.source_xml())
        .map(str::to_owned);
    let indices = |list: &str| {
        format_scheme
            .and_then(|scheme| scheme.child(list))
            .map(|list| list.children().map(|node| node.index()).collect::<Vec<u32>>())
            .unwrap_or_default()
    };
    let fill_styles = indices("fillStyleLst");
    let background_fill_styles = indices("bgFillStyleLst");
    let line_styles = indices("lnStyleLst");
    let effect_styles = indices("effectStyleLst");
    Ok(ThemeDataInternal {
        id: path.to_owned(),
        name,
        colors,
        major_fonts,
        minor_fonts,
        rels,
        document,
        format_scheme_xml,
        fill_styles,
        background_fill_styles,
        line_styles,
        effect_styles,
    })
}

fn color_map(root: XmlNode<'_>) -> BTreeMap<String, String> {
    let mapping = root.child("clrMap").or_else(|| {
        root.child("clrMapOvr")
            .and_then(|override_node| override_node.child("overrideClrMapping"))
    });
    mapping
        .map(|mapping| {
            mapping
                .attributes()
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn merge_color_maps(
    base: &BTreeMap<String, String>,
    override_map: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut result = base.clone();
    result.extend(override_map.clone());
    result
}

fn named_fill<'a>(properties: XmlNode<'a>, name: &str) -> Option<XmlNode<'a>> {
    (properties.name() == name)
        .then_some(properties)
        .or_else(|| properties.child(name))
}

fn image_crop(fill: XmlNode<'_>) -> Option<crate::ImageCrop> {
    let source = fill.child("srcRect")?;
    let edge = |name: &str| {
        source
            .attr(name)
            .and_then(percentage)
            .unwrap_or(0.0)
            .clamp(-1.0, 1.0)
    };
    let crop = crate::ImageCrop {
        top: edge("t"),
        right: edge("r"),
        bottom: edge("b"),
        left: edge("l"),
    };
    (crop.top != 0.0 || crop.right != 0.0 || crop.bottom != 0.0 || crop.left != 0.0).then_some(crop)
}

/// Parse DrawingML recolor effects declared directly on an `a:blip`.
fn parse_blip_effects(
    blip: XmlNode<'_>,
    context: Option<&ColorContext<'_>>,
) -> Option<crate::ImageEffects> {
    let mut effects = crate::ImageEffects::default();
    let mut any = false;
    if let Some(node) = blip.child("biLevel") {
        effects.bi_level_threshold = Some(
            node.attr("thresh")
                .and_then(percentage)
                .map(|value| value.clamp(0.0, 1.0))
                .unwrap_or(0.5),
        );
        any = true;
    }
    if blip.child("grayscl").is_some() {
        effects.grayscale = Some(true);
        any = true;
    }
    if let Some(node) = blip.child("duotone") {
        let colors = node
            .children()
            .iter()
            .filter_map(|child| parse_color(child, context))
            .collect::<Vec<_>>();
        if colors.len() >= 2 {
            effects.duotone = Some(colors.into_iter().take(2).collect());
            any = true;
        }
    }
    if let Some(node) = blip.child("lum") {
        let brightness = node
            .attr("bright")
            .and_then(percentage)
            .map(|value| value.clamp(-1.0, 1.0));
        let contrast = node
            .attr("contrast")
            .and_then(percentage)
            .map(|value| value.clamp(-1.0, 1.0));
        if brightness.is_some() || contrast.is_some() {
            effects.brightness = brightness;
            effects.contrast = contrast;
            any = true;
        }
    }
    any.then_some(effects)
}

fn parse_blip_fill(
    fill: XmlNode<'_>,
    context: Option<&ColorContext<'_>>,
    rels: &Relationships,
) -> Option<FillStyle> {
    let blip = fill.child("blip")?;
    let relationship = blip.attr("embed").or_else(|| blip.attr("link"))?;
    let asset_id = rels.get(relationship)?.clone();
    let opacity = blip
        .child("alphaModFix")
        .or_else(|| blip.child("alphaMod"))
        .and_then(|effect| effect.attr("amt").or_else(|| effect.attr("val")))
        .and_then(percentage)
        .map(|value| value.clamp(0.0, 1.0));
    Some(FillStyle::Image {
        asset_id,
        mode: if fill.child("tile").is_some() {
            crate::FillImageMode::Tile
        } else {
            crate::FillImageMode::Stretch
        },
        crop: image_crop(fill),
        opacity,
        effects: parse_blip_effects(blip, context),
    })
}

fn parse_fill_with_placeholder(
    properties: XmlNode<'_>,
    context: Option<&ColorContext<'_>>,
    rels: &Relationships,
    placeholder: Option<&ColorValue>,
) -> Option<FillStyle> {
    if named_fill(properties, "noFill").is_some() {
        return Some(FillStyle::None);
    }
    if let Some(solid) = named_fill(properties, "solidFill") {
        return parse_color_with_placeholder(solid, context, placeholder)
            .map(|color| FillStyle::Solid { color });
    }
    if let Some(gradient) = named_fill(properties, "gradFill") {
        let stops = gradient
            .child("gsLst")
            .into_iter()
            .flat_map(|list| list.children_named("gs"))
            .filter_map(|stop| {
                Some(GradientStop {
                    position: stop
                        .attr("pos")
                        .and_then(|value| value.parse::<f64>().ok())
                        .unwrap_or(0.0)
                        / 100_000.0,
                    color: parse_color_with_placeholder(stop, context, placeholder)?,
                })
            })
            .collect::<Vec<_>>();
        if !stops.is_empty() {
            let angle = gradient
                .child("lin")
                .and_then(|node| node.attr("ang"))
                .and_then(|value| value.parse::<f64>().ok())
                .map(|value| value / 60_000.0);
            return Some(FillStyle::Gradient { angle, stops });
        }
    }
    if let Some(pattern) = named_fill(properties, "pattFill") {
        let foreground =
            parse_color_with_placeholder(pattern.child("fgClr")?, context, placeholder)?;
        let background =
            parse_color_with_placeholder(pattern.child("bgClr")?, context, placeholder)?;
        return Some(FillStyle::Pattern {
            preset: pattern.attr("prst").unwrap_or("pct5").to_owned(),
            foreground,
            background,
        });
    }
    if let Some(fill) = named_fill(properties, "blipFill") {
        return parse_blip_fill(fill, context, rels);
    }
    None
}

fn parse_fill(
    properties: XmlNode<'_>,
    context: Option<&ColorContext<'_>>,
    rels: &Relationships,
) -> Option<FillStyle> {
    parse_fill_with_placeholder(properties, context, rels, None)
}

fn parse_line_node_with_placeholder(
    line: XmlNode<'_>,
    context: Option<&ColorContext<'_>>,
    placeholder: Option<&ColorValue>,
) -> Option<LineStyle> {
    if line.child("noFill").is_some() {
        return None;
    }
    let color = line
        .child("solidFill")
        .and_then(|fill| parse_color_with_placeholder(fill, context, placeholder))
        .or_else(|| {
            line.child("gradFill").and_then(|gradient| {
                gradient
                    .child("gsLst")
                    .and_then(|list| list.child("gs"))
                    .and_then(|stop| parse_color_with_placeholder(stop, context, placeholder))
            })
        });
    let width = line
        .attr("w")
        .and_then(|value| value.parse::<f64>().ok())
        .map(|value| value / 9_525.0)
        .or_else(|| color.as_ref().map(|_| 1.0));
    let dash = line
        .child("prstDash")
        .and_then(|value| value.attr("val"))
        .map(str::to_owned);
    let start_arrow = line
        .child("headEnd")
        .and_then(|value| value.attr("type"))
        .filter(|value| *value != "none")
        .map(str::to_owned);
    let end_arrow = line
        .child("tailEnd")
        .and_then(|value| value.attr("type"))
        .filter(|value| *value != "none")
        .map(str::to_owned);
    (color.is_some()
        || width.is_some()
        || dash.is_some()
        || start_arrow.is_some()
        || end_arrow.is_some())
    .then_some(LineStyle {
        color,
        width,
        dash,
        start_arrow,
        end_arrow,
    })
}

fn parse_line_node(line: XmlNode<'_>, context: Option<&ColorContext<'_>>) -> Option<LineStyle> {
    parse_line_node_with_placeholder(line, context, None)
}

fn style_matrix_fill(reference: XmlNode<'_>, context: Option<&ColorContext<'_>>) -> Option<FillStyle> {
    let index = reference.attr("idx")?.parse::<usize>().ok()?;
    if matches!(index, 0 | 1000) {
        return Some(FillStyle::None);
    }
    let theme = context?.theme?;
    let placeholder = parse_color(reference, context);
    let style = if index >= 1001 {
        theme.background_fill_style(index - 1001)
    } else {
        theme.fill_style(index - 1)
    };
    style
        .and_then(|style| {
            parse_fill_with_placeholder(style, context, &theme.rels, placeholder.as_ref())
        })
        .or_else(|| placeholder.map(|color| FillStyle::Solid { color }))
}

fn style_matrix_line(reference: XmlNode<'_>, context: Option<&ColorContext<'_>>) -> Option<LineStyle> {
    let index = reference.attr("idx")?.parse::<usize>().ok()?;
    if index == 0 {
        return None;
    }
    let theme = context?.theme?;
    let placeholder = parse_color(reference, context);
    theme
        .line_style(index - 1)
        .and_then(|line| parse_line_node_with_placeholder(line, context, placeholder.as_ref()))
        .or_else(|| {
            placeholder.map(|color| LineStyle {
                color: Some(color),
                ..Default::default()
            })
        })
}

type RunStyle = crate::TextRunProperties;

impl RunStyle {
    fn overlay(mut self, other: RunStyle) -> Self {
        if other.font_family.is_some() {
            self.font_family = other.font_family;
        }
        if other.east_asian_font_family.is_some() {
            self.east_asian_font_family = other.east_asian_font_family;
        }
        if other.complex_script_font_family.is_some() {
            self.complex_script_font_family = other.complex_script_font_family;
        }
        if other.symbol_font_family.is_some() {
            self.symbol_font_family = other.symbol_font_family;
        }
        if other.font_size_pt.is_some() {
            self.font_size_pt = other.font_size_pt;
        }
        if other.bold.is_some() {
            self.bold = other.bold;
        }
        if other.italic.is_some() {
            self.italic = other.italic;
        }
        if other.underline.is_some() {
            self.underline = other.underline;
        }
        if other.strike.is_some() {
            self.strike = other.strike;
        }
        if other.color.is_some() {
            self.color = other.color;
        }
        if other.baseline.is_some() {
            self.baseline = other.baseline;
        }
        if other.language.is_some() {
            self.language = other.language;
        }
        if other.alternative_language.is_some() {
            self.alternative_language = other.alternative_language;
        }
        if other.right_to_left.is_some() {
            self.right_to_left = other.right_to_left;
        }
        if other.hyperlink.is_some() {
            self.hyperlink = other.hyperlink;
        }
        if other.character_spacing_pt.is_some() {
            self.character_spacing_pt = other.character_spacing_pt;
        }
        if other.kerning_threshold_pt.is_some() {
            self.kerning_threshold_pt = other.kerning_threshold_pt;
        }
        self
    }
}

fn run_font_family(
    properties: XmlNode<'_>,
    element: &str,
    colors: Option<&ColorContext<'_>>,
) -> Option<String> {
    properties
        .child(element)
        .and_then(|node| node.attr("typeface"))
        .and_then(|typeface| {
            colors
                .and_then(|context| context.theme)
                .and_then(|theme| theme.resolve_font(typeface))
                .or_else(|| (!typeface.starts_with('+')).then(|| typeface.to_owned()))
        })
}

fn parse_run_style(
    properties: Option<XmlNode<'_>>,
    colors: Option<&ColorContext<'_>>,
    rels: &Relationships,
) -> RunStyle {
    let Some(properties) = properties else {
        return RunStyle::default();
    };
    let font_family = run_font_family(properties, "latin", colors);
    let east_asian_font_family = run_font_family(properties, "ea", colors);
    let complex_script_font_family = run_font_family(properties, "cs", colors);
    let symbol_font_family = run_font_family(properties, "sym", colors);
    let hyperlink = properties
        .child("hlinkClick")
        .and_then(|link| link.attr("id"))
        .and_then(|id| rels.get(id))
        .cloned();
    let mut attributes = [None; 10];
    for (name, value) in properties.unqualified_attributes() {
        let index = match name {
            "sz" => 0,
            "b" => 1,
            "i" => 2,
            "u" => 3,
            "strike" => 4,
            "baseline" => 5,
            "lang" => 6,
            "altLang" => 7,
            "spc" => 8,
            "kern" => 9,
            _ => continue,
        };
        if attributes[index].is_none() {
            attributes[index] = Some(value);
        }
    }
    let [size, bold, italic, underline, strike, baseline, language, alternative_language, spacing, kerning] =
        attributes;
    RunStyle {
        font_family: font_family
            .clone()
            .or_else(|| east_asian_font_family.clone())
            .or_else(|| complex_script_font_family.clone()),
        east_asian_font_family,
        complex_script_font_family,
        symbol_font_family,
        font_size_pt: size
            .and_then(|value| value.parse::<f64>().ok())
            .map(|value| value / 100.0),
        bold: bold.and_then(bool_value),
        italic: italic.and_then(bool_value),
        underline: underline.map(|value| value != "none" && value != "0"),
        strike: strike.map(|value| value != "noStrike" && value != "none" && value != "0"),
        color: parse_color(properties, colors),
        baseline: baseline
            .and_then(|value| value.parse::<f64>().ok())
            .map(|value| value / 1_000.0),
        language: language.map(str::to_owned),
        alternative_language: alternative_language.map(str::to_owned),
        right_to_left: properties
            .child("rtl")
            .map(|node| node.attr("val").and_then(bool_value).unwrap_or(true)),
        hyperlink,
        character_spacing_pt: spacing
            .and_then(|value| value.parse::<f64>().ok())
            .map(|value| value / 100.0),
        kerning_threshold_pt: kerning
            .and_then(|value| value.parse::<f64>().ok())
            .map(|value| value / 100.0),
    }
}

fn text_run(text: String, style: RunStyle) -> TextRun {
    TextRun {
        text,
        math: None,
        field: None,
        font_family: style.font_family,
        east_asian_font_family: style.east_asian_font_family,
        complex_script_font_family: style.complex_script_font_family,
        symbol_font_family: style.symbol_font_family,
        font_size_pt: style.font_size_pt,
        bold: style.bold,
        italic: style.italic,
        underline: style.underline,
        strike: style.strike,
        color: style.color,
        baseline: style.baseline,
        language: style.language,
        alternative_language: style.alternative_language,
        right_to_left: style.right_to_left,
        hyperlink: style.hyperlink,
        character_spacing_pt: style.character_spacing_pt,
        kerning_threshold_pt: style.kerning_threshold_pt,
    }
}

#[derive(Clone, Default)]
struct ParagraphStyle {
    alignment: Option<String>,
    level: Option<usize>,
    bullet: Option<Option<crate::TextBullet>>,
    line_spacing: Option<crate::TextSpacing>,
    space_before: Option<crate::TextSpacing>,
    space_after: Option<crate::TextSpacing>,
    rtl: Option<bool>,
    margin_left_emu: Option<i64>,
    indent_emu: Option<i64>,
    default_tab_size_emu: Option<i64>,
    tab_stops: Option<Vec<crate::TextTabStop>>,
    east_asian_line_break: Option<bool>,
    latin_line_break: Option<bool>,
    hanging_punctuation: Option<bool>,
}

impl ParagraphStyle {
    fn overlay(mut self, other: ParagraphStyle) -> Self {
        if other.alignment.is_some() {
            self.alignment = other.alignment;
        }
        if other.level.is_some() {
            self.level = other.level;
        }
        if other.bullet.is_some() {
            self.bullet = other.bullet;
        }
        if other.line_spacing.is_some() {
            self.line_spacing = other.line_spacing;
        }
        if other.space_before.is_some() {
            self.space_before = other.space_before;
        }
        if other.space_after.is_some() {
            self.space_after = other.space_after;
        }
        if other.rtl.is_some() {
            self.rtl = other.rtl;
        }
        if other.margin_left_emu.is_some() {
            self.margin_left_emu = other.margin_left_emu;
        }
        if other.indent_emu.is_some() {
            self.indent_emu = other.indent_emu;
        }
        if other.default_tab_size_emu.is_some() {
            self.default_tab_size_emu = other.default_tab_size_emu;
        }
        if other.tab_stops.is_some() {
            self.tab_stops = other.tab_stops;
        }
        if other.east_asian_line_break.is_some() {
            self.east_asian_line_break = other.east_asian_line_break;
        }
        if other.latin_line_break.is_some() {
            self.latin_line_break = other.latin_line_break;
        }
        if other.hanging_punctuation.is_some() {
            self.hanging_punctuation = other.hanging_punctuation;
        }
        self
    }
}

fn spacing_value(node: Option<XmlNode<'_>>) -> Option<crate::TextSpacing> {
    let node = node?;
    node.child("spcPts")
        .and_then(|value| value.attr("val"))
        .and_then(|value| value.parse::<f64>().ok())
        .map(|value| crate::TextSpacing {
            value: value / 100.0,
            unit: crate::TextSpacingUnit::Points,
        })
        .or_else(|| {
            node.child("spcPct")
                .and_then(|value| value.attr("val"))
                .and_then(percentage)
                .map(|value| crate::TextSpacing {
                    value,
                    unit: crate::TextSpacingUnit::Percent,
                })
        })
}

fn parse_paragraph_style(properties: Option<XmlNode<'_>>) -> ParagraphStyle {
    let Some(properties) = properties else {
        return ParagraphStyle::default();
    };
    let alignment = properties.attr("algn").and_then(|value| {
        match value {
            "l" => Some("left"),
            "ctr" => Some("center"),
            "r" => Some("right"),
            "just" | "justLow" => Some("justify"),
            "dist" | "thaiDist" => Some("distributed"),
            _ => None,
        }
        .map(str::to_owned)
    });
    let bullet_font = properties
        .child("buFont")
        .and_then(|font| font.attr("typeface"))
        .map(str::to_owned);
    let bullet_size_pt = properties
        .child("buSzPts")
        .and_then(|size| size.attr("val"))
        .and_then(|value| value.parse::<f64>().ok())
        .map(|value| value / 100.0);
    let bullet_size_percent = properties
        .child("buSzPct")
        .and_then(|size| size.attr("val"))
        .and_then(percentage);
    let bullet = if let Some(character) = properties.child("buChar") {
        Some(Some(crate::TextBullet {
            kind: "character".into(),
            value: character.attr("char").map(str::to_owned),
            font_family: bullet_font.clone(),
            font_size_pt: bullet_size_pt,
            size_percent: bullet_size_percent,
            start_at: None,
        }))
    } else if let Some(number) = properties.child("buAutoNum") {
        Some(Some(crate::TextBullet {
            kind: "number".into(),
            value: number.attr("type").map(str::to_owned),
            font_family: bullet_font.clone(),
            font_size_pt: bullet_size_pt,
            size_percent: bullet_size_percent,
            start_at: number
                .attr("startAt")
                .and_then(|value| value.parse::<usize>().ok()),
        }))
    } else if properties.child("buBlip").is_some() {
        Some(Some(crate::TextBullet {
            kind: "picture".into(),
            value: None,
            font_family: bullet_font,
            font_size_pt: bullet_size_pt,
            size_percent: bullet_size_percent,
            start_at: None,
        }))
    } else if properties.child("buNone").is_some() {
        Some(None)
    } else {
        None
    };
    let tab_stops = properties.child("tabLst").map(|list| {
        list.children_named("tab")
            .filter_map(|tab| {
                let position_emu = tab.attr("pos")?.parse::<i64>().ok()?;
                let alignment = match tab.attr("algn") {
                    Some("ctr") => crate::TextTabAlignment::Center,
                    Some("r") => crate::TextTabAlignment::Right,
                    Some("dec") => crate::TextTabAlignment::Decimal,
                    _ => crate::TextTabAlignment::Left,
                };
                Some(crate::TextTabStop {
                    position_emu,
                    alignment,
                })
            })
            .collect()
    });
    ParagraphStyle {
        alignment,
        level: properties
            .attr("lvl")
            .and_then(|value| value.parse::<usize>().ok()),
        bullet,
        line_spacing: spacing_value(properties.child("lnSpc")),
        space_before: spacing_value(properties.child("spcBef")),
        space_after: spacing_value(properties.child("spcAft")),
        rtl: properties.attr("rtl").and_then(bool_value),
        margin_left_emu: properties
            .attr("marL")
            .and_then(|value| value.parse::<i64>().ok()),
        indent_emu: properties
            .attr("indent")
            .and_then(|value| value.parse::<i64>().ok()),
        default_tab_size_emu: properties
            .attr("defTabSz")
            .and_then(|value| value.parse::<i64>().ok()),
        tab_stops,
        east_asian_line_break: properties.attr("eaLnBrk").and_then(bool_value),
        latin_line_break: properties.attr("latinLnBrk").and_then(bool_value),
        hanging_punctuation: properties.attr("hangingPunct").and_then(bool_value),
    }
}

// Layout and master placeholders inherit through their `a:lstStyle` only.
// Their `a:p` children are prompt text ("Click to edit Master text styles",
// "Second level", ...), not styles: paragraph N of a slide must not pick up
// the `lvl`/`marL` of prompt paragraph N.
fn fallback_paragraph_properties(text_body: Option<XmlNode<'_>>, level: usize) -> Option<XmlNode<'_>> {
    text_body?
        .child("lstStyle")
        .and_then(|style| style.child(&format!("lvl{}pPr", level.saturating_add(1))))
}

fn style_level_properties(style: Option<XmlNode<'_>>, level: usize) -> Option<XmlNode<'_>> {
    style.and_then(|style| {
        style
            .child(&format!("lvl{}pPr", level.saturating_add(1)))
            .or_else(|| style.child("defPPr"))
    })
}

struct TextParseContext<'a, 'color> {
    source_part: Option<&'a str>,
    presentation_text_style: Option<XmlNode<'a>>,
    shape_default_style: RunStyle,
    colors: Option<&'a ColorContext<'color>>,
    rels: &'a Relationships,
}

fn parse_text_paragraphs(
    text_body: Option<XmlNode<'_>>,
    layout_text_body: Option<XmlNode<'_>>,
    master_text_body: Option<XmlNode<'_>>,
    master_text_style: Option<XmlNode<'_>>,
    context: &TextParseContext<'_, '_>,
) -> Vec<TextParagraph> {
    let Some(text_body) = text_body else {
        return Vec::new();
    };
    text_body
        .children_named("p")
        .map(|paragraph| {
            let own_properties = paragraph.child("pPr");
            let level = own_properties
                .and_then(|properties| properties.attr("lvl"))
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            let master_style_properties = style_level_properties(master_text_style, level);
            let presentation_style_properties =
                style_level_properties(context.presentation_text_style, level);
            let master_properties = fallback_paragraph_properties(master_text_body, level);
            let layout_properties = fallback_paragraph_properties(layout_text_body, level);
            let paragraph_style = parse_paragraph_style(presentation_style_properties)
                .overlay(parse_paragraph_style(master_style_properties))
                .overlay(parse_paragraph_style(master_properties))
                .overlay(parse_paragraph_style(layout_properties))
                .overlay(parse_paragraph_style(own_properties));
            // Presentation defaults are the base of the cascade. An explicit
            // shape or table-cell style must override them, not vice versa.
            let default_style = parse_run_style(
                presentation_style_properties.and_then(|properties| properties.child("defRPr")),
                context.colors,
                context.rels,
            )
            .overlay(context.shape_default_style.clone())
            .overlay(parse_run_style(
                master_style_properties.and_then(|properties| properties.child("defRPr")),
                context.colors,
                context.rels,
            ))
            .overlay(parse_run_style(
                master_properties.and_then(|properties| properties.child("defRPr")),
                context.colors,
                context.rels,
            ))
            .overlay(parse_run_style(
                layout_properties.and_then(|properties| properties.child("defRPr")),
                context.colors,
                context.rels,
            ))
            .overlay(parse_run_style(
                own_properties.and_then(|properties| properties.child("defRPr")),
                context.colors,
                context.rels,
            ));
            let mut runs = Vec::new();
            let mut children = paragraph.children().iter();
            let mut branches = Vec::new();
            loop {
                let Some(child) = children.next() else {
                    if let Some(previous) = branches.pop() { children = previous; continue; }
                    break;
                };
                match child.name() {
                    "AlternateContent" => {
                        if let Some(branch) = ["Choice", "Fallback"].into_iter().flat_map(|name| child.children_named(name)).find(|branch| branch.children().iter().any(|child| matches!(child.name(), "r" | "fld" | "br" | "tab" | "t" | "m" | "oMath" | "oMathPara" | "AlternateContent"))) {
                            branches.push(children);
                            children = branch.children().iter();
                        }
                    }
                    "fld" if fields::push_run(child, &default_style, context, &mut runs) => {}
                    "r" | "fld" => {
                        let style = default_style.clone().overlay(parse_run_style(
                            child.child("rPr"),
                            context.colors,
                            context.rels,
                        ));
                        let value = child
                            .children_named("t")
                            .map(XmlNode::text_content)
                            .collect::<String>();
                        runs.push(text_run(value, style));
                    }
                    "br" => {
                        let style = default_style.clone().overlay(parse_run_style(
                            child.child("rPr"),
                            context.colors,
                            context.rels,
                        ));
                        runs.push(text_run("\n".into(), style));
                    }
                    "tab" => {
                        let style = default_style.clone().overlay(parse_run_style(
                            child.child("rPr"),
                            context.colors,
                            context.rels,
                        ));
                        runs.push(text_run("\t".into(), style));
                    }
                    "m" | "oMath" | "oMathPara" => {
                        let source = if matches!(child.name(), "oMath" | "oMathPara") { Some(child) }
                            else { child.descendant("oMathPara").or_else(|| child.descendant("oMath")) };
                        if let Some(source) = source {
                            let source_xml = source.source_xml().unwrap_or_default();
                            let math = rust_office_art::parse_office_math(source_xml).unwrap_or_else(||
                                rust_office_art::OfficeMath::unparsed(source_xml.into(), source.text_content(), source.name() == "oMathPara"));
                            if !math.unsupported.is_empty() {
                                if let Some(colors) = context.colors {
                                    colors.diagnostics.warn("math-fallback", "Equation has features retained in the source that are not rendered in MathML.", Some("math"));
                                }
                            }
                            let style = default_style.clone().overlay(parse_run_style(child.descendant("rPr"), context.colors, context.rels));
                            let mut run = text_run(math.text.clone(), style);
                            run.font_size_pt = math.font_size_pt.or(run.font_size_pt);
                            run.math = Some(Box::new(math));
                            runs.push(run);
                        }
                    }
                    "t" => runs.push(text_run(child.text_content(), default_style.clone())),
                    _ => {}
                }
            }
            let alignment = runs.iter().filter_map(|run| run.math.as_deref()).find_map(|math| math.justification.as_deref()).and_then(|value| match value {
                    "center" | "centerGroup" => Some("center".to_owned()),
                    "left" => Some("left".to_owned()),
                    "right" => Some("right".to_owned()),
                    _ => None,
                }).or(paragraph_style.alignment);
            TextParagraph {
                runs,
                alignment,
                level: paragraph_style
                    .level
                    .or(Some(level))
                    .filter(|level| *level > 0),
                bullet: paragraph_style.bullet.flatten(),
                line_spacing: paragraph_style.line_spacing,
                space_before: paragraph_style.space_before,
                space_after: paragraph_style.space_after,
                rtl: paragraph_style.rtl,
                margin_left_emu: paragraph_style.margin_left_emu,
                indent_emu: paragraph_style.indent_emu,
                default_tab_size_emu: paragraph_style.default_tab_size_emu,
                tab_stops: paragraph_style.tab_stops,
                east_asian_line_break: paragraph_style.east_asian_line_break,
                latin_line_break: paragraph_style.latin_line_break,
                hanging_punctuation: paragraph_style.hanging_punctuation,
            }
        })
        .collect()
}

fn inherited_body_attribute<'a>(bodies: &[Option<XmlNode<'a>>], name: &str) -> Option<&'a str> {
    bodies.iter().find_map(|body| {
        body.and_then(|body| body.child("bodyPr"))
            .and_then(|properties| properties.attr(name))
    })
}

fn vertical_alignment_from_value(value: Option<&str>) -> Option<VerticalAlignment> {
    match value {
        Some("ctr") | Some("just") | Some("dist") => Some(VerticalAlignment::Middle),
        Some("b") => Some(VerticalAlignment::Bottom),
        Some("t") => Some(VerticalAlignment::Top),
        _ => None,
    }
}

fn inherited_vertical_alignment(bodies: &[Option<XmlNode<'_>>]) -> Option<VerticalAlignment> {
    vertical_alignment_from_value(inherited_body_attribute(bodies, "anchor"))
}

fn inherited_text_insets(bodies: &[Option<XmlNode<'_>>]) -> Option<crate::TextInsets> {
    if bodies.iter().all(Option::is_none) {
        return None;
    }
    let edge = |name: &str, default: i64| {
        inherited_body_attribute(bodies, name)
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(default)
    };
    Some(crate::TextInsets {
        top: edge("tIns", 45_720),
        right: edge("rIns", 91_440),
        bottom: edge("bIns", 45_720),
        left: edge("lIns", 91_440),
    })
}

fn inherited_autofit(bodies: &[Option<XmlNode<'_>>]) -> Option<crate::TextAutofit> {
    let autofit = bodies.iter().find_map(|body| {
        let properties = body.and_then(|body| body.child("bodyPr"))?;
        if properties.child("noAutofit").is_some() {
            Some(crate::TextAutofit {
                mode: crate::TextAutoFitMode::None,
                font_scale: None,
                line_spacing_reduction: None,
            })
        } else if let Some(normal) = properties.child("normAutofit") {
            Some(crate::TextAutofit {
                mode: crate::TextAutoFitMode::Normal,
                font_scale: normal.attr("fontScale").and_then(percentage),
                line_spacing_reduction: normal.attr("lnSpcReduction").and_then(percentage),
            })
        } else if properties.child("spAutoFit").is_some() {
            Some(crate::TextAutofit {
                mode: crate::TextAutoFitMode::Shape,
                font_scale: None,
                line_spacing_reduction: None,
            })
        } else {
            None
        }
    });
    autofit.or_else(|| {
        bodies
            .iter()
            .any(Option::is_some)
            .then_some(crate::TextAutofit {
                mode: crate::TextAutoFitMode::None,
                font_scale: None,
                line_spacing_reduction: None,
            })
    })
}

fn shape_font_style(node: Option<XmlNode<'_>>, colors: Option<&ColorContext<'_>>) -> RunStyle {
    let Some(reference) = node
        .and_then(|node| node.child("style"))
        .and_then(|style| style.child("fontRef"))
    else {
        return RunStyle::default();
    };
    let fonts = match reference.attr("idx") {
        Some("major") => colors
            .and_then(|context| context.theme)
            .map(|theme| &theme.major_fonts),
        Some("minor") => colors
            .and_then(|context| context.theme)
            .map(|theme| &theme.minor_fonts),
        _ => None,
    };
    RunStyle {
        font_family: fonts.and_then(|fonts| fonts.get("latin")).cloned(),
        east_asian_font_family: fonts.and_then(|fonts| fonts.get("eastAsia")).cloned(),
        complex_script_font_family: fonts.and_then(|fonts| fonts.get("complexScript")).cloned(),
        color: parse_color(reference, colors),
        ..Default::default()
    }
}

fn shape_tree(root: XmlNode<'_>) -> Option<XmlNode<'_>> {
    root.child("cSld").and_then(|slide| slide.child("spTree"))
}

#[derive(Clone)]
struct PlaceholderKey<'a> {
    index: Option<&'a str>,
    kind: Option<&'a str>,
}

fn normalized_placeholder_kind(kind: &str) -> &str {
    match kind {
        "ctrTitle" | "title" => "title",
        "body" | "obj" | "subTitle" => "body",
        value => value,
    }
}

fn placeholder_key(node: XmlNode<'_>) -> Option<PlaceholderKey<'_>> {
    let placeholder = node.descendant("ph")?;
    Some(PlaceholderKey {
        index: placeholder.attr("idx"),
        kind: placeholder.attr("type").map(normalized_placeholder_kind),
    })
}

fn find_placeholder<'a>(root: XmlNode<'a>, source: XmlNode<'_>) -> Option<XmlNode<'a>> {
    let key = placeholder_key(source)?;
    let candidates = shape_tree(root)?.children().iter().filter(|candidate| {
        matches!(
            candidate.name(),
            "sp" | "cxnSp" | "pic" | "graphicFrame"
        )
    });
    if let Some(index) = key.index {
        if let Some(candidate) = candidates.clone().find(|candidate| {
            placeholder_key(*candidate).and_then(|value| value.index) == Some(index)
        }) {
            return Some(candidate);
        }
    }
    key.kind.and_then(|kind| {
        candidates
            .into_iter()
            .find(|candidate| placeholder_key(*candidate).and_then(|value| value.kind) == Some(kind))
    })
}

fn master_text_style<'a>(master: Option<XmlNode<'a>>, source: XmlNode<'_>) -> Option<XmlNode<'a>> {
    let styles = master?.child("txStyles")?;
    // `p:ph@type` defaults to "obj" (ECMA-376 CT_Placeholder), so a typeless
    // placeholder such as python-pptx's `<p:ph idx="1"/>` content placeholder
    // takes the body style. Only non-placeholder shapes use otherStyle.
    match placeholder_key(source).map(|key| key.kind.unwrap_or("body")) {
        Some(kind) if kind == "title" => styles.child("titleStyle"),
        Some(kind) if kind == "body" => styles.child("bodyStyle"),
        _ => styles.child("otherStyle"),
    }
}

fn apply_transform(target: &mut Transform, xfrm: Option<XmlNode<'_>>) {
    let Some(xfrm) = xfrm else {
        return;
    };
    if let Some(off) = xfrm.child("off") {
        if let Some(value) = off.attr("x").and_then(|value| value.parse().ok()) {
            target.x = value;
        }
        if let Some(value) = off.attr("y").and_then(|value| value.parse().ok()) {
            target.y = value;
        }
    }
    if let Some(ext) = xfrm.child("ext") {
        if let Some(value) = ext.attr("cx").and_then(|value| value.parse().ok()) {
            target.width = value;
        }
        if let Some(value) = ext.attr("cy").and_then(|value| value.parse().ok()) {
            target.height = value;
        }
    }
    if let Some(value) = xfrm.attr("rot").and_then(|value| value.parse::<f64>().ok()) {
        target.rotation = Some(value / 60_000.0);
    }
    if let Some(value) = xfrm.attr("flipH").and_then(bool_value) {
        target.flip_horizontal = Some(value);
    }
    if let Some(value) = xfrm.attr("flipV").and_then(bool_value) {
        target.flip_vertical = Some(value);
    }
}

fn shape_xfrm(node: Option<XmlNode<'_>>) -> Option<XmlNode<'_>> {
    node?.child("spPr")?.child("xfrm")
}

fn inherited_shape_transform(
    own: XmlNode<'_>,
    layout: Option<XmlNode<'_>>,
    master: Option<XmlNode<'_>>,
) -> Transform {
    let mut transform = Transform::default();
    apply_transform(&mut transform, shape_xfrm(master));
    apply_transform(&mut transform, shape_xfrm(layout));
    apply_transform(&mut transform, shape_xfrm(Some(own)));
    transform
}

#[cfg(test)]
fn compact_path_number(value: f64) -> String {
    let mut output = String::new();
    push_compact_path_number(&mut output, value);
    output
}

/// Appends `value` formatted as `format!("{value:.8}")` with trailing zeros, a
/// trailing decimal point and a bare negative zero removed.
fn push_compact_path_number(output: &mut String, value: f64) {
    if push_fixed_eight(output, value) {
        return;
    }
    let mut rendered = format!("{value:.8}");
    while rendered.contains('.') && rendered.ends_with('0') {
        rendered.pop();
    }
    if rendered.ends_with('.') {
        rendered.pop();
    }
    if rendered == "-0" {
        output.push('0');
    } else {
        output.push_str(&rendered);
    }
}

/// Exact fixed-point rendering of finite values below 2^63 to eight decimals.
/// The binary value is scaled by 10^8 in integer arithmetic and rounded half
/// to even, which is the rounding the standard formatter applies to the exact
/// decimal expansion. Returns false when the caller must use the formatter.
fn push_fixed_eight(output: &mut String, value: f64) -> bool {
    const SCALE: u128 = 100_000_000;
    if !value.is_finite() {
        return false;
    }
    let bits = value.to_bits();
    let negative = bits >> 63 == 1;
    let biased = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1u64 << 52) - 1);
    let (mantissa, exponent) = if biased == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1u64 << 52), biased - 1075)
    };
    let scaled = if exponent >= 0 {
        if exponent > 10 {
            return false;
        }
        ((mantissa as u128) << exponent) * SCALE
    } else {
        let shift = (-exponent) as u32;
        let numerator = (mantissa as u128) * SCALE;
        if shift > 120 {
            0
        } else {
            let quotient = numerator >> shift;
            let remainder = numerator & ((1u128 << shift) - 1);
            let half = 1u128 << (shift - 1);
            if remainder > half || (remainder == half && quotient & 1 == 1) {
                quotient + 1
            } else {
                quotient
            }
        }
    };
    if negative && scaled != 0 {
        output.push('-');
    }
    let mut integer = scaled / SCALE;
    let mut fractional = scaled % SCALE;
    let mut digits = [0u8; 40];
    let mut length = 0;
    loop {
        digits[length] = b'0' + (integer % 10) as u8;
        length += 1;
        integer /= 10;
        if integer == 0 {
            break;
        }
    }
    for index in (0..length).rev() {
        output.push(digits[index] as char);
    }
    if fractional != 0 {
        let mut width = 8;
        while fractional % 10 == 0 {
            fractional /= 10;
            width -= 1;
        }
        output.push('.');
        let mut buffer = [b'0'; 8];
        let mut position = width;
        while fractional != 0 {
            position -= 1;
            buffer[position] = b'0' + (fractional % 10) as u8;
            fractional /= 10;
        }
        for byte in &buffer[..width] {
            output.push(*byte as char);
        }
    }
    true
}

fn custom_path_point(node: XmlNode<'_>) -> Option<(f64, f64)> {
    Some((
        node.attr("x")?.parse::<f64>().ok()?,
        node.attr("y")?.parse::<f64>().ok()?,
    ))
}

/// Converts DrawingML custom paths into SVG path data normalized to a 0..1
/// view box. Most authored freeforms use literal path coordinates; unsupported
/// guide formulas are skipped instead of degrading the shape to a rectangle.
fn custom_geometry_path(node: XmlNode<'_>) -> Option<String> {
    let paths = node.child("spPr")?.child("custGeom")?.child("pathLst")?;
    let mut output = String::new();
    let mut has_drawn_segment = false;
    fn push_command(output: &mut String, letter: &str) {
        if !output.is_empty() {
            output.push(' ');
        }
        output.push_str(letter);
    }
    for path in paths.children_named("path") {
        let width = path
            .attr("w")
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| *value > 0.0)?;
        let height = path
            .attr("h")
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| *value > 0.0)?;
        let normalized = |output: &mut String, (x, y): (f64, f64)| {
            output.push(' ');
            push_compact_path_number(output, x / width);
            output.push(' ');
            push_compact_path_number(output, y / height);
        };
        let mut current = None;
        for command in path.children() {
            match command.name() {
                "moveTo" => {
                    if let Some(point) = command.child("pt").and_then(custom_path_point) {
                        push_command(&mut output, "M");
                        normalized(&mut output, point);
                        current = Some(point);
                    }
                }
                "lnTo" => {
                    if let Some(point) = command.child("pt").and_then(custom_path_point) {
                        push_command(&mut output, "L");
                        normalized(&mut output, point);
                        current = Some(point);
                        has_drawn_segment = true;
                    }
                }
                "cubicBezTo" => {
                    let points = command
                        .children_named("pt")
                        .filter_map(custom_path_point)
                        .collect::<Vec<_>>();
                    if let [first, second, third] = points.as_slice() {
                        push_command(&mut output, "C");
                        normalized(&mut output, *first);
                        normalized(&mut output, *second);
                        normalized(&mut output, *third);
                        current = Some(*third);
                        has_drawn_segment = true;
                    }
                }
                "quadBezTo" => {
                    let points = command
                        .children_named("pt")
                        .filter_map(custom_path_point)
                        .collect::<Vec<_>>();
                    if let [first, second] = points.as_slice() {
                        push_command(&mut output, "Q");
                        normalized(&mut output, *first);
                        normalized(&mut output, *second);
                        current = Some(*second);
                        has_drawn_segment = true;
                    }
                }
                "arcTo" => {
                    let Some((start_x, start_y)) = current else {
                        continue;
                    };
                    let Some(radius_x) = command
                        .attr("wR")
                        .and_then(|value| value.parse::<f64>().ok())
                    else {
                        continue;
                    };
                    let Some(radius_y) = command
                        .attr("hR")
                        .and_then(|value| value.parse::<f64>().ok())
                    else {
                        continue;
                    };
                    let Some(start_angle) = command.attr("stAng").and_then(angle_degrees) else {
                        continue;
                    };
                    let Some(sweep_angle) = command.attr("swAng").and_then(angle_degrees) else {
                        continue;
                    };
                    let start_radians = start_angle.to_radians();
                    let end_radians = (start_angle + sweep_angle).to_radians();
                    let center_x = start_x - radius_x * start_radians.cos();
                    let center_y = start_y - radius_y * start_radians.sin();
                    let endpoint = (
                        center_x + radius_x * end_radians.cos(),
                        center_y + radius_y * end_radians.sin(),
                    );
                    push_command(&mut output, "A");
                    output.push(' ');
                    push_compact_path_number(&mut output, radius_x / width);
                    output.push(' ');
                    push_compact_path_number(&mut output, radius_y / height);
                    output.push_str(if sweep_angle.abs() > 180.0 { " 0 1" } else { " 0 0" });
                    output.push_str(if sweep_angle >= 0.0 { " 1" } else { " 0" });
                    normalized(&mut output, endpoint);
                    current = Some(endpoint);
                    has_drawn_segment = true;
                }
                "close" => push_command(&mut output, "Z"),
                _ => {}
            }
        }
    }
    (has_drawn_segment && !output.is_empty()).then_some(output)
}

fn shape_fill(
    node: XmlNode<'_>,
    colors: Option<&ColorContext<'_>>,
    rels: &Relationships,
) -> Option<Option<FillStyle>> {
    // The slide already paints its background, so a background-fill shape is
    // transparent in the normalized composition. Falling through to fillRef
    // here incorrectly paints it with the theme accent color.
    if node.attr("useBgFill").and_then(bool_value).unwrap_or(false) {
        return Some(Some(FillStyle::None));
    }
    if let Some(properties) = node.child("spPr") {
        let has_fill = [
            "noFill",
            "solidFill",
            "gradFill",
            "pattFill",
            "blipFill",
            "grpFill",
        ]
        .into_iter()
        .any(|name| properties.child(name).is_some());
        if has_fill {
            return Some(parse_fill(properties, colors, rels));
        }
    }
    node.child("style")
        .and_then(|style| style.child("fillRef"))
        .map(|fill| style_matrix_fill(fill, colors))
}

fn shape_line(node: XmlNode<'_>, colors: Option<&ColorContext<'_>>) -> Option<Option<LineStyle>> {
    if let Some(line) = node
        .child("spPr")
        .and_then(|properties| properties.child("ln"))
    {
        return Some(parse_line_node(line, colors));
    }
    node.child("style")
        .and_then(|style| style.child("lnRef"))
        .map(|line| style_matrix_line(line, colors))
}

/// Which node list a parse context fills, for on-demand artwork targeting.
#[derive(Clone, Copy)]
enum ArtworkPart<'a> {
    Slide(usize),
    Layout(&'a str),
    Master(&'a str),
}

/// How render-time node ids relate to the ids stored in the model.
#[derive(Clone, Copy)]
enum ArtworkIdMatch {
    Exact,
    Prefixed(&'static str),
    Any,
}

/// The assets a part can see while its nodes are parsed.
///
/// The parse loads assets part by part, so a slide parsed early never sees an
/// asset a later slide or layout loads. A retained render reproduces that view
/// through the insertion generation of each asset; the parse that fills the map
/// has no generations and sees everything loaded so far.
#[derive(Clone, Copy)]
struct AssetView<'a> {
    assets: &'a BTreeMap<String, PresentationAsset>,
    generations: Option<(&'a HashMap<String, u32>, u32)>,
}

impl<'a> AssetView<'a> {
    fn unfiltered(assets: &'a BTreeMap<String, PresentationAsset>) -> Self {
        Self {
            assets,
            generations: None,
        }
    }

    fn get(&self, path: &str) -> Option<&'a PresentationAsset> {
        let asset = self.assets.get(path)?;
        if let Some((generations, visible)) = self.generations {
            if generations.get(path).is_none_or(|generation| *generation > visible) {
                return None;
            }
        }
        Some(asset)
    }

    fn contains_key(&self, path: &str) -> bool {
        self.get(path).is_some()
    }

    fn len(&self) -> usize {
        self.assets.len()
    }
}

#[derive(Clone, Copy)]
struct NodeParseContext<'a, 'color> {
    artwork_target: Option<&'a crate::ArtworkTarget>,
    artwork_part: ArtworkPart<'a>,
    artwork_id_match: ArtworkIdMatch,
    shape_cache: &'a RefCell<paint::ShapeCache>,
    editing_cache: &'a RefCell<editing::Cache>,
    source_part: Option<&'a str>,
    options: &'a ParseOptions,
    slide_index: usize,
    paint_scale: (f64, f64),
    rels: &'a Relationships,
    related_parts: &'a HashMap<String, Vec<u8>>,
    related_relationships: &'a HashMap<String, Relationships>,
    assets: AssetView<'a>,
    chart_companions: &'a HashMap<String, ChartCompanionParts>,
    legacy_chart_defaults: bool,
    limits: &'a ParseLimits,
    colors: Option<&'a ColorContext<'color>>,
    diagnostics: &'a ParseDiagnostics,
    layout_root: Option<XmlNode<'a>>,
    master_root: Option<XmlNode<'a>>,
    layout_rels: Option<&'a Relationships>,
    master_rels: Option<&'a Relationships>,
    presentation_text_style: Option<XmlNode<'a>>,
    table_styles: Option<XmlNode<'a>>,
}

impl NodeParseContext<'_, '_> {
    /// Whether this node's artwork is generated in this parse.
    fn renders_artwork(&self, id: &str) -> bool {
        let Some(target) = self.artwork_target else {
            return !self.options.defer_artwork;
        };
        let part_matches = match self.artwork_part {
            ArtworkPart::Slide(index) => {
                target.slide_index == Some(index)
                    && target.layout_id.is_none()
                    && target.master_id.is_none()
            }
            ArtworkPart::Layout(path) => {
                target.slide_index.is_none() && target.layout_id.as_deref() == Some(path)
            }
            ArtworkPart::Master(path) => {
                target.slide_index.is_none()
                    && target.layout_id.is_none()
                    && target.master_id.as_deref() == Some(path)
            }
        };
        part_matches
            && match self.artwork_id_match {
                ArtworkIdMatch::Exact => target.node_id == id,
                ArtworkIdMatch::Prefixed(prefix) => {
                    target.node_id.strip_prefix(prefix).and_then(|rest| rest.strip_prefix(':')) == Some(id)
                }
                ArtworkIdMatch::Any => true,
            }
    }
}

fn parse_shape_node(
    node: XmlNode<'_>,
    node_index: usize,
    context: &NodeParseContext<'_, '_>,
) -> SlideNode {
    let (id, name) = node_identity(node, context.slide_index, node_index);
    let layout = context
        .layout_root
        .and_then(|root| find_placeholder(root, node));
    let master_source = layout.unwrap_or(node);
    let master = context
        .master_root
        .and_then(|root| find_placeholder(root, master_source));
    let text_body = node.child("txBody");
    let layout_text_body = layout.and_then(|shape| shape.child("txBody"));
    let master_text_body = master.and_then(|shape| shape.child("txBody"));
    let geometry = [Some(node), layout, master]
        .into_iter()
        .flatten()
        .find_map(|shape| {
            shape
                .child("spPr")
                .and_then(|properties| properties.child("prstGeom"))
                .and_then(|geometry| geometry.attr("prst"))
                .map(str::to_owned)
        });
    let fill = [
        (Some(node), Some(context.rels)),
        (layout, context.layout_rels),
        (master, context.master_rels),
    ]
    .into_iter()
    .find_map(|(shape, rels)| shape_fill(shape?, context.colors, rels?))
    .flatten();
    let line = [Some(node), layout, master]
        .into_iter()
        .flatten()
        .find_map(|shape| shape_line(shape, context.colors))
        .flatten();
    let text_bodies = [text_body, layout_text_body, master_text_body];
    let shape_default_style = shape_font_style(master, context.colors)
        .overlay(shape_font_style(layout, context.colors))
        .overlay(shape_font_style(Some(node), context.colors));
    let text_context = TextParseContext {
        source_part: context.source_part,
        presentation_text_style: context.presentation_text_style,
        shape_default_style,
        colors: context.colors,
        rels: context.rels,
    };
    let transform = inherited_shape_transform(node, layout, master);
    let render = context.renders_artwork(&id);
    let rendered = if render {
        let (properties_xml, style_xml) = shape_appearance_xml(
            &[
                (Some(node), Some(context.rels)),
                (layout, context.layout_rels),
                (master, context.master_rels),
            ],
            &transform,
        );
        paint::render_shape(
            properties_xml,
            style_xml,
            transform.width as f64 * context.paint_scale.0 / 9525.0,
            transform.height as f64 * context.paint_scale.1 / 9525.0,
            context,
        )
    } else {
        None
    };
    if render && rendered.is_none() && transform.width > 0 && transform.height > 0 {
        context.diagnostics.warn("unsupported-shape-appearance", "Shape paint could not be rendered by the shared engine; structured geometry, fill, line and source XML remain available.", Some("shape"));
    }
    let (shape_svg, paint_bounds) = rendered
        .map(|paint| (Some(paint.svg), paint.paint_bounds))
        .unwrap_or_default();
    SlideNode::Shape {
        metadata: Default::default(),
        id,
        name,
        transform,
        shape_svg,
        paint_bounds,
        source_xml: node.source_xml().map(str::to_owned),
        geometry: ShapeGeometry {
            preset: geometry,
            path: custom_geometry_path(node),
        },
        fill,
        line,
        paragraphs: parse_text_paragraphs(
            text_body,
            layout_text_body,
            master_text_body,
            master_text_style(context.master_root, master_source),
            &text_context,
        ),
        vertical_alignment: inherited_vertical_alignment(&text_bodies),
        text_insets: inherited_text_insets(&text_bodies),
        autofit: inherited_autofit(&text_bodies),
        text_rotation: inherited_body_attribute(&text_bodies, "rot").and_then(angle_degrees),
        text_transform: None,
        vertical_text: inherited_body_attribute(&text_bodies, "vert").map(str::to_owned),
        horizontal_overflow: inherited_body_attribute(&text_bodies, "horzOverflow")
            .map(str::to_owned),
        vertical_overflow: inherited_body_attribute(&text_bodies, "vertOverflow")
            .map(str::to_owned),
        text_wrap: text_bodies.iter().any(Option::is_some).then(|| {
            inherited_body_attribute(&text_bodies, "wrap")
                .unwrap_or("square")
                .to_owned()
        }),
        column_count: text_bodies.iter().any(Option::is_some).then(|| {
            inherited_body_attribute(&text_bodies, "numCol")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(1)
        }),
        column_spacing: inherited_body_attribute(&text_bodies, "spcCol")
            .and_then(|value| value.parse::<i64>().ok()),
        right_to_left_columns: text_bodies.iter().any(Option::is_some).then(|| {
            inherited_body_attribute(&text_bodies, "rtlCol")
                .and_then(bool_value)
                .unwrap_or(false)
        }),
        space_first_last_paragraph: text_bodies.iter().any(Option::is_some).then(|| {
            inherited_body_attribute(&text_bodies, "spcFirstLastPara")
                .and_then(bool_value)
                .unwrap_or(false)
        }),
    }
}

fn collect_model_assets(
    node: XmlNode<'_>,
    rels: &Relationships,
    paths: &mut BTreeMap<String, String>,
) {
    static MASK: OnceLock<u64> = OnceLock::new();
    let mask = *MASK.get_or_init(|| {
        [
            "model3d",
            "oleObj",
            "audioFile",
            "videoFile",
            "quickTimeFile",
            "wavAudioFile",
            "media",
            "snd",
            "sndTgt",
            "contentPart",
            "control",
        ]
        .iter()
        .fold(0, |mask, name| mask | XmlNode::name_bit(name.as_bytes()))
    });
    if node.subtree_names() & mask == 0 {
        return;
    }
    let relationship = match node.name() {
        "model3d" | "wavAudioFile" | "snd" | "sndTgt" => node.attr("embed"),
        "oleObj" | "contentPart" | "control" => node.attr("id"),
        "audioFile" | "videoFile" | "quickTimeFile" => node.attr("link"),
        "media" => node.attr("link").or_else(|| node.attr("embed")),
        _ => None,
    };
    if let Some(path) = relationship.and_then(|id| rels.internal(id)) {
        if node.name() == "model3d" {
            paths.insert(path.clone(), node.name().to_string());
        } else {
            paths
                .entry(path.clone())
                .or_insert_with(|| node.name().to_string());
        }
    }
    for child in node.children() {
        collect_model_assets(child, rels, paths);
    }
}

fn asset_content_type(path: &str, types: XmlNode<'_>) -> String {
    types
        .children_named("Override")
        .find(|node| {
            node.attr("PartName")
                .is_some_and(|name| name.trim_start_matches('/') == path)
        })
        .or_else(|| {
            types.children_named("Default").find(|node| {
                node.attr("Extension").is_some_and(|extension| {
                    path.rsplit('.')
                        .next()
                        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
                })
            })
        })
        .and_then(|node| node.attr("ContentType"))
        .map(str::to_owned)
        .unwrap_or_else(|| content_type(path))
}

fn load_picture_assets(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    paths: &BTreeMap<String, String>,
    assets: &mut BTreeMap<String, PresentationAsset>,
    limits: &ParseLimits,
    options: &ParseOptions,
    content_types: XmlNode<'_>,
    mut generations: Option<&mut AssetGenerations>,
) -> Result<(), ParseError> {
    if let Some(generations) = generations.as_deref_mut() {
        generations.advance();
    }
    for path in paths
        .keys()
        .filter(|path| !assets.contains_key(*path))
        .cloned()
        .collect::<Vec<_>>()
    {
        let content_type = asset_content_type(&path, content_types);
        if !options.include_3d_models
            && (paths[&path] == "model3d" || content_type.starts_with("model/"))
        {
            continue;
        }
        if let Some(data) = read_entry(archive, &path, limits)? {
            if let Some(generations) = generations.as_deref_mut() {
                generations.record(&path);
            }
            assets.insert(
                path.clone(),
                PresentationAsset {
                    id: path.clone(),
                    content_type,
                    byte_length: data.len(),
                    file_name: path.rsplit('/').next().map(str::to_owned),
                    data: Some(data.into()),
                },
            );
        }
    }
    Ok(())
}

struct OfficeArtColors<'a, 'b> {
    colors: Option<&'a ColorContext<'b>>,
    limits: &'a ParseLimits,
    assets: AssetView<'a>,
    relationships: &'a Relationships,
}

fn encode_asset_base64(bytes: &[u8]) -> String {
    #[cfg(any(not(target_arch = "wasm32"), target_feature = "simd128"))]
    {
        base64_simd::STANDARD.encode_to_string(bytes)
    }
    #[cfg(all(target_arch = "wasm32", not(target_feature = "simd128")))]
    {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }
}

impl rust_office_art::PictureContext for OfficeArtColors<'_, '_> {
    fn resolve_image(&self, relationship_id: &str) -> Option<String> {
        let id = self
            .relationships
            .get(relationship_id)
            .map(String::as_str)
            .unwrap_or(relationship_id);
        let asset = self.assets.get(id)?;
        Some(format!(
            "data:{};base64,{}",
            asset.content_type,
            encode_asset_base64(asset.data.as_ref()?)
        ))
    }

    fn resolve_color(&self, xml: &str) -> Option<rust_office_art::ResolvedColor> {
        let node = parse_xml_tree(xml.as_bytes().to_vec(), self.limits, "picture color").ok()?;
        let color = parse_color(node.root(), self.colors)?;
        Some(rust_office_art::ResolvedColor {
            color: color.value,
            opacity: color.alpha,
        })
    }
    fn resolve_style_reference(
        &self,
        kind: rust_office_art::StyleReferenceKind,
        index: u32,
        placeholder_xml: &str,
    ) -> Option<String> {
        let theme = self.colors?.theme?;
        let index = index as usize;
        let style = match kind {
            rust_office_art::StyleReferenceKind::Fill if index >= 1001 => {
                theme.background_fill_style(index - 1001)?
            }
            rust_office_art::StyleReferenceKind::Fill => {
                theme.fill_style(index.checked_sub(1)?)?
            }
            rust_office_art::StyleReferenceKind::Line => {
                theme.line_style(index.checked_sub(1)?)?
            }
            rust_office_art::StyleReferenceKind::Effect => {
                let style = theme.effect_style(index.checked_sub(1)?)?;
                if style.child("scene3d").is_some() || style.child("sp3d").is_some() {
                    return None;
                }
                style.child("effectLst")?
            }
        };
        let placeholder = parse_xml_tree(placeholder_xml.as_bytes().to_vec(), self.limits, "style color")
            .ok()
            .and_then(|node| parse_color(node.root(), self.colors));
        let mut output = String::new();
        office_art_node_xml_with_color(
            style,
            "a",
            &mut output,
            placeholder.as_ref(),
            Some(&theme.rels),
        );
        Some(output)
    }
}

fn office_art_node_xml(node: XmlNode<'_>, prefix: &str, output: &mut String) {
    office_art_node_xml_with_color(node, prefix, output, None, None);
}

fn office_art_node_xml_with_color(
    node: XmlNode<'_>,
    prefix: &str,
    output: &mut String,
    placeholder: Option<&ColorValue>,
    relationships: Option<&Relationships>,
) {
    let prefix = if node.name() == "useLocalDpi" {
        "a14"
    } else {
        prefix
    };
    let replacement = (node.name() == "schemeClr" && node.attr("val") == Some("phClr"))
        .then_some(placeholder)
        .flatten();
    let name = if replacement.is_some() {
        "srgbClr"
    } else {
        &node.name()
    };
    output.push('<');
    output.push_str(prefix);
    output.push(':');
    output.push_str(name);
    for (name, value) in node.attributes() {
        if value.starts_with("http://schemas.") || value.starts_with("http://purl.org/") {
            continue;
        }
        let relationship = matches!(name, "embed" | "link");
        let resolved = relationship
            .then(|| relationships.and_then(|rels| rels.get(value)))
            .flatten();
        let value = if let Some(resolved) = resolved {
            resolved.as_str()
        } else if name == "val" {
            replacement
                .map(|color| color.value.trim_start_matches('#'))
                .unwrap_or(value)
        } else {
            value
        };
        output.push(' ');
        if relationship {
            output.push_str("r:");
        }
        output.push_str(name);
        output.push_str("=\"");
        output.push_str(&quick_xml::escape::escape(value));
        output.push('"');
    }
    output.push('>');
    output.push_str(&quick_xml::escape::escape(node.text()));
    if let Some(alpha) = replacement.and_then(|color| color.alpha) {
        let _ = write!(output, r#"<a:alpha val="{}"/>"#, (alpha * 100000.0).round());
    }
    for child in node.children() {
        office_art_node_xml_with_color(child, "a", output, placeholder, relationships);
    }
    output.push_str("</");
    output.push_str(prefix);
    output.push(':');
    output.push_str(name);
    output.push('>');
}

fn shape_appearance_xml(
    sources: &[(Option<XmlNode<'_>>, Option<&Relationships>)],
    frame: &Transform,
) -> (String, String) {
    let mut properties = format!(
        r#"<a:spPr><a:xfrm rot="{}" flipH="{}" flipV="{}"><a:off x="0" y="0"/><a:ext cx="{}" cy="{}"/></a:xfrm>"#,
        frame.rotation.unwrap_or(0.0) * 60000.0,
        frame.flip_horizontal.unwrap_or(false) as u8,
        frame.flip_vertical.unwrap_or(false) as u8,
        frame.width,
        frame.height
    );
    let mut inherited = std::collections::HashSet::new();
    if sources
        .first()
        .and_then(|(source, _)| *source)
        .and_then(|source| source.attr("useBgFill"))
        .and_then(bool_value)
        .unwrap_or(false)
    {
        properties.push_str("<a:noFill/>");
        inherited.insert("fill");
    }
    for (source, relationships) in sources
        .iter()
        .filter_map(|(source, rels)| Some(((*source)?.child("spPr")?, *rels)))
    {
        for child in source.children() {
            let key = match child.name() {
                "noFill" | "solidFill" | "gradFill" | "pattFill" | "blipFill" | "grpFill" => "fill",
                "custGeom" | "prstGeom" => "geometry",
                "effectLst" | "effectDag" => "effects",
                name => name,
            };
            if child.name() != "xfrm" && inherited.insert(key) {
                office_art_node_xml_with_color(child, "a", &mut properties, None, relationships);
            }
        }
    }
    properties.push_str("</a:spPr>");
    let mut style = "<a:style>".to_owned();
    inherited.clear();
    for source in sources
        .iter()
        .filter_map(|(source, _)| (*source)?.child("style"))
    {
        for child in source.children() {
            if inherited.insert(child.name()) {
                office_art_node_xml(child, "a", &mut style);
            }
        }
    }
    style.push_str("</a:style>");
    (properties, style)
}

fn picture_appearance_xml(
    fill: XmlNode<'_>,
    sources: &[Option<XmlNode<'_>>],
    frame: &Transform,
) -> String {
    let mut xml = "<pic:pic>".to_owned();
    office_art_node_xml(fill, "pic", &mut xml);
    xml.push_str(&format!(r#"<pic:spPr><a:xfrm rot="{}" flipH="{}" flipV="{}"><a:off x="0" y="0"/><a:ext cx="{}" cy="{}"/></a:xfrm>"#,
        frame.rotation.unwrap_or(0.0) * 60000.0, frame.flip_horizontal.unwrap_or(false) as u8,
        frame.flip_vertical.unwrap_or(false) as u8, frame.width, frame.height));
    let mut inherited = std::collections::HashSet::new();
    for properties in sources
        .iter()
        .flatten()
        .filter_map(|source| source.child("spPr"))
    {
        for child in properties.children() {
            if child.name() != "xfrm" && inherited.insert(child.name()) {
                office_art_node_xml(child, "a", &mut xml);
            }
        }
    }
    xml.push_str("</pic:spPr></pic:pic>");
    xml
}

fn picture_needs_rendering(
    fill: XmlNode<'_>,
    sources: &[Option<XmlNode<'_>>],
    frame: &Transform,
) -> bool {
    frame.rotation.is_some_and(|value| value != 0.0)
        || frame.flip_horizontal.unwrap_or(false)
        || frame.flip_vertical.unwrap_or(false)
        || fill.child("tile").is_some()
        || image_crop(fill).is_some()
        || fill
            .child("stretch")
            .and_then(|node| node.child("fillRect"))
            .is_some_and(|node| !node.attributes().is_empty())
        || fill
            .child("blip")
            .is_some_and(|node| node.children().iter().any(|child| child.name() != "extLst"))
        || sources
            .iter()
            .flatten()
            .filter_map(|node| node.child("spPr"))
            .any(|properties| {
                properties
                    .children()
                    .iter()
                    .any(|child| match child.name() {
                        "custGeom" | "scene3d" | "sp3d" | "effectDag" | "ln" => true,
                        "effectLst" => !child.children().is_empty(),
                        "prstGeom" => child.attr("prst").is_some_and(|value| value != "rect"),
                        _ => false,
                    })
            })
}

fn parse_model3d_node(
    frame: XmlNode<'_>,
    model: XmlNode<'_>,
    node_index: usize,
    context: &NodeParseContext<'_, '_>,
) -> SlideNode {
    let (id, name) = node_identity(frame, context.slide_index, node_index);
    let transform = parse_transform(frame.child("xfrm").or_else(|| {
        model
            .child("spPr")
            .and_then(|properties| properties.child("xfrm"))
    }));
    let asset_id = model
        .child("raster")
        .and_then(|raster| raster.child("blip"))
        .and_then(|blip| blip.attr("embed"))
        .and_then(|id| context.rels.get(id))
        .filter(|path| context.assets.contains_key(*path))
        .cloned();
    let source = model.source_xml().unwrap_or_default();
    let model_3d = rust_office_art::model3d::parse_model3d(source, context.options, |id| {
        let path = context.rels.get(id)?;
        let asset = context.assets.get(path)?;
        Some(rust_office_art::model3d::Model3DAsset {
            part_name: path.clone(),
            content_type: asset.content_type.clone(),
            data: asset.data.clone(),
        })
    })
    .map(Box::new);
    if model_3d
        .as_ref()
        .is_some_and(|model| model.model_relationship_id.is_some() && model.asset.is_none())
    {
        context.diagnostics.warn(
            "missing-model3d-asset",
            "The embedded 3D model asset could not be resolved.",
            Some("model3d"),
        );
    }
    if asset_id.is_none() {
        context.diagnostics.warn(
            "missing-model3d-preview",
            "The 3D model has no available saved preview.",
            Some("model3d"),
        );
    }
    SlideNode::Image {
        metadata: Default::default(),
        model_3d,
        ink: None,
        id,
        name,
        transform,
        asset_id: asset_id.unwrap_or_else(|| "missing-asset".into()),
        fallback_asset_id: None,
        crop: None,
        opacity: None,
        preserve_aspect_ratio: false,
        effects: None,
        picture_svg: None,
        fallback_picture: None,
        paint_bounds: None,
        source_xml: model.source_xml().map(str::to_owned),
    }
}

fn svg_image_relationship(node: XmlNode<'_>) -> Option<String> {
    let source = node.source_xml()?;
    let mut reader = NsReader::from_reader(source.as_bytes());
    let mut depth = 0;
    loop {
        let (namespace, event) = reader.read_resolved_event().ok()?;
        match event {
            Event::Start(start) | Event::Empty(start) if depth == 1 => {
                if !matches!(namespace, ResolveResult::Bound(ns) if ns.as_ref() == b"http://schemas.microsoft.com/office/drawing/2016/SVG/main")
                    || start.local_name().as_ref() != b"svgBlip"
                {
                    return None;
                }
                return start
                    .attributes()
                    .with_checks(false)
                    .flatten()
                    .find_map(|attribute| {
                        let (namespace, name) = reader.resolve_attribute(attribute.key);
                        (name.as_ref() == b"embed"
                            && matches!(namespace, ResolveResult::Bound(ns) if matches!(ns.as_ref(),
                        b"http://schemas.openxmlformats.org/officeDocument/2006/relationships"
                        | b"http://purl.oclc.org/ooxml/officeDocument/relationships")))
                        .then(|| {
                            attribute
                                .unescape_value()
                                .ok()
                                .map(|value| value.into_owned())
                        })
                        .flatten()
                    });
            }
            Event::Start(_) => depth += 1,
            Event::End(_) | Event::Eof => return None,
            _ => {}
        }
    }
}

fn parse_image_node(
    node: XmlNode<'_>,
    node_index: usize,
    context: &NodeParseContext<'_, '_>,
) -> SlideNode {
    let (id, name) = node_identity(node, context.slide_index, node_index);
    let layout = context
        .layout_root
        .and_then(|root| find_placeholder(root, node));
    let master_source = layout.unwrap_or(node);
    let master = context
        .master_root
        .and_then(|root| find_placeholder(root, master_source));
    let resolved_fill = [
        (Some(node), Some(context.rels)),
        (layout, context.layout_rels),
        (master, context.master_rels),
    ]
    .into_iter()
    .find_map(|(source, rels)| {
        let fill = source?.child("blipFill")?;
        let blip = fill.child("blip")?;
        let rels = rels?;
        let alternative = blip.child("extLst").and_then(|extensions| {
            extensions.children_named("ext").find_map(|extension| {
                extension.children_named("svgBlip").find_map(|svg| {
                    svg_image_relationship(svg)
                        .and_then(|id| rels.get(&id))
                        .filter(|id| context.assets.contains_key(*id))
                })
            })
        });
        let original = blip
            .attr("embed")
            .or_else(|| blip.attr("link"))
            .and_then(|id| rels.get(id));
        let fallback_asset_id = alternative.and_then(|selected| {
            original
                .filter(|id| *id != selected && context.assets.contains_key(*id))
                .cloned()
        });
        let asset_id = alternative.or(original)?.clone();
        Some((fill, asset_id, fallback_asset_id))
    });
    let asset_id = resolved_fill
        .as_ref()
        .map(|(_, asset_id, _)| asset_id.clone());
    let fallback_asset_id = resolved_fill
        .as_ref()
        .and_then(|(_, _, fallback)| fallback.clone());
    let crop = resolved_fill
        .as_ref()
        .and_then(|(fill, _, _)| image_crop(*fill));
    let opacity = resolved_fill.as_ref().and_then(|(fill, _, _)| {
        fill.child("blip")
            .and_then(|blip| blip.child("alphaModFix"))
            .and_then(|effect| effect.attr("amt"))
            .and_then(percentage)
            .map(|value| value.clamp(0.0, 1.0))
    });
    let preserve_aspect_ratio = resolved_fill
        .as_ref()
        .map(|(fill, _, _)| fill.child("stretch").is_none())
        .unwrap_or(true);
    let effects = resolved_fill.as_ref().and_then(|(fill, _, _)| {
        fill.child("blip")
            .and_then(|blip| parse_blip_effects(blip, context.colors))
    });
    let transform = inherited_shape_transform(node, layout, master);
    let sources = [Some(node), layout, master];
    let render_asset = |fill: XmlNode<'_>, asset_id: &str| {
        let asset = context.assets.get(asset_id)?;
        if !picture_needs_rendering(fill, &sources, &transform)
            && !matches!(asset.content_type.as_str(), "image/tiff" | "image/tif")
        {
            return None;
        }
        let bytes = asset.data.as_ref()?;
        let source = format!(
            "data:{};base64,{}",
            asset.content_type,
            encode_asset_base64(bytes)
        );
        let appearance = picture_appearance_xml(fill, &sources, &transform);
        rust_office_art::render_picture(
            &appearance,
            &source,
            transform.width as f64 * context.paint_scale.0 / 9525.0,
            transform.height as f64 * context.paint_scale.1 / 9525.0,
            &OfficeArtColors {
                colors: context.colors,
                limits: context.limits,
                assets: context.assets,
                relationships: context.rels,
            },
        )
    };
    let render = context.renders_artwork(&id);
    let rendered = if render {
        resolved_fill
            .as_ref()
            .and_then(|(fill, asset_id, _)| render_asset(*fill, asset_id))
    } else {
        None
    };
    let fallback_picture = if render {
        resolved_fill.as_ref().and_then(|(fill, _, fallback)| {
            render_asset(*fill, fallback.as_deref()?).map(|picture| {
                Box::new(crate::PictureArtwork {
                    svg: picture.svg,
                    paint_bounds: picture.paint_bounds,
                })
            })
        })
    } else {
        None
    };
    if render
        && rendered.is_none()
        && resolved_fill
            .as_ref()
            .is_some_and(|(fill, _, _)| picture_needs_rendering(*fill, &sources, &transform))
    {
        let message = if fallback_picture.is_some() {
            "Preferred image appearance could not be generated; compatibility-image artwork is available."
        } else {
            "Picture appearance is retained in source XML; the available asset and basic effects are used as fallback."
        };
        context
            .diagnostics
            .warn("unsupported-picture-appearance", message, Some("image"));
    }
    let (picture_svg, paint_bounds) = rendered
        .map(|paint| (Some(paint.svg), paint.paint_bounds))
        .unwrap_or_default();
    if asset_id.is_none() {
        context.diagnostics.warn(
            "missing-asset",
            format!("Image {id} has no resolvable embedded asset."),
            Some("image"),
        );
    }
    SlideNode::Image {
        metadata: Default::default(),
        model_3d: None,
        ink: None,
        id,
        name,
        transform,
        asset_id: asset_id.unwrap_or_else(|| "missing-asset".into()),
        fallback_asset_id,
        crop,
        opacity,
        preserve_aspect_ratio,
        effects,
        picture_svg,
        fallback_picture,
        paint_bounds,
        source_xml: node.source_xml().map(str::to_owned),
    }
}

/// PowerPoint references predefined table styles by GUID and writes style
/// definitions only when the author customizes them.
const BUILTIN_MEDIUM_STYLE_2: &[(&str, &str, &str)] = &[(
    "{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}",
    "Medium Style 2 - Accent 1",
    "accent1",
)];

/// ECMA-376 / PowerPoint definition of the "Medium Style 2" family for one
/// accent: a 20 % tint body, 40 % tint odd bands, solid-accent header and
/// first/last column with bold light text, and light (lt1) 1 pt grid lines.
fn medium_style_2_xml(style_id: &str, style_name: &str, accent: &str) -> String {
    let line = |width: u32| {
        format!(
            r#"<a:ln w="{width}" cmpd="sng"><a:solidFill><a:schemeClr val="lt1"/></a:solidFill></a:ln>"#
        )
    };
    let thin = line(12_700);
    let thick = line(38_100);
    let tint = |value: u32| {
        format!(
            r#"<a:fill><a:solidFill><a:schemeClr val="{accent}"><a:tint val="{value}"/></a:schemeClr></a:solidFill></a:fill>"#
        )
    };
    let solid =
        format!(r#"<a:fill><a:solidFill><a:schemeClr val="{accent}"/></a:solidFill></a:fill>"#);
    let text = |bold: &str, color: &str| {
        format!(
            r#"<a:tcTxStyle{bold}><a:fontRef idx="minor"><a:prstClr val="black"/></a:fontRef><a:schemeClr val="{color}"/></a:tcTxStyle>"#
        )
    };
    let emphasis = text(r#" b="on""#, "lt1");
    format!(
        r#"<a:tblStyle xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" styleId="{style_id}" styleName="{style_name}"><a:wholeTbl>{whole_text}<a:tcStyle><a:tcBdr><a:left>{thin}</a:left><a:right>{thin}</a:right><a:top>{thin}</a:top><a:bottom>{thin}</a:bottom><a:insideH>{thin}</a:insideH><a:insideV>{thin}</a:insideV></a:tcBdr>{tint20}</a:tcStyle></a:wholeTbl><a:band1H><a:tcStyle><a:tcBdr/>{tint40}</a:tcStyle></a:band1H><a:band2H><a:tcStyle><a:tcBdr/></a:tcStyle></a:band2H><a:band1V><a:tcStyle><a:tcBdr/>{tint40}</a:tcStyle></a:band1V><a:band2V><a:tcStyle><a:tcBdr/></a:tcStyle></a:band2V><a:lastCol>{emphasis}<a:tcStyle><a:tcBdr/>{solid}</a:tcStyle></a:lastCol><a:firstCol>{emphasis}<a:tcStyle><a:tcBdr/>{solid}</a:tcStyle></a:firstCol><a:lastRow>{emphasis}<a:tcStyle><a:tcBdr><a:top>{thick}</a:top></a:tcBdr>{solid}</a:tcStyle></a:lastRow><a:firstRow>{emphasis}<a:tcStyle><a:tcBdr><a:bottom>{thick}</a:bottom></a:tcBdr>{solid}</a:tcStyle></a:firstRow></a:tblStyle>"#,
        whole_text = text("", "dk1"),
        tint20 = tint(20_000),
        tint40 = tint(40_000),
    )
}

fn builtin_table_style(style_id: &str) -> Option<XmlNode<'static>> {
    static STYLES: OnceLock<Vec<XmlDocument>> = OnceLock::new();
    STYLES
        .get_or_init(|| {
            BUILTIN_MEDIUM_STYLE_2
                .iter()
                .filter_map(|(id, name, accent)| {
                    parse_xml_tree(medium_style_2_xml(id, name, accent).as_bytes().to_vec(),
                        &ParseLimits::default(),
                        "built-in table style",
                    )
                    .ok()
                })
                .collect()
        })
        .iter()
        .map(XmlDocument::root)
        .find(|style| {
            style
                .attr("styleId")
                .is_some_and(|id| id.eq_ignore_ascii_case(style_id))
        })
}

fn selected_table_style<'a>(table: XmlNode<'_>, styles: Option<XmlNode<'a>>) -> Option<XmlNode<'a>> {
    let requested = table
        .child("tblPr")
        .and_then(|properties| properties.child("tableStyleId"))
        .map(XmlNode::text_content)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            styles
                .and_then(|styles| styles.attr("def"))
                .map(str::to_owned)
        })?;
    let requested = requested.trim();
    styles
        .and_then(|styles| {
            styles
                .children_named("tblStyle")
                .find(|style| style.attr("styleId") == Some(requested))
        })
        .or_else(|| builtin_table_style(requested))
}

fn table_flag(table: XmlNode<'_>, name: &str) -> bool {
    table
        .child("tblPr")
        .and_then(|properties| properties.attr(name))
        .and_then(bool_value)
        .unwrap_or(false)
}

fn table_style_regions<'a>(
    table: XmlNode<'_>,
    style: Option<XmlNode<'a>>,
    row_index: usize,
    column_index: usize,
    row_count: usize,
    column_count: usize,
) -> Vec<XmlNode<'a>> {
    let Some(style) = style else {
        return Vec::new();
    };
    let mut regions = Vec::new();
    if let Some(region) = style.child("wholeTbl") {
        regions.push(region);
    }
    let first_row = table_flag(table, "firstRow");
    let last_row = table_flag(table, "lastRow");
    let first_column = table_flag(table, "firstCol");
    let last_column = table_flag(table, "lastCol");
    if table_flag(table, "bandRow") {
        let band_index = row_index.saturating_sub(usize::from(first_row));
        let name = if band_index.is_multiple_of(2) {
            "band1H"
        } else {
            "band2H"
        };
        if let Some(region) = style.child(name) {
            regions.push(region);
        }
    }
    if table_flag(table, "bandCol") {
        let band_index = column_index.saturating_sub(usize::from(first_column));
        let name = if band_index.is_multiple_of(2) {
            "band1V"
        } else {
            "band2V"
        };
        if let Some(region) = style.child(name) {
            regions.push(region);
        }
    }
    if first_column && column_index == 0 {
        if let Some(region) = style.child("firstCol") {
            regions.push(region);
        }
    }
    if last_column && column_index.saturating_add(1) == column_count {
        if let Some(region) = style.child("lastCol") {
            regions.push(region);
        }
    }
    if first_row && row_index == 0 {
        if let Some(region) = style.child("firstRow") {
            regions.push(region);
        }
    }
    if last_row && row_index.saturating_add(1) == row_count {
        if let Some(region) = style.child("lastRow") {
            regions.push(region);
        }
    }
    let corner = match (
        row_index == 0,
        row_index.saturating_add(1) == row_count,
        column_index == 0,
        column_index.saturating_add(1) == column_count,
    ) {
        (true, _, true, _) => Some("nwCell"),
        (true, _, _, true) => Some("neCell"),
        (_, true, true, _) => Some("swCell"),
        (_, true, _, true) => Some("seCell"),
        _ => None,
    };
    if let Some(region) = corner.and_then(|name| style.child(name)) {
        regions.push(region);
    }
    regions
}

fn table_region_fill(region: XmlNode<'_>, context: &NodeParseContext<'_, '_>) -> Option<FillStyle> {
    let style = region.child("tcStyle")?;
    style
        .child("fill")
        .and_then(|fill| parse_fill(fill, context.colors, context.rels))
        .or_else(|| {
            style
                .child("fillRef")
                .and_then(|reference| style_matrix_fill(reference, context.colors))
        })
}

fn table_region_text_style(region: XmlNode<'_>, context: &NodeParseContext<'_, '_>) -> RunStyle {
    let Some(style) = region.child("tcTxStyle") else {
        return RunStyle::default();
    };
    let reference = style.child("fontRef");
    let fonts = match reference.and_then(|reference| reference.attr("idx")) {
        Some("major") => context
            .colors
            .and_then(|colors| colors.theme)
            .map(|theme| &theme.major_fonts),
        Some("minor") => context
            .colors
            .and_then(|colors| colors.theme)
            .map(|theme| &theme.minor_fonts),
        _ => None,
    };
    let color = style
        .children()
        .iter()
        .find(|child| {
            matches!(
                child.name(),
                "srgbClr" | "scrgbClr" | "hslClr" | "schemeClr" | "sysClr" | "prstClr"
            )
        })
        .and_then(|color| parse_color(color, context.colors))
        .or_else(|| reference.and_then(|reference| parse_color(reference, context.colors)));
    RunStyle {
        font_family: fonts.and_then(|fonts| fonts.get("latin")).cloned(),
        east_asian_font_family: fonts.and_then(|fonts| fonts.get("eastAsia")).cloned(),
        complex_script_font_family: fonts.and_then(|fonts| fonts.get("complexScript")).cloned(),
        bold: style.attr("b").and_then(bool_value),
        italic: style.attr("i").and_then(bool_value),
        color,
        ..Default::default()
    }
}

fn table_region_borders(
    region: XmlNode<'_>,
    context: &NodeParseContext<'_, '_>,
    row_index: usize,
    column_index: usize,
    row_count: usize,
    column_count: usize,
) -> BTreeMap<String, LineStyle> {
    let Some(borders) = region
        .child("tcStyle")
        .and_then(|style| style.child("tcBdr"))
    else {
        return BTreeMap::new();
    };
    let mut result = BTreeMap::new();
    for (side, interior) in [
        ("top", (row_index > 0).then_some("insideH")),
        (
            "right",
            (column_index.saturating_add(1) < column_count).then_some("insideV"),
        ),
        (
            "bottom",
            (row_index.saturating_add(1) < row_count).then_some("insideH"),
        ),
        ("left", (column_index > 0).then_some("insideV")),
    ] {
        let line = borders
            .child(side)
            .or_else(|| interior.and_then(|name| borders.child(name)))
            .and_then(|container| container.child("ln"))
            .and_then(|line| parse_line_node(line, context.colors));
        if let Some(line) = line {
            result.insert(side.to_owned(), line);
        }
    }
    result
}

fn parse_table_cell(
    cell: XmlNode<'_>,
    context: &NodeParseContext<'_, '_>,
    regions: &[XmlNode<'_>],
    row_index: usize,
    column_index: usize,
    row_count: usize,
    column_count: usize,
) -> TableCell {
    let properties = cell.child("tcPr");
    let mut borders = BTreeMap::new();
    let mut style_fill = None;
    let mut text_style = RunStyle::default();
    for region in regions.iter().copied() {
        if let Some(fill) = table_region_fill(region, context) {
            style_fill = Some(fill);
        }
        borders.extend(table_region_borders(
            region,
            context,
            row_index,
            column_index,
            row_count,
            column_count,
        ));
        text_style = text_style.overlay(table_region_text_style(region, context));
    }
    if let Some(properties) = properties {
        for (element, side) in [
            ("lnT", "top"),
            ("lnR", "right"),
            ("lnB", "bottom"),
            ("lnL", "left"),
        ] {
            if let Some(line) = properties
                .child(element)
                .and_then(|line| parse_line_node(line, context.colors))
            {
                borders.insert(side.to_owned(), line);
            }
        }
    }
    let text_context = TextParseContext {
        source_part: context.source_part,
        presentation_text_style: context.presentation_text_style,
        shape_default_style: text_style,
        colors: context.colors,
        rels: context.rels,
    };
    TableCell {
        merge: None,
        row_span: cell
            .attr("rowSpan")
            .and_then(|value| value.parse().ok())
            .filter(|value| *value > 1),
        col_span: cell
            .attr("gridSpan")
            .and_then(|value| value.parse().ok())
            .filter(|value| *value > 1),
        fill: properties
            .and_then(|properties| parse_fill(properties, context.colors, context.rels))
            .or(style_fill),
        borders,
        paragraphs: parse_text_paragraphs(cell.child("txBody"), None, None, None, &text_context),
        text_insets: cell.child("txBody").map(|_| {
            let value = |name: &str, default: i64| {
                properties
                    .and_then(|properties| properties.attr(name))
                    .and_then(|value| value.parse::<i64>().ok())
                    .unwrap_or(default)
            };
            crate::TextInsets {
                top: value("marT", 45_720),
                right: value("marR", 91_440),
                bottom: value("marB", 45_720),
                left: value("marL", 91_440),
            }
        }),
        vertical_alignment: vertical_alignment_from_value(
            properties.and_then(|properties| properties.attr("anchor")),
        ),
        text_rotation: properties
            .and_then(|properties| properties.attr("vert"))
            .and_then(|value| match value {
                "vert270" => Some(270.0),
                "vert" | "eaVert" | "wordArtVert" | "wordArtVertRtl" => Some(90.0),
                _ => None,
            }),
    }
}

fn is_merge_continuation(cell: XmlNode<'_>) -> bool {
    ["hMerge", "vMerge"]
        .into_iter()
        .filter_map(|name| cell.attr(name))
        .any(|value| bool_value(value).unwrap_or(false))
}

#[derive(Clone, Copy)]
struct TableMergeOrigin {
    position: crate::TableCellPosition,
    rows: usize,
    columns: usize,
}

impl TableMergeOrigin {
    fn covers(self, row: usize, column: usize) -> bool {
        row >= self.position.row
            && column >= self.position.column
            && row - self.position.row < self.rows
            && column - self.position.column < self.columns
    }
}

fn parse_table_node(
    frame: XmlNode<'_>,
    table: XmlNode<'_>,
    node_index: usize,
    context: &NodeParseContext<'_, '_>,
) -> SlideNode {
    let (id, name) = node_identity(frame, context.slide_index, node_index);
    let column_widths = table
        .child("tblGrid")
        .into_iter()
        .flat_map(|grid| grid.children_named("gridCol"))
        .filter_map(|column| column.attr("w").and_then(|value| value.parse().ok()))
        .collect();
    let row_nodes = table.children_named("tr").collect::<Vec<_>>();
    let row_count = row_nodes.len();
    let column_count = table
        .child("tblGrid")
        .map(|grid| grid.children_named("gridCol").count())
        .filter(|count| *count > 0)
        .unwrap_or_else(|| {
            row_nodes
                .iter()
                .map(|row| row.children_named("tc").count())
                .max()
                .unwrap_or(0)
        });
    let style = selected_table_style(table, context.table_styles);
    let mut previous_origins: Vec<Option<TableMergeOrigin>> = Vec::new();
    let rows = row_nodes
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            let mut origins: Vec<Option<TableMergeOrigin>> = Vec::new();
            let cells = row
                .children_named("tc")
                .enumerate()
                .map(|(column_index, cell)| {
                    let regions = table_style_regions(
                        table,
                        style,
                        row_index,
                        column_index,
                        row_count,
                        column_count,
                    );
                    let mut parsed = parse_table_cell(
                        cell,
                        context,
                        &regions,
                        row_index,
                        column_index,
                        row_count,
                        column_count,
                    );
                    let origin = if is_merge_continuation(cell) {
                        let horizontal = cell.attr("hMerge").and_then(bool_value).unwrap_or(false);
                        let vertical = cell.attr("vMerge").and_then(bool_value).unwrap_or(false);
                        let left = origins
                            .last()
                            .copied()
                            .flatten()
                            .filter(|origin| origin.covers(row_index, column_index));
                        let above = previous_origins
                            .get(column_index)
                            .copied()
                            .flatten()
                            .filter(|origin| origin.covers(row_index, column_index));
                        let origin = match (horizontal, vertical) {
                            (true, false) => left,
                            (false, true) => above,
                            (true, true) => left
                                .zip(above)
                                .filter(|(left, above)| left.position == above.position)
                                .map(|(left, _)| left),
                            _ => None,
                        };
                        parsed.merge = Some(Box::new(crate::TableCellMerge {
                            horizontal,
                            vertical,
                            origin: origin.map(|origin| origin.position),
                        }));
                        origin
                    } else {
                        Some(TableMergeOrigin {
                            position: crate::TableCellPosition {
                                row: row_index,
                                column: column_index,
                            },
                            rows: parsed.row_span.unwrap_or(1),
                            columns: parsed.col_span.unwrap_or(1),
                        })
                    };
                    origins.push(origin);
                    parsed
                })
                .collect();
            previous_origins = origins;
            cells
        })
        .collect::<Vec<Vec<TableCell>>>();
    let row_heights = table
        .children_named("tr")
        .filter_map(|row| row.attr("h").and_then(|value| value.parse().ok()))
        .collect();
    SlideNode::Table {
        metadata: Default::default(),
        id,
        name,
        transform: parse_transform(frame.child("xfrm")),
        rows,
        column_widths,
        row_heights,
    }
}

fn indexed_cache_values(node: XmlNode<'_>) -> Vec<Option<String>> {
    let cache = [
        "strCache",
        "numCache",
        "strLit",
        "numLit",
        "multiLvlStrCache",
    ]
    .into_iter()
    .find_map(|name| node.descendant(name))
    .unwrap_or(node);
    let mut points = Vec::new();
    cache.collect_descendants("pt", &mut points);
    let declared = cache
        .descendant("ptCount")
        .and_then(|count| count.attr("val"))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let observed = points
        .iter()
        .filter_map(|point| point.attr("idx")?.parse::<usize>().ok())
        .max()
        .map(|index| index.saturating_add(1))
        .unwrap_or(0);
    let mut values = vec![None; declared.max(observed)];
    for point in points {
        let Some(index) = point
            .attr("idx")
            .and_then(|value| value.parse::<usize>().ok())
        else {
            continue;
        };
        if index >= values.len() {
            values.resize(index.saturating_add(1), None);
        }
        values[index] = point
            .child("v")
            .or_else(|| point.child("t"))
            .map(XmlNode::text_content);
    }
    values
}

fn category_cache_is_numeric(node: XmlNode<'_>) -> bool {
    node.descendant("numCache").is_some() || node.descendant("numLit").is_some()
}

fn chart_title(root: XmlNode<'_>) -> Option<String> {
    let title = root.descendant("title")?;
    let mut values = Vec::new();
    for name in ["t", "v"] {
        let mut nodes = Vec::new();
        title.collect_descendants(name, &mut nodes);
        values.extend(
            nodes
                .into_iter()
                .map(XmlNode::text_content)
                .filter(|value| !value.trim().is_empty()),
        );
    }
    (!values.is_empty()).then(|| values.join(""))
}

struct ParsedChart {
    chart_type: String,
    title: Option<String>,
    series: Vec<ChartSeries>,
    has_legend: Option<bool>,
    svg: Option<String>,
}

fn render_shared_chart(
    root: XmlNode<'_>,
    xml: &[u8],
    frame: &Transform,
    context: &NodeParseContext<'_, '_>,
    companions: Option<&ChartCompanionParts>,
) -> Option<String> {
    if frame.width <= 0 || frame.height <= 0 {
        return None;
    }
    let chart = root.child("chart");
    let group = chart
        .and_then(|chart| chart.child("plotArea"))
        .and_then(|plot| {
            plot.children()
                .iter()
                .find(|node| node.name().ends_with("Chart"))
        });

    let colors = context.colors;
    let theme = colors.and_then(|context| context.theme);
    let theme_colors = theme.map(|theme| {
        let mut palette: HashMap<_, _> = theme.colors.clone().into_iter().collect();
        if let Some(context) = colors {
            for (token, mapped) in context.color_map {
                if let Some(rgb) = theme.colors.get(mapped) {
                    palette.insert(token.clone(), rgb.clone());
                }
            }
        }
        palette
    });
    let xml = std::str::from_utf8(xml).ok()?;
    let automatic_title = chart.is_some_and(|chart| {
        chart
            .child("title")
            .is_some_and(|title| title.child("tx").is_none())
            && !matches!(
                chart
                    .child("autoTitleDeleted")
                    .and_then(|node| node.attr("val")),
                Some("1" | "true")
            )
            && group.is_some_and(|group| group.children_named("ser").count() == 1)
    });
    let data = automatic_title
        .then(|| {
            let mut data = rust_office_charts::parse_chart_xml(xml)?;
            data.title = data.series.first()?.name.clone();
            Some(data)
        })
        .flatten();
    let options = rust_office_charts::RenderOptions {
        width: Some(frame.width as f64 * context.paint_scale.0 / 9525.),
        height: Some(frame.height as f64 * context.paint_scale.1 / 9525.),
        theme_colors: theme_colors.as_ref(),
        font_family: theme
            .and_then(|theme| theme.minor_fonts.get("latin"))
            .map(String::as_str),
        style_xml: companions.and_then(|parts| parts.style_xml.as_deref()),
        color_style_xml: companions.and_then(|parts| parts.colors_xml.as_deref()),
        theme_override_xml: companions.and_then(|parts| parts.theme_override_xml.as_deref()),
        data: data.as_ref(),
        legacy_defaults: context.legacy_chart_defaults,
        interactive: true,
        ..Default::default()
    };
    rust_office_charts::render_chart(xml, &options).map(|chart| chart.svg)
}

fn parse_chart_part(
    xml: &[u8],
    part_name: &str,
    context: &NodeParseContext<'_, '_>,
    transform: &Transform,
    companions: Option<&ChartCompanionParts>,
    render: bool,
) -> Result<ParsedChart, ParseError> {
    let document = parse_xml_tree(xml.to_vec(), context.limits, part_name)?;
    let root = document.root();
    let chart_types = [
        ("barChart", "bar"),
        ("bar3DChart", "bar"),
        ("lineChart", "line"),
        ("line3DChart", "line"),
        ("pieChart", "pie"),
        ("pie3DChart", "pie"),
        ("ofPieChart", "pie"),
        ("doughnutChart", "doughnut"),
        ("areaChart", "area"),
        ("area3DChart", "area"),
        ("scatterChart", "scatter"),
        ("radarChart", "radar"),
        ("bubbleChart", "bubble"),
        ("stockChart", "stock"),
        ("surfaceChart", "surface"),
        ("surface3DChart", "surface"),
    ];
    let plots: Vec<_> = root
        .child("chart")
        .and_then(|chart| chart.child("plotArea"))
        .into_iter()
        .flat_map(|area| area.children())
        .filter_map(|node| {
            chart_types
                .iter()
                .find(|(name, _)| node.name() == *name)
                .map(|(_, kind)| (node, *kind))
        })
        .collect();
    let chart_type = match plots.as_slice() {
        [] if root.descendant("chartData").is_some() => "chartEx",
        [] => "ooxml",
        [(_, kind)] => *kind,
        _ => "combo",
    };
    let series = plots
        .iter()
        .flat_map(|(plot, _)| plot.children_named("ser"))
        .map(|series| {
            let name = series
                .child("tx")
                .and_then(|value| indexed_cache_values(value).into_iter().flatten().next())
                .or_else(|| {
                    series
                        .child("tx")
                        .and_then(|value| value.child("v"))
                        .map(XmlNode::text_content)
                });
            let categories = series
                .child("cat")
                .or_else(|| series.child("xVal"))
                .map(|categories| {
                    let numeric = category_cache_is_numeric(categories);
                    indexed_cache_values(categories)
                        .into_iter()
                        .map(|value| match value {
                            None => serde_json::Value::Null,
                            Some(value) if numeric => value
                                .parse::<f64>()
                                .ok()
                                .and_then(serde_json::Number::from_f64)
                                .map(serde_json::Value::Number)
                                .unwrap_or(serde_json::Value::Null),
                            Some(value) => serde_json::Value::String(value),
                        })
                        .collect()
                })
                .filter(|values: &Vec<serde_json::Value>| !values.is_empty());
            let values = series
                .child("val")
                .or_else(|| series.child("yVal"))
                .map(indexed_cache_values)
                .unwrap_or_default()
                .into_iter()
                .map(|value| value.and_then(|value| value.parse::<f64>().ok()))
                .collect();
            let color = series
                .child("spPr")
                .and_then(|properties| properties.child("solidFill"))
                .and_then(|fill| parse_color(fill, context.colors));
            ChartSeries {
                name,
                categories,
                values,
                color,
            }
        })
        .collect();
    Ok(ParsedChart {
        svg: if render {
            render_shared_chart(root, xml, transform, context, companions)
        } else {
            None
        },
        chart_type: chart_type.into(),
        title: chart_title(root),
        series,
        has_legend: Some(root.descendant("legend").is_some()),
    })
}

#[derive(Default)]
struct ParsedSmartArtData {
    semantic_labels: Vec<(String, Vec<TextParagraph>)>,
    drawing_relationship_id: Option<String>,
    background: Option<FillStyle>,
}

fn paragraphs_have_content(paragraphs: &[TextParagraph]) -> bool {
    paragraphs.iter().any(|paragraph| {
        paragraph
            .runs
            .iter()
            .any(|run| !run.text.trim().is_empty() || run.field.is_some())
    })
}

fn parse_smartart_data(xml: &[u8], context: &NodeParseContext<'_, '_>) -> ParsedSmartArtData {
    let Some(data) = std::str::from_utf8(xml)
        .ok()
        .and_then(rust_office_art::smartart::parse_smartart_data)
    else {
        context.diagnostics.warn(
            "unsupported-smartart-data",
            "The SmartArt data model could not be read within the shared parser limits.",
            Some("smartart"),
        );
        return ParsedSmartArtData::default();
    };
    let mut parsed = ParsedSmartArtData {
        drawing_relationship_id: data.drawing_relationship_id.clone(),
        background: data
            .background_xml
            .as_deref()
            .and_then(|xml| {
                parse_xml_tree(xml.as_bytes().to_vec(), context.limits, "SmartArt background").ok()
            })
            .and_then(|node| parse_fill(node.root(), context.colors, context.rels)),
        ..Default::default()
    };
    for point in &data.points {
        if matches!(
            point.kind.as_str(),
            "pres" | "doc" | "parTrans" | "sibTrans"
        ) {
            continue;
        }
        let text_body = point.text_body_xml.as_deref().and_then(|xml| {
            parse_xml_tree(fields::canonical_fragment(xml).as_bytes().to_vec(),
                context.limits,
                "SmartArt text",
            )
            .ok()
        });
        let text_context = TextParseContext {
            source_part: context.source_part,
            presentation_text_style: context.presentation_text_style,
            shape_default_style: RunStyle::default(),
            colors: context.colors,
            rels: context.rels,
        };
        let paragraphs = parse_text_paragraphs(text_body.as_ref().map(XmlDocument::root), None, None, None, &text_context);
        if paragraphs_have_content(&paragraphs) {
            parsed
                .semantic_labels
                .push((point.model_id.clone(), paragraphs));
        }
    }
    parsed
}

fn smartart_child_transform(frame: &Transform) -> Transform {
    Transform {
        x: 0,
        y: 0,
        width: frame.width.max(1),
        height: frame.height.max(1),
        ..Default::default()
    }
}

fn parse_materialized_smartart(
    frame_node: XmlNode<'_>,
    node_index: usize,
    drawing: &rust_office_art::smartart::SmartArtDrawing,
    context: &NodeParseContext<'_, '_>,
) -> Option<SlideNode> {
    fn parse_cached_node(
        entry: &rust_office_art::smartart::SmartArtDrawingNode,
        index: usize,
        frame_id: &str,
        context: &NodeParseContext<'_, '_>,
    ) -> Option<SlideNode> {
        let shape_document = parse_xml_tree(
            fields::canonical_fragment(&entry.source_xml).as_bytes().to_vec(),
            context.limits,
            "SmartArt drawing",
        )
        .ok()?;
        let shape = shape_document.root();
        if shape.child("txXfrm").is_some() && entry.text_transform.is_none() {
            context.diagnostics.warn(
                "unsupported-smartart-text-transform",
                "The SmartArt text transform has incomplete or unsupported dimensions; text uses the shape frame as a rendering fallback.",
                Some("smartart"),
            );
        }
        let model_id = entry
            .model_id
            .clone()
            .unwrap_or_else(|| format!("shape-{index}"));
        let mut parsed = parse_slide_node(shape, index, context).ok().flatten()?;
        let (id, name) = match &mut parsed {
            SlideNode::Shape { id, name, .. }
            | SlideNode::Image { id, name, .. }
            | SlideNode::Group { id, name, .. }
            | SlideNode::Table { id, name, .. }
            | SlideNode::Chart { id, name, .. }
            | SlideNode::Media { id, name, .. }
            | SlideNode::Unknown { id, name, .. } => (id, name),
        };
        *id = format!("{frame_id}:smartart:{model_id}");
        if name.is_empty() {
            *name = format!("SmartArt shape {}", index + 1);
        }
        if let SlideNode::Shape { text_transform, .. } = &mut parsed {
            *text_transform = entry.text_transform.as_ref().map(|frame| Transform {
                x: frame.x,
                y: frame.y,
                width: frame.width,
                height: frame.height,
                rotation: Some(frame.rotation),
                flip_horizontal: Some(frame.flip_horizontal),
                flip_vertical: Some(frame.flip_vertical),
            });
        }
        if let SlideNode::Group {
            id,
            children,
            transform,
            child_transform,
            ..
        } = &mut parsed
        {
            let child_context = group_paint_context(context, transform, child_transform.as_ref());
            *children = entry
                .children
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| parse_cached_node(entry, index, id, &child_context))
                .collect();
        }
        Some(parsed)
    }
    let (frame_id, frame_name) = node_identity(frame_node, context.slide_index, node_index);
    let frame = parse_transform(frame_node.child("xfrm"));
    let properties = drawing
        .group_properties_xml
        .as_deref()
        .and_then(|xml| parse_xml_tree(xml.as_bytes().to_vec(), context.limits, "SmartArt group").ok());
    let drawing_xfrm = properties
        .as_ref()
        .and_then(|properties| properties.root().child("xfrm"));
    let child_transform =
        parse_child_transform(drawing_xfrm).or_else(|| Some(smartart_child_transform(&frame)));
    let child_context = group_paint_context(context, &frame, child_transform.as_ref());
    let children = drawing
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| parse_cached_node(entry, index, &frame_id, &child_context))
        .collect::<Vec<_>>();
    if children.is_empty() {
        return None;
    }
    if !drawing.unsupported_elements.is_empty() {
        context.diagnostics.warn("unsupported-smartart-content", "Some cached SmartArt drawing elements are retained by the shared reader but cannot be rendered.", Some("smartart"));
    }
    Some(SlideNode::Group {
        metadata: Default::default(),
        id: frame_id,
        name: frame_name,
        transform: frame.clone(),
        children,
        child_transform,
    })
}

fn normalize_smartart_fallback_text(paragraphs: &mut [TextParagraph]) {
    for paragraph in paragraphs {
        paragraph.alignment = Some("center".into());
        paragraph.bullet = None;
        paragraph.level = None;
        paragraph.margin_left_emu = None;
        paragraph.indent_emu = None;
        paragraph.line_spacing = None;
        paragraph.space_before = None;
        paragraph.space_after = None;
        for run in &mut paragraph.runs {
            run.font_size_pt = Some(run.font_size_pt.unwrap_or(12.0).clamp(8.0, 18.0));
            run.color = Some(ColorValue {
                value: "#172033".into(),
                alpha: None,
            });
        }
    }
}

fn smartart_fallback_group(
    frame_node: XmlNode<'_>,
    node_index: usize,
    data: &ParsedSmartArtData,
    context: &NodeParseContext<'_, '_>,
) -> Option<SlideNode> {
    if data.semantic_labels.is_empty() {
        return None;
    }
    let (frame_id, frame_name) = node_identity(frame_node, context.slide_index, node_index);
    let frame = parse_transform(frame_node.child("xfrm"));
    let width = frame.width.max(1);
    let height = frame.height.max(1);
    let count = data.semantic_labels.len();
    let aspect = width as f64 / height as f64;
    let columns = ((count as f64 * aspect).sqrt().ceil() as usize).clamp(1, count);
    let rows = count.div_ceil(columns);
    let gap = (width.min(height) / 40).max(1);
    let padding = gap.saturating_mul(2);
    let horizontal_gaps = gap.saturating_mul(columns.saturating_sub(1) as i64);
    let vertical_gaps = gap.saturating_mul(rows.saturating_sub(1) as i64);
    let cell_width = width
        .saturating_sub(padding.saturating_mul(2))
        .saturating_sub(horizontal_gaps)
        .max(columns as i64)
        / columns as i64;
    let cell_height = height
        .saturating_sub(padding.saturating_mul(2))
        .saturating_sub(vertical_gaps)
        .max(rows as i64)
        / rows as i64;
    let mut children =
        Vec::with_capacity(count.saturating_add(usize::from(data.background.is_some())));
    if let Some(fill) = data.background.clone() {
        children.push(SlideNode::Shape {
            metadata: Default::default(),
            shape_svg: None,
            paint_bounds: None,
            source_xml: None,
            id: format!("{frame_id}:smartart:background"),
            name: "SmartArt background".into(),
            transform: smartart_child_transform(&frame),
            geometry: ShapeGeometry {
                preset: Some("rect".into()),
                path: None,
            },
            fill: Some(fill),
            line: None,
            paragraphs: Vec::new(),
            vertical_alignment: None,
            text_insets: None,
            autofit: None,
            text_rotation: None,
            text_transform: None,
            vertical_text: None,
            horizontal_overflow: None,
            vertical_overflow: None,
            text_wrap: None,
            column_count: None,
            column_spacing: None,
            right_to_left_columns: None,
            space_first_last_paragraph: None,
        });
    }
    for (label_index, (model_id, source_paragraphs)) in data.semantic_labels.iter().enumerate() {
        let column = label_index % columns;
        let row = label_index / columns;
        let mut paragraphs = source_paragraphs.clone();
        normalize_smartart_fallback_text(&mut paragraphs);
        let horizontal_inset = (cell_width / 20).max(0);
        let vertical_inset = (cell_height / 12).max(0);
        children.push(SlideNode::Shape {
            metadata: Default::default(),
            shape_svg: None,
            paint_bounds: None,
            source_xml: None,
            id: format!("{frame_id}:smartart:fallback:{model_id}"),
            name: format!("SmartArt item {}", label_index + 1),
            transform: Transform {
                x: padding + column as i64 * (cell_width + gap),
                y: padding + row as i64 * (cell_height + gap),
                width: cell_width,
                height: cell_height,
                ..Default::default()
            },
            geometry: ShapeGeometry {
                preset: Some("roundRect".into()),
                path: None,
            },
            fill: Some(FillStyle::Solid {
                color: ColorValue {
                    value: "#E8EEF8".into(),
                    alpha: None,
                },
            }),
            line: Some(LineStyle {
                color: Some(ColorValue {
                    value: "#5273A8".into(),
                    alpha: None,
                }),
                width: Some(1.0),
                ..Default::default()
            }),
            paragraphs,
            vertical_alignment: Some(VerticalAlignment::Middle),
            text_insets: Some(crate::TextInsets {
                top: vertical_inset,
                right: horizontal_inset,
                bottom: vertical_inset,
                left: horizontal_inset,
            }),
            autofit: Some(crate::TextAutofit {
                mode: crate::TextAutoFitMode::Normal,
                font_scale: Some(0.85),
                line_spacing_reduction: Some(0.1),
            }),
            text_rotation: None,
            text_transform: None,
            vertical_text: None,
            horizontal_overflow: Some("clip".into()),
            vertical_overflow: Some("clip".into()),
            text_wrap: Some("square".into()),
            column_count: Some(1),
            column_spacing: None,
            right_to_left_columns: Some(false),
            space_first_last_paragraph: Some(false),
        });
    }
    context.diagnostics.warn(
        "degraded-rendering",
        format!(
            "SmartArt frame {frame_id} has no materialized drawing; rendered a semantic fallback."
        ),
        Some("smartart"),
    );
    Some(SlideNode::Group {
        metadata: Default::default(),
        id: frame_id,
        name: frame_name,
        transform: frame.clone(),
        children,
        child_transform: Some(smartart_child_transform(&frame)),
    })
}

fn parse_smartart_frame(
    frame_node: XmlNode<'_>,
    node_index: usize,
    graphic_data: XmlNode<'_>,
    context: &NodeParseContext<'_, '_>,
) -> Result<Option<SlideNode>, ParseError> {
    let Some(relationships) = graphic_data.descendant("relIds") else {
        return Ok(None);
    };
    let Some(data_relationship_id) = relationships.attr("dm") else {
        return Ok(None);
    };
    let Some(data_path) = context.rels.get(data_relationship_id) else {
        context.diagnostics.warn(
            "missing-part",
            "SmartArt data relationship could not be resolved.",
            Some("smartart"),
        );
        return Ok(None);
    };
    let Some(data_xml) = context.related_parts.get(data_path) else {
        context.diagnostics.warn(
            "missing-part",
            format!("SmartArt data part {data_path} is missing."),
            Some("smartart"),
        );
        return Ok(None);
    };
    parse_xml_tree(data_xml.to_vec(), context.limits, data_path)?;
    let data_rels = context
        .related_relationships
        .get(data_path)
        .unwrap_or(context.rels);
    let data_context = NodeParseContext {
        source_part: Some(data_path),
        rels: data_rels,
        ..*context
    };
    let data = parse_smartart_data(data_xml, &data_context);
    if let Some(drawing_relationship_id) = data.drawing_relationship_id.as_deref() {
        let mut drawing_paths = Vec::with_capacity(2);
        for path in [
            data_rels.get(drawing_relationship_id),
            context.rels.get(drawing_relationship_id),
        ]
        .into_iter()
        .flatten()
        {
            if !drawing_paths.contains(&path) {
                drawing_paths.push(path);
            }
        }
        for drawing_path in &drawing_paths {
            let Some(drawing_xml) = context.related_parts.get(*drawing_path) else {
                continue;
            };
            let Some(drawing) = std::str::from_utf8(drawing_xml)
                .ok()
                .and_then(rust_office_art::smartart::parse_smartart_drawing)
            else {
                continue;
            };
            parse_xml_tree(drawing_xml.to_vec(), context.limits, drawing_path)?;
            let drawing_context = NodeParseContext {
                artwork_target: context.artwork_target,
                artwork_part: context.artwork_part,
                artwork_id_match: if context.artwork_target.is_some() {
                    ArtworkIdMatch::Any
                } else {
                    context.artwork_id_match
                },
                source_part: Some(drawing_path),
                rels: context
                    .related_relationships
                    .get(*drawing_path)
                    .unwrap_or(context.rels),
                layout_root: None,
                master_root: None,
                layout_rels: None,
                master_rels: None,
                ..*context
            };
            if let Some(group) =
                parse_materialized_smartart(frame_node, node_index, &drawing, &drawing_context)
            {
                return Ok(Some(group));
            }
        }
        if drawing_paths.is_empty() {
            context.diagnostics.warn(
                "missing-part",
                "SmartArt drawing relationship could not be resolved.",
                Some("smartart"),
            );
        } else {
            for drawing_path in drawing_paths {
                if !context.related_parts.contains_key(drawing_path) {
                    context.diagnostics.warn(
                        "missing-part",
                        format!("SmartArt drawing part {drawing_path} is missing."),
                        Some("smartart"),
                    );
                }
            }
        }
    }
    Ok(smartart_fallback_group(
        frame_node, node_index, &data, context,
    ))
}

fn media_time_ms(value: &str) -> Option<f64> {
    let split = value
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(value.len());
    let (number, unit) = value.split_at(split);
    let valid = !number.is_empty()
        && number.split('.').count() <= 2
        && number
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    if !valid {
        return None;
    }
    let scale = match unit {
        "" | "ms" => 1.0,
        "s" => 1000.0,
        "min" => 60_000.0,
        "h" => 3_600_000.0,
        "µs" => 0.001,
        "ns" => 0.000001,
        _ => return None,
    };
    let result = number.parse::<f64>().ok()? * scale;
    result.is_finite().then_some(result)
}

fn media_playback(media: XmlNode<'_>) -> crate::MediaPlayback {
    let time = |child, attribute| {
        media
            .child(child)
            .and_then(|node| node.attr(attribute))
            .and_then(media_time_ms)
    };
    crate::MediaPlayback {
        trim_start_ms: time("trim", "st"),
        trim_end_ms: time("trim", "end"),
        fade_in_ms: time("fade", "in"),
        fade_out_ms: time("fade", "out"),
        bookmarks: media
            .child("bmkLst")
            .into_iter()
            .flat_map(|list| list.children_named("bmk"))
            .filter_map(|bookmark| {
                Some(crate::MediaBookmark {
                    name: bookmark.attr("name")?.to_owned(),
                    time_ms: media_time_ms(bookmark.attr("time")?)?,
                })
            })
            .collect(),
    }
}

fn media_source(
    id: Option<&str>,
    context: &NodeParseContext<'_, '_>,
) -> (Option<String>, Option<String>) {
    let Some((id, path)) = id.and_then(|id| context.rels.get(id).map(|path| (id, path))) else {
        context.diagnostics.warn(
            "missing-media-source",
            "The media relationship could not be resolved.",
            Some("media"),
        );
        return (None, None);
    };
    if context.rels.is_external(id) {
        (None, Some(path.clone()))
    } else {
        if !context.assets.contains_key(path) {
            context.diagnostics.warn(
                "missing-media-asset",
                "The embedded media payload could not be resolved.",
                Some("media"),
            );
        }
        (Some(path.clone()), None)
    }
}

fn parse_picture_or_media(
    node: XmlNode<'_>,
    node_index: usize,
    context: &NodeParseContext<'_, '_>,
) -> SlideNode {
    let properties = nonvisual_properties(node).and_then(|node| node.child("nvPr"));
    let classic = properties.and_then(|properties| {
        properties.children().iter().find(|child| {
            matches!(
                child.name(),
                "audioCd" | "wavAudioFile" | "audioFile" | "videoFile" | "quickTimeFile"
            )
        })
    });
    let extension = properties
        .and_then(|properties| properties.child("extLst"))
        .and_then(|extensions| extensions.descendant("media"));
    let poster = parse_image_node(node, node_index, context);
    if classic.is_none() && extension.is_none() {
        return poster;
    }
    let relationship = extension
        .and_then(|media| media.attr("link").or_else(|| media.attr("embed")))
        .or_else(|| classic.and_then(|media| media.attr("link").or_else(|| media.attr("embed"))));
    let (asset_id, source_url) =
        if classic.is_some_and(|media| media.name() == "audioCd") && relationship.is_none() {
            (None, None)
        } else {
            media_source(relationship, context)
        };
    let media_type = match classic.map(|media| media.name()) {
        Some("audioCd" | "wavAudioFile" | "audioFile") => crate::MediaType::Audio,
        Some("videoFile" | "quickTimeFile") => crate::MediaType::Video,
        _ => match asset_id
            .as_ref()
            .and_then(|id| context.assets.get(id))
            .map(|asset| asset.content_type.as_str())
        {
            Some(kind) if kind.starts_with("audio/") => crate::MediaType::Audio,
            Some(kind) if kind.starts_with("video/") => crate::MediaType::Video,
            _ => crate::MediaType::Unknown,
        },
    };
    let SlideNode::Image {
        id,
        name,
        transform,
        asset_id: poster_id,
        ..
    } = &poster
    else {
        unreachable!()
    };
    SlideNode::Media {
        metadata: Default::default(),
        id: id.clone(),
        name: name.clone(),
        transform: transform.clone(),
        media_type,
        asset_id,
        source_url,
        poster_asset_id: (poster_id != "missing-asset").then(|| poster_id.clone()),
        poster: Some(Box::new(poster)),
        playback: extension.map(media_playback),
        embedded_object: None,
        source_xml: None,
    }
}

fn parse_embedded_object(
    frame: XmlNode<'_>,
    object: XmlNode<'_>,
    node_index: usize,
    context: &NodeParseContext<'_, '_>,
) -> SlideNode {
    let (id, name) = node_identity(frame, context.slide_index, node_index);
    let (asset_id, source_url) = media_source(object.attr("id"), context);
    let poster = object
        .descendant("pic")
        .or_else(|| {
            frame
                .descendant("graphicData")
                .and_then(|data| data.descendant("pic"))
        })
        .map(|picture| {
            let mut poster = parse_image_node(picture, node_index, context);
            if let SlideNode::Image {
                id: poster_id,
                name: poster_name,
                ..
            } = &mut poster
            {
                *poster_id = id.clone();
                *poster_name = name.clone();
            }
            Box::new(poster)
        });
    let poster_asset_id = poster.as_deref().and_then(|poster| match poster {
        SlideNode::Image { asset_id, .. } if asset_id != "missing-asset" => Some(asset_id.clone()),
        _ => None,
    });
    SlideNode::Media {
        metadata: Default::default(),
        id,
        name,
        transform: parse_transform(frame.child("xfrm")),
        media_type: crate::MediaType::Ole,
        asset_id,
        source_url,
        poster_asset_id,
        poster,
        playback: None,
        embedded_object: Some(crate::EmbeddedObject {
            control: None,
            program_id: object.attr("progId").map(str::to_owned),
            name: object.attr("name").map(str::to_owned),
            relationship_id: object.attr("id").map(str::to_owned),
            linked: object.child("link").is_some(),
            show_as_icon: object.attr("showAsIcon").and_then(bool_value),
            preview_width_emu: object.attr("imgW").and_then(|value| value.parse().ok()),
            preview_height_emu: object.attr("imgH").and_then(|value| value.parse().ok()),
            follow_color_scheme: object
                .child("embed")
                .and_then(|embed| embed.attr("followColorScheme"))
                .map(str::to_owned),
            update_automatic: object
                .child("link")
                .and_then(|link| link.attr("updateAutomatic"))
                .and_then(bool_value),
        }),
        source_xml: object.source_xml().map(str::to_owned),
    }
}

fn parse_graphic_frame(
    node: XmlNode<'_>,
    node_index: usize,
    context: &NodeParseContext<'_, '_>,
) -> Result<SlideNode, ParseError> {
    let graphic_data = node.descendant("graphicData");
    if let Some(object) = graphic_data.and_then(|data| data.descendant("oleObj")) {
        return Ok(parse_embedded_object(node, object, node_index, context));
    }
    if let Some(model) = graphic_data.and_then(|value| value.child("model3d")) {
        return Ok(parse_model3d_node(node, model, node_index, context));
    }
    if let Some(table) = graphic_data.and_then(|value| value.child("tbl")) {
        return Ok(parse_table_node(node, table, node_index, context));
    }
    if let Some(chart) = graphic_data.and_then(|value| value.child("chart")) {
        let relationship = chart.attr("id");
        let path = relationship.and_then(|value| context.rels.get(value));
        let resolved = path.and_then(|path| context.related_parts.get(path).map(|xml| (path, xml)));
        let transform = parse_transform(node.child("xfrm"));
        let (id, name) = node_identity(node, context.slide_index, node_index);
        let render = context.renders_artwork(&id);
        let (chart, chart_xml, companions) = if let Some((path, xml)) = resolved {
            let companions = context.chart_companions.get(path);
            (
                parse_chart_part(xml, path, context, &transform, companions, render)?,
                Some(String::from_utf8_lossy(xml).into_owned()),
                companions,
            )
        } else {
            context.diagnostics.warn(
                "missing-part",
                "Chart relationship or chart part could not be resolved.",
                Some("chart"),
            );
            (
                ParsedChart {
                    svg: None,
                    chart_type: "ooxml".into(),
                    title: None,
                    series: Vec::new(),
                    has_legend: None,
                },
                None,
                None,
            )
        };
        if render && chart.svg.is_none() && chart_xml.is_some() {
            context.diagnostics.warn("unsupported-chart-rendering", "The chart source and cached series are retained; shared SVG rendering is unavailable for this chart.", Some("chart"));
        }
        return Ok(SlideNode::Chart {
            metadata: Default::default(),
            id,
            name,
            transform,
            chart_svg: chart.svg,
            chart_type: chart.chart_type,
            title: chart.title,
            series: chart.series,
            has_legend: chart.has_legend,
            chart_xml,
            chart_part_name: path.cloned(),
            chart_style_xml: companions.and_then(|parts| parts.style_xml.clone()),
            chart_colors_xml: companions.and_then(|parts| parts.colors_xml.clone()),
        });
    }
    if let Some(smartart) = graphic_data.filter(|value| value.descendant("relIds").is_some()) {
        if let Some(group) = parse_smartart_frame(node, node_index, smartart, context)? {
            return Ok(group);
        }
    }
    if let Some(picture) = graphic_data.and_then(|value| value.descendant("pic")) {
        let mut fallback = parse_image_node(picture, node_index, context);
        let (frame_id, frame_name) = node_identity(node, context.slide_index, node_index);
        if let SlideNode::Image { id, name, .. } = &mut fallback {
            *id = frame_id;
            *name = frame_name;
        }
        return Ok(fallback);
    }
    let (id, name) = node_identity(node, context.slide_index, node_index);
    context.diagnostics.warn(
        "unsupported-feature",
        format!("Graphic frame {id} contains an unsupported payload."),
        Some("graphic-frame"),
    );
    Ok(SlideNode::Unknown {
        metadata: Default::default(),
        id,
        name,
        transform: parse_transform(node.child("xfrm")),
        feature: "graphic-frame".into(),
    })
}

fn group_paint_context<'a, 'color>(
    context: &NodeParseContext<'a, 'color>,
    frame: &Transform,
    child: Option<&Transform>,
) -> NodeParseContext<'a, 'color> {
    let child = child.unwrap_or(frame);
    let scale = |parent: f64, extent: i64, child_extent: i64| {
        let scale = parent * extent as f64 / child_extent as f64;
        if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            parent
        }
    };
    NodeParseContext {
        paint_scale: (
            scale(context.paint_scale.0, frame.width, child.width),
            scale(context.paint_scale.1, frame.height, child.height),
        ),
        ..*context
    }
}

fn parse_group_node(
    node: XmlNode<'_>,
    node_index: usize,
    context: &NodeParseContext<'_, '_>,
) -> Result<SlideNode, ParseError> {
    let (id, name) = node_identity(node, context.slide_index, node_index);
    let xfrm = node
        .child("grpSpPr")
        .and_then(|properties| properties.child("xfrm"));
    let transform = parse_transform(xfrm);
    let child_transform = parse_child_transform(xfrm);
    let child_context = group_paint_context(context, &transform, child_transform.as_ref());
    let mut children = Vec::new();
    for (child_index, child) in node.children().iter().enumerate() {
        if let Some(child) = parse_slide_node(child, child_index, &child_context)? {
            children.push(child);
        }
    }
    Ok(SlideNode::Group {
        metadata: Default::default(),
        id,
        name,
        transform,
        children,
        child_transform,
    })
}

fn parse_content_part(
    node: XmlNode<'_>,
    node_index: usize,
    context: &NodeParseContext<'_, '_>,
) -> SlideNode {
    let (id, name) = node_identity(node, context.slide_index, node_index);
    let transform = parse_transform(node.child("xfrm"));
    let asset_id = node
        .attr("id")
        .and_then(|id| context.rels.internal(id))
        .cloned();
    let source = asset_id
        .as_ref()
        .and_then(|path| context.related_parts.get(path))
        .and_then(|bytes| std::str::from_utf8(bytes).ok());
    let drawing = source.and_then(rust_office_art::parse_ink);
    // Ink is rendered even when artwork is deferred: whether the render
    // succeeds decides between the ink and its saved preview in
    // markup-compatibility fallbacks. `parse_slide_node` drops a deferred
    // preview once that choice is made.
    let rendered = drawing.as_ref().and_then(|drawing| {
        rust_office_art::render_ink_in_frame(
            drawing,
            transform.width as f64 * context.paint_scale.0 / 9525.0,
            transform.height as f64 * context.paint_scale.1 / 9525.0,
        )
    });
    if rendered.is_none() {
        context.diagnostics.warn(
            "ink-fallback",
            "Ink source is retained; a compatible saved preview is used when available.",
            Some("ink"),
        );
    }
    let (picture_svg, paint_bounds) = rendered
        .map(|paint| (Some(paint.svg), paint.paint_bounds))
        .unwrap_or_default();
    SlideNode::Image {
        metadata: Default::default(),
        model_3d: None,
        ink: Some(Box::new(crate::InkContent {
            asset_id: asset_id.clone(),
            source_xml: source.map(str::to_owned),
            drawing: drawing.map(Box::new),
        })),
        id,
        name,
        transform,
        asset_id: asset_id.unwrap_or_else(|| "missing-asset".into()),
        fallback_asset_id: None,
        crop: None,
        opacity: None,
        preserve_aspect_ratio: false,
        effects: None,
        picture_svg,
        fallback_picture: None,
        paint_bounds,
        source_xml: node.source_xml().map(str::to_owned),
    }
}

fn parse_slide_node(
    node: XmlNode<'_>,
    node_index: usize,
    context: &NodeParseContext<'_, '_>,
) -> Result<Option<SlideNode>, ParseError> {
    let mut parsed = parse_slide_node_with_ink_artwork(node, node_index, context)?;
    // `parse_content_part` always renders ink so that markup-compatibility
    // fallbacks are chosen exactly as in an eager parse; a deferred node keeps
    // the ink source but not the generated preview.
    if let Some(SlideNode::Image {
        id,
        ink: Some(_),
        picture_svg,
        paint_bounds,
        ..
    }) = parsed.as_mut()
    {
        if !context.renders_artwork(id) {
            *picture_svg = None;
            *paint_bounds = None;
        }
    }
    Ok(parsed)
}

fn parse_slide_node_with_ink_artwork(
    node: XmlNode<'_>,
    node_index: usize,
    context: &NodeParseContext<'_, '_>,
) -> Result<Option<SlideNode>, ParseError> {
    let mut parsed = match node.name() {
        "sp" | "cxnSp" => Ok(Some(parse_shape_node(node, node_index, context))),
        "pic" => Ok(Some(parse_picture_or_media(node, node_index, context))),
        "model3d" => Ok(Some(parse_model3d_node(node, node, node_index, context))),
        "contentPart" => Ok(Some(parse_content_part(node, node_index, context))),
        "grpSp" => Ok(Some(parse_group_node(node, node_index, context)?)),
        "graphicFrame" => Ok(Some(parse_graphic_frame(node, node_index, context)?)),
        // Markup-compatibility wrapper used by PowerPoint for modern content
        // (chartEx funnel/treemap/waterfall charts, newer media). Prefer the
        // richest branch that parses to a supported node, otherwise keep any
        // fallback rendering.
        "AlternateContent" => {
            let mut fallback: Option<SlideNode> = None;
            let mut model_without_preview = None;
            let mut ink_without_preview = None;
            let mut media_without_preview = None;
            for branch_name in ["Choice", "Fallback"] {
                for branch in node.children_named(branch_name) {
                    for child in branch.children() {
                        let Some(mut parsed) =
                            parse_slide_node_with_ink_artwork(child, node_index, context)?
                        else {
                            continue;
                        };
                        if matches!(
                            &parsed,
                            SlideNode::Image {
                                ink: Some(_),
                                picture_svg: None,
                                fallback_picture: None,
                                ..
                            }
                        ) {
                            if ink_without_preview.is_none() {
                                ink_without_preview = Some(parsed);
                            }
                            continue;
                        }
                        if let SlideNode::Image { ink, .. } = &mut parsed {
                            if let Some(SlideNode::Image { ink: original, .. }) =
                                ink_without_preview.take()
                            {
                                *ink = original;
                            }
                        }
                        if matches!(&parsed, SlideNode::Media { poster: None, .. }) {
                            if media_without_preview.is_none() {
                                media_without_preview = Some(parsed);
                            }
                            continue;
                        }
                        if matches!(&parsed, SlideNode::Image { .. }) {
                            if let Some(mut media) = media_without_preview.take() {
                                if let SlideNode::Media {
                                    poster,
                                    poster_asset_id,
                                    ..
                                } = &mut media
                                {
                                    if let SlideNode::Image { asset_id, .. } = &parsed {
                                        *poster_asset_id =
                                            (asset_id != "missing-asset").then(|| asset_id.clone());
                                    }
                                    *poster = Some(Box::new(parsed));
                                }
                                return Ok(Some(media));
                            }
                        }
                        if matches!(&parsed, SlideNode::Image { asset_id, source_xml: Some(xml), .. }
                            if asset_id == "missing-asset" && rust_office_art::model3d::model3d_xml(xml).is_some())
                        {
                            if model_without_preview.is_none() {
                                model_without_preview = Some(parsed);
                            }
                            continue;
                        }
                        if let SlideNode::Image {
                            model_3d,
                            source_xml,
                            ..
                        } = &mut parsed
                        {
                            if let Some(SlideNode::Image {
                                model_3d: original,
                                source_xml: original_xml,
                                ..
                            }) = model_without_preview.take()
                            {
                                *model_3d = original;
                                *source_xml = original_xml;
                            }
                        }
                        if !matches!(parsed, SlideNode::Unknown { .. }) {
                            return Ok(Some(parsed));
                        }
                        if fallback.is_none() {
                            fallback = Some(parsed);
                        }
                    }
                }
            }
            Ok(media_without_preview
                .or(model_without_preview)
                .or(ink_without_preview)
                .or(fallback))
        }
        "nvGrpSpPr" | "grpSpPr" | "extLst" => Ok(None),
        feature => {
            context.diagnostics.warn(
                "unsupported-feature",
                format!("Slide-tree node {feature} is not supported."),
                Some(feature),
            );
            Ok(None)
        }
    }?;
    if let (Some(parsed), Some(metadata)) = (parsed.as_mut(), node_metadata(node, context)) {
        *parsed.metadata_mut() = metadata;
    }
    Ok(parsed)
}

fn part_background(
    root: XmlNode<'_>,
    colors: Option<&ColorContext<'_>>,
    rels: &Relationships,
) -> Option<FillStyle> {
    let background = root.child("cSld")?.child("bg")?;
    background
        .child("bgPr")
        .and_then(|properties| parse_fill(properties, colors, rels))
        .or_else(|| {
            background
                .child("bgRef")
                .and_then(|reference| style_matrix_fill(reference, colors))
        })
}

struct ParsedSlideContent {
    transition: Option<Box<crate::SlideTransition>>,
    timing: Option<Box<crate::SlideTiming>>,
    name: Option<String>,
    hidden: Option<bool>,
    background: Option<FillStyle>,
    nodes: Vec<SlideNode>,
}

fn vml_length(value: &str) -> Option<i64> {
    let value = value.trim();
    let (number, factor) = if let Some(v) = value.strip_suffix("pt") {
        (v, 12700.0)
    } else if let Some(v) = value.strip_suffix("px") {
        (v, 9525.0)
    } else if let Some(v) = value.strip_suffix("in") {
        (v, 914400.0)
    } else if let Some(v) = value.strip_suffix("cm") {
        (v, 360000.0)
    } else if let Some(v) = value.strip_suffix("mm") {
        (v, 36000.0)
    } else {
        (value, 9525.0)
    };
    let emu = number.parse::<f64>().ok()? * factor;
    (emu.is_finite() && emu.abs() <= i64::MAX as f64).then_some(emu.round() as i64)
}

fn clean_vml_conditionals(xml: &str) -> String {
    let mut result = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(at) = rest.find("<![") {
        result.push_str(&rest[..at]);
        rest = &rest[at..];
        if rest.starts_with("<![if ") || rest.starts_with("<![endif]") {
            if let Some(end) = rest.find("]>") {
                rest = &rest[end + 2..];
                continue;
            }
        }
        result.push_str("<![");
        rest = &rest[3..];
    }
    result.push_str(rest);
    result
}

fn control_vml_preview(
    control: XmlNode<'_>,
    context: &NodeParseContext<'_, '_>,
) -> Option<(Transform, Option<String>, String)> {
    let shape_id = control.attr("spid")?;
    let mut paths = context
        .rels
        .values()
        .filter(|path| context.related_parts.contains_key(*path))
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let Some(xml) = context
            .related_parts
            .get(path)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
        else {
            continue;
        };
        if !xml.contains("urn:schemas-microsoft-com:vml") {
            continue;
        }
        let cleaned = clean_vml_conditionals(xml);
        let document = match parse_xml_tree(cleaned.as_bytes().to_vec(), context.limits, path) {
            Ok(document) => document,
            Err(_) => continue,
        };
        let root = document.root();
        for shape in root.children_named("shape") {
            if !shape
                .attr("spid")
                .or_else(|| shape.attr("id"))
                .is_some_and(|id| id == shape_id || id.strip_prefix("_x0000_s") == Some(shape_id))
            {
                continue;
            }
            let styles = shape
                .attr("style")
                .unwrap_or_default()
                .split(';')
                .filter_map(|part| part.split_once(':'))
                .map(|(k, v)| (k.trim(), v.trim()))
                .collect::<HashMap<_, _>>();
            let transform = Transform {
                x: styles
                    .get("left")
                    .and_then(|v| vml_length(v))
                    .unwrap_or_default(),
                y: styles
                    .get("top")
                    .and_then(|v| vml_length(v))
                    .unwrap_or_default(),
                width: styles
                    .get("width")
                    .and_then(|v| vml_length(v))
                    .unwrap_or_default(),
                height: styles
                    .get("height")
                    .and_then(|v| vml_length(v))
                    .unwrap_or_default(),
                rotation: styles
                    .get("rotation")
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite()),
                ..Default::default()
            };
            let asset = shape
                .child("imagedata")
                .and_then(|image| image.attr("relid").or_else(|| image.attr("id")))
                .and_then(|id| context.related_relationships.get(path)?.internal(id))
                .cloned();
            return Some((transform, asset, xml.to_owned()));
        }
    }
    None
}

fn parse_control(control: XmlNode<'_>, index: usize, context: &NodeParseContext<'_, '_>) -> SlideNode {
    let id = control
        .attr("spid")
        .map(str::to_owned)
        .unwrap_or_else(|| format!("control-{}-{index}", context.slide_index));
    let name = control.attr("name").unwrap_or_default().to_owned();
    let (asset_id, source_url) = media_source(control.attr("id"), context);
    let source = asset_id
        .as_ref()
        .and_then(|path| context.related_parts.get(path))
        .and_then(|bytes| std::str::from_utf8(bytes).ok());
    let document = source
        .and_then(|xml| parse_xml_tree(xml.as_bytes().to_vec(), context.limits, "embedded control").ok());
    let root = document
        .as_ref()
        .map(XmlDocument::root)
        .filter(|root| root.name() == "ocx");
    let mut binary_asset_ids = asset_id
        .as_ref()
        .and_then(|path| context.related_relationships.get(path))
        .map(|rels| {
            rels.values()
                .filter(|path| context.assets.contains_key(*path))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    binary_asset_ids.sort();
    let mut transform = Transform {
        width: control
            .attr("imgW")
            .and_then(|v| v.parse().ok())
            .unwrap_or_default(),
        height: control
            .attr("imgH")
            .and_then(|v| v.parse().ok())
            .unwrap_or_default(),
        ..Default::default()
    };
    let mut poster = control
        .child("pic")
        .map(|picture| parse_image_node(picture, index, context));
    if let Some(SlideNode::Image {
        transform: frame, ..
    }) = poster.as_ref()
    {
        transform = frame.clone();
    }
    if poster.is_none() {
        if let Some((frame, asset, xml)) = control_vml_preview(control, context) {
            transform = frame;
            if let Some(asset_id) = asset {
                poster = Some(SlideNode::Image {
                    metadata: Default::default(),
                    model_3d: None,
                    ink: None,
                    id: id.clone(),
                    name: name.clone(),
                    transform: transform.clone(),
                    asset_id,
                    fallback_asset_id: None,
                    crop: None,
                    opacity: None,
                    preserve_aspect_ratio: false,
                    effects: None,
                    picture_svg: None,
                    fallback_picture: None,
                    paint_bounds: None,
                    source_xml: Some(xml),
                });
            }
        }
    }
    if poster.is_none() {
        context.diagnostics.warn(
            "missing-control-preview",
            "Embedded control has no supported saved preview.",
            Some("control"),
        );
    }
    if root.is_none() {
        context.diagnostics.warn(
            "missing-control-definition",
            "Embedded control definition is missing or cannot be parsed.",
            Some("control"),
        );
    }
    let poster_asset_id = poster.as_ref().and_then(|poster| match poster {
        SlideNode::Image { asset_id, .. } => Some(asset_id.clone()),
        _ => None,
    });
    let properties = root
        .map(|root| {
            root.children_named("ocxPr")
                .map(|property| crate::ControlProperty {
                    name: property.attr("name").unwrap_or_default().to_owned(),
                    value: property.attr("value").map(str::to_owned),
                })
                .collect()
        })
        .unwrap_or_default();
    SlideNode::Media {
        metadata: Some(Box::new(crate::NodeMetadata {
            source_id: Some(id.clone()),
            source_part: context.source_part.map(str::to_owned),
            ..Default::default()
        })),
        id,
        name: name.clone(),
        transform,
        media_type: crate::MediaType::Control,
        asset_id,
        source_url,
        poster_asset_id,
        poster: poster.map(Box::new),
        playback: None,
        embedded_object: Some(crate::EmbeddedObject {
            control: Some(Box::new(crate::EmbeddedControl {
                class_id: root.and_then(|n| n.attr("classid")).map(str::to_owned),
                persistence: root.and_then(|n| n.attr("persistence")).map(str::to_owned),
                source_xml: source.map(str::to_owned),
                properties,
                binary_asset_ids,
            })),
            name: Some(name),
            relationship_id: control.attr("id").map(str::to_owned),
            show_as_icon: control.attr("showAsIcon").and_then(bool_value),
            preview_width_emu: control.attr("imgW").and_then(|v| v.parse().ok()),
            preview_height_emu: control.attr("imgH").and_then(|v| v.parse().ok()),
            ..Default::default()
        }),
        source_xml: control.source_xml().map(str::to_owned),
    }
}

fn append_controls(root: XmlNode<'_>, nodes: &mut Vec<SlideNode>, context: &NodeParseContext<'_, '_>) {
    if let Some(controls) = root.child("cSld").and_then(|slide| slide.child("controls")) {
        for (index, control) in controls.children_named("control").enumerate() {
            nodes.push(parse_control(control, index, context));
        }
    }
}

fn parse_part_nodes(
    root: XmlNode<'_>,
    context: &NodeParseContext<'_, '_>,
) -> Result<Vec<SlideNode>, ParseError> {
    let mut nodes = Vec::new();
    let Some(shape_tree) = shape_tree(root) else {
        return Ok(nodes);
    };
    for (node_index, node) in shape_tree.children().iter().enumerate() {
        if let Some(node) = parse_slide_node(node, node_index, context)? {
            nodes.push(node);
        }
    }
    append_controls(root, &mut nodes, context);
    Ok(nodes)
}

fn parse_decorative_part_nodes(
    root: XmlNode<'_>,
    context: &NodeParseContext<'_, '_>,
) -> Result<Vec<SlideNode>, ParseError> {
    let mut nodes = Vec::new();
    let Some(shape_tree) = shape_tree(root) else {
        return Ok(nodes);
    };
    for (node_index, node) in shape_tree.children().iter().enumerate() {
        if placeholder_key(node).is_some() {
            continue;
        }
        if let Some(node) = parse_slide_node(node, node_index, context)? {
            nodes.push(node);
        }
    }
    append_controls(root, &mut nodes, context);
    Ok(nodes)
}

fn prefix_node_ids(node: &mut SlideNode, prefix: &str) {
    match node {
        SlideNode::Shape { id, .. }
        | SlideNode::Image { id, .. }
        | SlideNode::Table { id, .. }
        | SlideNode::Chart { id, .. }
        | SlideNode::Media { id, .. }
        | SlideNode::Unknown { id, .. } => *id = format!("{prefix}:{id}"),
        SlideNode::Group { id, children, .. } => {
            *id = format!("{prefix}:{id}");
            for child in children {
                prefix_node_ids(child, prefix);
            }
        }
    }
}

fn master_shapes_are_visible(root: XmlNode<'_>) -> bool {
    root.attr("showMasterSp")
        .and_then(bool_value)
        .unwrap_or(true)
}

fn parse_slide(
    root: XmlNode<'_>,
    context: &NodeParseContext<'_, '_>,
) -> Result<ParsedSlideContent, ParseError> {
    let background = [
        (Some(root), Some(context.rels)),
        (context.layout_root, context.layout_rels),
        (context.master_root, context.master_rels),
    ]
    .into_iter()
    .find_map(|(part, rels)| part_background(part?, context.colors, rels?));
    let mut nodes = Vec::new();
    if master_shapes_are_visible(root)
        && context
            .layout_root
            .map(master_shapes_are_visible)
            .unwrap_or(true)
    {
        if let (Some(master), Some(master_rels)) = (context.master_root, context.master_rels) {
            let master_context = NodeParseContext {
                artwork_target: context.artwork_target,
                artwork_part: context.artwork_part,
                artwork_id_match: ArtworkIdMatch::Prefixed("master"),
                shape_cache: context.shape_cache,
                editing_cache: context.editing_cache,
                source_part: context.layout_rels.and_then(|rels| {
                    rels.related("slideMaster", "ppt/slideMasters/")
                        .map(String::as_str)
                }),
                slide_index: context.slide_index,
                paint_scale: context.paint_scale,
                rels: master_rels,
                related_parts: context.related_parts,
                related_relationships: context.related_relationships,
                assets: context.assets,
                chart_companions: context.chart_companions,
                legacy_chart_defaults: context.legacy_chart_defaults,
                options: context.options,
                limits: context.limits,
                colors: context.colors,
                diagnostics: context.diagnostics,
                layout_root: None,
                master_root: None,
                layout_rels: None,
                master_rels: None,
                presentation_text_style: context.presentation_text_style,
                table_styles: context.table_styles,
            };
            let mut inherited = parse_decorative_part_nodes(master, &master_context)?;
            for node in &mut inherited {
                prefix_node_ids(node, "master");
            }
            nodes.extend(inherited);
        }
    }
    if let (Some(layout), Some(layout_rels)) = (context.layout_root, context.layout_rels) {
        let layout_context = NodeParseContext {
            artwork_target: context.artwork_target,
            artwork_part: context.artwork_part,
            artwork_id_match: ArtworkIdMatch::Prefixed("layout"),
            shape_cache: context.shape_cache,
            editing_cache: context.editing_cache,
            source_part: context
                .rels
                .related("slideLayout", "ppt/slideLayouts/")
                .map(String::as_str),
            slide_index: context.slide_index,
            paint_scale: context.paint_scale,
            rels: layout_rels,
            related_parts: context.related_parts,
            related_relationships: context.related_relationships,
            assets: context.assets,
            chart_companions: context.chart_companions,
            legacy_chart_defaults: context.legacy_chart_defaults,
            options: context.options,
            limits: context.limits,
            colors: context.colors,
            diagnostics: context.diagnostics,
            layout_root: None,
            master_root: None,
            layout_rels: None,
            master_rels: None,
            presentation_text_style: context.presentation_text_style,
            table_styles: context.table_styles,
        };
        let mut inherited = parse_decorative_part_nodes(layout, &layout_context)?;
        for node in &mut inherited {
            prefix_node_ids(node, "layout");
        }
        nodes.extend(inherited);
    }
    nodes.extend(parse_part_nodes(root, context)?);
    let (transition, timing) = timing::parse(
        root,
        context.rels,
        context.assets.assets,
        context.colors,
        context.limits,
    )?;
    Ok(ParsedSlideContent {
        transition,
        timing,
        name: root
            .child("cSld")
            .and_then(|slide| slide.attr("name"))
            .map(str::to_owned),
        hidden: root
            .attr("show")
            .and_then(bool_value)
            .map(|show| !show)
            .filter(|hidden| *hidden),
        background,
        nodes,
    })
}

fn parse_core_metadata(xml: &[u8]) -> PresentationMetadata {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut field: Option<String> = None;
    let mut metadata = PresentationMetadata::default();
    loop {
        match reader.read_event() {
            Ok(Event::Start(start)) => {
                field =
                    Some(String::from_utf8_lossy(local_name(start.name().as_ref())).into_owned())
            }
            Ok(Event::Text(text)) => {
                if let Some(name) = field.as_deref() {
                    let value = text
                        .unescape()
                        .map(|text| text.into_owned())
                        .unwrap_or_default();
                    match name {
                        "title" => metadata.title = Some(value),
                        "subject" => metadata.subject = Some(value),
                        "creator" => metadata.creator = Some(value),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(_)) => field = None,
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    metadata
}

fn parse_notes(
    xml: &[u8],
    limits: &ParseLimits,
    part_name: &str,
    rels: &Relationships,
) -> Result<Vec<SlideNote>, ParseError> {
    let document = parse_xml_tree(xml.to_vec(), limits, part_name)?;
    let root = document.root();
    let mut shapes = Vec::new();
    root.collect_descendants("sp", &mut shapes);
    let mut paragraphs = Vec::new();
    let context = TextParseContext {
        source_part: Some(part_name),
        presentation_text_style: None,
        shape_default_style: RunStyle::default(),
        colors: None,
        rels,
    };
    for shape in shapes {
        let placeholder = shape
            .child("nvSpPr")
            .and_then(|properties| properties.child("nvPr"))
            .and_then(|properties| properties.child("ph"))
            .and_then(|placeholder| placeholder.attr("type"));
        if matches!(
            placeholder,
            Some("sldImg" | "sldNum" | "dt" | "hdr" | "ftr")
        ) {
            continue;
        }
        if let Some(body) = shape.child("txBody") {
            paragraphs.extend(parse_text_paragraphs(
                Some(body),
                None,
                None,
                None,
                &context,
            ));
        }
    }
    let text = paragraphs
        .iter()
        .map(|paragraph| {
            paragraph
                .runs
                .iter()
                .map(|run| run.text.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(
        if text.trim().is_empty()
            && !paragraphs
                .iter()
                .any(|paragraph| paragraph.runs.iter().any(|run| run.field.is_some()))
        {
            Vec::new()
        } else {
            vec![SlideNote { text, paragraphs }]
        },
    )
}

fn parse_text_values(xml: &[u8]) -> Vec<String> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut in_text = false;
    let mut values = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(start)) => {
                in_text = matches!(local_name(start.name().as_ref()), b"t" | b"text")
            }
            Ok(Event::Text(text)) if in_text => {
                if let Ok(value) = text.unescape() {
                    let value = value.trim();
                    if !value.is_empty() {
                        values.push(value.to_owned());
                    }
                }
            }
            Ok(Event::End(end)) if matches!(local_name(end.name().as_ref()), b"t" | b"text") => {
                in_text = false
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    values
}

fn content_type(path: &str) -> String {
    match path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "bmp" | "dib" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "wdp" => "image/vnd.ms-photo",
        "emf" => "image/x-emf",
        "wmf" => "image/x-wmf",
        "glb" => "model/gltf-binary",
        "gltf" => "model/gltf+json",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
    .into()
}

#[derive(Clone)]
struct ParsedXmlPart {
    path: String,
    document: XmlDocument,
    rels: Relationships,
}

fn relationship_part_path(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((directory, file_name)) => format!("{directory}/_rels/{file_name}.rels"),
        None => format!("_rels/{path}.rels"),
    }
}

fn load_xml_part(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    path: &str,
    limits: &ParseLimits,
) -> Result<Option<ParsedXmlPart>, ParseError> {
    let Some(xml) = read_entry(archive, path, limits)? else {
        return Ok(None);
    };
    let document = parse_xml_tree(xml, limits, path)?;
    let rels = match read_entry(archive, &relationship_part_path(path), limits)? {
        Some(xml) => relationships(&xml, path, limits.max_xml_depth)?,
        None => Relationships::default(),
    };
    Ok(Some(ParsedXmlPart {
        path: path.to_owned(),
        document,
        rels,
    }))
}

fn load_theme_part(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    path: &str,
    limits: &ParseLimits,
) -> Result<Option<ThemeDataInternal>, ParseError> {
    let Some(xml) = read_entry(archive, path, limits)? else {
        return Ok(None);
    };
    let rels = match read_entry(archive, &relationship_part_path(path), limits)? {
        Some(xml) => relationships(&xml, path, limits.max_xml_depth)?,
        None => Relationships::default(),
    };
    Ok(Some(parse_theme(xml, path, rels, limits)?))
}

fn related_path(rels: &Relationships, kind: &str, prefix: &str) -> Option<String> {
    rels.related(kind, prefix).cloned()
}

fn missing_part_warning(path: &str, slide_index: Option<usize>) -> PresentationWarning {
    let mut warning = PresentationWarning::warning(
        "missing-part",
        format!("Related presentation part {path} is missing."),
    );
    warning.slide_index = slide_index;
    warning.part_name = Some(path.to_owned());
    warning
}

pub fn parse(
    bytes: &[u8],
    limits: &ParseLimits,
    options: &ParseOptions,
) -> Result<PresentationDocument, ParseError> {
    parse_with_artwork_target(bytes, limits, options, None)
}

/// Parses with artwork rendered only for `target` when one is given; without a
/// target, `options.defer_artwork` decides whether any artwork is rendered.
pub(crate) fn parse_with_artwork_target(
    bytes: &[u8],
    limits: &ParseLimits,
    options: &ParseOptions,
    target: Option<&crate::ArtworkTarget>,
) -> Result<PresentationDocument, ParseError> {
    rust_office_art::with_cached_xml(|| {
        parse_cached(bytes, limits, options, target, false).map(|(document, _)| document)
    })
}

/// Parses with artwork deferred and keeps the inflated parts, their XML trees,
/// relationships, themes, color maps and related parts, so that nodes can be
/// rendered on demand without reading the archive again.
pub(crate) fn parse_retained(
    bytes: &[u8],
    limits: &ParseLimits,
    options: &ParseOptions,
) -> Result<(PresentationDocument, RetainedParse), ParseError> {
    let options = ParseOptions {
        defer_artwork: true,
        ..*options
    };
    rust_office_art::with_cached_xml(|| {
        let (document, retained) = parse_cached(bytes, limits, &options, None, true)?;
        Ok((
            document,
            retained.expect("a retaining parse keeps its state"),
        ))
    })
}

fn parse_cached(
    bytes: &[u8],
    limits: &ParseLimits,
    options: &ParseOptions,
    artwork_target: Option<&crate::ArtworkTarget>,
    retain: bool,
) -> Result<(PresentationDocument, Option<RetainedParse>), ParseError> {
    let cursor = Cursor::new(bytes);
    let mut archive =
        ZipArchive::new(cursor).map_err(|error| ParseError::Corrupt(format!("ZIP: {error}")))?;
    if archive.len() > limits.max_entries {
        return Err(ParseError::ResourceLimit("ZIP entry count".into()));
    }
    let total: u64 = (0..archive.len())
        .filter_map(|index| archive.by_index(index).ok().map(|file| file.size()))
        .sum();
    if total > limits.max_total_uncompressed_bytes as u64 {
        return Err(ParseError::ResourceLimit("ZIP expanded size".into()));
    }
    if archive.by_name("EncryptedPackage").is_ok() {
        return Err(ParseError::Encrypted);
    }

    let content_types = read_entry(&mut archive, "[Content_Types].xml", limits)?
        .map(|xml| parse_xml_tree(xml, limits, "[Content_Types].xml"))
        .transpose()?
        .unwrap_or_else(|| XmlDocument::leaf("Types"));
    let package_rels = match read_entry(&mut archive, "_rels/.rels", limits)? {
        Some(xml) => relationships(&xml, "", limits.max_xml_depth)?,
        None => Relationships::default(),
    };
    let app_path = package_rels
        .related("extended-properties", "\0")
        .map(String::as_str)
        .unwrap_or("docProps/app.xml");
    let legacy_chart_defaults = read_entry(&mut archive, app_path, limits)?
        .and_then(|xml| parse_xml_tree(xml, limits, app_path).ok())
        .and_then(|document| {
            document.root().child("AppVersion")?
                .text()
                .trim()
                .split('.')
                .next()?
                .parse::<u16>()
                .ok()
        })
        .is_some_and(|version| (1..=12).contains(&version));
    let presentation_path = package_rels
        .related("officeDocument", "\0")
        .map(String::as_str)
        .unwrap_or("ppt/presentation.xml");
    let presentation = read_entry(&mut archive, presentation_path, limits)?
        .ok_or_else(|| ParseError::Corrupt(format!("missing {presentation_path}")))?;
    let rel_bytes = read_entry(
        &mut archive,
        &relationship_part_path(presentation_path),
        limits,
    )?
    .unwrap_or_default();
    let rels = relationships(&rel_bytes, presentation_path, limits.max_xml_depth)?;
    let mut comment_authors = None;
    let mut modern_comment_authors = None;
    let (size, slide_paths) = parse_presentation_xml(&presentation, &rels, limits)?;
    let embedded_font_refs = parse_embedded_font_references(&presentation);
    let presentation_document = parse_xml_tree(presentation, limits, presentation_path)?;
    let presentation_root = presentation_document.root();
    if presentation_root.name() != "presentation" {
        return Err(ParseError::Corrupt("invalid presentation root".into()));
    }
    let presentation_text_style = presentation_root.child("defaultTextStyle");
    let table_styles_path = related_path(&rels, "tableStyles", "ppt/tableStyles")
        .unwrap_or_else(|| "ppt/tableStyles.xml".to_owned());
    let table_styles_root = match read_entry(&mut archive, &table_styles_path, limits)? {
        Some(xml) => Some(parse_xml_tree(xml, limits, &table_styles_path)?),
        None => None,
    };
    if slide_paths.is_empty()
        && presentation_root
            .child("sldIdLst")
            .is_some_and(|list| list.children_named("sldId").next().is_some())
    {
        return Err(ParseError::Corrupt(
            "presentation has no related slides".into(),
        ));
    }

    let mut slides = Vec::with_capacity(slide_paths.len());
    let mut asset_paths = BTreeMap::<String, String>::new();
    let mut assets = BTreeMap::new();
    let mut layout_parts = BTreeMap::<String, ParsedXmlPart>::new();
    let mut master_parts = BTreeMap::<String, ParsedXmlPart>::new();
    let mut theme_parts = BTreeMap::<String, ThemeDataInternal>::new();
    let mut document_warnings = Vec::new();
    let shape_cache = RefCell::new(paint::ShapeCache::default());
    let editing_cache = RefCell::new(editing::Cache::default());
    let mut generations = retain.then(AssetGenerations::default);
    let mut retained_slides = Vec::new();
    let mut retained_layouts = BTreeMap::new();
    let mut retained_master_views = BTreeMap::new();
    for (index, slide_path) in slide_paths.iter().enumerate() {
        let xml = read_entry(&mut archive, slide_path, limits)?
            .ok_or_else(|| ParseError::Corrupt(format!("missing {slide_path}")))?;
        let slide_document = parse_xml_tree(xml, limits, slide_path)?;
        let slide_root = slide_document.root();
        let slide_relationship_xml =
            read_entry(&mut archive, &relationship_part_path(slide_path), limits)?;
        let slide_rels = match slide_relationship_xml.as_deref() {
            Some(bytes) => relationships(bytes, slide_path, limits.max_xml_depth)?,
            None => Relationships::default(),
        };

        let layout_path = related_path(&slide_rels, "slideLayout", "ppt/slideLayouts/");
        if let Some(path) = layout_path.as_deref() {
            if !layout_parts.contains_key(path) {
                match load_xml_part(&mut archive, path, limits)? {
                    Some(part) => {
                        layout_parts.insert(path.to_owned(), part);
                    }
                    None => document_warnings.push(missing_part_warning(path, Some(index))),
                }
            }
        }
        let layout = layout_path.as_ref().and_then(|path| layout_parts.get(path));
        let master_path = layout
            .as_ref()
            .and_then(|part| related_path(&part.rels, "slideMaster", "ppt/slideMasters/"));
        if let Some(path) = master_path.as_deref() {
            if !master_parts.contains_key(path) {
                match load_xml_part(&mut archive, path, limits)? {
                    Some(part) => {
                        master_parts.insert(path.to_owned(), part);
                    }
                    None => document_warnings.push(missing_part_warning(path, Some(index))),
                }
            }
        }
        let master = master_path.as_ref().and_then(|path| master_parts.get(path));
        let theme_path = master
            .as_ref()
            .and_then(|part| related_path(&part.rels, "theme", "ppt/theme/"));
        if let Some(path) = theme_path.as_deref() {
            if !theme_parts.contains_key(path) {
                match load_theme_part(&mut archive, path, limits)? {
                    Some(theme) => {
                        theme_parts.insert(path.to_owned(), theme);
                    }
                    None => document_warnings.push(missing_part_warning(path, Some(index))),
                }
            }
        }
        let theme = theme_path.as_ref().and_then(|path| theme_parts.get(path));

        let mut related_paths = BTreeMap::<String, bool>::new();
        for target in slide_rels.values().filter(|target| {
            target.starts_with("ppt/charts/") || target.starts_with("ppt/diagrams/")
        }) {
            related_paths.insert(target.clone(), target.starts_with("ppt/charts/"));
        }
        let comments_path = related_path(&slide_rels, "comments", "ppt/comments/").or_else(|| {
            slide_rels
                .entries
                .values()
                .filter(|entry| {
                    !entry.external
                        && entry.kind
                            == "http://schemas.microsoft.com/office/2018/10/relationships/comments"
                })
                .min_by_key(|entry| entry.order)
                .map(|entry| entry.target.clone())
        });
        {
            for (relationship_type, target) in slide_rels.typed() {
                let is_chart = is_chart_relationship_type(&relationship_type);
                if is_chart
                    || is_diagram_relationship_type(&relationship_type)
                    || relationship_type.rsplit('/').next() == Some("control")
                    || (relationship_type.rsplit('/').next() == Some("vmlDrawing")
                        && slide_root.descendant("control").is_some())
                {
                    related_paths
                        .entry(target.clone())
                        .and_modify(|existing| *existing |= is_chart)
                        .or_insert(is_chart);
                }
            }
        }
        for (root, rels) in [
            (Some(slide_root), Some(&slide_rels)),
            (layout.map(|part| part.document.root()), layout.map(|part| &part.rels)),
            (master.map(|part| part.document.root()), master.map(|part| &part.rels)),
        ] {
            if let (Some(root), Some(rels)) = (root, rels) {
                let mut content_parts = Vec::new();
                root.collect_descendants("contentPart", &mut content_parts);
                root.collect_descendants("control", &mut content_parts);
                if content_parts.iter().any(|node| node.name() == "control") {
                    for path in rels.values().filter(|path| path.ends_with(".vml")) {
                        related_paths.entry(path.clone()).or_insert(false);
                    }
                }
                for content in content_parts {
                    if let Some(path) = content.attr("id").and_then(|id| rels.internal(id)) {
                        related_paths.entry(path.clone()).or_insert(false);
                    }
                }
            }
        }
        let mut related_parts = HashMap::new();
        let mut related_relationships = HashMap::new();
        let mut chart_companions: HashMap<String, ChartCompanionParts> = HashMap::new();
        while let Some((target, is_chart_part)) = related_paths.pop_first() {
            if related_parts.contains_key(&target) {
                continue;
            }
            if related_parts.len() >= limits.max_entries {
                return Err(ParseError::ResourceLimit(
                    "related presentation parts".into(),
                ));
            }
            if let Some(bytes) = read_entry(&mut archive, &target, limits)? {
                related_parts.insert(target.clone(), bytes);
                let Some(rels_xml) =
                    read_entry(&mut archive, &relationship_part_path(&target), limits)?
                else {
                    continue;
                };
                let part_rels = relationships(&rels_xml, &target, limits.max_xml_depth)?;
                for path in part_rels.asset_paths() {
                    asset_paths.insert(path.clone(), path.clone());
                }
                if !is_chart_part {
                    for (kind, path) in part_rels.typed() {
                        if matches!(
                            kind.rsplit('/').next(),
                            Some("activeXControlBinary" | "image")
                        ) {
                            asset_paths
                                .entry(path.clone())
                                .or_insert_with(|| kind.to_owned());
                        }
                    }
                    for path in part_rels
                        .values()
                        .filter(|path| path.starts_with("ppt/diagrams/"))
                    {
                        if !related_parts.contains_key(path) {
                            related_paths.entry(path.clone()).or_insert(false);
                        }
                    }
                    for (kind, path) in part_rels.typed() {
                        if is_diagram_relationship_type(&kind) && !related_parts.contains_key(path)
                        {
                            related_paths.entry(path.clone()).or_insert(false);
                        }
                    }
                    related_relationships.insert(target.clone(), part_rels);
                    continue;
                }
                let mut companions = ChartCompanionParts::default();
                for (rel_type, rel_target) in part_rels.typed() {
                    let slot = match rel_type {
                        CHART_STYLE_REL_TYPE => &mut companions.style_xml,
                        CHART_COLOR_STYLE_REL_TYPE => &mut companions.colors_xml,
                        kind if kind.rsplit('/').next() == Some("themeOverride") => {
                            &mut companions.theme_override_xml
                        }
                        _ => continue,
                    };
                    if slot.is_none() {
                        if let Some(bytes) = read_entry(&mut archive, &rel_target, limits)? {
                            *slot = Some(String::from_utf8_lossy(&bytes).into_owned());
                        }
                    }
                }
                related_relationships.insert(target.clone(), part_rels);
                if companions.style_xml.is_some()
                    || companions.colors_xml.is_some()
                    || companions.theme_override_xml.is_some()
                {
                    chart_companions.insert(target, companions);
                }
            }
        }
        for rels in [
            Some(&slide_rels),
            layout.as_ref().map(|part| &part.rels),
            master.as_ref().map(|part| &part.rels),
            theme.as_ref().map(|part| &part.rels),
        ]
        .into_iter()
        .flatten()
        {
            for target in rels.asset_paths() {
                asset_paths
                    .entry(target.clone())
                    .or_insert_with(|| target.clone());
            }
        }
        collect_model_assets(slide_root, &slide_rels, &mut asset_paths);
        for part in [layout.as_ref(), master.as_ref()].into_iter().flatten() {
            collect_model_assets(part.document.root(), &part.rels, &mut asset_paths);
        }
        load_picture_assets(
            &mut archive,
            &asset_paths,
            &mut assets,
            limits,
            options,
            content_types.root(),
            generations.as_mut(),
        )?;
        let notes = if let Some(target) = slide_rels.related("notesSlide", "ppt/notesSlides/") {
            let notes_rels =
                match read_entry(&mut archive, &relationship_part_path(target), limits)? {
                    Some(xml) => relationships(&xml, target, limits.max_xml_depth)?,
                    None => Relationships::default(),
                };
            read_entry(&mut archive, target, limits)?
                .map(|xml| parse_notes(&xml, limits, target, &notes_rels))
                .transpose()?
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let comments = if let Some(target) = comments_path {
            if let Some(xml) = read_entry(&mut archive, &target, limits)? {
                let modern = comments::is_modern(&xml);
                let author_cache = if modern {
                    &mut modern_comment_authors
                } else {
                    &mut comment_authors
                };
                if author_cache.is_none() {
                    let mut authors = HashMap::new();
                    for (kind, path) in rels.typed() {
                        let selected = if modern {
                            kind == "http://schemas.microsoft.com/office/2018/10/relationships/authors"
                        } else {
                            office_relationship(&kind, "commentAuthors")
                        };
                        if selected {
                            if let Some(xml) = read_entry(&mut archive, path, limits)? {
                                authors.extend(if modern {
                                    comments::modern_authors(&xml, limits, path)?
                                } else {
                                    comments::authors(&xml, limits, path)?
                                });
                            }
                        }
                    }
                    *author_cache = Some(authors);
                }
                let comment_rels = if modern {
                    match read_entry(&mut archive, &relationship_part_path(&target), limits)? {
                        Some(xml) => relationships(&xml, &target, limits.max_xml_depth)?,
                        None => Relationships::default(),
                    }
                } else {
                    Relationships::default()
                };
                let empty_authors = HashMap::new();
                let authors = author_cache.as_ref().unwrap();
                comments::parse(
                    &xml,
                    limits,
                    &target,
                    if modern { &empty_authors } else { authors },
                    if modern { authors } else { &empty_authors },
                    &comment_rels,
                )?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let master_color_map = master
            .as_ref()
            .map(|part| color_map(part.document.root()))
            .unwrap_or_default();
        let layout_color_map = layout
            .as_ref()
            .map(|part| merge_color_maps(&master_color_map, &color_map(part.document.root())))
            .unwrap_or_else(|| master_color_map.clone());
        let slide_color_map = merge_color_maps(&layout_color_map, &color_map(slide_root));
        let diagnostics = ParseDiagnostics::new(Some(index), slide_path);
        let content = {
            let colors = ColorContext {
                theme,
                color_map: &slide_color_map,
                diagnostics: &diagnostics,
                paint_key: paint::ColorKey::new(theme, &slide_color_map),
            };
            let context = NodeParseContext {
                artwork_target,
                artwork_part: ArtworkPart::Slide(index),
                artwork_id_match: ArtworkIdMatch::Exact,
                shape_cache: &shape_cache,
                editing_cache: &editing_cache,
                source_part: Some(slide_path),
                slide_index: index,
                paint_scale: (1.0, 1.0),
                rels: &slide_rels,
                related_parts: &related_parts,
                related_relationships: &related_relationships,
                assets: AssetView::unfiltered(&assets),
                chart_companions: &chart_companions,
                legacy_chart_defaults,
                options,
                limits,
                colors: Some(&colors),
                diagnostics: &diagnostics,
                layout_root: layout.as_ref().map(|part| part.document.root()),
                master_root: master.as_ref().map(|part| part.document.root()),
                layout_rels: layout.as_ref().map(|part| &part.rels),
                master_rels: master.as_ref().map(|part| &part.rels),
                presentation_text_style,
                table_styles: table_styles_root.as_ref().map(XmlDocument::root),
            };
            parse_slide(slide_root, &context)?
        };
        let slide_warnings = diagnostics.take();
        let retained_paths = retain.then(|| (layout_path.clone(), master_path.clone()));
        slides.push(PresentationSlide {
            id: format!("slide-{}", index + 1),
            index,
            name: content.name,
            hidden: content.hidden,
            master_id: master_path,
            layout_id: layout_path,
            background: content.background,
            transition: content.transition,
            timing: content.timing,
            nodes: content.nodes,
            notes,
            comments,
            source_part: Some(slide_path.clone()),
            warnings: slide_warnings,
        });
        if let Some((layout_path, master_path)) = retained_paths {
            retained_slides.push(RetainedSlide {
                path: slide_path.clone(),
                document: slide_document,
                rels: slide_rels,
                layout_path,
                master_path,
                theme_path,
                color_map: slide_color_map,
                related_parts,
                related_relationships,
                chart_companions,
                generation: generations
                    .as_ref()
                    .map_or(0, |generations| generations.current),
            });
        }
    }

    let mut declared_master_paths = rels
        .paths("slideMaster", "ppt/slideMasters/")
        .cloned()
        .collect::<Vec<_>>();
    declared_master_paths.sort_unstable();
    for path in declared_master_paths {
        if master_parts.contains_key(&path) {
            continue;
        }
        match load_xml_part(&mut archive, &path, limits)? {
            Some(part) => {
                master_parts.insert(path, part);
            }
            None => document_warnings.push(missing_part_warning(&path, None)),
        }
    }
    let mut declared_layout_paths = master_parts
        .values()
        .flat_map(|part| part.rels.paths("slideLayout", "ppt/slideLayouts/"))
        .cloned()
        .collect::<Vec<_>>();
    declared_layout_paths.sort_unstable();
    let layout_warning_offset = document_warnings.len();
    let mut declared_theme_paths = master_parts
        .values()
        .flat_map(|part| part.rels.paths("theme", "ppt/theme/"))
        .cloned()
        .collect::<Vec<_>>();
    declared_theme_paths.sort_unstable();
    for path in declared_theme_paths {
        if theme_parts.contains_key(&path) {
            continue;
        }
        match load_theme_part(&mut archive, &path, limits)? {
            Some(theme) => {
                theme_parts.insert(path.clone(), theme);
            }
            None => document_warnings.push(missing_part_warning(&path, None)),
        }
    }
    for target in master_parts
        .values()
        .chain(layout_parts.values())
        .flat_map(|part| part.rels.asset_paths())
        .chain(
            theme_parts
                .values()
                .flat_map(|theme| theme.rels.asset_paths()),
        )
    {
        asset_paths
            .entry(target.clone())
            .or_insert_with(|| target.clone());
    }

    for part in master_parts.values().chain(layout_parts.values()) {
        collect_model_assets(part.document.root(), &part.rels, &mut asset_paths);
    }
    load_picture_assets(
        &mut archive,
        &asset_paths,
        &mut assets,
        limits,
        options,
        content_types.root(),
        generations.as_mut(),
    )?;
    let empty_related_parts = HashMap::new();
    let empty_related_relationships = HashMap::new();
    let empty_chart_companions = HashMap::new();
    let mut masters = Vec::new();
    for part in master_parts.values() {
        let theme_path = related_path(&part.rels, "theme", "ppt/theme/");
        let theme = theme_path.as_ref().and_then(|path| theme_parts.get(path));
        let part_color_map = color_map(part.document.root());
        let diagnostics = ParseDiagnostics::new(None, &part.path);
        let (nodes, transition, timing) = {
            let colors = ColorContext {
                theme,
                color_map: &part_color_map,
                diagnostics: &diagnostics,
                paint_key: paint::ColorKey::new(theme, &part_color_map),
            };
            let context = NodeParseContext {
                artwork_target,
                artwork_part: ArtworkPart::Master(&part.path),
                artwork_id_match: ArtworkIdMatch::Exact,
                shape_cache: &shape_cache,
                editing_cache: &editing_cache,
                source_part: Some(&part.path),
                slide_index: 0,
                paint_scale: (1.0, 1.0),
                rels: &part.rels,
                related_parts: &empty_related_parts,
                related_relationships: &empty_related_relationships,
                assets: AssetView::unfiltered(&assets),
                chart_companions: &empty_chart_companions,
                legacy_chart_defaults,
                options,
                limits,
                colors: Some(&colors),
                diagnostics: &diagnostics,
                layout_root: None,
                master_root: None,
                layout_rels: None,
                master_rels: None,
                presentation_text_style,
                table_styles: table_styles_root.as_ref().map(XmlDocument::root),
            };
            let nodes = parse_part_nodes(part.document.root(), &context)?;
            let (transition, timing) =
                timing::parse(part.document.root(), &part.rels, &assets, Some(&colors), limits)?;
            (nodes, transition, timing)
        };
        document_warnings.extend(diagnostics.take());
        if retain {
            retained_master_views.insert(
                part.path.clone(),
                (
                    part_color_map,
                    generations.as_ref().map_or(0, |generations| generations.current),
                ),
            );
        }
        masters.push(crate::SlideMaster {
            id: part.path.clone(),
            name: part
                .document
                .root()
                .child("cSld")
                .and_then(|slide| slide.attr("name"))
                .map(str::to_owned),
            theme_id: theme_path,
            transition,
            timing,
            nodes,
        });
    }

    let layout_paths: std::collections::BTreeSet<_> = declared_layout_paths
        .iter()
        .chain(layout_parts.keys())
        .cloned()
        .collect();
    let mut missing_layout_warnings = Vec::new();
    let mut layouts = Vec::with_capacity(layout_paths.len());
    for path in layout_paths {
        let part = if let Some(part) = layout_parts.remove(&path) {
            part
        } else if let Some(part) = load_xml_part(&mut archive, &path, limits)? {
            let mut part_asset_paths = BTreeMap::new();
            for target in part.rels.asset_paths() {
                part_asset_paths.insert(target.clone(), target.clone());
            }
            collect_model_assets(part.document.root(), &part.rels, &mut part_asset_paths);
            load_picture_assets(
                &mut archive,
                &part_asset_paths,
                &mut assets,
                limits,
                options,
                content_types.root(),
                generations.as_mut(),
            )?;
            part
        } else {
            missing_layout_warnings.extend(
                declared_layout_paths
                    .iter()
                    .filter(|declared| *declared == &path)
                    .map(|_| missing_part_warning(&path, None)),
            );
            continue;
        };
        let master_path = related_path(&part.rels, "slideMaster", "ppt/slideMasters/");
        let master = master_path.as_ref().and_then(|path| master_parts.get(path));
        let theme = master
            .and_then(|part| related_path(&part.rels, "theme", "ppt/theme/"))
            .and_then(|path| theme_parts.get(&path));
        let master_color_map = master.map(|part| color_map(part.document.root())).unwrap_or_default();
        let part_color_map = merge_color_maps(&master_color_map, &color_map(part.document.root()));
        let diagnostics = ParseDiagnostics::new(None, &part.path);
        let (nodes, transition, timing) = {
            let colors = ColorContext {
                theme,
                color_map: &part_color_map,
                diagnostics: &diagnostics,
                paint_key: paint::ColorKey::new(theme, &part_color_map),
            };
            let context = NodeParseContext {
                artwork_target,
                artwork_part: ArtworkPart::Layout(&part.path),
                artwork_id_match: ArtworkIdMatch::Exact,
                shape_cache: &shape_cache,
                editing_cache: &editing_cache,
                source_part: Some(&part.path),
                slide_index: 0,
                paint_scale: (1.0, 1.0),
                rels: &part.rels,
                related_parts: &empty_related_parts,
                related_relationships: &empty_related_relationships,
                assets: AssetView::unfiltered(&assets),
                chart_companions: &empty_chart_companions,
                legacy_chart_defaults,
                options,
                limits,
                colors: Some(&colors),
                diagnostics: &diagnostics,
                layout_root: None,
                master_root: master.map(|part| part.document.root()),
                layout_rels: None,
                master_rels: master.map(|part| &part.rels),
                presentation_text_style,
                table_styles: table_styles_root.as_ref().map(XmlDocument::root),
            };
            let nodes = parse_part_nodes(part.document.root(), &context)?;
            let (transition, timing) =
                timing::parse(part.document.root(), &part.rels, &assets, Some(&colors), limits)?;
            (nodes, transition, timing)
        };
        document_warnings.extend(diagnostics.take());
        layouts.push(crate::SlideLayout {
            id: part.path.clone(),
            name: part
                .document
                .root()
                .child("cSld")
                .and_then(|slide| slide.attr("name"))
                .map(str::to_owned),
            master_id: master_path,
            transition,
            timing,
            nodes,
        });
        if retain {
            retained_layouts.insert(
                part.path.clone(),
                RetainedPart {
                    color_map: part_color_map,
                    generation: generations.as_ref().map_or(0, |generations| generations.current),
                    part,
                },
            );
        }
    }
    document_warnings.splice(
        layout_warning_offset..layout_warning_offset,
        missing_layout_warnings,
    );

    let mut embedded_fonts = Vec::new();
    let mut font_warnings = Vec::new();
    for font in embedded_font_refs {
        for (rel_id, style, weight) in font.variants {
            let Some(path) = rels.internal(&rel_id) else {
                font_warnings.push(PresentationWarning::warning(
                    "missing-font",
                    format!(
                        "Embedded font relationship {rel_id} for {} is missing.",
                        font.family
                    ),
                ));
                continue;
            };
            let Some(mut data) = read_entry(&mut archive, path, limits)? else {
                font_warnings.push(PresentationWarning::warning(
                    "missing-font",
                    format!("Embedded font data for {} is missing.", font.family),
                ));
                continue;
            };
            if !deobfuscate_embedded_font(path, &mut data) {
                font_warnings.push(PresentationWarning::warning(
                    "degraded-rendering",
                    format!("Embedded font {} could not be decoded safely.", font.family),
                ));
                continue;
            }
            let content_type = font_magic(&data)
                .unwrap_or("application/octet-stream")
                .to_owned();
            assets.insert(
                path.clone(),
                PresentationAsset {
                    id: path.clone(),
                    content_type,
                    byte_length: data.len(),
                    file_name: path.rsplit('/').next().map(str::to_owned),
                    data: Some(data.into()),
                },
            );
            embedded_fonts.push(PresentationEmbeddedFont {
                family: font.family.clone(),
                asset_id: path.clone(),
                style,
                weight,
            });
        }
    }
    let core_path = package_rels.typed().find_map(|(kind, path)| {
        (kind == "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties")
            .then_some(path.as_str())
    }).unwrap_or("docProps/core.xml");
    let metadata =
        read_entry(&mut archive, core_path, limits)?.map(|xml| parse_core_metadata(&xml));
    document_warnings.extend(font_warnings);
    let document = PresentationDocument {
        document_id: None,
        format: PresentationFormat::Pptx,
        size,
        slides,
        masters,
        layouts,
        themes: theme_parts
            .values()
            .map(ThemeDataInternal::public)
            .collect(),
        assets,
        embedded_fonts,
        warnings: document_warnings,
        metadata,
    };
    let retained = if retain {
        Some(RetainedParse {
            limits: *limits,
            options: *options,
            legacy_chart_defaults,
            presentation: presentation_document,
            table_styles: table_styles_root,
            themes: theme_parts,
            masters: master_parts
                .into_iter()
                .map(|(path, part)| {
                    let (color_map, generation) =
                        retained_master_views.remove(&path).unwrap_or_default();
                    (
                        path,
                        RetainedPart {
                            part,
                            color_map,
                            generation,
                        },
                    )
                })
                .collect(),
            layouts: retained_layouts,
            slides: retained_slides,
            asset_generations: generations
                .map(|generations| generations.by_path)
                .unwrap_or_default(),
            shape_cache,
            editing_cache,
        })
    } else {
        None
    };
    Ok((document, retained))
}

/// Insertion order of the assets, so that a retained render sees exactly the
/// assets the parse had loaded when it parsed the same part.
#[derive(Default)]
struct AssetGenerations {
    by_path: HashMap<String, u32>,
    current: u32,
}

impl AssetGenerations {
    fn advance(&mut self) {
        self.current += 1;
    }

    fn record(&mut self, path: &str) {
        self.by_path.insert(path.to_owned(), self.current);
    }
}

/// A layout or master kept for on-demand rendering, with the color map and
/// asset generation its nodes were parsed with.
struct RetainedPart {
    part: ParsedXmlPart,
    color_map: BTreeMap<String, String>,
    generation: u32,
}

/// A slide kept for on-demand rendering: its XML tree, relationships, merged
/// color map, related chart, diagram, ink and control parts, and the asset
/// generation its nodes were parsed with.
struct RetainedSlide {
    path: String,
    document: XmlDocument,
    rels: Relationships,
    layout_path: Option<String>,
    master_path: Option<String>,
    theme_path: Option<String>,
    color_map: BTreeMap<String, String>,
    related_parts: HashMap<String, Vec<u8>>,
    related_relationships: HashMap<String, Relationships>,
    chart_companions: HashMap<String, ChartCompanionParts>,
    generation: u32,
}

/// What a node render needs after a deferred parse, without the archive: the
/// inflated parts and their XML trees, relationships, themes, color maps,
/// related parts and the caches shared by renders.
pub(crate) struct RetainedParse {
    limits: ParseLimits,
    options: ParseOptions,
    legacy_chart_defaults: bool,
    presentation: XmlDocument,
    table_styles: Option<XmlDocument>,
    themes: BTreeMap<String, ThemeDataInternal>,
    masters: BTreeMap<String, RetainedPart>,
    layouts: BTreeMap<String, RetainedPart>,
    slides: Vec<RetainedSlide>,
    asset_generations: HashMap<String, u32>,
    shape_cache: RefCell<paint::ShapeCache>,
    editing_cache: RefCell<editing::Cache>,
}

impl RetainedParse {
    /// Renders one node's artwork from the retained state. `assets` is the
    /// document's asset map, which the retained parse filled; each part sees
    /// only the assets that were loaded when it was parsed.
    pub(crate) fn render_node_artwork(
        &self,
        assets: &BTreeMap<String, PresentationAsset>,
        target: &crate::ArtworkTarget,
    ) -> Result<Option<crate::NodeArtwork>, ParseError> {
        rust_office_art::with_cached_xml(|| {
            if let Some(index) = target.slide_index {
                self.render_slide_node(assets, index, target)
            } else if let Some(path) = &target.layout_id {
                self.render_layout_node(assets, path, target)
            } else if let Some(path) = &target.master_id {
                self.render_master_node(assets, path, target)
            } else {
                Ok(None)
            }
        })
    }

    fn asset_view<'a>(
        &'a self,
        assets: &'a BTreeMap<String, PresentationAsset>,
        generation: u32,
    ) -> AssetView<'a> {
        AssetView {
            assets,
            generations: Some((&self.asset_generations, generation)),
        }
    }

    fn render_slide_node(
        &self,
        assets: &BTreeMap<String, PresentationAsset>,
        index: usize,
        target: &crate::ArtworkTarget,
    ) -> Result<Option<crate::NodeArtwork>, ParseError> {
        let Some(slide) = self.slides.get(index) else {
            return Ok(None);
        };
        let layout = slide
            .layout_path
            .as_ref()
            .and_then(|path| self.layouts.get(path));
        let master = slide
            .master_path
            .as_ref()
            .and_then(|path| self.masters.get(path));
        let theme = slide
            .theme_path
            .as_ref()
            .and_then(|path| self.themes.get(path));
        let diagnostics = ParseDiagnostics::new(Some(index), &slide.path);
        let colors = ColorContext {
            theme,
            color_map: &slide.color_map,
            diagnostics: &diagnostics,
            paint_key: paint::ColorKey::new(theme, &slide.color_map),
        };
        let context = NodeParseContext {
            artwork_target: Some(target),
            artwork_part: ArtworkPart::Slide(index),
            artwork_id_match: ArtworkIdMatch::Exact,
            shape_cache: &self.shape_cache,
            editing_cache: &self.editing_cache,
            source_part: Some(&slide.path),
            slide_index: index,
            paint_scale: (1.0, 1.0),
            rels: &slide.rels,
            related_parts: &slide.related_parts,
            related_relationships: &slide.related_relationships,
            assets: self.asset_view(assets, slide.generation),
            chart_companions: &slide.chart_companions,
            legacy_chart_defaults: self.legacy_chart_defaults,
            options: &self.options,
            limits: &self.limits,
            colors: Some(&colors),
            diagnostics: &diagnostics,
            layout_root: layout.map(|layout| layout.part.document.root()),
            master_root: master.map(|master| master.part.document.root()),
            layout_rels: layout.map(|layout| &layout.part.rels),
            master_rels: master.map(|master| &master.part.rels),
            presentation_text_style: self.presentation.root().child("defaultTextStyle"),
            table_styles: self.table_styles.as_ref().map(XmlDocument::root),
        };
        render_slide_target(slide.document.root(), &context, &target.node_id)
    }

    fn render_master_node(
        &self,
        assets: &BTreeMap<String, PresentationAsset>,
        path: &str,
        target: &crate::ArtworkTarget,
    ) -> Result<Option<crate::NodeArtwork>, ParseError> {
        let Some(master) = self.masters.get(path) else {
            return Ok(None);
        };
        let theme = related_path(&master.part.rels, "theme", "ppt/theme/")
            .and_then(|path| self.themes.get(&path));
        let empty_related_parts = HashMap::new();
        let empty_related_relationships = HashMap::new();
        let empty_chart_companions = HashMap::new();
        let diagnostics = ParseDiagnostics::new(None, &master.part.path);
        let colors = ColorContext {
            theme,
            color_map: &master.color_map,
            diagnostics: &diagnostics,
            paint_key: paint::ColorKey::new(theme, &master.color_map),
        };
        let context = NodeParseContext {
            artwork_target: Some(target),
            artwork_part: ArtworkPart::Master(&master.part.path),
            artwork_id_match: ArtworkIdMatch::Exact,
            shape_cache: &self.shape_cache,
            editing_cache: &self.editing_cache,
            source_part: Some(&master.part.path),
            slide_index: 0,
            paint_scale: (1.0, 1.0),
            rels: &master.part.rels,
            related_parts: &empty_related_parts,
            related_relationships: &empty_related_relationships,
            assets: self.asset_view(assets, master.generation),
            chart_companions: &empty_chart_companions,
            legacy_chart_defaults: self.legacy_chart_defaults,
            options: &self.options,
            limits: &self.limits,
            colors: Some(&colors),
            diagnostics: &diagnostics,
            layout_root: None,
            master_root: None,
            layout_rels: None,
            master_rels: None,
            presentation_text_style: self.presentation.root().child("defaultTextStyle"),
            table_styles: self.table_styles.as_ref().map(XmlDocument::root),
        };
        let nodes =
            parse_matching_part_nodes(master.part.document.root(), &context, &target.node_id, false)?;
        Ok(crate::find_node(&nodes, &target.node_id).and_then(crate::node_artwork))
    }

    fn render_layout_node(
        &self,
        assets: &BTreeMap<String, PresentationAsset>,
        path: &str,
        target: &crate::ArtworkTarget,
    ) -> Result<Option<crate::NodeArtwork>, ParseError> {
        let Some(layout) = self.layouts.get(path) else {
            return Ok(None);
        };
        let master = related_path(&layout.part.rels, "slideMaster", "ppt/slideMasters/")
            .and_then(|path| self.masters.get(&path));
        let theme = master
            .and_then(|master| related_path(&master.part.rels, "theme", "ppt/theme/"))
            .and_then(|path| self.themes.get(&path));
        let empty_related_parts = HashMap::new();
        let empty_related_relationships = HashMap::new();
        let empty_chart_companions = HashMap::new();
        let diagnostics = ParseDiagnostics::new(None, &layout.part.path);
        let colors = ColorContext {
            theme,
            color_map: &layout.color_map,
            diagnostics: &diagnostics,
            paint_key: paint::ColorKey::new(theme, &layout.color_map),
        };
        let context = NodeParseContext {
            artwork_target: Some(target),
            artwork_part: ArtworkPart::Layout(&layout.part.path),
            artwork_id_match: ArtworkIdMatch::Exact,
            shape_cache: &self.shape_cache,
            editing_cache: &self.editing_cache,
            source_part: Some(&layout.part.path),
            slide_index: 0,
            paint_scale: (1.0, 1.0),
            rels: &layout.part.rels,
            related_parts: &empty_related_parts,
            related_relationships: &empty_related_relationships,
            assets: self.asset_view(assets, layout.generation),
            chart_companions: &empty_chart_companions,
            legacy_chart_defaults: self.legacy_chart_defaults,
            options: &self.options,
            limits: &self.limits,
            colors: Some(&colors),
            diagnostics: &diagnostics,
            layout_root: None,
            master_root: master.map(|master| master.part.document.root()),
            layout_rels: None,
            master_rels: master.map(|master| &master.part.rels),
            presentation_text_style: self.presentation.root().child("defaultTextStyle"),
            table_styles: self.table_styles.as_ref().map(XmlDocument::root),
        };
        let nodes =
            parse_matching_part_nodes(layout.part.document.root(), &context, &target.node_id, false)?;
        Ok(crate::find_node(&nodes, &target.node_id).and_then(crate::node_artwork))
    }
}

/// Renders `node_id` on a slide the way `parse_slide` would have stored it:
/// inherited master nodes come first when the master shapes are shown, then the
/// layout's, then the slide's own, and the first node with that id wins. Only
/// the node lists that can hold the id are parsed, without their prefixes, and
/// background, timing and transition are not needed.
fn render_slide_target(
    root: XmlNode<'_>,
    context: &NodeParseContext<'_, '_>,
    node_id: &str,
) -> Result<Option<crate::NodeArtwork>, ParseError> {
    if let Some(id) = node_id.strip_prefix("master:") {
        if master_shapes_are_visible(root)
            && context
                .layout_root
                .map(master_shapes_are_visible)
                .unwrap_or(true)
        {
            if let (Some(master), Some(master_rels)) = (context.master_root, context.master_rels) {
                let master_context = NodeParseContext {
                    artwork_id_match: ArtworkIdMatch::Prefixed("master"),
                    source_part: context.layout_rels.and_then(|rels| {
                        rels.related("slideMaster", "ppt/slideMasters/")
                            .map(String::as_str)
                    }),
                    rels: master_rels,
                    layout_root: None,
                    master_root: None,
                    layout_rels: None,
                    master_rels: None,
                    ..*context
                };
                let nodes = parse_matching_part_nodes(master, &master_context, id, true)?;
                if let Some(node) = crate::find_node(&nodes, id) {
                    return Ok(crate::node_artwork(node));
                }
            }
        }
    }
    if let Some(id) = node_id.strip_prefix("layout:") {
        if let (Some(layout), Some(layout_rels)) = (context.layout_root, context.layout_rels) {
            let layout_context = NodeParseContext {
                artwork_id_match: ArtworkIdMatch::Prefixed("layout"),
                source_part: context
                    .rels
                    .related("slideLayout", "ppt/slideLayouts/")
                    .map(String::as_str),
                rels: layout_rels,
                layout_root: None,
                master_root: None,
                layout_rels: None,
                master_rels: None,
                ..*context
            };
            let nodes = parse_matching_part_nodes(layout, &layout_context, id, true)?;
            if let Some(node) = crate::find_node(&nodes, id) {
                return Ok(crate::node_artwork(node));
            }
        }
    }
    let nodes = parse_matching_part_nodes(root, context, node_id, false)?;
    Ok(crate::find_node(&nodes, node_id).and_then(crate::node_artwork))
}

/// The shape-tree nodes of a part that can carry `target_id`, parsed as
/// `parse_part_nodes` or `parse_decorative_part_nodes` would parse them. Nodes
/// that cannot carry the id are skipped, as are the controls appended after the
/// shape tree, which never carry artwork.
fn parse_matching_part_nodes(
    root: XmlNode<'_>,
    context: &NodeParseContext<'_, '_>,
    target_id: &str,
    decorative: bool,
) -> Result<Vec<SlideNode>, ParseError> {
    let mut nodes = Vec::new();
    let Some(shape_tree) = shape_tree(root) else {
        return Ok(nodes);
    };
    for (node_index, node) in shape_tree.children().iter().enumerate() {
        if decorative && placeholder_key(node).is_some() {
            continue;
        }
        if !node_can_carry(node, node_index, context.slide_index, target_id) {
            continue;
        }
        if let Some(node) = parse_slide_node(node, node_index, context)? {
            nodes.push(node);
        }
    }
    Ok(nodes)
}

/// Whether a shape-tree node or one of its descendants can have `target_id`.
/// Groups and markup-compatibility wrappers are parsed to find out; every
/// other node takes its id from its non-visual properties or its position, and
/// materialized SmartArt children extend their frame's id with `:smartart:`.
fn node_can_carry(node: XmlNode<'_>, node_index: usize, slide_index: usize, target_id: &str) -> bool {
    match node.name() {
        "grpSp" | "AlternateContent" => true,
        "sp" | "cxnSp" | "pic" | "contentPart" | "model3d" | "graphicFrame" => {
            let (id, _) = node_identity(node, slide_index, node_index);
            id == target_id
                || (node.name() == "graphicFrame"
                    && target_id
                        .strip_prefix(id.as_str())
                        .is_some_and(|rest| rest.starts_with(":smartart:")))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xml(value: &[u8]) -> XmlDocument {
        parse_xml_tree(value.to_vec(), &ParseLimits::default(), "test.xml").unwrap()
    }

    #[test]
    fn drawingml_tint_and_shade_mix_in_linear_light() {
        let tinted =
            xml(br#"<a:srgbClr xmlns:a="a" val="00FF00"><a:tint val="50000"/></a:srgbClr>"#);
        let shaded =
            xml(br#"<a:srgbClr xmlns:a="a" val="00FF00"><a:shade val="50000"/></a:srgbClr>"#);
        assert_eq!(parse_color(tinted.root(), None).unwrap().value, "#BCFFBC");
        assert_eq!(parse_color(shaded.root(), None).unwrap().value, "#00BC00");
    }

    #[test]
    fn image_fill_preserves_blip_opacity() {
        let fill = xml(
            br#"<a:blipFill xmlns:a="a" xmlns:r="r"><a:blip r:embed="rId1"><a:alphaModFix amt="14000"/></a:blip><a:stretch/></a:blipFill>"#,
        );
        let rels = Relationships {
            entries: HashMap::from([(
                "rId1".into(),
                Relationship {
                    target: "ppt/media/image.png".into(),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        assert!(matches!(
            parse_blip_fill(fill.root(), None, &rels),
            Some(FillStyle::Image { opacity: Some(value), .. }) if (value - 0.14).abs() < f64::EPSILON
        ));
    }

    #[test]
    fn background_fill_shapes_do_not_fall_through_to_theme_styles() {
        let shape = xml(br#"<p:sp xmlns:p="p" useBgFill="1"><p:spPr/></p:sp>"#);
        assert!(matches!(
            shape_fill(shape.root(), None, &Relationships::default()),
            Some(Some(FillStyle::None))
        ));
    }

    #[test]
    fn custom_geometry_is_normalized_as_svg_path_data() {
        let shape = xml(
            br#"<p:sp xmlns:p="p" xmlns:a="a"><p:spPr><a:custGeom><a:pathLst><a:path w="200" h="100"><a:moveTo><a:pt x="0" y="0"/></a:moveTo><a:lnTo><a:pt x="200" y="100"/></a:lnTo><a:close/></a:path></a:pathLst></a:custGeom></p:spPr></p:sp>"#,
        );
        assert_eq!(
            custom_geometry_path(shape.root()).as_deref(),
            Some("M 0 0 L 1 1 Z")
        );
    }
}

#[cfg(test)]
mod fixed_number_tests {
    use super::compact_path_number;

    fn reference(value: f64) -> String {
        let mut rendered = format!("{value:.8}");
        while rendered.contains('.') && rendered.ends_with('0') {
            rendered.pop();
        }
        if rendered.ends_with('.') {
            rendered.pop();
        }
        if rendered == "-0" {
            "0".into()
        } else {
            rendered
        }
    }

    #[test]
    fn fixed_eight_matches_the_standard_formatter() {
        let mut values = Vec::new();
        for shift in 0..=24u32 {
            for numerator in 0..=4096u64 {
                let value = numerator as f64 / (1u64 << shift) as f64;
                values.push(value);
                values.push(-value);
            }
        }
        for width in 1..=512u64 {
            for x in 0..=width {
                values.push(x as f64 / width as f64);
            }
        }
        for width in [914400.0, 12192000.0, 6858000.0, 1234567.0, 3.0, 7.0, 11.0, 13.0] {
            for x in 0..5000 {
                values.push(x as f64 * 37.0 / width);
                values.push(-(x as f64) * 37.0 / width);
            }
        }
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        for _ in 0..200_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let raw = f64::from_bits(state);
            if raw.is_finite() {
                values.push(raw);
            }
            let unit = (state >> 11) as f64 / (1u64 << 53) as f64;
            values.push(unit * 2.0 - 1.0);
            values.push(unit * 1e6);
            values.push(unit * 1e-7);
            values.push((unit * 1e9).round() / 1e8);
        }
        values.extend([
            0.0,
            -0.0,
            f64::MIN_POSITIVE,
            1e-300,
            -1e-300,
            1e15,
            -1e15,
            1e18,
            1e19,
            9.007199254740992e15,
            0.001953125,
            -0.001953125,
            2.5e-9,
            1.5e-8,
            5e-9,
            -5e-9,
            0.125,
            123456789.123456789,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ]);
        for value in values {
            assert_eq!(compact_path_number(value), reference(value), "{value:e}");
        }
    }
}

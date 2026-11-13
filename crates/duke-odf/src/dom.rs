//! A small read-only XML tree for OpenDocument parts.
//!
//! OpenDocument uses a fixed set of namespaces, so each name is resolved to a
//! [`Ns`] value while the tree is built and every later comparison is an
//! integer compare. Names and unescaped text borrow from the input. The tree
//! is built by one scan of the input, with no separate tokenizer in front.

use std::borrow::Cow;

/// The namespaces the readers distinguish.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ns {
    Office,
    Style,
    Text,
    Table,
    Draw,
    Fo,
    Svg,
    Presentation,
    Meta,
    Dr3d,
    DrawExt,
    Chart,
    ChartExt,
    LibreOfficeExt,
    Number,
    Xlink,
    Dc,
    MathMl,
    Xml,
    /// Another OpenDocument namespace, such as forms or data styles.
    OtherOdf,
    /// A namespace from outside OpenDocument.
    Foreign,
}

impl Ns {
    fn classify(uri: &str) -> Self {
        if let Some(name) = uri.strip_prefix("urn:oasis:names:tc:opendocument:xmlns:") {
            return match name {
                "office:1.0" => Self::Office,
                "style:1.0" => Self::Style,
                "text:1.0" => Self::Text,
                "table:1.0" => Self::Table,
                "drawing:1.0" => Self::Draw,
                "xsl-fo-compatible:1.0" => Self::Fo,
                "svg-compatible:1.0" => Self::Svg,
                "presentation:1.0" => Self::Presentation,
                "meta:1.0" => Self::Meta,
                "dr3d:1.0" => Self::Dr3d,
                "chart:1.0" => Self::Chart,
                "datastyle:1.0" => Self::Number,
                _ => Self::OtherOdf,
            };
        }
        match uri {
            "http://www.w3.org/1999/xlink" => Self::Xlink,
            "http://purl.org/dc/elements/1.1/" => Self::Dc,
            "http://www.w3.org/1998/Math/MathML" => Self::MathMl,
            "http://www.w3.org/XML/1998/namespace" => Self::Xml,
            "http://openoffice.org/2010/draw" => Self::DrawExt,
            "http://openoffice.org/2010/chart" => Self::ChartExt,
            "urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0" => {
                Self::LibreOfficeExt
            }
            uri if uri.starts_with("urn:oasis:names:tc:opendocument:") => Self::OtherOdf,
            _ => Self::Foreign,
        }
    }

    /// An element from outside OpenDocument, which readers treat as if only
    /// its content were there.
    pub(crate) fn is_foreign(self) -> bool {
        matches!(
            self,
            Self::Foreign
                | Self::Xlink
                | Self::Dc
                | Self::Xml
                | Self::ChartExt
                | Self::LibreOfficeExt
        )
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DomError {
    /// More nodes than the caller allows.
    Limit,
    Malformed(String),
}

pub(crate) struct Attribute<'i> {
    pub(crate) namespace: Option<Ns>,
    pub(crate) name: &'i str,
    pub(crate) value: Cow<'i, str>,
}

struct NodeData<'i> {
    /// Empty for a text node.
    name: &'i str,
    namespace: Option<Ns>,
    text: Cow<'i, str>,
    attributes: (u32, u32),
    parent: u32,
    first_child: u32,
    next_sibling: u32,
    /// Byte range of an element in the input, tags included.
    source: (u32, u32),
    is_text: bool,
}

pub(crate) struct Document<'i> {
    input: &'i str,
    /// Index 0 is the document itself; real nodes follow.
    nodes: Vec<NodeData<'i>>,
    attributes: Vec<Attribute<'i>>,
    root: u32,
}

#[derive(Clone, Copy)]
pub(crate) struct Node<'a, 'i> {
    document: &'a Document<'i>,
    id: u32,
}

#[derive(Clone, Copy)]
pub(crate) struct TagName<'a> {
    name: &'a str,
    namespace: Option<Ns>,
}

impl<'a> TagName<'a> {
    pub(crate) fn name(self) -> &'a str {
        self.name
    }

    pub(crate) fn namespace(self) -> Option<Ns> {
        self.namespace
    }
}

impl<'a, 'i> Node<'a, 'i> {
    fn data(self) -> &'a NodeData<'i> {
        &self.document.nodes[self.id as usize]
    }

    pub(crate) fn is_element(self) -> bool {
        !self.data().is_text
    }

    pub(crate) fn is_text(self) -> bool {
        self.data().is_text
    }

    pub(crate) fn tag_name(self) -> TagName<'a> {
        let data = self.data();
        TagName {
            name: data.name,
            namespace: data.namespace,
        }
    }

    /// The text of a text node.
    pub(crate) fn text(self) -> Option<&'a str> {
        let data = self.data();
        data.is_text.then(|| data.text.as_ref())
    }

    pub(crate) fn attributes(self) -> &'a [Attribute<'i>] {
        let (start, end) = self.data().attributes;
        &self.document.attributes[start as usize..end as usize]
    }

    pub(crate) fn attribute(self, namespace: Ns, name: &str) -> Option<&'a str> {
        self.attributes()
            .iter()
            .find(|attribute| attribute.namespace == Some(namespace) && attribute.name == name)
            .map(|attribute| attribute.value.as_ref())
    }

    pub(crate) fn children(self) -> Children<'a, 'i> {
        Children {
            document: self.document,
            next: self.data().first_child,
        }
    }

    /// Every node below this one, in document order.
    pub(crate) fn descendants(self) -> Descendants<'a, 'i> {
        Descendants {
            document: self.document,
            top: self.id,
            next: self.data().first_child,
        }
    }

    /// The element's XML as written, tags included.
    pub(crate) fn source(self) -> &'i str {
        let (start, end) = self.data().source;
        &self.document.input[start as usize..end as usize]
    }

    /// Retains inherited namespace bindings when an embedded document is detached.
    pub(crate) fn standalone_source(self) -> String {
        let mut bindings = std::collections::BTreeSet::new();
        let mut inherited = String::new();
        let mut current = self;
        loop {
            let mut reader = quick_xml::Reader::from_str(current.source());
            if let Ok(quick_xml::events::Event::Start(start) | quick_xml::events::Event::Empty(start)) = reader.read_event() {
                for attribute in start.attributes().flatten() {
                    let key = attribute.key.as_ref();
                    if (key == b"xmlns" || key.starts_with(b"xmlns:")) && bindings.insert(key.to_vec()) && current.id != self.id {
                        if let (Ok(key), Ok(value)) = (std::str::from_utf8(key), std::str::from_utf8(&attribute.value)) {
                            inherited.push(' ');
                            inherited.push_str(key);
                            inherited.push_str("=\"");
                            inherited.push_str(&value.replace('"', "&quot;"));
                            inherited.push('"');
                        }
                    }
                }
            }
            let parent = current.data().parent;
            if parent == 0 { break; }
            current = Node { document: self.document, id: parent };
        }
        let source = self.source();
        let end = source.find(|ch: char| ch.is_ascii_whitespace() || ch == '>' || ch == '/').unwrap_or(source.len());
        format!("{}{}{}", &source[..end], inherited, &source[end..])
    }
}

pub(crate) struct Children<'a, 'i> {
    document: &'a Document<'i>,
    next: u32,
}

impl<'a, 'i> Iterator for Children<'a, 'i> {
    type Item = Node<'a, 'i>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next == 0 {
            return None;
        }
        let node = Node {
            document: self.document,
            id: self.next,
        };
        self.next = node.data().next_sibling;
        Some(node)
    }
}

pub(crate) struct Descendants<'a, 'i> {
    document: &'a Document<'i>,
    top: u32,
    next: u32,
}

impl<'a, 'i> Iterator for Descendants<'a, 'i> {
    type Item = Node<'a, 'i>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next == 0 {
            return None;
        }
        let node = Node {
            document: self.document,
            id: self.next,
        };
        let data = node.data();
        self.next = if data.first_child != 0 {
            data.first_child
        } else {
            // Climb until a sibling exists or the starting node is reached.
            let mut current = self.next;
            loop {
                let data = &self.document.nodes[current as usize];
                if data.next_sibling != 0 {
                    break data.next_sibling;
                }
                current = data.parent;
                if current == self.top || current == 0 {
                    break 0;
                }
            }
        };
        Some(node)
    }
}

/// Namespace prefixes in scope, with a small cache in front of the linear search.
struct Scopes<'i> {
    bindings: Vec<(&'i str, Option<Ns>)>,
    cache: [(&'i str, Option<Ns>, u32); 32],
    generation: u32,
}

impl<'i> Scopes<'i> {
    fn new() -> Self {
        Self {
            bindings: Vec::new(),
            cache: [("", None, 0); 32],
            generation: 1,
        }
    }

    fn declare(&mut self, prefix: &'i str, uri: &str) {
        let namespace = (!uri.is_empty()).then(|| Ns::classify(uri));
        self.bindings.push((prefix, namespace));
        self.generation += 1;
    }

    fn truncate(&mut self, length: usize) {
        if self.bindings.len() != length {
            self.bindings.truncate(length);
            self.generation += 1;
        }
    }

    /// The namespace bound to a prefix; the empty prefix is the default namespace.
    fn resolve(&mut self, prefix: &'i str) -> Option<Ns> {
        let bytes = prefix.as_bytes();
        let slot = match (bytes.first(), bytes.last()) {
            (Some(first), Some(last)) => {
                (bytes.len() ^ ((*first as usize) << 1) ^ ((*last as usize) << 3)) & 31
            }
            _ => 0,
        };
        let cached = self.cache[slot];
        if cached.2 == self.generation && cached.0 == prefix {
            return cached.1;
        }
        let namespace = match self.bindings.iter().rev().find(|(bound, _)| *bound == prefix) {
            Some((_, namespace)) => *namespace,
            None if prefix == "xml" => Some(Ns::Xml),
            // An undeclared prefix: keep the content reachable instead of failing.
            None if prefix.is_empty() => None,
            None => Some(Ns::Foreign),
        };
        self.cache[slot] = (prefix, namespace, self.generation);
        namespace
    }
}

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

fn unescape(raw: &str) -> Result<Cow<'_, str>, DomError> {
    if memchr::memchr(b'&', raw.as_bytes()).is_some() {
        quick_xml::escape::unescape(raw).map_err(|error| DomError::Malformed(error.to_string()))
    } else {
        Ok(Cow::Borrowed(raw))
    }
}

fn malformed(message: &str) -> DomError {
    DomError::Malformed(message.to_owned())
}

impl<'i> Document<'i> {
    /// Builds the tree in one pass over the input. A document type
    /// declaration is refused: nothing in OpenDocument needs one and it is
    /// the door to entity expansion.
    pub(crate) fn parse(input: &'i str, max_nodes: usize) -> Result<Self, DomError> {
        if input.len() > u32::MAX as usize {
            return Err(DomError::Limit);
        }
        let mut document = Self {
            input,
            nodes: Vec::with_capacity(input.len() / 40 + 2),
            attributes: Vec::with_capacity(input.len() / 48),
            root: 0,
        };
        document.nodes.push(NodeData {
            name: "",
            namespace: None,
            text: Cow::Borrowed(""),
            attributes: (0, 0),
            parent: 0,
            first_child: 0,
            next_sibling: 0,
            source: (0, 0),
            is_text: false,
        });
        let bytes = input.as_bytes();
        // Open elements: node, its last child, namespace bindings before it
        // and its name as written, which its end tag must repeat.
        let mut open: Vec<(u32, u32, usize, &'i str)> = vec![(0, 0, 0, "")];
        let mut scopes = Scopes::new();
        let mut position = 0;
        while position < bytes.len() {
            if bytes[position] != b'<' {
                let end = memchr::memchr(b'<', &bytes[position..]).map_or(bytes.len(), |found| position + found);
                document.text(&mut open, position, end, true, max_nodes)?;
                position = end;
                continue;
            }
            let rest = &bytes[position..];
            match rest.get(1) {
                Some(b'/') => {
                    let Some(close) = memchr::memchr(b'>', rest) else {
                        return Err(malformed("an unterminated end tag"));
                    };
                    let name = input[position + 2..position + close].trim_end_matches(|ch| matches!(ch, ' ' | '\t' | '\r' | '\n'));
                    let Some((id, _, bindings, opened)) = open.pop().filter(|_| !open.is_empty()) else {
                        return Err(malformed("an end tag without a start tag"));
                    };
                    if name != opened {
                        return Err(malformed("an end tag that does not match its start tag"));
                    }
                    position += close + 1;
                    document.nodes[id as usize].source.1 = position as u32;
                    scopes.truncate(bindings);
                }
                Some(b'?') => {
                    let Some(close) = memchr::memmem::find(rest, b"?>") else {
                        return Err(malformed("an unterminated processing instruction"));
                    };
                    position += close + 2;
                }
                Some(b'!') if rest.starts_with(b"<!--") => {
                    let Some(close) = memchr::memmem::find(&rest[4..], b"-->") else {
                        return Err(malformed("an unterminated comment"));
                    };
                    position += 4 + close + 3;
                }
                Some(b'!') if rest.starts_with(b"<![CDATA[") => {
                    let Some(close) = memchr::memmem::find(rest, b"]]>") else {
                        return Err(malformed("an unterminated CDATA section"));
                    };
                    document.text(&mut open, position + "<![CDATA[".len(), position + close, false, max_nodes)?;
                    position += close + 3;
                }
                Some(b'!') => return Err(malformed("a document type declaration is not allowed")),
                Some(_) => {
                    if open.len() == 1 && document.root != 0 {
                        return Err(malformed("more than one root element"));
                    }
                    let bindings = scopes.bindings.len();
                    let (id, qualified, after, empty) =
                        document.element(position, &mut scopes, max_nodes)?;
                    document.nodes[id as usize].source = (position as u32, after as u32);
                    document.link(&mut open, id);
                    if open.len() == 1 {
                        document.root = id;
                    }
                    if empty {
                        scopes.truncate(bindings);
                    } else {
                        open.push((id, 0, bindings, qualified));
                    }
                    position = after;
                }
                None => return Err(malformed("an unterminated tag")),
            }
        }
        if open.len() != 1 {
            return Err(malformed("an element is not closed"));
        }
        if document.root == 0 {
            return Err(malformed("no root element"));
        }
        Ok(document)
    }

    /// Adds the text between two input positions to the innermost open
    /// element; `escaped` is false for the content of a CDATA section.
    fn text(
        &mut self,
        open: &mut [(u32, u32, usize, &'i str)],
        start: usize,
        end: usize,
        escaped: bool,
        max_nodes: usize,
    ) -> Result<(), DomError> {
        let input = self.input;
        let raw = &input[start..end];
        let blank = raw.bytes().all(is_space);
        let parent = open[open.len() - 1].0;
        if parent == 0 {
            if blank {
                return Ok(());
            }
            return Err(malformed("text outside the root element"));
        }
        // White space between elements only matters inside text content,
        // where it separates words.
        let significant = match self.nodes[parent as usize].namespace {
            Some(Ns::Text) => true,
            Some(namespace) => namespace.is_foreign(),
            None => true,
        };
        if blank && !significant {
            return Ok(());
        }
        let text = if escaped { unescape(raw)? } else { Cow::Borrowed(raw) };
        let (source_start, source_end) = if escaped {
            (start, end)
        } else {
            (start - "<![CDATA[".len(), end + "]]>".len())
        };
        let id = self.push(
            NodeData {
                name: "",
                namespace: None,
                text,
                attributes: (0, 0),
                parent,
                first_child: 0,
                next_sibling: 0,
                source: (source_start as u32, source_end as u32),
                is_text: true,
            },
            max_nodes,
        )?;
        self.link(open, id);
        Ok(())
    }

    pub(crate) fn root_element(&self) -> Node<'_, 'i> {
        Node {
            document: self,
            id: self.root,
        }
    }

    fn push(&mut self, node: NodeData<'i>, max_nodes: usize) -> Result<u32, DomError> {
        if self.nodes.len() > max_nodes {
            return Err(DomError::Limit);
        }
        self.nodes.push(node);
        Ok(self.nodes.len() as u32 - 1)
    }

    /// Appends `id` to the children of the innermost open element.
    fn link(&mut self, open: &mut [(u32, u32, usize, &'i str)], id: u32) {
        let (parent, last, _, _) = open[open.len() - 1];
        self.nodes[id as usize].parent = parent;
        if last == 0 {
            self.nodes[parent as usize].first_child = id;
        } else {
            self.nodes[last as usize].next_sibling = id;
        }
        let top = open.len() - 1;
        open[top].1 = id;
    }

    /// Reads the start tag at `start`, its name and attributes, and returns
    /// the new node, the name as written, the position after the tag and
    /// whether the element is empty.
    fn element(
        &mut self,
        start: usize,
        scopes: &mut Scopes<'i>,
        max_nodes: usize,
    ) -> Result<(u32, &'i str, usize, bool), DomError> {
        let input = self.input;
        let bytes = input.as_bytes();
        let mut cursor = start + 1;
        while bytes.get(cursor).is_some_and(|byte| !is_space(*byte) && !matches!(*byte, b'>' | b'/')) {
            cursor += 1;
        }
        let qualified = &input[start + 1..cursor];
        if qualified.is_empty() {
            return Err(malformed("a tag without a name"));
        }
        let first = self.attributes.len();
        let empty = loop {
            while bytes.get(cursor).is_some_and(|byte| is_space(*byte)) {
                cursor += 1;
            }
            match bytes.get(cursor) {
                Some(b'>') => {
                    cursor += 1;
                    break false;
                }
                Some(b'/') if bytes.get(cursor + 1) == Some(&b'>') => {
                    cursor += 2;
                    break true;
                }
                Some(_) => {}
                None => return Err(malformed("an unterminated tag")),
            }
            let key_start = cursor;
            while bytes
                .get(cursor)
                .is_some_and(|byte| !is_space(*byte) && !matches!(*byte, b'=' | b'>' | b'/'))
            {
                cursor += 1;
            }
            let key = &input[key_start..cursor];
            while bytes.get(cursor).is_some_and(|byte| is_space(*byte)) {
                cursor += 1;
            }
            if bytes.get(cursor) != Some(&b'=') || key.is_empty() {
                return Err(malformed("an attribute without a value"));
            }
            cursor += 1;
            while bytes.get(cursor).is_some_and(|byte| is_space(*byte)) {
                cursor += 1;
            }
            let quote = match bytes.get(cursor) {
                Some(quote @ (b'"' | b'\'')) => *quote,
                _ => return Err(malformed("an attribute value without quotes")),
            };
            let value_start = cursor + 1;
            let Some(length) = memchr::memchr(quote, &bytes[value_start..]) else {
                return Err(malformed("an unterminated attribute value"));
            };
            cursor = value_start + length + 1;
            let value = &input[value_start..value_start + length];
            match key.strip_prefix("xmlns") {
                Some("") => scopes.declare("", &unescape(value)?),
                Some(prefix) if prefix.starts_with(':') => scopes.declare(&prefix[1..], &unescape(value)?),
                _ => self.attributes.push(Attribute {
                    namespace: None,
                    name: key,
                    value: unescape(value)?,
                }),
            }
        };
        let (namespace, name) = match qualified.split_once(':') {
            Some((prefix, name)) => (scopes.resolve(prefix), name),
            None => (scopes.resolve(""), qualified),
        };
        for attribute in &mut self.attributes[first..] {
            // An attribute without a prefix has no namespace.
            let (namespace, name) = match attribute.name.split_once(':') {
                Some((prefix, name)) => (scopes.resolve(prefix), name),
                None => (None, attribute.name),
            };
            attribute.namespace = namespace;
            attribute.name = name;
        }
        let id = self.push(
            NodeData {
                name,
                namespace,
                text: Cow::Borrowed(""),
                attributes: (first as u32, self.attributes.len() as u32),
                parent: 0,
                first_child: 0,
                next_sibling: 0,
                source: (0, 0),
                is_text: false,
            },
            max_nodes,
        )?;
        Ok((id, qualified, cursor, empty))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(node: Node<'_, '_>) -> Vec<String> {
        node.children()
            .map(|child| match child.text() {
                Some(text) => format!("#{text}"),
                None => format!("{:?}:{}", child.tag_name().namespace(), child.tag_name().name()),
            })
            .collect()
    }

    #[test]
    fn resolves_namespaces_attributes_and_entities() {
        let xml = concat!(
            r#"<?xml version="1.0"?><!-- c --><o:document xmlns:o="urn:oasis:names:tc:opendocument:xmlns:office:1.0" "#,
            r#"xmlns:t="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:x="urn:example" o:version = '1.3'>"#,
            r#"<t:p t:style-name="A &amp; B" plain="1">one <t:span>two</t:span> &lt;3 <![CDATA[<raw>]]></t:p>"#,
            r#"<x:other xmlns:t="urn:example:shadow"><t:p/></x:other><t:p/>"#,
            "</o:document>\n",
        );
        let document = Document::parse(xml, usize::MAX).unwrap();
        let root = document.root_element();
        assert_eq!(root.tag_name().name(), "document");
        assert_eq!(root.tag_name().namespace(), Some(Ns::Office));
        assert_eq!(root.attribute(Ns::Office, "version"), Some("1.3"));
        // Namespace declarations are not attributes.
        assert_eq!(root.attributes().len(), 1);
        assert_eq!(names(root), ["Some(Text):p", "Some(Foreign):other", "Some(Text):p"]);

        let paragraph = root.children().next().unwrap();
        assert_eq!(paragraph.attribute(Ns::Text, "style-name"), Some("A & B"));
        assert_eq!(paragraph.attribute(Ns::Text, "plain"), None);
        assert_eq!(paragraph.attributes()[1].namespace, None);
        assert_eq!(names(paragraph), ["#one ", "Some(Text):span", "# <3 ", "#<raw>"]);
        assert_eq!(paragraph.source(), &xml[xml.find("<t:p t").unwrap()..xml.find("<x:other").unwrap()]);
        let text: String = paragraph.descendants().filter_map(Node::text).collect();
        assert_eq!(text, "one two <3 <raw>");

        // A prefix rebound on an inner element applies there only.
        let other = root.children().nth(1).unwrap();
        assert_eq!(names(other), ["Some(Foreign):p"]);
    }

    #[test]
    fn keeps_white_space_only_where_it_is_content() {
        let xml = concat!(
            r#"<o:body xmlns:o="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:t="urn:oasis:names:tc:opendocument:xmlns:text:1.0">"#,
            "\n  <t:p><t:span>a</t:span> <t:span>b</t:span></t:p>\n  <o:forms> </o:forms>\n</o:body>",
        );
        let document = Document::parse(xml, usize::MAX).unwrap();
        let root = document.root_element();
        assert_eq!(names(root), ["Some(Text):p", "Some(Office):forms"]);
        let paragraph = root.children().next().unwrap();
        assert_eq!(names(paragraph), ["Some(Text):span", "# ", "Some(Text):span"]);
        assert_eq!(root.children().nth(1).unwrap().children().count(), 0);
    }

    #[test]
    fn rejects_malformed_input_and_enforces_the_node_limit() {
        for xml in [
            "<a><b></a>",
            "<a>",
            "<a/><b/>",
            "<a/>junk",
            "",
            "<a b></a>",
            "<a b=c/>",
            "<a>&unknown;</a>",
            r#"<!DOCTYPE a [<!ENTITY e "x">]><a/>"#,
            "<a><b></c></a>",
            "</a>",
            "<a",
            "<a b='1'",
            "<a b='1/>",
            "<a><!-- open</a>",
            "<a><![CDATA[open</a>",
            "<a><?pi open</a>",
            "<a></a",
            "< a/>",
            "<a =\"1\"/>",
            "<a/>text",
            "<",
        ] {
            assert!(
                matches!(Document::parse(xml, usize::MAX), Err(DomError::Malformed(_))),
                "{xml}"
            );
        }
        // Forms the scanner accepts: space before `>`, a `>` inside a value,
        // either quote, comments and instructions anywhere.
        let xml = "<?xml version='1.0'?>\n<a x = \"1>2\" y='\"'  ><!-- c --><b\tz=\"\"/><?pi?></a >\n<!-- end -->\n";
        let document = Document::parse(xml, usize::MAX).unwrap();
        let root = document.root_element();
        assert_eq!(root.attributes()[0].value, "1>2");
        assert_eq!(root.attributes()[1].value, "\"");
        assert_eq!(names(root), ["None:b"]);
        assert_eq!(root.source(), &xml[xml.find("<a").unwrap()..xml.find("\n<!-- end").unwrap()]);
        assert_eq!(Document::parse("<a><b/><b/><b/></a>", 2).err(), Some(DomError::Limit));
        assert!(Document::parse("<a><b/></a>", 2).is_ok());
    }
}

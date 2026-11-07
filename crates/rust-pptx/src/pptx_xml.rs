//! Arena-backed XML tree for one presentation part.
//!
//! A part is parsed into one `XmlDocument`: the part bytes plus flat node,
//! attribute and side-string tables. Names, attribute values, text and
//! retained source XML are byte ranges into the part when the part is valid
//! UTF-8 and contains no entities at that position; everything else lives in
//! the side-string table. `XmlNode` is a `Copy` handle into a document.

use std::fmt;

pub(super) const NONE: u32 = u32::MAX;

#[derive(Clone, Copy, Debug)]
pub(super) enum StrRef {
    None,
    Buffer { start: u32, len: u32 },
    Owned(u32),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RawAttribute {
    pub(super) name: StrRef,
    pub(super) value: StrRef,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RawNode {
    pub(super) name: StrRef,
    pub(super) attributes_start: u32,
    pub(super) attributes_len: u32,
    pub(super) text: StrRef,
    pub(super) first_child: u32,
    pub(super) next_sibling: u32,
    pub(super) subtree_names: u64,
    pub(super) source_xml: StrRef,
}

impl RawNode {
    pub(super) fn new(
        name: StrRef,
        attributes_start: u32,
        attributes_len: u32,
        subtree_names: u64,
    ) -> Self {
        Self {
            name,
            attributes_start,
            attributes_len,
            text: StrRef::None,
            first_child: NONE,
            next_sibling: NONE,
            subtree_names,
            source_xml: StrRef::None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct XmlDocument {
    pub(super) buffer: Vec<u8>,
    pub(super) base: usize,
    pub(super) nodes: Vec<RawNode>,
    pub(super) attributes: Vec<RawAttribute>,
    pub(super) strings: Vec<String>,
    pub(super) root: u32,
}

impl XmlDocument {
    /// The part bytes without a leading byte-order mark.
    pub(super) fn bytes(&self) -> &[u8] {
        &self.buffer[self.base..]
    }

    pub(super) fn root(&self) -> XmlNode<'_> {
        XmlNode {
            document: self,
            index: self.root,
        }
    }

    pub(super) fn node(&self, index: u32) -> XmlNode<'_> {
        XmlNode {
            document: self,
            index,
        }
    }

    pub(super) fn resolve(&self, value: StrRef) -> &str {
        match value {
            StrRef::None => "",
            StrRef::Buffer { start, len } => {
                let bytes = &self.bytes()[start as usize..start as usize + len as usize];
                debug_assert!(std::str::from_utf8(bytes).is_ok());
                // SAFETY: buffer references are created only after the whole part
                // passed UTF-8 validation, and every range starts and ends at an
                // ASCII delimiter, so the slice is valid UTF-8 on char boundaries.
                unsafe { std::str::from_utf8_unchecked(bytes) }
            }
            StrRef::Owned(index) => self.strings[index as usize].as_str(),
        }
    }

    pub(super) fn intern(&mut self, value: String) -> StrRef {
        self.strings.push(value);
        StrRef::Owned(self.strings.len() as u32 - 1)
    }

    /// Builds a document from an owned tree, for synthesized fragments.
    pub(super) fn from_owned(root: OwnedNode) -> Self {
        let mut document = XmlDocument::default();
        document.root = push_owned(&mut document, root);
        document
    }

    pub(super) fn leaf(name: &str) -> Self {
        Self::from_owned(OwnedNode::new(name))
    }
}

fn push_owned(document: &mut XmlDocument, node: OwnedNode) -> u32 {
    let OwnedNode {
        name,
        attributes,
        text,
        children,
        source_xml,
    } = node;
    let mut subtree_names = XmlNode::name_bit(name.as_bytes());
    let name = document.intern(name);
    let attributes_start = document.attributes.len() as u32;
    for (key, value) in attributes {
        let name = document.intern(key);
        let value = document.intern(value);
        document.attributes.push(RawAttribute { name, value });
    }
    let attributes_len = document.attributes.len() as u32 - attributes_start;
    let index = document.nodes.len() as u32;
    let mut raw = RawNode::new(name, attributes_start, attributes_len, subtree_names);
    if !text.is_empty() {
        raw.text = document.intern(text);
    }
    if let Some(source) = source_xml {
        raw.source_xml = document.intern(source);
    }
    document.nodes.push(raw);
    let mut last = NONE;
    for child in children {
        let child_index = push_owned(document, child);
        subtree_names |= document.nodes[child_index as usize].subtree_names;
        if last == NONE {
            document.nodes[index as usize].first_child = child_index;
        } else {
            document.nodes[last as usize].next_sibling = child_index;
        }
        last = child_index;
    }
    document.nodes[index as usize].subtree_names = subtree_names;
    index
}

/// An owned tree used to synthesize documents from other representations.
#[derive(Clone, Debug, Default)]
pub(super) struct OwnedNode {
    pub(super) name: String,
    pub(super) attributes: Vec<(String, String)>,
    pub(super) text: String,
    pub(super) children: Vec<OwnedNode>,
    pub(super) source_xml: Option<String>,
}

impl OwnedNode {
    pub(super) fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            ..Self::default()
        }
    }
}

/// A node handle. Copying it is free; it borrows its document.
#[derive(Clone, Copy)]
pub(super) struct XmlNode<'t> {
    document: &'t XmlDocument,
    index: u32,
}

impl fmt::Debug for XmlNode<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("XmlNode")
            .field("name", &self.name())
            .field("attributes", &self.attributes().collect::<Vec<_>>())
            .field("text", &self.text())
            .field("children", &self.children().collect::<Vec<_>>())
            .finish()
    }
}

impl<'t> XmlNode<'t> {
    pub(super) fn name_bit(name: &[u8]) -> u64 {
        let hash = name.iter().fold(2166136261u32, |hash, byte| {
            (hash ^ u32::from(*byte)).wrapping_mul(16777619)
        });
        1 << (hash % 64)
    }

    fn raw(self) -> &'t RawNode {
        &self.document.nodes[self.index as usize]
    }

    pub(super) fn index(self) -> u32 {
        self.index
    }

    pub(super) fn name(self) -> &'t str {
        self.document.resolve(self.raw().name)
    }

    pub(super) fn text(self) -> &'t str {
        self.document.resolve(self.raw().text)
    }

    pub(super) fn source_xml(self) -> Option<&'t str> {
        match self.raw().source_xml {
            StrRef::None => None,
            value => Some(self.document.resolve(value)),
        }
    }

    pub(super) fn subtree_names(self) -> u64 {
        self.raw().subtree_names
    }

    pub(super) fn attributes(self) -> Attributes<'t> {
        let raw = self.raw();
        Attributes {
            document: self.document,
            next: raw.attributes_start,
            end: raw.attributes_start + raw.attributes_len,
        }
    }

    pub(super) fn attr(self, name: &str) -> Option<&'t str> {
        self.attributes()
            .find_map(|(key, value)| (key == name).then_some(value))
    }

    pub(super) fn children(self) -> Children<'t> {
        Children {
            document: self.document,
            next: self.raw().first_child,
        }
    }

    pub(super) fn child(self, name: &str) -> Option<XmlNode<'t>> {
        self.children().find(|child| child.name() == name)
    }

    pub(super) fn children_named<'n>(self, name: &'n str) -> ChildrenNamed<'t, 'n> {
        ChildrenNamed {
            children: self.children(),
            name,
        }
    }

    pub(super) fn descendant(self, name: &str) -> Option<XmlNode<'t>> {
        self.descendant_with_bit(name, Self::name_bit(name.as_bytes()))
    }

    fn descendant_with_bit(self, name: &str, bit: u64) -> Option<XmlNode<'t>> {
        if self.subtree_names() & bit == 0 {
            return None;
        }
        if self.name() == name {
            return Some(self);
        }
        self.children()
            .find_map(|child| child.descendant_with_bit(name, bit))
    }

    pub(super) fn collect_descendants(self, name: &str, output: &mut Vec<XmlNode<'t>>) {
        if self.name() == name {
            output.push(self);
        }
        for child in self.children() {
            child.collect_descendants(name, output);
        }
    }

    pub(super) fn text_content(self) -> String {
        let mut value = self.text().to_owned();
        for child in self.children() {
            value.push_str(&child.text_content());
        }
        value
    }
}

#[derive(Clone, Copy)]
pub(super) struct Attributes<'t> {
    document: &'t XmlDocument,
    next: u32,
    end: u32,
}

impl<'t> Iterator for Attributes<'t> {
    type Item = (&'t str, &'t str);

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.end {
            return None;
        }
        let attribute = self.document.attributes[self.next as usize];
        self.next += 1;
        Some((
            self.document.resolve(attribute.name),
            self.document.resolve(attribute.value),
        ))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.end - self.next) as usize;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for Attributes<'_> {}

impl<'t> Attributes<'t> {
    pub(super) fn is_empty(self) -> bool {
        self.next >= self.end
    }

    pub(super) fn iter(self) -> Self {
        self
    }
}

#[derive(Clone, Copy)]
pub(super) struct Children<'t> {
    document: &'t XmlDocument,
    next: u32,
}

impl<'t> Iterator for Children<'t> {
    type Item = XmlNode<'t>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next == NONE {
            return None;
        }
        let node = XmlNode {
            document: self.document,
            index: self.next,
        };
        self.next = self.document.nodes[self.next as usize].next_sibling;
        Some(node)
    }
}

impl<'t> Children<'t> {
    pub(super) fn is_empty(self) -> bool {
        self.next == NONE
    }

    pub(super) fn iter(self) -> Self {
        self
    }
}

#[derive(Clone, Copy)]
pub(super) struct ChildrenNamed<'t, 'n> {
    children: Children<'t>,
    name: &'n str,
}

impl<'t> Iterator for ChildrenNamed<'t, '_> {
    type Item = XmlNode<'t>;

    fn next(&mut self) -> Option<Self::Item> {
        let name = self.name;
        self.children.find(|child| child.name() == name)
    }
}

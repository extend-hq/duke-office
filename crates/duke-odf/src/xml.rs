//! OpenDocument namespaces and small helpers over the XML tree.

pub(crate) use crate::dom::{Node, Ns};

pub(crate) const OFFICE: Ns = Ns::Office;
pub(crate) const STYLE: Ns = Ns::Style;
pub(crate) const TEXT: Ns = Ns::Text;
pub(crate) const TABLE: Ns = Ns::Table;
pub(crate) const DRAW: Ns = Ns::Draw;
pub(crate) const FO: Ns = Ns::Fo;
pub(crate) const SVG: Ns = Ns::Svg;
pub(crate) const PRESENTATION: Ns = Ns::Presentation;
pub(crate) const META: Ns = Ns::Meta;
pub(crate) const DR3D: Ns = Ns::Dr3d;
pub(crate) const CHART: Ns = Ns::Chart;
pub(crate) const CHART_EXT: Ns = Ns::ChartExt;
pub(crate) const LO_EXT: Ns = Ns::LibreOfficeExt;
pub(crate) const NUMBER: Ns = Ns::Number;
pub(crate) const XLINK: Ns = Ns::Xlink;
pub(crate) const DC: Ns = Ns::Dc;
pub(crate) const MATHML: Ns = Ns::MathMl;
pub(crate) const XML: Ns = Ns::Xml;

pub(crate) fn is(node: Node<'_, '_>, namespace: Ns, name: &str) -> bool {
    let tag = node.tag_name();
    tag.namespace() == Some(namespace) && tag.name() == name && node.is_element()
}

pub(crate) fn attr<'a>(node: Node<'a, '_>, namespace: Ns, name: &str) -> Option<&'a str> {
    node.attribute(namespace, name)
}

pub(crate) fn owned(node: Node<'_, '_>, namespace: Ns, name: &str) -> Option<String> {
    attr(node, namespace, name)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(crate) fn child<'a, 'i>(node: Node<'a, 'i>, namespace: Ns, name: &str) -> Option<Node<'a, 'i>> {
    node.children().find(|child| is(*child, namespace, name))
}

pub(crate) fn elements<'a, 'i>(node: Node<'a, 'i>) -> impl Iterator<Item = Node<'a, 'i>> {
    node.children().filter(|child| child.is_element())
}

/// All descendant text, in document order.
pub(crate) fn text_content(node: Node<'_, '_>) -> String {
    node.descendants().filter_map(Node::text).collect()
}

/// Decodes an OpenDocument style name: `Heading_20_1` is `Heading 1`.
pub fn decode_style_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut rest = name;
    while let Some(start) = rest.find('_') {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let decoded = tail.find('_').and_then(|end| {
            let hex = &tail[..end];
            (!hex.is_empty() && hex.len() <= 6)
                .then(|| u32::from_str_radix(hex, 16).ok().and_then(char::from_u32))
                .flatten()
                .map(|ch| (ch, end))
        });
        match decoded {
            Some((ch, end)) => {
                out.push(ch);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('_');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::decode_style_name;

    #[test]
    fn decodes_escaped_style_names() {
        assert_eq!(decode_style_name("Heading_20_1"), "Heading 1");
        assert_eq!(decode_style_name("Text_20_body"), "Text body");
        assert_eq!(decode_style_name("snake_case_name"), "snake_case_name");
        assert_eq!(decode_style_name("Plain"), "Plain");
    }
}

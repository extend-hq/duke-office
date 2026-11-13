use std::collections::{BTreeMap, HashSet};

use roxmltree::{Document, Node, ParsingOptions};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartArtPoint {
    pub model_id: String,
    pub kind: String,
    pub presentation_association: Option<String>,
    pub text_body_xml: Option<String>,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartArtConnection {
    pub model_id: Option<String>,
    pub kind: Option<String>,
    pub source_id: String,
    pub destination_id: String,
    pub source_order: Option<i64>,
    pub destination_order: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartArtData {
    pub points: Vec<SmartArtPoint>,
    pub connections: Vec<SmartArtConnection>,
    pub background_xml: Option<String>,
    pub drawing_relationship_id: Option<String>,
}

impl SmartArtData {
    pub fn semantic_point(&self, model_id: &str) -> Option<&SmartArtPoint> {
        let mut next = model_id;
        let mut visited = HashSet::new();
        while visited.len() < 128 && visited.insert(next) {
            let point = self.points.iter().find(|point| point.model_id == next)?;
            if let Some(association) = point.presentation_association.as_deref() {
                next = association;
            } else {
                return Some(point);
            }
        }
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SmartArtDrawingKind {
    Shape,
    Connector,
    Picture,
    Group,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartArtTextTransform {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    pub rotation: f64,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartArtDrawingNode {
    pub kind: SmartArtDrawingKind,
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_transform: Option<SmartArtTextTransform>,
    /// Canonical standalone XML. Group children are represented separately.
    pub source_xml: String,
    pub children: Vec<SmartArtDrawingNode>,
}

fn text_transform(node: Node<'_, '_>) -> Option<SmartArtTextTransform> {
    let offset = child(node, "off")?;
    let extent = child(node, "ext")?;
    let number = |node, name| attribute(node, name)?.parse::<i64>().ok();
    let rotation = attribute(node, "rot")
        .map(|value| value.parse::<i64>().ok())
        .unwrap_or(Some(0))? as f64 / 60_000.0;
    let flip = |name| match attribute(node, name) {
        None | Some("0" | "false") => Some(false),
        Some("1" | "true") => Some(true),
        _ => None,
    };
    let frame = SmartArtTextTransform {
        x: number(offset, "x")?,
        y: number(offset, "y")?,
        width: number(extent, "cx")?,
        height: number(extent, "cy")?,
        rotation,
        flip_horizontal: flip("flipH")?,
        flip_vertical: flip("flipV")?,
    };
    (frame.width > 0 && frame.height > 0).then_some(frame)
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartArtDrawing {
    pub group_properties_xml: Option<String>,
    pub nodes: Vec<SmartArtDrawingNode>,
    pub unsupported_elements: Vec<String>,
}

fn bounded_document(xml: &str) -> Option<Document<'_>> {
    if xml.len() > 32 * 1024 * 1024 {
        return None;
    }
    let document = Document::parse_with_options(
        xml,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: 100_000,
        },
    )
    .ok()?;
    if document
        .descendants()
        .any(|node| node.ancestors().take(130).count() > 128)
    {
        return None;
    }
    Some(document)
}

fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == name)
}

fn attribute<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| attribute.name() == name)
        .map(|attribute| attribute.value())
}

fn prefix(namespace: &str) -> Option<&'static str> {
    Some(match namespace {
        "http://schemas.openxmlformats.org/drawingml/2006/main"
        | "http://purl.oclc.org/ooxml/drawingml/main" => "a",
        "http://schemas.openxmlformats.org/presentationml/2006/main"
        | "http://purl.oclc.org/ooxml/presentationml/main" => "p",
        "http://schemas.openxmlformats.org/drawingml/2006/diagram"
        | "http://purl.oclc.org/ooxml/drawingml/diagram" => "dgm",
        "http://schemas.microsoft.com/office/drawing/2008/diagram" => "dsp",
        "http://schemas.microsoft.com/office/drawing/2010/main" => "a14",
        "http://schemas.microsoft.com/office/drawing/2016/SVG/main" => "asvg",
        "http://schemas.microsoft.com/office/word/2010/wordprocessingShape" => "wps",
        "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup" => "wpg",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
        | "http://purl.oclc.org/ooxml/officeDocument/relationships" => "r",
        "http://www.w3.org/XML/1998/namespace" => "xml",
        _ => return None,
    })
}

fn escape(value: &str, output: &mut String) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            _ => output.push(character),
        }
    }
}

fn is_visual(name: &str) -> bool {
    matches!(name, "sp" | "cxnSp" | "pic" | "grpSp")
}

fn canonical_xml(node: Node<'_, '_>, group_shell: bool) -> String {
    let mut namespaces = BTreeMap::<String, String>::new();
    fn collect_namespaces(
        element: Node<'_, '_>,
        group_shell: bool,
        namespaces: &mut BTreeMap<String, String>,
    ) {
        for namespace in element.tag_name().namespace().into_iter().chain(
            element
                .attributes()
                .filter_map(|attribute| attribute.namespace()),
        ) {
            if !namespaces.contains_key(namespace) {
                let mut name = prefix(namespace)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("ns{}", namespaces.len()));
                if namespaces.values().any(|existing| *existing == name) {
                    name = format!("ns{}", namespaces.len());
                }
                namespaces.insert(namespace.to_owned(), name);
            }
        }
        for child in element.children().filter(Node::is_element) {
            if !group_shell || !is_visual(child.tag_name().name()) {
                collect_namespaces(child, false, namespaces);
            }
        }
    }
    collect_namespaces(node, group_shell, &mut namespaces);
    fn qualified(
        name: &str,
        namespace: Option<&str>,
        namespaces: &BTreeMap<String, String>,
    ) -> String {
        namespace
            .and_then(|namespace| namespaces.get(namespace))
            .map(|prefix| format!("{prefix}:{name}"))
            .unwrap_or_else(|| name.to_owned())
    }
    fn write(
        node: Node<'_, '_>,
        namespaces: &BTreeMap<String, String>,
        root: bool,
        group_shell: bool,
        output: &mut String,
    ) {
        if node.is_text() {
            escape(node.text().unwrap_or_default(), output);
            return;
        }
        if !node.is_element() {
            return;
        }
        let tag = qualified(
            node.tag_name().name(),
            node.tag_name().namespace(),
            namespaces,
        );
        output.push('<');
        output.push_str(&tag);
        if root {
            for (namespace, prefix) in namespaces {
                if prefix == "xml" {
                    continue;
                }
                output.push_str(&format!(" xmlns:{prefix}=\""));
                escape(namespace, output);
                output.push('"');
            }
        }
        for attribute in node.attributes() {
            output.push(' ');
            output.push_str(&qualified(
                attribute.name(),
                attribute.namespace(),
                namespaces,
            ));
            output.push_str("=\"");
            escape(attribute.value(), output);
            output.push('"');
        }
        output.push('>');
        for child in node.children() {
            if root && group_shell && child.is_element() && is_visual(child.tag_name().name()) {
                continue;
            }
            write(child, namespaces, false, false, output);
        }
        output.push_str("</");
        output.push_str(&tag);
        output.push('>');
    }
    let mut output = String::new();
    write(node, &namespaces, true, group_shell, &mut output);
    output
}

pub fn parse_smartart_data(xml: &str) -> Option<SmartArtData> {
    let document = bounded_document(xml)?;
    let root = document.root_element();
    let root = if root.tag_name().name() == "dataModel" {
        root
    } else {
        root.descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "dataModel")?
    };
    let mut result = SmartArtData {
        background_xml: child(root, "bg").map(|node| canonical_xml(node, false)),
        drawing_relationship_id: root
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "dataModelExt")
            .and_then(|node| attribute(node, "relId"))
            .map(str::to_owned),
        ..Default::default()
    };
    if let Some(points) = child(root, "ptLst") {
        for point in points
            .children()
            .filter(|node| node.is_element() && node.tag_name().name() == "pt")
        {
            let Some(model_id) = attribute(point, "modelId") else {
                continue;
            };
            let text_body = child(point, "t");
            let text = text_body
                .into_iter()
                .flat_map(|body| body.descendants())
                .filter(|node| node.is_element() && node.tag_name().name() == "p")
                .map(|paragraph| {
                    paragraph
                        .descendants()
                        .filter(|node| node.is_element() && node.tag_name().name() == "t")
                        .filter_map(|node| node.text())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            result.points.push(SmartArtPoint {
                model_id: model_id.to_owned(),
                kind: attribute(point, "type").unwrap_or("node").to_owned(),
                presentation_association: child(point, "prSet")
                    .and_then(|node| attribute(node, "presAssocID"))
                    .map(str::to_owned),
                text_body_xml: text_body.map(|node| canonical_xml(node, false)),
                text,
            });
        }
    }
    if let Some(connections) = child(root, "cxnLst") {
        for connection in connections
            .children()
            .filter(|node| node.is_element() && node.tag_name().name() == "cxn")
        {
            let (Some(source), Some(destination)) = (
                attribute(connection, "srcId"),
                attribute(connection, "destId"),
            ) else {
                continue;
            };
            result.connections.push(SmartArtConnection {
                model_id: attribute(connection, "modelId").map(str::to_owned),
                kind: attribute(connection, "type").map(str::to_owned),
                source_id: source.to_owned(),
                destination_id: destination.to_owned(),
                source_order: attribute(connection, "srcOrd").and_then(|value| value.parse().ok()),
                destination_order: attribute(connection, "destOrd")
                    .and_then(|value| value.parse().ok()),
            });
        }
    }
    Some(result)
}

pub fn parse_smartart_drawing(xml: &str) -> Option<SmartArtDrawing> {
    let document = bounded_document(xml)?;
    let tree = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "spTree")?;
    fn nodes(parent: Node<'_, '_>, unsupported: &mut Vec<String>) -> Vec<SmartArtDrawingNode> {
        let mut output = Vec::new();
        for node in parent.children().filter(Node::is_element) {
            let kind = match node.tag_name().name() {
                "sp" => SmartArtDrawingKind::Shape,
                "cxnSp" => SmartArtDrawingKind::Connector,
                "pic" => SmartArtDrawingKind::Picture,
                "grpSp" => SmartArtDrawingKind::Group,
                "nvGrpSpPr" | "grpSpPr" | "extLst" => continue,
                name => {
                    unsupported.push(name.to_owned());
                    SmartArtDrawingKind::Unknown
                }
            };
            let grouped = kind == SmartArtDrawingKind::Group;
            output.push(SmartArtDrawingNode {
                kind,
                model_id: attribute(node, "modelId").map(str::to_owned),
                text_transform: child(node, "txXfrm").and_then(text_transform),
                source_xml: canonical_xml(node, grouped),
                children: if grouped {
                    nodes(node, unsupported)
                } else {
                    Vec::new()
                },
            });
        }
        output
    }
    let mut result = SmartArtDrawing {
        group_properties_xml: child(tree, "grpSpPr").map(|node| canonical_xml(node, false)),
        ..Default::default()
    };
    result.nodes = nodes(tree, &mut result.unsupported_elements);
    Some(result)
}

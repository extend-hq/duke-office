mod animation_model;
mod legacy;
mod legacy_cfb;
mod model;
mod pptx;

pub use animation_model::*;
pub use model::*;
pub use rust_office_art::ParseOptions;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("input is empty")]
    Empty,
    #[error("input exceeds the configured {0} byte limit")]
    InputLimit(usize),
    #[error("unsupported presentation format")]
    UnsupportedFormat,
    #[error("encrypted presentations are not supported")]
    Encrypted,
    #[error("corrupt presentation: {0}")]
    Corrupt(String),
    #[error("resource limit exceeded: {0}")]
    ResourceLimit(String),
}

#[derive(Debug, Clone)]
pub struct ParseLimits {
    pub max_input_bytes: usize,
    pub max_entries: usize,
    pub max_entry_bytes: usize,
    pub max_total_uncompressed_bytes: usize,
    pub max_xml_depth: usize,
    pub max_xml_nodes: usize,
    pub max_xml_attributes: usize,
    pub max_xml_text_bytes: usize,
    pub max_xml_attribute_bytes: usize,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: usize::MAX,
            max_entries: 20_000,
            max_entry_bytes: usize::MAX,
            max_total_uncompressed_bytes: usize::MAX,
            max_xml_depth: 256,
            max_xml_nodes: 500_000,
            max_xml_attributes: 1_000_000,
            max_xml_text_bytes: 64 * 1024 * 1024,
            max_xml_attribute_bytes: 32 * 1024 * 1024,
        }
    }
}

pub fn detect_format(bytes: &[u8]) -> Result<PresentationFormat, ParseError> {
    if bytes.is_empty() {
        return Err(ParseError::Empty);
    }
    if bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
    {
        return Ok(PresentationFormat::Pptx);
    }
    const OLE_MAGIC: &[u8; 8] = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1";
    if bytes.starts_with(OLE_MAGIC) {
        return Ok(PresentationFormat::Ppt);
    }
    Err(ParseError::UnsupportedFormat)
}

pub fn parse_presentation(
    bytes: &[u8],
    limits: &ParseLimits,
) -> Result<PresentationDocument, ParseError> {
    parse_presentation_with_options(bytes, limits, &ParseOptions::default())
}

pub fn parse_presentation_with_options(
    bytes: &[u8],
    limits: &ParseLimits,
    options: &ParseOptions,
) -> Result<PresentationDocument, ParseError> {
    if bytes.len() > limits.max_input_bytes {
        return Err(ParseError::InputLimit(limits.max_input_bytes));
    }
    match detect_format(bytes)? {
        PresentationFormat::Pptx => pptx::parse(bytes, limits, options),
        PresentationFormat::Ppt => legacy::parse(bytes, limits),
    }
}

/// One node of a presentation, addressed for on-demand artwork rendering.
///
/// Exactly one of `slide_index`, `layout_id` or `master_id` selects the node
/// list; `node_id` is the node's `id` in the parsed model, including the
/// `layout:` or `master:` prefix of inherited decorative nodes.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ArtworkTarget {
    pub slide_index: Option<usize>,
    pub layout_id: Option<String>,
    pub master_id: Option<String>,
    pub node_id: String,
}

/// Generated artwork for one node, as the eager parse would have stored it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeArtwork {
    pub svg: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paint_bounds: Option<rust_office_art::PaintBounds>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_picture: Option<Box<PictureArtwork>>,
}

/// Renders the artwork of one node. The presentation is parsed again with
/// every other node's artwork deferred, so the cost is one parse without
/// rendering plus the one render. The result equals the eager parse's artwork
/// for that node, or `None` when the node has none or does not exist.
pub fn render_node_artwork(
    bytes: &[u8],
    limits: &ParseLimits,
    options: &ParseOptions,
    target: &ArtworkTarget,
) -> Result<Option<NodeArtwork>, ParseError> {
    if bytes.len() > limits.max_input_bytes {
        return Err(ParseError::InputLimit(limits.max_input_bytes));
    }
    let document = match detect_format(bytes)? {
        PresentationFormat::Pptx => pptx::parse_with_artwork_target(bytes, limits, options, Some(target))?,
        PresentationFormat::Ppt => return Ok(None),
    };
    let nodes = if let Some(index) = target.slide_index {
        match document.slides.get(index) {
            Some(slide) => &slide.nodes,
            None => return Ok(None),
        }
    } else if let Some(id) = &target.layout_id {
        match document.layouts.iter().find(|layout| &layout.id == id) {
            Some(layout) => &layout.nodes,
            None => return Ok(None),
        }
    } else if let Some(id) = &target.master_id {
        match document.masters.iter().find(|master| &master.id == id) {
            Some(master) => &master.nodes,
            None => return Ok(None),
        }
    } else {
        return Ok(None);
    };
    Ok(find_node(nodes, &target.node_id).and_then(node_artwork))
}

fn find_node<'a>(nodes: &'a [SlideNode], id: &str) -> Option<&'a SlideNode> {
    for node in nodes {
        let node_id = match node {
            SlideNode::Shape { id, .. }
            | SlideNode::Image { id, .. }
            | SlideNode::Group { id, .. }
            | SlideNode::Table { id, .. }
            | SlideNode::Chart { id, .. }
            | SlideNode::Media { id, .. }
            | SlideNode::Unknown { id, .. } => id,
        };
        if node_id == id {
            return Some(node);
        }
        if let SlideNode::Group { children, .. } = node {
            if let Some(found) = find_node(children, id) {
                return Some(found);
            }
        }
    }
    None
}

fn node_artwork(node: &SlideNode) -> Option<NodeArtwork> {
    match node {
        SlideNode::Shape {
            shape_svg: Some(svg),
            paint_bounds,
            ..
        } => Some(NodeArtwork {
            svg: svg.clone(),
            paint_bounds: paint_bounds.clone(),
            fallback_picture: None,
        }),
        SlideNode::Image {
            picture_svg: Some(svg),
            paint_bounds,
            fallback_picture,
            ..
        } => Some(NodeArtwork {
            svg: svg.clone(),
            paint_bounds: paint_bounds.clone(),
            fallback_picture: fallback_picture.clone(),
        }),
        SlideNode::Chart {
            chart_svg: Some(svg), ..
        } => Some(NodeArtwork {
            svg: svg.clone(),
            paint_bounds: None,
            fallback_picture: None,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_ooxml_and_ole_magic() {
        assert_eq!(
            detect_format(b"PK\x03\x04rest").unwrap(),
            PresentationFormat::Pptx
        );
        assert_eq!(
            detect_format(b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1rest").unwrap(),
            PresentationFormat::Ppt
        );
    }

    #[test]
    fn rejects_unknown_input() {
        assert!(matches!(
            detect_format(b"not powerpoint"),
            Err(ParseError::UnsupportedFormat)
        ));
    }
}

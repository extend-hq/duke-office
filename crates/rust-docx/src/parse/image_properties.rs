use crate::model::{
    ImageAssetReference, ImageCrop, ImagePresentation, ImagePropertyAttribute, ImagePropertyNode,
    ImageTile, ImageTransform,
};
use crate::xml::{decode_xml_entities, extract_balanced_tag_blocks_in_order};

use super::context::{ParseContext, ParsedStyleSheet};

fn attribute<'a>(node: &'a ImagePropertyNode, name: &str) -> Option<&'a str> {
    node.attributes
        .iter()
        .find(|attribute| attribute.name == name)
        .map(|attribute| attribute.value.as_str())
}

fn finite(value: Option<&str>) -> Option<f64> {
    value?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

fn percentage(value: Option<&str>) -> Option<f64> {
    let value = value?.trim();
    match value.strip_suffix('%') {
        Some(percent) => finite(Some(percent)).map(|value| value / 100.0),
        None => finite(Some(value)).map(|value| value / 100_000.0),
    }
}

fn vml_fraction(value: Option<&str>) -> Option<f64> {
    let value = value?.trim();
    match value.strip_suffix('f') {
        Some(fixed) => finite(Some(fixed)).map(|value| value / 65_536.0),
        None if value.ends_with('%') => percentage(Some(value)),
        None => finite(Some(value)),
    }
}

fn boolean(value: Option<&str>) -> Option<bool> {
    match value? {
        "1" | "true" | "t" | "on" => Some(true),
        "0" | "false" | "f" | "off" => Some(false),
        _ => None,
    }
}

fn first<'a>(nodes: &'a [ImagePropertyNode], name: &str) -> Option<&'a ImagePropertyNode> {
    for node in nodes {
        if node.name == name {
            return Some(node);
        }
        if let Some(found) = first(&node.children, name) {
            return Some(found);
        }
    }
    None
}

fn direct<'a>(node: &'a ImagePropertyNode, name: &str) -> Option<&'a ImagePropertyNode> {
    node.children.iter().find(|child| child.name == name)
}

fn property_node(tag: &quick_xml::events::BytesStart<'_>) -> Option<ImagePropertyNode> {
    let attributes = tag
        .attributes()
        .map(|attribute| {
            let attribute = attribute.ok()?;
            Some(ImagePropertyAttribute {
                name: attribute.key.as_ref().to_owned(),
                value: decode_xml_entities(attribute.value.as_ref()),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(ImagePropertyNode {
        name: tag.name().as_ref().to_owned(),
        attributes,
        children: Vec::new(),
        text: None,
    })
}

fn read_properties(xml: &str, remaining: &mut usize) -> Option<ImagePropertyNode> {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(xml);
    let mut stack = Vec::<ImagePropertyNode>::new();
    let mut skipped = 0usize;
    loop {
        match reader.read_event().ok()? {
            Event::Start(tag) => {
                if skipped != 0 || tag.name().as_ref() == "w:txbxContent" {
                    skipped += 1;
                    continue;
                }
                if stack.len() >= 64 || *remaining == 0 {
                    return None;
                }
                *remaining -= 1;
                stack.push(property_node(&tag)?);
            }
            Event::Empty(tag) if skipped == 0 => {
                if *remaining == 0 {
                    return None;
                }
                *remaining -= 1;
                let node = property_node(&tag)?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    return Some(node);
                }
            }
            Event::End(_) => {
                if skipped != 0 {
                    skipped -= 1;
                    continue;
                }
                let node = stack.pop()?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    return Some(node);
                }
            }
            Event::Text(text) if skipped == 0 => {
                let text = decode_xml_entities(text.as_ref());
                if !text.trim().is_empty() {
                    stack
                        .last_mut()?
                        .text
                        .get_or_insert_with(String::new)
                        .push_str(&text);
                }
            }
            Event::CData(text) if skipped == 0 => {
                stack
                    .last_mut()?
                    .text
                    .get_or_insert_with(String::new)
                    .push_str(text.as_ref());
            }
            Event::GeneralRef(reference) if skipped == 0 => {
                let text = decode_xml_entities(&format!("&{};", reference.as_ref()));
                stack
                    .last_mut()?
                    .text
                    .get_or_insert_with(String::new)
                    .push_str(&text);
            }
            Event::Eof => return None,
            _ => {}
        }
    }
}

fn collect_assets(
    node: &ImagePropertyNode,
    context: &ParseContext<'_, impl std::borrow::Borrow<ParsedStyleSheet>>,
    assets: &mut Vec<ImageAssetReference>,
) {
    let local = node.name.rsplit(':').next().unwrap_or(&node.name);
    if matches!(
        local,
        "blip" | "svgBlip" | "imgLayer" | "imagedata" | "fill" | "image"
    ) {
        for (name, kind) in [
            ("r:embed", "embedded"),
            ("r:link", "linked"),
            ("r:id", "relationship"),
            ("o:relid", "relationship"),
            ("r:href", "linked"),
            ("src", "source"),
        ] {
            if let Some(value) = attribute(node, name) {
                let relationship_id = (name != "src").then(|| value.to_owned());
                let target = if name == "src" {
                    Some(value.to_owned())
                } else {
                    context.relationships.get(value).cloned()
                };
                let embedded_asset_available = target
                    .as_deref()
                    .is_some_and(|target| context.binary_assets.contains_key(target));
                assets.push(ImageAssetReference {
                    element: node.name.clone(),
                    kind: kind.to_owned(),
                    relationship_id,
                    target,
                    embedded_asset_available,
                });
            }
        }
    }
    for child in &node.children {
        collect_assets(child, context, assets);
    }
}

fn rectangle(node: &ImagePropertyNode) -> ImageCrop {
    ImageCrop {
        left_fraction: percentage(attribute(node, "l")),
        top_fraction: percentage(attribute(node, "t")),
        right_fraction: percentage(attribute(node, "r")),
        bottom_fraction: percentage(attribute(node, "b")),
    }
}

fn drawing_transform(node: &ImagePropertyNode) -> ImageTransform {
    let offset = direct(node, "a:off");
    let extent = direct(node, "a:ext");
    ImageTransform {
        rotation_degrees: finite(attribute(node, "rot")).map(|value| value / 60_000.0),
        flip_horizontal: boolean(attribute(node, "flipH")),
        flip_vertical: boolean(attribute(node, "flipV")),
        offset_x_emu: offset.and_then(|node| finite(attribute(node, "x"))),
        offset_y_emu: offset.and_then(|node| finite(attribute(node, "y"))),
        width_emu: extent.and_then(|node| finite(attribute(node, "cx"))),
        height_emu: extent.and_then(|node| finite(attribute(node, "cy"))),
    }
}

fn vml_transform(node: &ImagePropertyNode) -> Option<ImageTransform> {
    let style = attribute(node, "style")?;
    let mut transform = ImageTransform::default();
    for (name, value) in style.split(';').filter_map(|entry| entry.split_once(':')) {
        let value = value.trim();
        match name.trim() {
            "rotation" => transform.rotation_degrees = finite(Some(value)),
            "flip" => {
                transform.flip_horizontal = Some(value.contains('x'));
                transform.flip_vertical = Some(value.contains('y'));
            }
            _ => {}
        }
    }
    (transform != ImageTransform::default()).then_some(transform)
}

fn is_plain(node: &ImagePropertyNode) -> bool {
    let attributes_are = |allowed: &[&str]| {
        node.attributes.iter().all(|attribute| {
            attribute.name == "xmlns"
                || attribute.name.starts_with("xmlns:")
                || allowed.contains(&attribute.name.as_str())
        })
    };
    let own_plain = match node.name.as_str() {
        "pic:pic" | "pic:nvPicPr" | "pic:cNvPicPr" | "pic:blipFill" | "pic:spPr" | "a:stretch"
        | "a:avLst" => attributes_are(&[]),
        "pic:cNvPr" => attributes_are(&["id", "name", "descr", "title"]),
        "a:picLocks" => attributes_are(&["noChangeAspect"]),
        "a:blip" => attributes_are(&["r:embed"]),
        "a:xfrm" => attributes_are(&[]),
        "a:off" => attributes_are(&["x", "y"]),
        "a:ext" => attributes_are(&["cx", "cy"]),
        "a:prstGeom" => attributes_are(&["prst"]) && attribute(node, "prst") == Some("rect"),
        "a:fillRect" => attributes_are(&[]),
        "wp:effectExtent" => {
            attributes_are(&["l", "t", "r", "b"])
                && ["l", "t", "r", "b"].iter().all(|name| {
                    attribute(node, name).is_none_or(|value| finite(Some(value)) == Some(0.0))
                })
        }
        _ => false,
    };
    own_plain && node.text.is_none() && node.children.iter().all(is_plain)
}

pub(super) fn parse_image_presentation(
    xml: &str,
    context: &ParseContext<'_, impl std::borrow::Borrow<ParsedStyleSheet>>,
) -> Option<Box<ImagePresentation>> {
    let ranges = extract_balanced_tag_blocks_in_order(
        xml,
        &[
            "pic:pic",
            "wps:spPr",
            "wps:style",
            "wpg:grpSpPr",
            "a:blipFill",
            "a:blip",
            "wp:effectExtent",
            "wp:wrapTight",
            "wp:wrapThrough",
            "wp14:sizeRelH",
            "wp14:sizeRelV",
            "v:group",
            "v:shape",
            "v:rect",
            "v:roundrect",
            "v:oval",
            "v:image",
            "v:imagedata",
        ],
    );
    if ranges.is_empty() {
        return None;
    }
    let mut remaining = 16_384usize;
    let mut properties = Vec::with_capacity(ranges.len());
    for range in ranges {
        let Some(node) = read_properties(&xml[range.start..range.end], &mut remaining) else {
            context.push_warning(
                "Image presentation properties exceed parsing limits or are malformed",
            );
            return None;
        };
        properties.push(node);
    }
    if properties.iter().all(is_plain) {
        return None;
    }

    let mut assets = Vec::new();
    for property in &properties {
        collect_assets(property, context, &mut assets);
    }
    let transform = first(&properties, "a:xfrm")
        .map(drawing_transform)
        .or_else(|| first(&properties, "v:shape").and_then(vml_transform));
    let source_rectangle = first(&properties, "a:srcRect").map(rectangle).or_else(|| {
        let node = first(&properties, "v:imagedata")?;
        let crop = ImageCrop {
            left_fraction: vml_fraction(attribute(node, "cropleft")),
            top_fraction: vml_fraction(attribute(node, "croptop")),
            right_fraction: vml_fraction(attribute(node, "cropright")),
            bottom_fraction: vml_fraction(attribute(node, "cropbottom")),
        };
        [
            crop.left_fraction,
            crop.top_fraction,
            crop.right_fraction,
            crop.bottom_fraction,
        ]
        .iter()
        .any(Option::is_some)
        .then_some(crop)
    });
    let fill_rectangle = first(&properties, "a:fillRect").map(rectangle);
    let tile = first(&properties, "a:tile").map(|node| ImageTile {
        offset_x_emu: finite(attribute(node, "tx")),
        offset_y_emu: finite(attribute(node, "ty")),
        scale_x: percentage(attribute(node, "sx")),
        scale_y: percentage(attribute(node, "sy")),
        alignment: attribute(node, "algn").map(str::to_owned),
        flip: attribute(node, "flip").map(str::to_owned),
    });
    Some(Box::new(ImagePresentation {
        transform_applied: false,
        assets,
        transform,
        source_rectangle,
        fill_rectangle,
        tile,
        properties,
    }))
}

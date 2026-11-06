use std::borrow::Borrow;

use crate::model::{
    ImageAssetReference, ImagePaintBounds, ImagePropertyAttribute, ImagePropertyNode, ImageRunNode,
};
use crate::xml::{extract_balanced_tag_slices, get_attribute};

use super::context::{ParseContext, ParsedStyleSheet};

fn property(
    node: roxmltree::Node<'_, '_>,
    remaining: &mut usize,
    depth: usize,
) -> Option<ImagePropertyNode> {
    if *remaining == 0 || depth > 64 {
        return None;
    }
    *remaining -= 1;
    let name = if node.tag_name().namespace() == Some("http://www.w3.org/2003/InkML") {
        format!("inkml:{}", node.tag_name().name())
    } else {
        node.tag_name().name().to_owned()
    };
    let children = node
        .children()
        .filter(roxmltree::Node::is_element)
        .map(|child| property(child, remaining, depth + 1))
        .collect::<Option<Vec<_>>>()?;
    let text = node
        .children()
        .filter(roxmltree::Node::is_text)
        .filter_map(|child| child.text())
        .collect::<String>();
    Some(ImagePropertyNode {
        name,
        attributes: node
            .attributes()
            .map(|attribute| ImagePropertyAttribute {
                name: if attribute.namespace() == Some("http://www.w3.org/XML/1998/namespace") {
                    format!("xml:{}", attribute.name())
                } else {
                    attribute.name().to_owned()
                },
                value: attribute.value().to_owned(),
            })
            .collect(),
        children,
        text: (!text.trim().is_empty()).then_some(text),
    })
}

pub(super) fn apply_cached_ink_metadata(
    source: &str,
    image: &mut ImageRunNode,
    context: &ParseContext<'_, impl Borrow<ParsedStyleSheet>>,
) {
    let choice = crate::xml::direct_child_tag(source, "mc:Choice").unwrap_or(source);
    let Some(content) = extract_balanced_tag_slices(choice, "w14:contentPart")
        .into_iter()
        .next()
    else {
        return;
    };
    let Some(id) = get_attribute(content, "r:id") else {
        return;
    };
    let target = context.relationships.get(&id);
    let ink = target.and_then(|target| context.parts.get(target));
    let presentation = image.presentation.get_or_insert_with(Default::default);
    presentation.assets.retain(|asset| {
        !(asset.element == "w14:contentPart" && asset.relationship_id.as_deref() == Some(&id))
    });
    presentation.assets.push(ImageAssetReference {
        element: "w14:contentPart".into(),
        kind: "ink".into(),
        relationship_id: Some(id),
        target: target.cloned(),
        embedded_asset_available: ink.is_some(),
    });
    if let Some(ink) = ink {
        let parsed = (ink.content.len() <= 1024 * 1024)
            .then(|| {
                roxmltree::Document::parse_with_options(
                    &ink.content,
                    roxmltree::ParsingOptions {
                        allow_dtd: false,
                        nodes_limit: 16_384,
                    },
                )
            })
            .and_then(Result::ok);
        if let Some(document) = parsed {
            if let Some(root) = property(document.root_element(), &mut 16_384, 0) {
                presentation
                    .properties
                    .retain(|node| node.name != "inkml:ink");
                presentation.properties.push(root);
            }
        } else {
            context.push_warning("Ink source exceeds presentation metadata limits or is malformed; the original part remains preserved");
        }
    }
    image.semantic_kind = Some("ink".into());
    let extent = extract_balanced_tag_slices(choice, "wp:extent")
        .into_iter()
        .next();
    let dimensions = extent
        .and_then(|extent| {
            Some((
                super::util::emu_to_pixels(get_attribute(extent, "cx").as_deref())?,
                super::util::emu_to_pixels(get_attribute(extent, "cy").as_deref())?,
            ))
        })
        .filter(|(width, height)| *width > 0.0 && *height > 0.0);
    let Some((width, height)) = dimensions else {
        return;
    };
    let anchor = super::images::parse_floating_anchor_from_run_xml(choice);
    let old_bounds = image.paint_bounds.as_deref();
    let paint_width = old_bounds
        .map(|bounds| bounds.width_px)
        .or(image.width_px)
        .unwrap_or(width);
    let paint_height = old_bounds
        .map(|bounds| bounds.height_px)
        .or(image.height_px)
        .unwrap_or(height);
    let offset = |horizontal| {
        let original = anchor.as_ref();
        let fallback = image.floating.as_ref();
        let (old, new) = if horizontal {
            (
                fallback.and_then(|value| value.x_px),
                original.and_then(|value| value.x_px),
            )
        } else {
            (
                fallback.and_then(|value| value.y_px),
                original.and_then(|value| value.y_px),
            )
        };
        old.zip(new).map_or(0.0, |(old, new)| old - new)
    };
    let x = old_bounds.map_or(0.0, |bounds| bounds.x_px) + offset(true);
    let y = old_bounds.map_or(0.0, |bounds| bounds.y_px) + offset(false);
    if x != 0.0 || y != 0.0 || paint_width != width || paint_height != height {
        image.paint_bounds = Some(Box::new(ImagePaintBounds {
            x_px: x,
            y_px: y,
            width_px: paint_width,
            height_px: paint_height,
            layout_width_px: width,
            layout_height_px: height,
        }));
    }
    if let Some(rendered) =
        ink.and_then(|ink| rust_office_art::render_ink(&ink.content, width, height))
    {
        image.src = Some(super::util::svg_data_uri(&rendered.svg));
        image.content_type = Some("image/svg+xml".into());
        image.data = None;
        image.part_name = None;
        image.crop = None;
        image.css_filter = None;
        image.css_opacity = None;
        image.paint_bounds = rendered.paint_bounds.map(|bounds| {
            Box::new(ImagePaintBounds {
                x_px: bounds.x_px,
                y_px: bounds.y_px,
                width_px: bounds.width_px,
                height_px: bounds.height_px,
                layout_width_px: bounds.layout_width_px,
                layout_height_px: bounds.layout_height_px,
            })
        });
    }
    image.width_px = Some(width);
    image.height_px = Some(height);
    image.floating = anchor;
    if let Some(properties) = extract_balanced_tag_slices(choice, "wp:docPr")
        .into_iter()
        .next()
    {
        image.alt = get_attribute(properties, "descr")
            .or_else(|| get_attribute(properties, "name"))
            .or(image.alt.take());
    }
}

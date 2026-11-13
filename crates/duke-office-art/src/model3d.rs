use crate::xml::{direct_child_tag, escape_xml_text, extract_balanced_tag_slices};
use crate::{PictureContext, RenderedPicture};
use quick_xml::{events::Event, Reader};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model3DAsset {
    pub part_name: String,
    pub content_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_bytes::ByteBuf>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model3D {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_relationship_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_model_relationship_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<Model3DAsset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<Model3DPreview>,
    pub source_xml: String,
}

pub fn model3d_xml(xml: &str) -> Option<&str> {
    if xml.len() > 4 * 1024 * 1024 || !xml.contains("model3d") {
        return None;
    }
    let mut reader = Reader::from_str(xml);
    loop {
        let start = reader.buffer_position() as usize;
        match reader.read_event().ok()? {
            Event::Start(tag) if tag.local_name().as_ref() == b"model3d" => {
                reader.read_to_end(tag.name()).ok()?;
                let model = &xml[start..reader.buffer_position() as usize];
                return (model.len() <= 256 * 1024).then_some(model);
            }
            Event::Empty(tag) if tag.local_name().as_ref() == b"model3d" => {
                let model = &xml[start..reader.buffer_position() as usize];
                return (model.len() <= 256 * 1024).then_some(model);
            }
            Event::Eof => return None,
            _ => {}
        }
    }
}

fn attribute(xml: &str, name: &[u8]) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let (Event::Start(tag) | Event::Empty(tag)) = reader.read_event().ok()? else {
        return None;
    };
    tag.attributes().filter_map(Result::ok).find_map(|attr| {
        (attr.key.local_name().as_ref() == name)
            .then(|| {
                attr.decode_and_unescape_value(reader.decoder())
                    .ok()
                    .map(|value| value.into_owned())
            })
            .flatten()
    })
}

fn child<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    crate::xml::direct_child_local(xml, name)
}

pub fn parse_model3d(
    xml: &str,
    options: &crate::ParseOptions,
    mut resolve_asset: impl FnMut(&str) -> Option<Model3DAsset>,
) -> Option<Model3D> {
    if !options.include_3d_models {
        return None;
    }
    let model = model3d_xml(xml)?;
    let model_relationship_id = attribute(model, b"embed");
    Some(Model3D {
        asset: model_relationship_id
            .as_deref()
            .and_then(&mut resolve_asset),
        model_relationship_id,
        linked_model_relationship_id: attribute(model, b"link"),
        preview: parse_model3d_preview(model),
        source_xml: model.to_owned(),
    })
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model3DPreview {
    pub model_relationship_id: Option<String>,
    pub raster_relationship_id: String,
    pub renderer: Option<String>,
    pub renderer_version: Option<String>,
}

pub fn parse_model3d_preview(xml: &str) -> Option<Model3DPreview> {
    let model = model3d_xml(xml)?;
    let raster = child(model, "raster")?;
    let blip = child(raster, "blip")?;
    Some(Model3DPreview {
        model_relationship_id: attribute(model, b"embed"),
        raster_relationship_id: attribute(blip, b"embed")?,
        renderer: attribute(raster, b"rName"),
        renderer_version: attribute(raster, b"rVer"),
    })
}

pub fn render_model3d_preview(
    xml: &str,
    source: &str,
    width: f64,
    height: f64,
    context: &impl PictureContext,
) -> Option<RenderedPicture> {
    let preview = parse_model3d_preview(xml)?;
    let model = extract_balanced_tag_slices(xml, "am3d:model3d")
        .into_iter()
        .next()?;
    let properties = direct_child_tag(model, "am3d:spPr")?
        .replacen("am3d:spPr", "pic:spPr", 1)
        .replace("</am3d:spPr>", "</pic:spPr>");
    let picture = format!(
        r#"<pic:pic><pic:blipFill><a:blip r:embed="{}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>{properties}</pic:pic>"#,
        escape_xml_text(&preview.raster_relationship_id),
    );
    crate::render_picture(&picture, source, width, height, context)
}

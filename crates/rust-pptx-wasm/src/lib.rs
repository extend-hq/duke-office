use rust_pptx::{
    detect_format, parse_presentation_with_options as parse_core_presentation, ParseLimits,
};
use serde::Serialize;
use serde_wasm_bindgen::Serializer;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn detect_presentation_format(bytes: &[u8]) -> Result<String, JsValue> {
    detect_format(bytes)
        .map(|format| {
            match format {
                rust_pptx::PresentationFormat::Pptx => "pptx",
                rust_pptx::PresentationFormat::Ppt => "ppt",
            }
            .into()
        })
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

fn parse_options(options: Option<JsValue>) -> Result<rust_pptx::ParseOptions, JsValue> {
    match options.filter(|value| !value.is_null() && !value.is_undefined()) {
        Some(value) => serde_wasm_bindgen::from_value(value)
            .map_err(|error| JsValue::from_str(&format!("Invalid parser options: {error}"))),
        None => Ok(rust_pptx::ParseOptions::default()),
    }
}

/// Renders the artwork of one node of a presentation parsed with `deferArtwork`.
/// `target` is `{ slideIndex | layoutId | masterId, nodeId }`; the result is the
/// node's `{ svg, paintBounds?, fallbackPicture? }`, or `undefined` when it has none.
#[wasm_bindgen]
pub fn render_presentation_node_artwork(
    bytes: &[u8],
    options: Option<JsValue>,
    target: JsValue,
) -> Result<JsValue, JsValue> {
    let options = parse_options(options)?;
    let target: rust_pptx::ArtworkTarget = serde_wasm_bindgen::from_value(target)
        .map_err(|error| JsValue::from_str(&format!("Invalid artwork target: {error}")))?;
    let artwork = rust_pptx::render_node_artwork(bytes, &ParseLimits::default(), &options, &target)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    artwork
        .serialize(&Serializer::new().serialize_maps_as_objects(true))
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn parse_presentation(bytes: &[u8], options: Option<JsValue>) -> Result<JsValue, JsValue> {
    let options = parse_options(options)?;
    let document = parse_core_presentation(bytes, &ParseLimits::default(), &options)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;

    // The TypeScript model exposes keyed collections as `Record<string, T>` rather than
    // `Map<string, T>`. Keep byte buffers as Uint8Array and optional values as undefined,
    // while making every Rust map cross the Wasm boundary as a plain JavaScript object.
    document
        .serialize(&Serializer::new().serialize_maps_as_objects(true))
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

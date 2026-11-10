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

fn to_js(value: &impl Serialize) -> Result<JsValue, JsValue> {
    value
        .serialize(&Serializer::new().serialize_maps_as_objects(true))
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

/// A presentation parsed once with `deferArtwork` and kept for rendering many
/// nodes on demand without parsing the file again.
///
/// `new ParsedPresentation(bytes, options)` parses the presentation; `document()`
/// returns the deferred model, and `render_node_artwork(target)` renders one node
/// from the retained parts with the same result as `render_presentation_node_artwork`.
/// The handle holds the inflated parts and their XML trees in WebAssembly memory
/// until `free()` is called.
#[wasm_bindgen]
pub struct ParsedPresentation {
    inner: rust_pptx::ParsedPresentation,
}

#[wasm_bindgen]
impl ParsedPresentation {
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8], options: Option<JsValue>) -> Result<ParsedPresentation, JsValue> {
        let options = parse_options(options)?;
        let inner =
            rust_pptx::parse_presentation_retained(bytes, &ParseLimits::default(), &options)
                .map_err(|error| JsValue::from_str(&error.to_string()))?;
        Ok(Self { inner })
    }

    /// The deferred model, as `parse_presentation(bytes, { ...options, deferArtwork: true })`
    /// returns it.
    pub fn document(&self) -> Result<JsValue, JsValue> {
        to_js(self.inner.document())
    }

    /// Renders one node's `{ svg, paintBounds?, fallbackPicture? }`, or `undefined`
    /// when the node has none or does not exist. `target` is
    /// `{ slideIndex | layoutId | masterId, nodeId }`.
    pub fn render_node_artwork(&self, target: JsValue) -> Result<JsValue, JsValue> {
        let target: rust_pptx::ArtworkTarget = serde_wasm_bindgen::from_value(target)
            .map_err(|error| JsValue::from_str(&format!("Invalid artwork target: {error}")))?;
        let artwork = self
            .inner
            .render_node_artwork(&target)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        to_js(&artwork)
    }
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

#[wasm_bindgen]
pub fn extract_presentation_blocks(
    document: JsValue,
    options: Option<JsValue>,
) -> Result<JsValue, JsValue> {
    let document: rust_pptx::PresentationBlockInput = serde_wasm_bindgen::from_value(document)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let options = match options.filter(|v| !v.is_null() && !v.is_undefined()) {
        Some(value) => serde_wasm_bindgen::from_value(value)
            .map_err(|error| JsValue::from_str(&error.to_string()))?,
        None => rust_pptx::ExtractPresentationBlocksOptions::default(),
    };
    let structured = rust_pptx::extract_presentation_block_input(&document, &options)
        .map_err(|error| JsValue::from_str(&error))?;
    to_js(&structured)
}

#[wasm_bindgen]
pub fn parse_presentation_bundle_wasm(bytes: &[u8], options_json: &str) -> Result<String, JsValue> {
    let options = serde_json::from_str(options_json)
        .map_err(|error| JsValue::from_str(&format!("Invalid bundle options: {error}")))?;
    let bundle = rust_pptx::parse_presentation_bundle(bytes, options)
        .map_err(|error| JsValue::from_str(&error))?;
    serde_json::to_string(&bundle).map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn export_presentation_wasm(request_json: &str) -> Result<String, JsValue> {
    let request = serde_json::from_str(request_json)
        .map_err(|error| JsValue::from_str(&format!("Invalid export request: {error}")))?;
    let result =
        rust_pptx::export_presentation(&request).map_err(|error| JsValue::from_str(&error))?;
    serde_json::to_string(&result).map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn extract_presentation_bundle(
    document: JsValue,
    options: Option<JsValue>,
) -> Result<JsValue, JsValue> {
    let document = serde_wasm_bindgen::from_value(document)
        .map_err(|error| JsValue::from_str(&format!("Invalid presentation: {error}")))?;
    let options = match options.filter(|value| !value.is_null() && !value.is_undefined()) {
        Some(value) => serde_wasm_bindgen::from_value(value)
            .map_err(|error| JsValue::from_str(&format!("Invalid bundle options: {error}")))?,
        None => rust_pptx::ExtractPresentationBundleOptions::default(),
    };
    let bundle = rust_pptx::extract_presentation_bundle(&document, options)
        .map_err(|error| JsValue::from_str(&error))?;
    bundle
        .serialize(&Serializer::json_compatible())
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn parse_presentation_with_limits(
    bytes: &[u8],
    options: Option<JsValue>,
    limits: JsValue,
) -> Result<JsValue, JsValue> {
    let options = parse_options(options)?;
    let limits = serde_wasm_bindgen::from_value(limits)
        .map_err(|error| JsValue::from_str(&format!("Invalid parser limits: {error}")))?;
    let document = parse_core_presentation(bytes, &limits, &options)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    to_js(&document)
}

#[wasm_bindgen]
pub fn extract_presentation_blocks_with_limits(
    document: JsValue,
    options: Option<JsValue>,
    limits: JsValue,
) -> Result<JsValue, JsValue> {
    let document = serde_wasm_bindgen::from_value(document)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let options = match options.filter(|value| !value.is_null() && !value.is_undefined()) {
        Some(value) => serde_wasm_bindgen::from_value(value)
            .map_err(|error| JsValue::from_str(&error.to_string()))?,
        None => rust_pptx::ExtractPresentationBlocksOptions::default(),
    };
    let limits = serde_wasm_bindgen::from_value(limits)
        .map_err(|error| JsValue::from_str(&format!("Invalid extraction limits: {error}")))?;
    let structured =
        rust_pptx::extract_presentation_block_input_with_limits(&document, &options, &limits)
            .map_err(|error| JsValue::from_str(&error))?;
    to_js(&structured)
}

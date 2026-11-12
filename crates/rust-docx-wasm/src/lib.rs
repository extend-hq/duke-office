use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rust_docx::{
    build_doc_model_with_options, model_to_document_xml, package_to_bytes, parse_document_bytes,
    try_serialize_doc_model, DocModel, ImageRunNode, OoxmlPackage, OoxmlPart,
};
use js_sys::{Array, JsString, Object, Reflect, Uint8Array};
use rust_docx::transport::{
    with_transport_payloads, TransportPayloads, BYTES_TOKEN_PREFIX, TEXT_TOKEN_PREFIX,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

mod model_transport;
mod split_json;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now() -> f64;
}

fn doc_model_from_json(json_str: &str) -> Result<DocModel, JsValue> {
    serde_json::from_str(json_str)
        .map_err(|error| JsValue::from_str(&format!("Invalid DocModel JSON: {error}")))
}

#[wasm_bindgen]
pub fn extract_document_review_wasm(model_json: &str) -> Result<String, JsValue> {
    let model = doc_model_from_json(model_json)?;
    serde_json::to_string(&rust_docx::extract_document_review(&model))
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn extract_document_blocks_with_revisions_wasm(model_json: &str) -> Result<String, JsValue> {
    let model = doc_model_from_json(model_json)?;
    serde_json::to_string(&rust_docx::extract_document_blocks_with_revisions(&model, Default::default()))
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn extract_document_blocks_wasm(model_json: &str) -> Result<String, JsValue> {
    let model = doc_model_from_json(model_json)?;
    serde_json::to_string(&rust_docx::extract_document_blocks(&model))
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn extract_document_blocks_with_options_wasm(
    model_json: &str,
    options_json: &str,
) -> Result<String, JsValue> {
    let model = doc_model_from_json(model_json)?;
    let options = serde_json::from_str(options_json).map_err(|error| {
        JsValue::from_str(&format!("Invalid block extraction options JSON: {error}"))
    })?;
    serde_json::to_string(&rust_docx::extract_document_blocks_with_options(
        &model, options,
    ))
    .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn parse_document_bundle_wasm(bytes: &[u8], options_json: &str) -> Result<String, JsValue> {
    let options = serde_json::from_str(options_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid bundle options: {e}")))?;
    let bundle = rust_docx::parse_document_bundle(bytes, options).map_err(|e| JsValue::from_str(&e))?;
    serde_json::to_string(&bundle).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen]
pub fn export_document_wasm(request_json: &str) -> Result<String, JsValue> {
    let request = serde_json::from_str(request_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid export request: {e}")))?;
    let result = rust_docx::export_document(&request).map_err(|e| JsValue::from_str(&e))?;
    serde_json::to_string(&result).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen]
pub fn inspect_document_source_wasm(bytes: &[u8]) -> Result<String, JsValue> {
    let package = parse_document_bytes(bytes).map_err(|error| JsValue::from_str(&error))?;
    serde_json::to_string(&rust_docx::document_source_graph(&package))
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

fn value_to_doc_model(value: &JsValue) -> Result<DocModel, JsValue> {
    if let Some(json_str) = value.as_string() {
        return doc_model_from_json(&json_str);
    }
    let model = stringify_model_value(value)
        .map_err(|error| JsValue::from_str(&format!("Failed to stringify model: {error:?}")))?;
    model.parse("DocModel")
}

/// Strings at least this long in a `src` field travel outside the model JSON.
const OUT_OF_BAND_TEXT_LENGTH: u32 = 4096;

/// Model JSON whose image payloads are referenced by transport tokens.
struct ModelJson {
    json: String,
    payloads: TransportPayloads,
}

impl ModelJson {
    fn parse<T: serde::de::DeserializeOwned>(self, what: &str) -> Result<T, JsValue> {
        let ModelJson { json, payloads } = self;
        with_transport_payloads(payloads, || serde_json::from_str(&json))
            .map_err(|error| JsValue::from_str(&format!("Invalid {what} JSON: {error}")))
    }
}

fn token(prefix: &str, index: usize) -> JsValue {
    JsValue::from_str(&format!("{prefix}{index}"))
}

fn intern(values: &mut Vec<JsValue>, value: JsValue) -> usize {
    match values.iter().position(|known| Object::is(known, &value)) {
        Some(index) => index,
        None => {
            values.push(value);
            values.len() - 1
        }
    }
}

/// Stringifies a JS model without expanding image payloads into JSON.
///
/// Image `data` buffers and long `src` strings are collected once (repeated
/// buffers and strings are shared) and replaced by short tokens in a shallow
/// copy of their holder; the caller's objects are not modified. Other byte
/// arrays keep the legacy number-array encoding.
fn stringify_model_value(value: &JsValue) -> Result<ModelJson, JsValue> {
    let bytes = Rc::new(RefCell::new(Vec::<JsValue>::new()));
    let text = Rc::new(RefCell::new(Vec::<JsValue>::new()));
    let (replacer_bytes, replacer_text) = (bytes.clone(), text.clone());
    let replacer = Closure::<dyn FnMut(JsValue, JsValue) -> JsValue>::new(move |_: JsValue, value: JsValue| {
        if value.is_instance_of::<Uint8Array>() {
            return Array::from(&value).into();
        }
        if !value.is_object() || Array::is_array(&value) {
            return value;
        }
        let get = |key: &str| Reflect::get(&value, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED);
        let data = get("data");
        let data = (data.is_instance_of::<Uint8Array>() && get("type").as_string().as_deref() == Some("image"))
            .then_some(data);
        let src = get("src");
        let src = src
            .dyn_ref::<JsString>()
            .is_some_and(|src| src.length() >= OUT_OF_BAND_TEXT_LENGTH)
            .then_some(src);
        if data.is_none() && src.is_none() {
            return value;
        }
        let holder = Object::assign(&Object::new(), value.unchecked_ref());
        if let Some(data) = data {
            let index = intern(&mut replacer_bytes.borrow_mut(), data);
            let _ = Reflect::set(&holder, &JsValue::from_str("data"), &token(BYTES_TOKEN_PREFIX, index));
        }
        if let Some(src) = src {
            let index = intern(&mut replacer_text.borrow_mut(), src);
            let _ = Reflect::set(&holder, &JsValue::from_str("src"), &token(TEXT_TOKEN_PREFIX, index));
        }
        holder.into()
    });
    let json = js_sys::JSON::stringify_with_replacer(value, replacer.as_ref())?
        .as_string()
        .ok_or_else(|| JsValue::from_str("Model JSON must be a string"))?;
    drop(replacer);
    let payloads = TransportPayloads {
        bytes: bytes
            .take()
            .into_iter()
            .map(|data| data.unchecked_into::<Uint8Array>().to_vec())
            .collect(),
        text: text
            .take()
            .into_iter()
            .map(|src| src.as_string().unwrap_or_default())
            .collect(),
    };
    Ok(ModelJson { json, payloads })
}

fn value_to_image_run(value: &JsValue) -> Result<ImageRunNode, JsValue> {
    if let Some(json_str) = value.as_string() {
        return serde_json::from_str(&json_str)
            .map_err(|error| JsValue::from_str(&format!("Invalid ImageRunNode JSON: {error}")));
    }
    stringify_model_value(value)
        .map_err(|error| JsValue::from_str(&format!("Failed to stringify image: {error:?}")))?
        .parse("ImageRunNode")
}

#[wasm_bindgen]
pub fn can_preserve_image_paint_source_wasm(
    image: &JsValue,
    base_package: Option<JsValue>,
    owner_part_name: Option<String>,
) -> Result<bool, JsValue> {
    let image = value_to_image_run(image)?;
    let base = match base_package {
        Some(value) => Some(value_to_ooxml_package(&value)?),
        None => None,
    };
    let owner = owner_part_name.unwrap_or_else(|| match base.as_ref() {
        Some(package) => rust_docx::main_document_part_name(package),
        None => rust_docx::main_document_part_name(&rust_docx::create_minimal_docx_package(None)),
    });
    Ok(rust_docx::can_preserve_image_paint_source(&image, base.as_ref(), &owner))
}

fn js_bytes_to_vec(value: &JsValue, key: &str) -> Result<Vec<u8>, JsValue> {
    if let Some(bytes) = value.dyn_ref::<js_sys::Uint8Array>() {
        return Ok(bytes.to_vec());
    }
    if let Some(buffer) = value.dyn_ref::<js_sys::ArrayBuffer>() {
        return Ok(js_sys::Uint8Array::new(buffer).to_vec());
    }
    // Legacy packages produced before the Uint8Array transport carry number[].
    if js_sys::Array::is_array(value) {
        return Ok(js_sys::Uint8Array::new(value).to_vec());
    }
    Err(JsValue::from_str(&format!(
        "Binary asset {key:?} must be a Uint8Array, ArrayBuffer, or number[]"
    )))
}

/// Binary assets cross the boundary structurally; only `parts` (XML text) goes
/// through JSON, where JSON.parse/stringify outperforms per-property reflection.
fn value_to_ooxml_package(value: &JsValue) -> Result<OoxmlPackage, JsValue> {
    if !value.is_object() || js_sys::Array::is_array(value) {
        return Err(JsValue::from_str("Package must be an object"));
    }

    let parts_value = Reflect::get(value, &JsValue::from_str("parts"))
        .map_err(|error| JsValue::from_str(&format!("Failed to read package parts: {error:?}")))?;
    if parts_value.is_undefined() || parts_value.is_null() {
        return Err(JsValue::from_str(
            "Invalid OoxmlPackage: missing field `parts`",
        ));
    }
    let parts_json = js_sys::JSON::stringify(&parts_value)
        .map_err(|error| JsValue::from_str(&format!("Failed to stringify parts: {error:?}")))?;
    let parts_json_str = parts_json
        .as_string()
        .ok_or_else(|| JsValue::from_str("Package parts must be JSON-serializable"))?;
    let parts: HashMap<String, OoxmlPart> = serde_json::from_str(&parts_json_str)
        .map_err(|error| JsValue::from_str(&format!("Invalid OoxmlPackage parts: {error}")))?;

    let assets_value = Reflect::get(value, &JsValue::from_str("binaryAssets"))
        .map_err(|error| JsValue::from_str(&format!("Failed to read binaryAssets: {error:?}")))?;
    if assets_value.is_undefined() || assets_value.is_null() {
        return Err(JsValue::from_str(
            "Invalid OoxmlPackage: missing field `binaryAssets`",
        ));
    }
    // typeof-based check, not instanceof: binaryAssets may be a
    // prototype-less dictionary or come from another realm.
    if !assets_value.is_object() {
        return Err(JsValue::from_str("binaryAssets must be an object"));
    }
    let mut binary_assets = HashMap::new();
    for key in Object::keys(assets_value.unchecked_ref::<Object>()).iter() {
        let name = key
            .as_string()
            .ok_or_else(|| JsValue::from_str("binaryAssets keys must be strings"))?;
        let entry = Reflect::get(&assets_value, &key).map_err(|error| {
            JsValue::from_str(&format!("Failed to read binary asset {name:?}: {error:?}"))
        })?;
        binary_assets.insert(name.clone(), js_bytes_to_vec(&entry, &name)?);
    }

    let warnings_value = Reflect::get(value, &JsValue::from_str("warnings"))
        .map_err(|error| JsValue::from_str(&format!("Failed to read warnings: {error:?}")))?;
    let mut warnings = Vec::new();
    if !warnings_value.is_undefined() && !warnings_value.is_null() {
        if !Array::is_array(&warnings_value) {
            return Err(JsValue::from_str("warnings must be a string array"));
        }
        for warning in Array::from(&warnings_value).iter() {
            let warning = warning
                .as_string()
                .ok_or_else(|| JsValue::from_str("warnings must contain only strings"))?;
            warnings.push(warning);
        }
    }

    Ok(OoxmlPackage {
        parts,
        binary_assets,
        warnings,
    })
}

fn package_to_js(pkg: &OoxmlPackage) -> Result<JsValue, JsValue> {
    // Part names mirror the map keys; emit the key as the part name so the two
    // can never disagree on the JS side.
    let mut parts_map = serde_json::Map::with_capacity(pkg.parts.len());
    for (key, part) in &pkg.parts {
        parts_map.insert(
            key.clone(),
            serde_json::json!({ "name": key, "content": part.content }),
        );
    }
    let parts_json = serde_json::to_string(&serde_json::Value::Object(parts_map))
        .map_err(|error| JsValue::from_str(&format!("Parts serialization failed: {error}")))?;
    let parts_js = js_sys::JSON::parse(&parts_json)
        .map_err(|error| JsValue::from_str(&format!("Parts JSON parse failed: {error:?}")))?;

    // Null-prototype object: asset names come verbatim from zip entry names,
    // and [[Set]] on a normal object would hit the Object.prototype __proto__
    // accessor for an entry named "__proto__" instead of storing the asset.
    let assets: Object = Object::create(JsValue::NULL.unchecked_ref());
    for (key, bytes) in &pkg.binary_assets {
        let array = js_sys::Uint8Array::from(bytes.as_slice());
        Reflect::set(&assets, &JsValue::from_str(key), &array.into()).map_err(|error| {
            JsValue::from_str(&format!("Failed to set binary asset {key:?}: {error:?}"))
        })?;
    }

    let result = Object::new();
    Reflect::set(&result, &JsValue::from_str("parts"), &parts_js)
        .map_err(|error| JsValue::from_str(&format!("Failed to set parts: {error:?}")))?;
    Reflect::set(&result, &JsValue::from_str("binaryAssets"), &assets)
        .map_err(|error| JsValue::from_str(&format!("Failed to set binaryAssets: {error:?}")))?;
    if !pkg.warnings.is_empty() {
        let warnings = Array::new();
        for warning in &pkg.warnings {
            warnings.push(&JsValue::from_str(warning));
        }
        Reflect::set(&result, &JsValue::from_str("warnings"), &warnings)
            .map_err(|error| JsValue::from_str(&format!("Failed to set warnings: {error:?}")))?;
    }
    Ok(result.into())
}

#[wasm_bindgen]
pub fn parse_docx_wasm(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let pkg = parse_document_bytes(bytes).map_err(|error| JsValue::from_str(&error))?;
    package_to_js(&pkg)
}

fn parse_options(value: Option<JsValue>) -> Result<rust_docx::ParseOptions, JsValue> {
    let Some(value) = value.filter(|value| !value.is_null() && !value.is_undefined()) else {
        return Ok(Default::default());
    };
    let json = js_sys::JSON::stringify(&value)?;
    serde_json::from_str(&json.as_string().unwrap_or_default())
        .map_err(|error| JsValue::from_str(&format!("Invalid parser options: {error}")))
}

#[wasm_bindgen]
pub fn build_doc_model_from_package(
    package: &JsValue,
    options: Option<JsValue>,
) -> Result<String, JsValue> {
    let pkg = value_to_ooxml_package(package)?;
    let model = build_doc_model_with_options(&pkg, &parse_options(options)?);
    serde_json::to_string(&model)
        .map_err(|error| JsValue::from_str(&format!("Model serialization failed: {error}")))
}

#[wasm_bindgen]
pub fn build_doc_model_from_package_js(
    package: &JsValue,
    options: Option<JsValue>,
) -> Result<JsValue, JsValue> {
    let pkg = value_to_ooxml_package(package)?;
    let model = build_doc_model_with_options(&pkg, &parse_options(options)?);
    let assets = Reflect::get(package, &JsValue::from_str("binaryAssets"))?;
    model_transport::model_to_js(model, &pkg, &assets)
}

#[wasm_bindgen]
pub fn build_doc_model_from_bytes(bytes: &[u8], options: Option<JsValue>) -> Result<JsValue, JsValue> {
    let started_at = performance_now();
    // OpenDocument text is built into the model directly: it has no Word
    // package, so the model travels with an empty one.
    let (pkg, parsed_at, model) = if rust_docx::is_odf_format(bytes) {
        let model = rust_docx::parse_odt(bytes).map_err(|error| JsValue::from_str(&error))?;
        let pkg = OoxmlPackage {
            parts: Default::default(),
            binary_assets: Default::default(),
            warnings: Vec::new(),
        };
        (pkg, started_at, model)
    } else {
        let pkg = parse_document_bytes(bytes).map_err(|error| JsValue::from_str(&error))?;
        let parsed_at = performance_now();
        let model = build_doc_model_with_options(&pkg, &parse_options(options)?);
        (pkg, parsed_at, model)
    };
    let package_started_at = performance_now();
    let package_js = package_to_js(&pkg)?;
    let package_ms = performance_now() - package_started_at;
    let assets = Reflect::get(&package_js, &JsValue::from_str("binaryAssets"))?;
    let model_js = model_transport::model_to_js(model, &pkg, &assets)?;
    let built_at = performance_now();

    let result = Object::new();
    Reflect::set(
        &result,
        &JsValue::from_str("package"),
        &package_js,
    )
    .map_err(|error| JsValue::from_str(&format!("Failed to set package: {error:?}")))?;
    Reflect::set(&result, &JsValue::from_str("model"), &model_js)
        .map_err(|error| JsValue::from_str(&format!("Failed to set model: {error:?}")))?;
    let timings = Object::new();
    Reflect::set(
        &timings,
        &JsValue::from_str("parseMs"),
        &JsValue::from_f64(parsed_at - started_at + package_ms),
    )?;
    Reflect::set(
        &timings,
        &JsValue::from_str("buildModelMs"),
        &JsValue::from_f64(built_at - parsed_at - package_ms),
    )?;
    Reflect::set(&result, &JsValue::from_str("timings"), &timings)?;
    Ok(result.into())
}

#[wasm_bindgen]
pub fn serialize_docx_wasm(
    model: &JsValue,
    base_package: Option<JsValue>,
) -> Result<Vec<u8>, JsValue> {
    let doc_model = value_to_doc_model(model)?;
    rust_docx::table_grid::validate_collapsed_table_grids(&doc_model)
        .map_err(|error| JsValue::from_str(&error))?;
    let base = match base_package {
        Some(value) => Some(value_to_ooxml_package(&value)?),
        None => None,
    };
    let package = try_serialize_doc_model(&doc_model, base.as_ref())
        .map_err(|error| JsValue::from_str(&error))?;
    package_to_bytes(&package).map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn serialize_docx_from_json_wasm(
    model_json: &str,
    base_package: Option<JsValue>,
) -> Result<Vec<u8>, JsValue> {
    let doc_model = doc_model_from_json(model_json)?;
    rust_docx::table_grid::validate_collapsed_table_grids(&doc_model)
        .map_err(|error| JsValue::from_str(&error))?;
    let base = match base_package {
        Some(value) => Some(value_to_ooxml_package(&value)?),
        None => None,
    };
    let package = try_serialize_doc_model(&doc_model, base.as_ref())
        .map_err(|error| JsValue::from_str(&error))?;
    package_to_bytes(&package).map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn model_to_document_xml_wasm(
    model: &JsValue,
    base_package: Option<JsValue>,
) -> Result<String, JsValue> {
    let doc_model = value_to_doc_model(model)?;
    rust_docx::table_grid::validate_collapsed_table_grids(&doc_model)
        .map_err(|error| JsValue::from_str(&error))?;
    let base = match base_package {
        Some(value) => Some(value_to_ooxml_package(&value)?),
        None => None,
    };
    rust_docx::validate_image_paint_bounds(&doc_model, base.as_ref())
        .map_err(|error| JsValue::from_str(&error))?;
    Ok(model_to_document_xml(&doc_model, base.as_ref()))
}

#[wasm_bindgen]
pub fn model_to_document_xml_from_json_wasm(
    model_json: &str,
    base_package: Option<JsValue>,
) -> Result<String, JsValue> {
    let doc_model = doc_model_from_json(model_json)?;
    rust_docx::table_grid::validate_collapsed_table_grids(&doc_model)
        .map_err(|error| JsValue::from_str(&error))?;
    let base = match base_package {
        Some(value) => Some(value_to_ooxml_package(&value)?),
        None => None,
    };
    rust_docx::validate_image_paint_bounds(&doc_model, base.as_ref())
        .map_err(|error| JsValue::from_str(&error))?;
    Ok(model_to_document_xml(&doc_model, base.as_ref()))
}

#[wasm_bindgen]
pub fn package_to_array_buffer_wasm(package: &JsValue) -> Result<Vec<u8>, JsValue> {
    let pkg = value_to_ooxml_package(package)?;
    package_to_bytes(&pkg).map_err(|error| JsValue::from_str(&error))
}

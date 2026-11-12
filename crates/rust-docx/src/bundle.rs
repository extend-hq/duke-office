//! Portable structured extraction with optional assets, charts, and tracked-change spans.
use crate::{
    ChartData, DocModel, DocumentComment, DocumentRevision, ExtractDocumentBlocksOptions,
    OoxmlPackage, StructuredDocument,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExtractDocumentBundleOptions {
    #[serde(flatten)]
    pub blocks: ExtractDocumentBlocksOptions,
    pub include_revisions: bool,
    pub include_charts: bool,
    pub deduplicate_assets: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentBundle {
    #[serde(flatten)]
    pub document: StructuredDocument,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revisions: Option<Vec<DocumentRevision>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<DocumentComment>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub charts: BTreeMap<String, ChartData>,
    /// IDs referenced by imageAssetId, sourceImageAssetId, previewImageAssetId, and dataAssetId.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub assets: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub revision_inlines: BTreeMap<String, Vec<crate::RevisionInline>>,
}

pub fn extract_document_bundle(
    model: &DocModel,
    package: &OoxmlPackage,
    options: ExtractDocumentBundleOptions,
) -> DocumentBundle {
    let (mut document, revisions, comments) = if options.include_revisions {
        let reviewed = crate::extract_document_blocks_with_revisions(model, options.blocks);
        (
            reviewed.document,
            Some(reviewed.revisions),
            reviewed.comments,
        )
    } else {
        (
            crate::extract_document_blocks_with_options(model, options.blocks),
            None,
            crate::review::extract_document_comments(model),
        )
    };
    let mut charts = BTreeMap::new();
    if options.include_charts {
        collect_chart_data(&document.blocks, package, &mut charts);
    }
    let revision_inlines = revisions
        .as_ref()
        .map(|r| crate::revision_inlines(&document.blocks, r))
        .unwrap_or_default();
    let assets = if options.deduplicate_assets {
        extract_document_assets(&mut document.blocks)
    } else {
        BTreeMap::new()
    };
    DocumentBundle {
        document,
        revisions,
        comments,
        charts,
        assets,
        revision_inlines,
    }
}

pub fn parse_document_bundle(
    bytes: &[u8],
    options: ExtractDocumentBundleOptions,
) -> Result<DocumentBundle, String> {
    if crate::is_odf_format(bytes) {
        // OpenDocument text is built into the model directly and has no Word package.
        let package = OoxmlPackage {
            parts: Default::default(),
            binary_assets: Default::default(),
            warnings: Vec::new(),
        };
        return Ok(extract_document_bundle(&crate::parse_odt(bytes)?, &package, options));
    }
    let mut package = crate::parse_document_bytes(bytes)?;
    let model = crate::build_doc_model_with_options(
        &package,
        &crate::ParseOptions {
            include_3d_models: options.blocks.include_3d_models,
            ..Default::default()
        },
    );
    if !options.include_charts {
        package.parts.clear();
        package.binary_assets.clear();
    }
    Ok(extract_document_bundle(&model, &package, options))
}

/// Replace repeated embedded asset payloads with document-local IDs.
pub fn extract_document_assets(blocks: &mut [crate::DocumentBlock]) -> BTreeMap<String, String> {
    let mut assets = std::collections::HashMap::new();
    collect_image_assets(blocks, &mut assets);
    assets.into_iter().map(|(url, id)| (id, url)).collect()
}

pub fn collect_chart_data(
    blocks: &[crate::DocumentBlock],
    package: &crate::OoxmlPackage,
    charts: &mut std::collections::BTreeMap<String, crate::ChartData>,
) {
    for block in blocks {
        if let crate::BlockDetails::Raw(details) = &block.details {
            if let Some(part) = details.get("chartPartName").and_then(Value::as_str) {
                if !charts.contains_key(part) {
                    if let Some(chart) = crate::extract_chart_data(package, part) {
                        charts.insert(part.to_owned(), chart);
                    }
                }
            }
        }
        collect_chart_data(&block.children, package, charts);
    }
}

fn collect_image_assets(
    blocks: &mut [crate::DocumentBlock],
    assets: &mut std::collections::HashMap<String, String>,
) {
    for block in blocks {
        if let crate::BlockDetails::Raw(details) = &mut block.details {
            if let Some(details) = details.as_object_mut() {
                if let Some(model) = details.get_mut("model3d") {
                    if let Some(asset) = model.get_mut("asset").and_then(Value::as_object_mut) {
                        if let Some(Value::Array(data)) = asset.get("data") {
                            let bytes: Option<Vec<u8>> = data
                                .iter()
                                .map(|value| value.as_u64().and_then(|n| u8::try_from(n).ok()))
                                .collect();
                            if let Some(bytes) = bytes {
                                asset.remove("data");
                                let url = format!(
                                    "data:model/gltf-binary;base64,{}",
                                    base64::engine::general_purpose::STANDARD.encode(bytes)
                                );
                                let next = assets.len().to_string();
                                let id = assets.entry(url).or_insert(next).clone();
                                asset.insert("dataAssetId".into(), Value::String(id));
                            }
                        }
                    }
                }
                for field in ["image", "sourceImage", "previewImage"] {
                    let key = format!("{field}Url");
                    if let Some(url) = details.get(&key).and_then(Value::as_str).map(str::to_owned)
                    {
                        details.remove(&key);
                        let next = assets.len().to_string();
                        let id = assets.entry(url).or_insert(next).clone();
                        details.insert(format!("{field}AssetId"), Value::String(id));
                    }
                }
            }
        }
        collect_image_assets(&mut block.children, assets);
    }
}

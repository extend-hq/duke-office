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
        collect_model_chart_data(model, &document.blocks, &mut charts);
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
        return Ok(extract_document_bundle(
            &crate::parse_odt(bytes)?,
            &package,
            options,
        ));
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

fn collect_model_chart_data(
    model: &DocModel,
    blocks: &[crate::DocumentBlock],
    charts: &mut BTreeMap<String, ChartData>,
) {
    let mut referenced = std::collections::BTreeSet::new();
    let mut blocks: Vec<_> = blocks.iter().collect();
    while let Some(block) = blocks.pop() {
        if let crate::BlockDetails::Raw(details) = &block.details {
            if let Some(part) = details.get("chartPartName").and_then(Value::as_str) {
                referenced.insert(part);
            }
        }
        blocks.extend(&block.children);
    }
    if referenced.is_empty() {
        return;
    }
    enum Pending<'a> {
        Nodes(&'a [crate::DocNode]),
        Paragraph(&'a crate::ParagraphNode),
        Table(&'a crate::TableNode),
    }
    let mut pending = vec![Pending::Nodes(&model.nodes)];
    let metadata = &model.metadata;
    for header in metadata.header_sections.iter().chain(
        metadata
            .sections
            .iter()
            .flatten()
            .flat_map(|s| &s.header_sections),
    ) {
        pending.push(Pending::Nodes(&header.nodes));
    }
    for footer in metadata.footer_sections.iter().chain(
        metadata
            .sections
            .iter()
            .flatten()
            .flat_map(|s| &s.footer_sections),
    ) {
        pending.push(Pending::Nodes(&footer.nodes));
    }
    for note in metadata
        .footnotes
        .iter()
        .chain(&metadata.endnotes)
        .flatten()
    {
        if let Some(nodes) = &note.nodes {
            pending.push(Pending::Nodes(nodes));
        }
    }
    for comment in metadata.comments.iter().flatten() {
        if let Some(nodes) = &comment.nodes {
            pending.push(Pending::Nodes(nodes));
        }
    }
    while let Some(item) = pending.pop() {
        match item {
            Pending::Nodes(nodes) => pending.extend(nodes.iter().map(|node| match node {
                crate::DocNode::Paragraph(p) => Pending::Paragraph(p),
                crate::DocNode::Table(t) => Pending::Table(t),
            })),
            Pending::Table(table) => {
                for node in table
                    .rows
                    .iter()
                    .flat_map(|r| &r.cells)
                    .flat_map(|c| &c.nodes)
                {
                    pending.push(match node {
                        crate::TableCellContentNode::Paragraph(p) => Pending::Paragraph(p),
                        crate::TableCellContentNode::Table(t) => Pending::Table(t),
                    });
                }
            }
            Pending::Paragraph(paragraph) => {
                for child in &paragraph.children {
                    if let crate::ParagraphChildNode::Image(image) = child {
                        if let (Some(part), Some(data)) =
                            (&image.chart_part_name, &image.chart_data)
                        {
                            if referenced.contains(part.as_str()) {
                                charts
                                    .entry(part.clone())
                                    .or_insert_with(|| (**data).clone());
                            }
                        }
                        if let Some(nodes) = &image.content_nodes {
                            pending.push(Pending::Nodes(nodes));
                        }
                    }
                }
            }
        }
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

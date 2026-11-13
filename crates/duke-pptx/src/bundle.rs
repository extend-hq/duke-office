use std::collections::BTreeMap;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    extraction_limits::{check_serialized_size, serialized_size},
    semantic::paragraph_details,
    ChartData, ExtractPresentationBlocksOptions, ExtractionLimits, ParseLimits, ParseOptions,
    PresentationBlock, PresentationDocument, PresentationFormat, PresentationMetadata,
    PresentationSize, PresentationWarning, SlideComment, SlideNode, StructuredPresentation,
    TextParagraph,
};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExtractPresentationBundleOptions {
    #[serde(flatten)]
    pub blocks: ExtractPresentationBlocksOptions,
    pub include_charts: bool,
    pub deduplicate_assets: bool,
    pub include_artwork: bool,
    pub limits: ExtractionLimits,
    pub parse_limits: ParseLimits,
}

impl Default for ExtractPresentationBundleOptions {
    fn default() -> Self {
        Self {
            blocks: Default::default(),
            include_charts: false,
            deduplicate_assets: false,
            include_artwork: false,
            limits: Default::default(),
            parse_limits: ParseLimits::server(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationSlideManifest {
    pub id: String,
    pub slide_index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_part: Option<String>,
    pub block_ids: Vec<String>,
    pub warnings: Vec<PresentationWarning>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationComment {
    pub slide_index: usize,
    #[serde(flatten)]
    pub comment: SlideComment,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationBundle {
    #[serde(flatten)]
    pub document: StructuredPresentation,
    pub format: PresentationFormat,
    pub size: PresentationSize,
    pub slides: Vec<PresentationSlideManifest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<PresentationComment>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub charts: BTreeMap<String, ChartData>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub assets: BTreeMap<String, String>,
    pub warnings: Vec<PresentationWarning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<PresentationMetadata>,
}

pub fn parse_presentation_bundle(
    bytes: &[u8],
    options: ExtractPresentationBundleOptions,
) -> Result<PresentationBundle, String> {
    let document = crate::parse_presentation_with_options(
        bytes,
        &options.parse_limits,
        &ParseOptions {
            defer_artwork: !options.include_artwork,
            ..Default::default()
        },
    )
    .map_err(|error| match error {
        crate::ParseError::InputLimit(_) | crate::ParseError::ResourceLimit(_) => {
            format!("LIMIT: {error}")
        }
        _ => error.to_string(),
    })?;
    extract_presentation_bundle(&document, options)
}

struct Assets<'a> {
    document: &'a PresentationDocument,
    options: &'a ExtractPresentationBundleOptions,
    data: BTreeMap<String, String>,
    sources: BTreeMap<String, String>,
    bytes: usize,
    encoded_bytes: usize,
    detail_bytes: usize,
}

impl Assets<'_> {
    fn record(&mut self, value: &impl Serialize) -> Result<(), String> {
        self.detail_bytes += serialized_size(
            value,
            self.options
                .limits
                .max_output_bytes
                .saturating_sub(self.detail_bytes),
        )?;
        Ok(())
    }

    fn insert(&mut self, content_type: &str, bytes: &[u8]) -> Result<String, String> {
        let mut hash = Sha256::new();
        hash.update(content_type.as_bytes());
        hash.update([0]);
        hash.update(bytes);
        let id = format!("asset_{:x}", hash.finalize());
        if self.data.contains_key(&id) {
            return Ok(id);
        }
        self.bytes = self.bytes.saturating_add(bytes.len());
        if self.data.len() >= self.options.limits.max_assets
            || self.bytes > self.options.limits.max_asset_bytes
        {
            return Err("LIMIT: extracted assets".into());
        }
        self.encoded_bytes = self.encoded_bytes.saturating_add(
            bytes
                .len()
                .div_ceil(3)
                .saturating_mul(4)
                .saturating_add(content_type.len())
                .saturating_add("data:;base64,".len()),
        );
        if self.encoded_bytes > self.options.limits.max_output_bytes {
            return Err("LIMIT: extracted asset output".into());
        }
        let data = format!(
            "data:{content_type};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        self.data.insert(id.clone(), data);
        Ok(id)
    }

    fn source(&mut self, source: &str) -> Result<Option<String>, String> {
        if let Some(id) = self.sources.get(source) {
            return Ok(Some(id.clone()));
        }
        let Some(asset) = self.document.assets.get(source) else {
            return Ok(None);
        };
        let Some(data) = &asset.data else {
            return Ok(None);
        };
        let id = self.insert(&asset.content_type, data)?;
        self.sources.insert(source.into(), id.clone());
        Ok(Some(id))
    }

    fn field(&self, details: &mut Value, field: &str, id: &str) {
        if self.options.deduplicate_assets {
            details[format!("{field}AssetId")] = json!(id);
        } else {
            details[format!("{field}Url")] = json!(self.data[id]);
        }
    }

    fn image(
        &mut self,
        details: &mut Value,
        source: &str,
        svg: Option<&str>,
        fallback: Option<&str>,
    ) -> Result<(), String> {
        if let Some(id) = self.source(source)? {
            self.field(details, "sourceImage", &id);
            self.field(details, "image", &id);
        }
        if let Some(fallback) = fallback {
            if let Some(id) = self.source(fallback)? {
                self.field(details, "previewImage", &id);
            }
        }
        if self.options.include_artwork {
            if let Some(svg) = svg {
                let id = self.insert("image/svg+xml", svg.as_bytes())?;
                self.field(details, "image", &id);
            }
        }
        Ok(())
    }
}

fn enrich_paragraphs(
    blocks: &mut [PresentationBlock],
    paragraphs: &[TextParagraph],
    assets: &mut Assets<'_>,
) -> Result<(), String> {
    let mut counters = BTreeMap::new();
    for (block, paragraph) in blocks.iter_mut().zip(paragraphs) {
        let details = paragraph_details(paragraph, &mut counters);
        assets.record(&details)?;
        block.details = serde_json::to_value(details).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn node_at_path<'a>(nodes: &'a [SlideNode], path: &[usize]) -> Option<&'a SlideNode> {
    let (first, rest) = path.split_first()?;
    let node = nodes.get(*first)?;
    if rest.is_empty() {
        return Some(node);
    }
    match node {
        SlideNode::Group { children, .. } => node_at_path(children, rest),
        _ => None,
    }
}

fn enrich_node(
    block: &mut PresentationBlock,
    node: &SlideNode,
    assets: &mut Assets<'_>,
    charts: &mut BTreeMap<String, ChartData>,
    warnings: &mut Vec<PresentationWarning>,
) -> Result<(), String> {
    let (metadata, transform) = match node {
        SlideNode::Shape {
            metadata,
            transform,
            ..
        }
        | SlideNode::Image {
            metadata,
            transform,
            ..
        }
        | SlideNode::Group {
            metadata,
            transform,
            ..
        }
        | SlideNode::Table {
            metadata,
            transform,
            ..
        }
        | SlideNode::Chart {
            metadata,
            transform,
            ..
        }
        | SlideNode::Media {
            metadata,
            transform,
            ..
        }
        | SlideNode::Unknown {
            metadata,
            transform,
            ..
        } => (metadata, transform),
    };
    block.details["transform"] = json!(transform);
    if let Some(metadata) = metadata {
        block.details["hidden"] = json!(metadata.hidden);
        block.details["decorative"] = json!(metadata.decorative);
        block.details["alt"] = json!(metadata.alt_text);
        block.details["accessibilityTitle"] = json!(metadata.accessibility_title);
        block.details["description"] = json!(metadata.description);
        block.details["hyperlink"] = json!(metadata.hyperlink);
    }
    match node {
        SlideNode::Shape {
            paragraphs,
            shape_svg,
            fill,
            ..
        } => {
            enrich_paragraphs(&mut block.children, paragraphs, assets)?;
            if let Some(crate::FillStyle::Image { asset_id, .. }) = fill {
                if let Some(id) = assets.source(asset_id)? {
                    assets.field(&mut block.details, "sourceImage", &id);
                }
                block.details["fill"] = json!(fill);
            }
            if assets.options.include_artwork {
                if let Some(svg) = shape_svg {
                    let id = assets.insert("image/svg+xml", svg.as_bytes())?;
                    assets.field(&mut block.details, "image", &id);
                }
            }
        }
        SlideNode::Group {
            children,
            child_transform,
            ..
        } => {
            block.details["childTransform"] = json!(child_transform);
            for (child, node) in block.children.iter_mut().zip(children) {
                enrich_node(child, node, assets, charts, warnings)?;
            }
        }
        SlideNode::Table {
            rows,
            column_widths,
            row_heights,
            ..
        } => {
            block.details["type"] = json!("table_details");
            block.details["rowCount"] = json!(rows.len());
            block.details["columnCount"] = json!(rows.iter().map(Vec::len).max().unwrap_or(0));
            block.details["columnWidths"] = json!(column_widths);
            block.details["rowHeights"] = json!(row_heights);
            for child in &mut block.children {
                let source = &child.source_refs[0];
                let cell = &rows[source.row_index.unwrap()][source.column_index.unwrap()];
                child.details["type"] = json!("table_cell_details");
                child.details["style"] = json!({
                    "fill": cell.fill, "borders": cell.borders, "textInsets": cell.text_insets,
                    "verticalAlignment": cell.vertical_alignment, "textRotation": cell.text_rotation,
                });
                assets.record(&child.details)?;
                enrich_paragraphs(&mut child.children, &cell.paragraphs, assets)?;
            }
        }
        SlideNode::Chart {
            chart_part_name,
            chart_xml,
            chart_data,
            chart_svg,
            ..
        } => {
            let key = chart_part_name.as_deref().unwrap_or(&block.id);
            block.details["chartId"] = json!(key);
            if let Some(part) = chart_part_name {
                block.details["chartPartName"] = json!(part);
            }
            if assets.options.include_charts && !charts.contains_key(key) {
                if let Some(data) = chart_data.as_deref().cloned().or_else(|| {
                    chart_xml
                        .as_deref()
                        .and_then(duke_office_charts::parse_chart_xml)
                }) {
                    assets.record(&data)?;
                    charts.insert(key.into(), data);
                } else {
                    let mut warning = PresentationWarning::warning(
                        "chart-data-unavailable",
                        "Cached chart data could not be extracted.",
                    );
                    warning.slide_index = Some(block.source_refs[0].slide_index);
                    warning.node_id = Some(block.source_refs[0].node_id.clone());
                    warning.part_name = chart_part_name.clone();
                    warnings.push(warning);
                }
            }
            if assets.options.include_artwork {
                if let Some(svg) = chart_svg {
                    let id = assets.insert("image/svg+xml", svg.as_bytes())?;
                    assets.field(&mut block.details, "image", &id);
                }
            }
        }
        SlideNode::Image {
            asset_id,
            fallback_asset_id,
            picture_svg,
            crop,
            effects,
            opacity,
            ..
        } => {
            block.details["crop"] = json!(crop);
            block.details["effects"] = json!(effects);
            block.details["opacity"] = json!(opacity);
            assets.image(
                &mut block.details,
                asset_id,
                picture_svg.as_deref(),
                fallback_asset_id.as_deref(),
            )?;
        }
        SlideNode::Media {
            poster_asset_id,
            media_type,
            source_url,
            ..
        } => {
            block.details["mediaType"] = json!(media_type);
            block.details["sourceUrl"] = json!(source_url);
            if let Some(source) = poster_asset_id {
                assets.image(&mut block.details, source, None, None)?;
            }
        }
        SlideNode::Unknown { .. } => {}
    }
    assets.record(&block.details)?;
    Ok(())
}

pub fn extract_presentation_bundle(
    document: &PresentationDocument,
    options: ExtractPresentationBundleOptions,
) -> Result<PresentationBundle, String> {
    let mut structured =
        crate::extract_presentation_blocks_with_limits(document, &options.blocks, &options.limits)?;
    let mut assets = Assets {
        document,
        options: &options,
        data: BTreeMap::new(),
        sources: BTreeMap::new(),
        bytes: 0,
        encoded_bytes: 0,
        detail_bytes: 0,
    };
    assets.record(&document.metadata)?;
    assets.record(&document.warnings)?;
    let mut comment_count = 0usize;
    for slide in &document.slides {
        let mut pending: Vec<_> = slide.comments.iter().map(|comment| (comment, 0)).collect();
        while let Some((comment, depth)) = pending.pop() {
            comment_count = comment_count.saturating_add(1);
            if depth > options.limits.max_depth
                || comment_count
                    .saturating_add(pending.len())
                    .saturating_add(comment.replies.len())
                    > options.limits.max_blocks
            {
                return Err("LIMIT: extraction comments".into());
            }
            pending.extend(comment.replies.iter().map(|reply| (reply, depth + 1)));
        }
        assets.record(&slide.comments)?;
        assets.record(&slide.warnings)?;
        assets.record(&slide.name)?;
        assets.record(&slide.source_part)?;
    }
    let mut charts = BTreeMap::new();
    let mut warnings = document.warnings.clone();
    let mut slides: Vec<_> = document
        .slides
        .iter()
        .enumerate()
        .map(|(slide_index, slide)| PresentationSlideManifest {
            id: slide.id.clone(),
            slide_index,
            name: slide.name.clone(),
            hidden: slide.hidden,
            source_part: slide.source_part.clone(),
            block_ids: Vec::new(),
            warnings: slide.warnings.clone(),
        })
        .collect();
    for block in &mut structured.blocks {
        let source = &block.source_refs[0];
        let slide_index = source.slide_index;
        let slide = &document.slides[slide_index];
        if source.story == "notes" {
            if let Some(note) = slide.notes.get(source.node_path[0]) {
                enrich_paragraphs(&mut block.children, &note.paragraphs, &mut assets)?;
            }
        } else if let Some(node) = node_at_path(&slide.nodes, &source.node_path) {
            enrich_node(block, node, &mut assets, &mut charts, &mut warnings)?;
        }
        slides[slide_index].block_ids.push(block.id.clone());
    }
    let bundle = PresentationBundle {
        document: structured,
        format: document.format,
        size: document.size.clone(),
        slides,
        comments: document
            .slides
            .iter()
            .enumerate()
            .flat_map(|(slide_index, slide)| {
                slide
                    .comments
                    .iter()
                    .cloned()
                    .map(move |comment| PresentationComment {
                        slide_index,
                        comment,
                    })
            })
            .collect(),
        charts,
        assets: if options.deduplicate_assets {
            assets.data
        } else {
            BTreeMap::new()
        },
        warnings,
        metadata: document.metadata.clone(),
    };
    check_serialized_size(&bundle, options.limits.max_output_bytes)?;
    Ok(bundle)
}

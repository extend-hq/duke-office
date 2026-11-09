use crate::{PresentationDocument, SlideNode, TableCellMerge};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationBlockInput {
    pub document_id: Option<String>,
    pub slides: Vec<BlockSlideInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockSlideInput {
    pub id: String,
    pub source_part: Option<String>,
    pub nodes: Vec<BlockNodeInput>,
    #[serde(default)]
    pub notes: Vec<BlockNoteInput>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockNodeInput {
    pub id: String,
    pub r#type: String,
    pub source_part: Option<String>,
    pub source_id: Option<String>,
    #[serde(default)]
    pub paragraphs: Vec<BlockParagraphInput>,
    #[serde(default)]
    pub rows: Vec<Vec<BlockCellInput>>,
    #[serde(default)]
    pub children: Vec<BlockNodeInput>,
    pub title: Option<String>,
    pub chart_type: Option<String>,
    pub asset_id: Option<String>,
    pub feature: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockCellInput {
    pub merge: Option<Box<TableCellMerge>>,
    pub row_span: Option<usize>,
    pub col_span: Option<usize>,
    pub paragraphs: Vec<BlockParagraphInput>,
}
#[derive(Deserialize)]
pub struct BlockParagraphInput {
    pub runs: Vec<BlockRunInput>,
}
#[derive(Deserialize)]
pub struct BlockRunInput {
    pub text: String,
}
#[derive(Deserialize)]
pub struct BlockNoteInput {
    pub text: String,
    #[serde(default)]
    pub paragraphs: Vec<BlockParagraphInput>,
}
fn input_paragraphs(paragraphs: &[crate::TextParagraph]) -> Vec<BlockParagraphInput> {
    paragraphs
        .iter()
        .map(|p| BlockParagraphInput {
            runs: p
                .runs
                .iter()
                .map(|r| BlockRunInput {
                    text: r.text.clone(),
                })
                .collect(),
        })
        .collect()
}
fn input_nodes(nodes: &[SlideNode]) -> Vec<BlockNodeInput> {
    nodes
        .iter()
        .map(|node| {
            let (id, metadata) = match node {
                SlideNode::Shape { id, metadata, .. }
                | SlideNode::Table { id, metadata, .. }
                | SlideNode::Group { id, metadata, .. }
                | SlideNode::Image { id, metadata, .. }
                | SlideNode::Chart { id, metadata, .. }
                | SlideNode::Media { id, metadata, .. }
                | SlideNode::Unknown { id, metadata, .. } => (id, metadata.as_deref()),
            };
            let mut out = BlockNodeInput {
                id: id.clone(),
                source_part: metadata.and_then(|m| m.source_part.clone()),
                source_id: metadata.and_then(|m| m.source_id.clone()),
                ..Default::default()
            };
            out.r#type = match node {
                SlideNode::Shape { paragraphs, .. } => {
                    out.paragraphs = input_paragraphs(paragraphs);
                    "shape"
                }
                SlideNode::Group { children, .. } => {
                    out.children = input_nodes(children);
                    "group"
                }
                SlideNode::Table { rows, .. } => {
                    out.rows = rows
                        .iter()
                        .map(|row| {
                            row.iter()
                                .map(|c| BlockCellInput {
                                    merge: c.merge.clone(),
                                    row_span: c.row_span,
                                    col_span: c.col_span,
                                    paragraphs: input_paragraphs(&c.paragraphs),
                                })
                                .collect()
                        })
                        .collect();
                    "table"
                }
                SlideNode::Chart {
                    title, chart_type, ..
                } => {
                    out.title = title.clone();
                    out.chart_type = Some(chart_type.clone());
                    "chart"
                }
                SlideNode::Image { asset_id, .. } => {
                    out.asset_id = Some(asset_id.clone());
                    "image"
                }
                SlideNode::Media { .. } => "media",
                SlideNode::Unknown { feature, .. } => {
                    out.feature = Some(feature.clone());
                    "unknown"
                }
            }
            .into();
            out
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PptxSourceRef {
    pub document_id: String,
    pub slide_index: usize,
    pub slide_id: String,
    pub node_id: String,
    pub node_path: Vec<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    pub story: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paragraph_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<usize>,
    pub offset_encoding: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationWord {
    pub object: String,
    pub id: String,
    pub block_id: String,
    pub content: String,
    pub source_refs: Vec<PptxSourceRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationBlock {
    pub object: String,
    pub id: String,
    pub r#type: String,
    pub content: String,
    pub details: Value,
    pub source_refs: Vec<PptxSourceRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_block_id: Option<String>,
    pub children: Vec<PresentationBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub words: Option<Vec<PresentationWord>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredPresentation {
    pub schema_version: u32,
    pub parser_version: String,
    pub document_id: String,
    pub blocks: Vec<PresentationBlock>,
    pub geometry_status: String,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExtractPresentationBlocksOptions {
    pub include_words: bool,
    pub include_notes: bool,
}

fn block(id: String, kind: &str, source: PptxSourceRef, parent: Option<&str>) -> PresentationBlock {
    PresentationBlock {
        object: "block".into(),
        id,
        r#type: kind.into(),
        content: String::new(),
        details: json!({}),
        source_refs: vec![source],
        parent_block_id: parent.map(str::to_owned),
        children: Vec::new(),
        words: None,
    }
}

fn paragraphs(
    parent: &mut PresentationBlock,
    paragraphs: &[BlockParagraphInput],
    include_words: bool,
) {
    for (index, paragraph) in paragraphs.iter().enumerate() {
        let mut source = parent.source_refs[0].clone();
        source.paragraph_index = Some(index);
        let mut child = block(
            format!("{}/paragraph/{index}", parent.id),
            "text",
            source,
            Some(&parent.id),
        );
        child.content = paragraph.runs.iter().map(|run| run.text.as_str()).collect();
        child.details = json!({"kind": "paragraph"});
        if include_words {
            let mut utf16 = 0;
            child.words = Some(
                child
                    .content
                    .split_word_bounds()
                    .filter_map(|word| {
                        let start = utf16;
                        utf16 += word.encode_utf16().count();
                        if !word.chars().any(char::is_alphanumeric) {
                            return None;
                        }
                        let mut source = child.source_refs[0].clone();
                        source.start = Some(start);
                        source.end = Some(utf16);
                        Some(PresentationWord {
                            object: "word".into(),
                            id: format!("{}/word/{start}-{utf16}", child.id),
                            block_id: child.id.clone(),
                            content: word.into(),
                            source_refs: vec![source],
                        })
                    })
                    .collect(),
            );
        }
        parent.children.push(child);
    }
    parent.content = parent
        .children
        .iter()
        .map(|b| b.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
}

fn nodes(
    nodes: &[BlockNodeInput],
    base: &PptxSourceRef,
    parent: Option<&str>,
    options: ExtractPresentationBlocksOptions,
) -> Vec<PresentationBlock> {
    nodes.iter().enumerate().map(|(index, node)| {
        let kind = match node.r#type.as_str() { "shape" => "text_box", "table" => "table", "group" => "group", _ => "figure" };
        let mut source = base.clone();
        source.node_id = node.id.clone();
        source.node_path.push(index);
        source.part_name = node.source_part.clone().or_else(|| base.part_name.clone());
        source.source_id = node.source_id.clone();
        let path = source.node_path.iter().map(usize::to_string).collect::<Vec<_>>().join(".");
        let id = format!("{}/slide/{}/node/{path}", base.document_id, base.slide_index);
        let mut result = block(id, kind, source, parent);
        match node.r#type.as_str() {
            "shape" => paragraphs(&mut result, &node.paragraphs, options.include_words),
            "group" => {
                result.children = self::nodes(&node.children, &result.source_refs[0], Some(&result.id), options);
                result.content = result.children.iter().map(|b| b.content.as_str()).collect::<Vec<_>>().join("\n");
            }
            "table" => {
                for (row_index, row) in node.rows.iter().enumerate() {
                    for (column_index, cell) in row.iter().enumerate() {
                        if cell.merge.as_ref().is_some_and(|m| m.horizontal || m.vertical) { continue; }
                        let mut source = result.source_refs[0].clone();
                        source.row_index = Some(row_index);
                        source.column_index = Some(column_index);
                        let mut child = block(format!("{}/cell/{row_index}/{column_index}", result.id), "table_cell", source, Some(&result.id));
                        child.details = json!({"rowIndex": row_index, "columnIndex": column_index, "rowSpan": cell.row_span.unwrap_or(1), "columnSpan": cell.col_span.unwrap_or(1)});
                        paragraphs(&mut child, &cell.paragraphs, options.include_words);
                        result.children.push(child);
                    }
                }
                result.content = result.children.iter().map(|b| b.content.as_str()).collect::<Vec<_>>().join("\n");
            }
            "chart" => { result.content = node.title.clone().unwrap_or_default(); result.details = json!({"kind": "chart", "chartType": node.chart_type}); }
            "image" => { result.details = json!({"kind": "image", "assetId": node.asset_id}); }
            "media" => { result.details = json!({"kind": "media"}); }
            _ => { result.details = json!({"kind": "unknown", "feature": node.feature}); }
        }
        result
    }).collect()
}

pub fn extract_presentation_block_input(
    document: &PresentationBlockInput,
    options: &ExtractPresentationBlocksOptions,
) -> Result<StructuredPresentation, String> {
    let document_id = document.document_id.as_ref().filter(|id| !id.is_empty())
        .ok_or("Presentation has no source identity; reparse the original bytes before extracting citations")?;
    let mut blocks = Vec::new();
    for (slide_index, slide) in document.slides.iter().enumerate() {
        let source = PptxSourceRef {
            document_id: document_id.clone(),
            slide_index,
            slide_id: slide.id.clone(),
            node_id: String::new(),
            node_path: Vec::new(),
            part_name: slide.source_part.clone(),
            source_id: None,
            story: "slide".into(),
            row_index: None,
            column_index: None,
            paragraph_index: None,
            start: None,
            end: None,
            offset_encoding: "utf-16".into(),
        };
        blocks.extend(nodes(&slide.nodes, &source, None, *options));
        if options.include_notes {
            for (index, note) in slide.notes.iter().enumerate() {
                let mut source = source.clone();
                source.story = "notes".into();
                source.node_id = format!("note-{index}");
                source.node_path = vec![index];
                source.part_name = None;
                let mut child = block(
                    format!("{document_id}/slide/{slide_index}/note/{index}"),
                    "note",
                    source,
                    None,
                );
                paragraphs(&mut child, &note.paragraphs, options.include_words);
                if child.children.is_empty() {
                    child.content = note.text.clone();
                }
                blocks.push(child);
            }
        }
    }
    Ok(StructuredPresentation {
        schema_version: 1,
        parser_version: env!("CARGO_PKG_VERSION").into(),
        document_id: document_id.clone(),
        blocks,
        geometry_status: "requires_layout".into(),
    })
}

pub fn extract_presentation_blocks(
    document: &PresentationDocument,
    options: &ExtractPresentationBlocksOptions,
) -> Result<StructuredPresentation, String> {
    let input = PresentationBlockInput {
        document_id: document.document_id.clone(),
        slides: document
            .slides
            .iter()
            .map(|s| BlockSlideInput {
                id: s.id.clone(),
                source_part: s.source_part.clone(),
                nodes: input_nodes(&s.nodes),
                notes: if options.include_notes {
                    s.notes
                        .iter()
                        .map(|n| BlockNoteInput {
                            text: n.text.clone(),
                            paragraphs: input_paragraphs(&n.paragraphs),
                        })
                        .collect()
                } else {
                    Vec::new()
                },
            })
            .collect(),
    };
    extract_presentation_block_input(&input, options)
}

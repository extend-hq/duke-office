//! HTML document export with selectable HTML, Markdown, or CSV tables.
mod styles;
use crate::{BlockType, DocumentBlock, InlineChange, RevisionInline};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableFormat {
    #[default]
    Html,
    Markdown,
    Csv,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ContentOverride {
    /// Trusted HTML supplied by the caller; it is not escaped or sanitized.
    pub html: Option<String>,
    pub text: Option<String>,
    pub replace_content: bool,
    pub list_label_present: bool,
    pub suppress_image: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExportOptions {
    pub table_format: TableFormat,
    /// Group adjacent insertion/deletion runs in a change element.
    pub group_changes: bool,
    pub max_depth: usize,
    pub max_blocks: usize,
    pub max_table_columns: usize,
    pub max_table_cells: usize,
    pub max_output_bytes: usize,
}
impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            table_format: TableFormat::Html,
            group_changes: false,
            max_depth: 64,
            max_blocks: 100_000,
            max_table_columns: 1024,
            max_table_cells: 1_000_000,
            max_output_bytes: 128 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExportRequest {
    pub blocks: Vec<DocumentBlock>,
    pub options: ExportOptions,
    pub overrides: BTreeMap<String, ContentOverride>,
    pub revision_inlines: BTreeMap<String, Vec<RevisionInline>>,
    /// Payloads returned by asset-deduplicated extraction, or caller-provided URLs.
    pub assets: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedDocument {
    pub content: String,
    pub block_content: BTreeMap<String, String>,
}

pub fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn safe_link(v: &str) -> bool {
    let lower = v.to_ascii_lowercase();
    !v.chars().any(char::is_control)
        && (lower.starts_with("http:")
            || lower.starts_with("https:")
            || lower.starts_with("mailto:")
            || v.starts_with('#'))
}
fn safe_image(v: &str) -> bool {
    let lower = v.to_ascii_lowercase();
    !v.chars().any(char::is_control)
        && (lower.starts_with("http://")
            || lower.starts_with("https://")
            || ["png", "gif", "jpeg", "webp", "svg+xml"]
                .iter()
                .any(|kind| {
                    lower.starts_with(&format!("data:image/{kind};"))
                        || lower.starts_with(&format!("data:image/{kind},"))
                }))
}
fn number(v: &Value, field: &str, fallback: usize) -> usize {
    v[field]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(fallback)
}
fn label(text: &str, d: &Value, already_present: bool) -> String {
    let Some(label) = d["listLabel"].as_str().filter(|v| !v.is_empty()) else {
        return text.into();
    };
    let trimmed = text.trim_start();
    if already_present
        && trimmed.strip_prefix(label).is_some_and(|rest| {
            rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace)
        })
    {
        return text.into();
    }
    format!("{label} {text}")
}
fn cells(block: &DocumentBlock) -> Vec<&DocumentBlock> {
    block
        .children
        .iter()
        .flat_map(|c| {
            if c.r#type == BlockType::TableHead {
                c.children.iter().collect()
            } else {
                vec![c]
            }
        })
        .filter(|c| c.r#type == BlockType::TableCell)
        .collect()
}
fn markup(pieces: Vec<(String, InlineChange)>, grouped: bool) -> String {
    let mut output = String::new();
    let mut state = InlineChange::Unchanged;
    for (text, next) in pieces {
        if text.is_empty() {
            continue;
        }
        if state != next {
            match state {
                InlineChange::Deletion => output.push_str("</del>"),
                InlineChange::Insertion => output.push_str("</ins>"),
                _ => {}
            }
            if grouped && next == InlineChange::Unchanged {
                output.push_str("</change>");
            }
            if grouped && state == InlineChange::Unchanged {
                output.push_str("<change>");
            }
            match next {
                InlineChange::Deletion => output.push_str("<del>"),
                InlineChange::Insertion => output.push_str("<ins>"),
                _ => {}
            }
            state = next;
        }
        output.push_str(&text);
    }
    match state {
        InlineChange::Deletion => output.push_str("</del>"),
        InlineChange::Insertion => output.push_str("</ins>"),
        _ => {}
    }
    if grouped && state != InlineChange::Unchanged {
        output.push_str("</change>");
    }
    output
}
struct Exporter<'a> {
    request: &'a ExportRequest,
    content: BTreeMap<String, String>,
    children: HashMap<&'a str, HashMap<&'a str, &'a DocumentBlock>>,
    bytes: usize,
}
impl Exporter<'_> {
    fn inline(
        &mut self,
        inline: &Value,
        block: &DocumentBlock,
        d: &Value,
        plain: bool,
    ) -> Result<String, String> {
        if inline["type"] == "block" {
            return match inline["blockId"]
                .as_str()
                .and_then(|id| {
                    self.children
                        .get(block.id.as_str())
                        .and_then(|children| children.get(id))
                })
                .copied()
            {
                Some(child) => {
                    if plain {
                        self.text(child)
                    } else {
                        self.block(child)
                    }
                }
                None => Ok(String::new()),
            };
        }
        let text = inline["text"].as_str().unwrap_or("");
        if plain {
            return Ok(escape_html(text));
        }
        let style = inline
            .get("style")
            .filter(|v| !v.is_null())
            .unwrap_or(&d["field"]["style"]);
        Ok(styles::text_html(
            text,
            style,
            inline["link"]
                .as_str()
                .or_else(|| d["field"]["link"].as_str()),
        ))
    }
    fn changed(
        &mut self,
        block: &DocumentBlock,
        d: &Value,
        plain: bool,
    ) -> Result<Option<String>, String> {
        let Some(pieces) = self.request.revision_inlines.get(&block.id) else {
            return Ok(None);
        };
        let mut rendered = Vec::new();
        for piece in pieces {
            rendered.push((
                self.inline(
                    &serde_json::to_value(&piece.inline).map_err(|e| e.to_string())?,
                    block,
                    d,
                    plain,
                )?,
                piece.change,
            ));
        }
        Ok(Some(markup(rendered, self.request.options.group_changes)))
    }
    fn text(&mut self, block: &DocumentBlock) -> Result<String, String> {
        let d = block.details.to_json_value();
        let replace = self.request.overrides.get(&block.id);
        if let Some(o) = replace.filter(|o| !o.replace_content) {
            if let Some(text) = o.text.as_ref().or(o.html.as_ref()) {
                return Ok(text.clone());
            }
        }
        if let Some(text) = self.changed(block, &d, true)? {
            return Ok(label(&text, &d, false));
        }
        if let Some(o) = replace {
            if let Some(text) = &o.text {
                return Ok(label(text, &d, o.list_label_present));
            }
        }
        if d["kind"] == "paragraph" {
            if block.children.iter().any(|c| {
                self.request
                    .overrides
                    .get(&c.id)
                    .is_some_and(|o| !o.replace_content)
            }) {
                if let Some(inlines) = d["inlines"].as_array() {
                    let mut result = String::new();
                    for inline in inlines {
                        if inline["type"] == "block" {
                            result.push_str(&self.inline(inline, block, &d, true)?);
                        } else {
                            result.push_str(inline["text"].as_str().unwrap_or(""));
                        }
                    }
                    return Ok(label(&result, &d, false));
                }
            }
            return Ok(label(&block.content, &d, false));
        }
        if block.children.is_empty() {
            Ok(block.content.clone())
        } else {
            block
                .children
                .iter()
                .map(|c| self.text(c))
                .collect::<Result<Vec<_>, _>>()
                .map(|v| v.join("\n"))
        }
    }
    fn table(&mut self, block: &DocumentBlock, d: &Value) -> Result<String, String> {
        let format = self.request.options.table_format;
        let mut rows: BTreeMap<usize, Vec<&DocumentBlock>> = BTreeMap::new();
        for cell in cells(block) {
            rows.entry(number(&cell.details.to_json_value(), "rowIndex", 0))
                .or_default()
                .push(cell);
        }
        let width = number(d, "columnCount", 1).max(1);
        let style = &d["style"];
        let mut rendered = Vec::new();
        for (row_index, mut row) in rows {
            row.sort_by_key(|c| number(&c.details.to_json_value(), "columnIndex", 0));
            let mut values = if format == TableFormat::Html {
                Vec::new()
            } else {
                vec![String::new(); width]
            };
            for cell in row {
                let cd = cell.details.to_json_value();
                let column = number(&cd, "columnIndex", 0);
                if format != TableFormat::Html {
                    values[column] = self.text(cell)?;
                    continue;
                }
                let tag = if cd["isHeader"] == true { "th" } else { "td" };
                let colspan = number(&cd, "columnSpan", 1).max(1);
                let rowspan = number(&cd, "rowSpan", 1).max(1);
                let content = if cell.children.is_empty() {
                    escape_html(&cell.content)
                } else {
                    cell.children
                        .iter()
                        .map(|c| self.block(c))
                        .collect::<Result<Vec<_>, _>>()?
                        .join("\n")
                };
                let mut borders = json!({"top":style["borders"][if row_index==0{"top"}else{"insideH"}],"bottom":style["borders"][if row_index.saturating_add(rowspan)==number(d,"rowCount",0){"bottom"}else{"insideH"}],"left":style["borders"][if column==0{"left"}else{"insideV"}],"right":style["borders"][if column.saturating_add(colspan)==width{"right"}else{"insideV"}]});
                if let Some(b) = cd["style"]["borders"].as_object() {
                    borders.as_object_mut().unwrap().extend(b.clone());
                }
                let mut cell_style = cd["style"].as_object().cloned().unwrap_or_default();
                cell_style.insert("borders".into(), borders);
                values.push(format!(
                    "<{tag} colspan=\"{colspan}\" rowspan=\"{rowspan}\"{}>{content}</{tag}>",
                    styles::cell(&Value::Object(cell_style), style)
                ));
            }
            rendered.push(match format {
                TableFormat::Html => format!(
                    "<tr{}>{}</tr>",
                    styles::row(&d["rowStyles"][row_index]),
                    values.join("")
                ),
                TableFormat::Csv => values
                    .iter()
                    .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
                    .collect::<Vec<_>>()
                    .join(","),
                TableFormat::Markdown => format!(
                    "| {} |",
                    values
                        .iter()
                        .map(|t| t
                            .replace('|', "\\|")
                            .replace("\r\n", "\n")
                            .replace('\n', "<br>"))
                        .collect::<Vec<_>>()
                        .join(" | ")
                ),
            });
        }
        Ok(match format {
            TableFormat::Html => format!(
                "<table{}>{}{}</table>",
                styles::table(style),
                styles::columns(style),
                rendered.join("")
            ),
            TableFormat::Markdown => {
                rendered.insert(
                    usize::from(!rendered.is_empty()),
                    format!("| {} |", vec!["---"; width].join(" | ")),
                );
                rendered.join("\n")
            }
            TableFormat::Csv => rendered.join("\n"),
        })
    }
    fn block(&mut self, block: &DocumentBlock) -> Result<String, String> {
        if let Some(content) = self.content.get(&block.id) {
            return Ok(content.clone());
        }
        let d = block.details.to_json_value();
        let o = self.request.overrides.get(&block.id);
        let content = if let Some(o) = o
            .filter(|o| o.replace_content && !self.request.revision_inlines.contains_key(&block.id))
        {
            o.html.clone().unwrap_or_else(|| {
                format!(
                    "<p>{}</p>",
                    escape_html(&label(
                        o.text.as_deref().unwrap_or(""),
                        &d,
                        o.list_label_present
                    ))
                    .replace('\n', "<br>")
                )
            })
        } else if block.r#type == BlockType::Table {
            self.table(block, &d)?
        } else if matches!(
            block.r#type,
            BlockType::Figure | BlockType::Watermark | BlockType::Barcode | BlockType::Formula
        ) {
            let image_url = |field: &str| {
                d[format!("{field}Url")].as_str().or_else(|| {
                    d[format!("{field}AssetId")]
                        .as_str()
                        .and_then(|id| self.request.assets.get(id).map(String::as_str))
                })
            };
            let primary = image_url("image").unwrap_or("");
            let url =
                if primary.to_ascii_lowercase().starts_with("data:image/") && safe_image(primary) {
                    primary
                } else {
                    image_url("previewImage").unwrap_or(primary)
                };
            let width = d["width"].as_f64().filter(|n| n.is_finite() && *n > 0.);
            let height = d["height"].as_f64().filter(|n| n.is_finite() && *n > 0.);
            let size = width
                .zip(height)
                .map(|(w, h)| format!(" width=\"{w}\" height=\"{h}\""))
                .unwrap_or_default();
            let image = if !o.is_some_and(|o| o.suppress_image) && safe_image(url) {
                format!(
                    "<img src=\"{}\" alt=\"{}\"{size} />",
                    escape_html(url),
                    escape_html(d["alt"].as_str().unwrap_or(""))
                )
            } else {
                String::new()
            };
            let text = o.and_then(|o| o.text.as_deref()).unwrap_or(&block.content);
            let caption = o.and_then(|o| o.html.clone()).unwrap_or_else(|| {
                if text.is_empty() {
                    String::new()
                } else {
                    format!("<p>{}</p>", escape_html(text).replace('\n', "<br>"))
                }
            });
            image + &caption
        } else if d["kind"] == "story"
            || matches!(block.r#type, BlockType::TableCell | BlockType::TableHead)
        {
            block
                .children
                .iter()
                .map(|c| self.block(c))
                .collect::<Result<Vec<_>, _>>()?
                .join("\n")
        } else {
            let tag = if matches!(block.r#type, BlockType::Heading | BlockType::SectionHeading) {
                format!("h{}", number(&d, "headingLevel", 1).clamp(1, 6))
            } else {
                "p".into()
            };
            let content = if let Some(changed) = self.changed(block, &d, false)? {
                changed
            } else if let Some(inlines) = d["inlines"].as_array() {
                inlines
                    .iter()
                    .map(|i| self.inline(i, block, &d, false))
                    .collect::<Result<Vec<_>, _>>()?
                    .join("")
            } else {
                styles::text_html(&block.content, &d["field"]["style"], None)
            };
            format!(
                "<{tag}{}>{}{content}</{tag}>",
                styles::paragraph(&d["style"]),
                escape_html(&label("", &d, false))
            )
        };
        self.bytes = self.bytes.saturating_add(content.len());
        if self.bytes > self.request.options.max_output_bytes {
            return Err("LIMIT: exported content".into());
        }
        self.content.insert(block.id.clone(), content.clone());
        Ok(content)
    }
}

/// Export parsed blocks or caller-selected projections without depending on an application schema.
pub fn export_document(request: &ExportRequest) -> Result<ExportedDocument, String> {
    let mut pending: Vec<_> = request.blocks.iter().map(|b| (b, 0)).collect();
    let mut ids = HashSet::new();
    let mut all = Vec::new();
    while let Some((block, depth)) = pending.pop() {
        if depth > request.options.max_depth || ids.len() >= request.options.max_blocks {
            return Err("LIMIT: export structure".into());
        }
        if !ids.insert(block.id.as_str()) {
            return Err("Duplicate export block ID".into());
        }
        if block.r#type == BlockType::Table {
            let d = block.details.to_json_value();
            let columns = number(&d, "columnCount", 1).max(1);
            let rows = number(&d, "rowCount", 0);
            if columns > request.options.max_table_columns
                || rows.saturating_mul(columns) > request.options.max_table_cells
                || cells(block).iter().any(|c| {
                    let details = c.details.to_json_value();
                    number(&details, "columnIndex", 0) >= columns
                        || number(&details, "rowIndex", 0) >= rows
                })
            {
                return Err("LIMIT: export table".into());
            }
        }
        all.push(block);
        pending.extend(block.children.iter().map(|b| (b, depth + 1)));
    }
    let children = all
        .iter()
        .map(|b| {
            (
                b.id.as_str(),
                b.children.iter().map(|c| (c.id.as_str(), c)).collect(),
            )
        })
        .collect();
    let mut exporter = Exporter {
        request,
        content: BTreeMap::new(),
        children,
        bytes: 0,
    };
    let content = request
        .blocks
        .iter()
        .map(|b| exporter.block(b))
        .collect::<Result<Vec<_>, _>>()?
        .join("\n\n");
    for block in all {
        exporter.block(block)?;
    }
    Ok(ExportedDocument {
        content,
        block_content: exporter.content,
    })
}

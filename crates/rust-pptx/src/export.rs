mod styles;

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{extraction_limits::check_serialized_size, PresentationBlock};
use rust_office_art::export::safe_image;
pub use rust_office_art::export::{escape_html, ContentOverride, ExportOptions, TableFormat};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExportRequest {
    pub blocks: Vec<PresentationBlock>,
    pub options: ExportOptions,
    pub overrides: BTreeMap<String, ContentOverride>,
    pub assets: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedPresentation {
    pub content: String,
    pub block_content: BTreeMap<String, String>,
}

fn number(details: &Value, key: &str, default: usize) -> usize {
    details[key]
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(default)
}

fn dimensions(block: &PresentationBlock) -> (usize, usize) {
    let rows = block
        .children
        .iter()
        .map(|cell| {
            number(&cell.details, "rowIndex", 0).saturating_add(number(&cell.details, "rowSpan", 1))
        })
        .max()
        .unwrap_or(0);
    let columns = block
        .children
        .iter()
        .map(|cell| {
            number(&cell.details, "columnIndex", 0).saturating_add(number(
                &cell.details,
                "columnSpan",
                1,
            ))
        })
        .max()
        .unwrap_or(0);
    (
        number(&block.details, "rowCount", rows),
        number(&block.details, "columnCount", columns),
    )
}

fn label(text: &str, details: &Value, already_present: bool) -> String {
    let Some(label) = details["listLabel"]
        .as_str()
        .filter(|label| !label.is_empty())
    else {
        return text.into();
    };
    if already_present
        && text.trim_start().strip_prefix(label).is_some_and(|rest| {
            rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace)
        })
    {
        return text.into();
    }
    format!("{label} {text}")
}

struct Exporter<'a> {
    request: &'a ExportRequest,
    content: BTreeMap<String, String>,
    bytes: usize,
}

impl Exporter<'_> {
    fn append(&self, output: &mut String, text: &str) -> Result<(), String> {
        if output.len().saturating_add(text.len())
            > self
                .request
                .options
                .max_output_bytes
                .saturating_sub(self.bytes)
        {
            return Err("LIMIT: exported content".into());
        }
        output.push_str(text);
        Ok(())
    }

    fn text(&self, block: &PresentationBlock) -> Result<String, String> {
        if let Some(value) = self
            .request
            .overrides
            .get(&block.id)
            .and_then(|o| o.text.as_ref().map(|text| (o, text)))
        {
            return Ok(label(value.1, &block.details, value.0.list_label_present));
        }
        if block.children.is_empty() || block.details["kind"] == "paragraph" {
            return Ok(label(&block.content, &block.details, false));
        }
        let mut result = String::new();
        for child in &block.children {
            if !result.is_empty() {
                self.append(&mut result, "\n")?;
            }
            self.append(&mut result, &self.text(child)?)?;
        }
        Ok(result)
    }

    fn paragraph(&self, block: &PresentationBlock) -> Result<String, String> {
        let mut content = String::new();
        if let Some(label) = block.details["listLabel"].as_str() {
            self.append(
                &mut content,
                &styles::text_html(&format!("{label} "), &block.details["listLabelStyle"], None),
            )?;
        }
        if let Some(inlines) = block.details["inlines"].as_array() {
            for inline in inlines {
                let text = inline["text"].as_str().unwrap_or("");
                let math = inline["math"]["sourceXml"]
                    .as_str()
                    .and_then(rust_office_art::parse_office_math);
                let html = math.map(|math| math.math_ml).unwrap_or_else(|| {
                    styles::text_html(text, &inline["style"], inline["link"].as_str())
                });
                self.append(&mut content, &html)?;
            }
        } else {
            self.append(
                &mut content,
                &styles::text_html(&block.content, &Value::Null, None),
            )?;
        }
        Ok(format!(
            "<p{}>{content}</p>",
            styles::paragraph(&block.details["style"])
        ))
    }

    fn image(&self, block: &PresentationBlock) -> String {
        if self
            .request
            .overrides
            .get(&block.id)
            .is_some_and(|value| value.suppress_image)
        {
            return String::new();
        }
        let url = |field: &str| {
            block.details[format!("{field}Url")].as_str().or_else(|| {
                block.details[format!("{field}AssetId")]
                    .as_str()
                    .and_then(|id| self.request.assets.get(id).map(String::as_str))
            })
        };
        let image = url("image")
            .filter(|url| safe_image(url))
            .or_else(|| url("previewImage").filter(|url| safe_image(url)));
        image
            .map(|url| {
                format!(
                    "<img src=\"{}\" alt=\"{}\" />",
                    escape_html(url),
                    escape_html(block.details["alt"].as_str().unwrap_or(""))
                )
            })
            .unwrap_or_default()
    }

    fn table(&mut self, block: &PresentationBlock) -> Result<String, String> {
        let (rows, columns) = dimensions(block);
        let format = self.request.options.table_format;
        let cells: BTreeMap<_, _> = block
            .children
            .iter()
            .map(|cell| {
                (
                    (
                        number(&cell.details, "rowIndex", 0),
                        number(&cell.details, "columnIndex", 0),
                    ),
                    cell,
                )
            })
            .collect();
        let mut remaining = vec![0usize; columns];
        let mut output = String::new();
        if format == TableFormat::Html {
            self.append(&mut output, "<table style=\"border-collapse:collapse\">")?;
            if let Some(widths) = block.details["columnWidths"]
                .as_array()
                .filter(|widths| widths.len() == columns)
            {
                self.append(&mut output, "<colgroup>")?;
                for width in widths {
                    let style = width
                        .as_u64()
                        .map(|v| format!(" style=\"width:{}pt\"", v as f64 / 12700.))
                        .unwrap_or_default();
                    self.append(&mut output, &format!("<col{style} />"))?;
                }
                self.append(&mut output, "</colgroup>")?;
            }
        }
        for row in 0..rows {
            if format == TableFormat::Html {
                self.append(
                    &mut output,
                    &format!("<tr{}>", styles::row(&block.details["rowHeights"][row])),
                )?;
            } else if row > 0 {
                self.append(&mut output, "\n")?;
            }
            let mut values = Vec::new();
            let mut column = 0;
            while column < columns {
                if remaining[column] > 0 {
                    remaining[column] -= 1;
                    if format != TableFormat::Html {
                        values.push(String::new());
                    }
                    column += 1;
                    continue;
                }
                if let Some(cell) = cells.get(&(row, column)) {
                    let colspan = number(&cell.details, "columnSpan", 1);
                    let rowspan = number(&cell.details, "rowSpan", 1);
                    remaining[column..column + colspan].fill(rowspan - 1);
                    if format == TableFormat::Html {
                        let content = self.block(cell)?;
                        let tag = if cell.details["isHeader"] == true {
                            "th"
                        } else {
                            "td"
                        };
                        self.append(&mut output, &format!("<{tag} colspan=\"{colspan}\" rowspan=\"{rowspan}\"{}>{content}</{tag}>", styles::cell(&cell.details["style"])))?;
                    } else {
                        values.push(self.text(cell)?);
                        values.extend((1..colspan).map(|_| String::new()));
                    }
                    column += colspan;
                } else {
                    if format == TableFormat::Html {
                        self.append(&mut output, "<td></td>")?;
                    } else {
                        values.push(String::new());
                    }
                    column += 1;
                }
            }
            match format {
                TableFormat::Html => self.append(&mut output, "</tr>")?,
                TableFormat::Csv => {
                    for (index, value) in values.iter().enumerate() {
                        if index > 0 {
                            self.append(&mut output, ",")?;
                        }
                        self.append(&mut output, &format!("\"{}\"", value.replace('"', "\"\"")))?;
                    }
                }
                TableFormat::Markdown => {
                    self.append(&mut output, "|")?;
                    for value in values {
                        self.append(
                            &mut output,
                            &format!(
                                " {} |",
                                escape_html(&value)
                                    .replace('|', "\\|")
                                    .replace("\r\n", "\n")
                                    .replace('\n', "<br>")
                            ),
                        )?;
                    }
                    if row == 0 {
                        self.append(&mut output, "\n|")?;
                        for _ in 0..columns {
                            self.append(&mut output, " --- |")?;
                        }
                    }
                }
            }
        }
        if format == TableFormat::Html {
            self.append(&mut output, "</table>")?;
        }
        Ok(output)
    }

    fn block(&mut self, block: &PresentationBlock) -> Result<String, String> {
        if let Some(content) = self.content.get(&block.id) {
            return Ok(content.clone());
        }
        let override_ = self.request.overrides.get(&block.id);
        let content = if let Some(value) = override_.filter(|value| value.replace_content) {
            value.html.clone().unwrap_or_else(|| {
                format!(
                    "<p>{}</p>",
                    escape_html(&label(
                        value.text.as_deref().unwrap_or(""),
                        &block.details,
                        value.list_label_present
                    ))
                    .replace('\n', "<br>")
                )
            })
        } else if block.r#type == "table" {
            self.table(block)?
        } else if block.r#type == "figure" {
            let mut content = self.image(block);
            let text = override_
                .and_then(|value| value.text.as_deref())
                .unwrap_or(&block.content);
            let caption = override_
                .and_then(|value| value.html.clone())
                .unwrap_or_else(|| {
                    if text.is_empty() {
                        String::new()
                    } else {
                        format!("<p>{}</p>", escape_html(text).replace('\n', "<br>"))
                    }
                });
            self.append(&mut content, &caption)?;
            content
        } else if !block.children.is_empty()
            && matches!(
                block.r#type.as_str(),
                "group" | "text_box" | "table_cell" | "note"
            )
        {
            let mut content = String::new();
            for child in &block.children {
                if !content.is_empty() {
                    self.append(&mut content, "\n")?;
                }
                let text = self.block(child)?;
                self.append(&mut content, &text)?;
            }
            content
        } else if block.r#type == "text_box" && block.content.is_empty() {
            self.image(block)
        } else {
            self.paragraph(block)?
        };
        self.bytes = self.bytes.saturating_add(content.len());
        if self.bytes > self.request.options.max_output_bytes {
            return Err("LIMIT: exported content".into());
        }
        self.content.insert(block.id.clone(), content.clone());
        Ok(content)
    }
}

pub fn export_presentation(request: &ExportRequest) -> Result<ExportedPresentation, String> {
    let mut pending: Vec<_> = request.blocks.iter().map(|block| (block, 0)).collect();
    let mut all = Vec::new();
    let mut ids = HashSet::new();
    while let Some((block, depth)) = pending.pop() {
        if depth > request.options.max_depth || ids.len() >= request.options.max_blocks {
            return Err("LIMIT: export structure".into());
        }
        if !ids.insert(block.id.as_str()) {
            return Err("Duplicate export block ID".into());
        }
        if block.r#type == "table" {
            let (rows, columns) = dimensions(block);
            let total = rows.checked_mul(columns).ok_or("LIMIT: export table")?;
            if columns > request.options.max_table_columns
                || total > request.options.max_table_cells
                || (columns == 0 && rows > request.options.max_table_cells)
            {
                return Err("LIMIT: export table".into());
            }
            let mut occupied = vec![false; total];
            for cell in &block.children {
                let r = number(&cell.details, "rowIndex", 0);
                let c = number(&cell.details, "columnIndex", 0);
                let rs = number(&cell.details, "rowSpan", 1);
                let cs = number(&cell.details, "columnSpan", 1);
                if cell.r#type != "table_cell"
                    || rs == 0
                    || cs == 0
                    || r.saturating_add(rs) > rows
                    || c.saturating_add(cs) > columns
                {
                    return Err("Invalid export table cell".into());
                }
                for row in r..r + rs {
                    for column in c..c + cs {
                        let index = row * columns + column;
                        if occupied[index] {
                            return Err("Overlapping export table cells".into());
                        }
                        occupied[index] = true;
                    }
                }
            }
        }
        all.push(block);
        pending.extend(block.children.iter().map(|child| (child, depth + 1)));
        if pending.len().saturating_add(ids.len()) > request.options.max_blocks {
            return Err("LIMIT: export structure".into());
        }
    }
    check_serialized_size(request, request.options.max_output_bytes)?;
    let mut exporter = Exporter {
        request,
        content: BTreeMap::new(),
        bytes: 0,
    };
    let mut content = String::new();
    for block in &request.blocks {
        let html = exporter.block(block)?;
        if !content.is_empty() && !html.is_empty() {
            exporter.append(&mut content, "\n\n")?;
        }
        exporter.append(&mut content, &html)?;
    }
    for block in all {
        exporter.block(block)?;
    }
    let result = ExportedPresentation {
        content,
        block_content: exporter.content,
    };
    check_serialized_size(&result, request.options.max_output_bytes)?;
    Ok(result)
}

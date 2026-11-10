//! Shared controls and escaping for semantic Office content export.

use serde::{Deserialize, Serialize};

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
    /// Trusted caller markup, not escaped or sanitized.
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

pub fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn safe_link(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    !value.chars().any(char::is_control)
        && (lower.starts_with("http:")
            || lower.starts_with("https:")
            || lower.starts_with("mailto:")
            || value.starts_with('#'))
}

pub fn safe_image(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    !value.chars().any(char::is_control)
        && (lower.starts_with("http://")
            || lower.starts_with("https://")
            || ["png", "gif", "jpeg", "webp", "svg+xml"]
                .iter()
                .any(|kind| {
                    lower.starts_with(&format!("data:image/{kind};"))
                        || lower.starts_with(&format!("data:image/{kind},"))
                }))
}

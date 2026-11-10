use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{TextField, TextFieldParagraphProperties, TextParagraph, TextRun, TextRunProperties};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParagraphInline {
    Text {
        start: usize,
        end: usize,
        text: String,
        style: Option<TextRunProperties>,
        link: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        field: Option<Box<TextField>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        math: Option<Box<rust_office_art::OfficeMath>>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParagraphDetails {
    pub r#type: String,
    pub kind: String,
    pub style: TextFieldParagraphProperties,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_label_style: Option<TextRunProperties>,
    pub inlines: Vec<ParagraphInline>,
}

pub(crate) fn run_style(run: &TextRun) -> TextRunProperties {
    TextRunProperties {
        font_family: run.font_family.clone(),
        east_asian_font_family: run.east_asian_font_family.clone(),
        complex_script_font_family: run.complex_script_font_family.clone(),
        symbol_font_family: run.symbol_font_family.clone(),
        font_size_pt: run.font_size_pt,
        bold: run.bold,
        italic: run.italic,
        underline: run.underline,
        strike: run.strike,
        color: run.color.clone(),
        baseline: run.baseline,
        language: run.language.clone(),
        alternative_language: run.alternative_language.clone(),
        right_to_left: run.right_to_left,
        hyperlink: run.hyperlink.clone(),
        character_spacing_pt: run.character_spacing_pt,
        kerning_threshold_pt: run.kerning_threshold_pt,
    }
}

pub(crate) fn paragraph_details(
    paragraph: &TextParagraph,
    counters: &mut BTreeMap<usize, usize>,
) -> ParagraphDetails {
    let mut offset = 0;
    let inlines = paragraph
        .runs
        .iter()
        .map(|run| {
            let start = offset;
            offset += run.text.encode_utf16().count();
            ParagraphInline::Text {
                start,
                end: offset,
                text: run.text.clone(),
                style: Some(run_style(run)),
                link: run.hyperlink.clone(),
                field: run.field.clone(),
                math: run.math.clone(),
            }
        })
        .collect();
    let list_label = paragraph
        .bullet
        .as_ref()
        .and_then(|bullet| match bullet.kind.as_str() {
            "character" => bullet.value.clone(),
            "number" => {
                let level = paragraph.level.unwrap_or(0);
                let value = counters
                    .get(&level)
                    .copied()
                    .unwrap_or(bullet.start_at.unwrap_or(1));
                counters.retain(|key, _| *key <= level);
                counters.insert(level, value.saturating_add(1));
                number_label(value, bullet.value.as_deref().unwrap_or("arabicPeriod"))
            }
            _ => None,
        });
    let list_label_style = paragraph.bullet.as_ref().map(|bullet| {
        let mut style = paragraph.runs.first().map(run_style).unwrap_or_default();
        if let Some(font) = &bullet.font_family {
            style.font_family = Some(font.clone());
        }
        style.font_size_pt = bullet.font_size_pt.or_else(|| {
            style
                .font_size_pt
                .map(|size| size * bullet.size_percent.unwrap_or(1.))
        });
        style.hyperlink = None;
        style
    });
    ParagraphDetails {
        r#type: "text_details".into(),
        kind: "paragraph".into(),
        style: TextFieldParagraphProperties {
            alignment: paragraph.alignment.clone(),
            level: paragraph.level,
            bullet: paragraph.bullet.clone(),
            line_spacing: paragraph.line_spacing,
            space_before: paragraph.space_before,
            space_after: paragraph.space_after,
            rtl: paragraph.rtl,
            margin_left_emu: paragraph.margin_left_emu,
            indent_emu: paragraph.indent_emu,
            default_tab_size_emu: paragraph.default_tab_size_emu,
            tab_stops: paragraph.tab_stops.clone(),
            east_asian_line_break: paragraph.east_asian_line_break,
            latin_line_break: paragraph.latin_line_break,
            hanging_punctuation: paragraph.hanging_punctuation,
            ..Default::default()
        },
        list_label,
        list_label_style,
        inlines,
    }
}

fn number_label(value: usize, scheme: &str) -> Option<String> {
    if value == 0 || value > 32767 {
        return None;
    }
    let (kind, suffix) = ["ParenBoth", "ParenR", "Period", "Plain"]
        .iter()
        .find_map(|suffix| scheme.strip_suffix(suffix).map(|kind| (kind, *suffix)))?;
    let mut text = match kind {
        "arabic" => value.to_string(),
        "alphaLc" | "alphaUc" => {
            let mut chars = Vec::new();
            let mut remaining = value;
            while remaining > 0 {
                chars.push((b'A' + ((remaining - 1) % 26) as u8) as char);
                remaining = (remaining - 1) / 26;
            }
            chars.iter().rev().collect()
        }
        "romanLc" | "romanUc" => {
            let mut remaining = value;
            let mut result = String::new();
            for (number, digits) in [
                (1000, "M"),
                (900, "CM"),
                (500, "D"),
                (400, "CD"),
                (100, "C"),
                (90, "XC"),
                (50, "L"),
                (40, "XL"),
                (10, "X"),
                (9, "IX"),
                (5, "V"),
                (4, "IV"),
                (1, "I"),
            ] {
                while remaining >= number {
                    result.push_str(digits);
                    remaining -= number;
                }
            }
            result
        }
        _ => return None,
    };
    if kind.ends_with("Lc") {
        text.make_ascii_lowercase();
    }
    Some(match suffix {
        "ParenBoth" => format!("({text})"),
        "ParenR" => format!("{text})"),
        "Period" => format!("{text}."),
        _ => text,
    })
}

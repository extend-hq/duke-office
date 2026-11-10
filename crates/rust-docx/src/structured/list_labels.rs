use std::collections::HashMap;
use std::fmt::Write;

use crate::model::{
    NumberingDefinitionSet, NumberingInstanceDefinition, NumberingLevelDefinition,
    ParagraphChildNode, ParagraphNode, ParagraphStyleDefinition, TextStyle,
};

const LEVELS: usize = 9;
const LEVEL_KEYS: [&str; LEVELS] = ["0", "1", "2", "3", "4", "5", "6", "7", "8"];
const MAX_LABEL_BYTES: usize = 4096;

pub(super) struct ListDefinitions<'a> {
    abstracts: HashMap<i64, &'a [NumberingLevelDefinition]>,
    instances: HashMap<i64, &'a NumberingInstanceDefinition>,
    paragraph_run_styles: HashMap<&'a str, &'a TextStyle>,
}

impl<'a> ListDefinitions<'a> {
    pub fn new(
        definitions: Option<&'a NumberingDefinitionSet>,
        paragraph_styles: &'a [ParagraphStyleDefinition],
    ) -> Self {
        let mut result = Self {
            abstracts: HashMap::new(),
            instances: HashMap::new(),
            paragraph_run_styles: paragraph_styles
                .iter()
                .filter_map(|s| Some((s.id.as_str(), s.run_style.as_ref()?)))
                .collect(),
        };
        if let Some(definitions) = definitions {
            result.abstracts.extend(
                definitions
                    .abstracts
                    .iter()
                    .map(|a| (a.abstract_num_id, a.levels.as_slice())),
            );
            result
                .instances
                .extend(definitions.instances.iter().map(|i| (i.num_id, i)));
        }
        result
    }

    fn level(
        &self,
        instance: &'a NumberingInstanceDefinition,
        index: usize,
    ) -> Option<&'a NumberingLevelDefinition> {
        let key = index as i64;
        instance
            .level_overrides
            .iter()
            .flatten()
            .find(|l| l.ilvl == key)
            .or_else(|| {
                self.abstracts
                    .get(&instance.abstract_num_id)?
                    .iter()
                    .find(|l| l.ilvl == key)
            })
    }

    pub fn label_style(&self, paragraph: &ParagraphNode) -> Option<TextStyle> {
        let paragraph_style = paragraph.style.as_ref()?;
        let num = paragraph_style.numbering.as_ref()?;
        let index = usize::try_from(num.ilvl).ok().filter(|&i| i < LEVELS)?;
        let level = self.level(self.instances.get(&num.num_id)?, index)?;
        let base = paragraph
            .children
            .iter()
            .find_map(|child| match child {
                ParagraphChildNode::Text(run) => run.style.as_deref(),
                ParagraphChildNode::FormField(field) => field.style.as_deref(),
                _ => None,
            })
            .or_else(|| {
                self.paragraph_run_styles
                    .get(paragraph_style.style_id.as_deref()?)
                    .copied()
            });
        let mut style = crate::parse::merge_text_styles(base.cloned(), level.run_style.clone());
        if level.format.as_deref() == Some("bullet") {
            if let Some(font) = &level.bullet_font_family {
                let style = style.get_or_insert_with(Default::default);
                style.font_family = Some(font.clone());
                style.font_family_ascii = Some(font.clone());
                style.resolved_font_family_ascii = Some(font.clone());
            }
            if let Some(color) = &level.bullet_color {
                style.get_or_insert_with(Default::default).color = Some(color.clone());
            }
        }
        style
    }
}

#[derive(Default)]
pub(super) struct StoryLists {
    counters: HashMap<i64, [Option<u64>; LEVELS]>,
    initialized_levels: HashMap<i64, [bool; LEVELS]>,
    abstract_counters: HashMap<i64, [Option<u64>; LEVELS]>,
    follows_separator: bool,
}

impl StoryLists {
    pub fn break_paragraph_flow(&mut self) {
        self.follows_separator = false;
    }

    pub fn label(
        &mut self,
        paragraph: &ParagraphNode,
        definitions: &ListDefinitions,
    ) -> Option<String> {
        let follows_separator = self.follows_separator;
        self.follows_separator = paragraph
            .source_paragraph_mark_properties_xml
            .as_deref()
            .and_then(|xml| crate::xml::direct_child_tag(xml, "w:specVanish"))
            .and_then(|tag| crate::xml::parse_on_off_attribute(tag, "specVanish"))
            == Some(true);

        let num = paragraph.style.as_ref()?.numbering.as_ref()?;
        if num.num_id <= 0 {
            return None;
        }
        let index = usize::try_from(num.ilvl).ok().filter(|&i| i < LEVELS)?;
        let instance = definitions.instances.get(&num.num_id)?;
        let level = definitions.level(instance, index)?;
        let counters = self.counters.entry(num.num_id).or_default();
        let shared = self
            .abstract_counters
            .entry(instance.abstract_num_id)
            .or_default();
        let initialized = self.initialized_levels.entry(num.num_id).or_default();
        let start = |i: usize, include_override: bool| {
            include_override
                .then(|| {
                    instance
                        .level_start_overrides
                        .as_ref()
                        .and_then(|s| s.get(LEVEL_KEYS[i]).copied())
                })
                .flatten()
                .or_else(|| definitions.level(instance, i)?.start)
                .and_then(|n| u64::try_from(n).ok())
                .unwrap_or(1)
        };
        let restart_trigger = |i: usize| {
            definitions
                .level(instance, i)
                .and_then(|l| l.lvl_restart)
                .filter(|&r| (0..=i as i64).contains(&r))
                .unwrap_or(i as i64)
        };
        // Related instances share outline progress, but explicit starts apply on first use.
        let mut parent_changed = false;
        for (i, count) in counters[..index].iter_mut().enumerate() {
            if let Some(value) = shared[i] {
                parent_changed |= *count != Some(value) && (i as i64) < restart_trigger(index);
                *count = Some(value);
            } else {
                count.get_or_insert_with(|| start(i, true));
            }
        }
        let current = if parent_changed {
            None
        } else {
            counters[index]
        };
        let first_use = !initialized[index];
        let has_start_override = instance
            .level_start_overrides
            .as_ref()
            .is_some_and(|s| s.contains_key(LEVEL_KEYS[index]))
            || instance
                .level_overrides
                .iter()
                .flatten()
                .any(|l| l.ilvl == num.ilvl && l.start.is_some());
        let continued = if first_use && current.is_none() && has_start_override {
            None
        } else {
            shared[index].or(current)
        };
        counters[index] =
            Some(continued.map_or_else(|| start(index, first_use), |n| n.saturating_add(1)));
        initialized[index] = true;
        shared[..=index].copy_from_slice(&counters[..=index]);
        for (i, count) in counters.iter_mut().enumerate().skip(index + 1) {
            if (index as i64) < restart_trigger(i) {
                *count = None;
                shared[i] = None;
            }
        }
        // A style separator joins the next paragraph to this one. Both advance
        // their counters, but Word prints only the first paragraph's marker.
        if follows_separator
            || level.picture_bullet.is_some()
            || level.format.as_deref() == Some("none")
        {
            return None;
        }
        let template = level.text.as_deref()?;
        let mut chars = template.chars().peekable();
        let mut label = String::new();
        while let Some(ch) = chars.next() {
            if ch == '%' && chars.peek().is_some_and(|c| matches!(c, '1'..='9')) {
                let i = chars.next()? as usize - '1' as usize;
                if i <= index {
                    let fmt = if level.is_lgl == Some(true) {
                        Some("decimal")
                    } else {
                        definitions.level(instance, i)?.format.as_deref()
                    };
                    write_counter(&mut label, counters[i]?, fmt)?;
                }
            } else {
                label.push(match (level.format.as_deref(), ch) {
                    (Some("bullet"), '\u{f0b7}') => '•',
                    (Some("bullet"), '\u{f0a7}') => '□',
                    (Some("bullet"), '\u{f0a8}') => '▪',
                    (Some("bullet"), '\u{f0d8}') => '➢',
                    _ => ch,
                });
            }
            if label.len() > MAX_LABEL_BYTES {
                return None;
            }
        }
        (!label.trim().is_empty()).then_some(label)
    }
}

fn write_counter(out: &mut String, n: u64, format: Option<&str>) -> Option<()> {
    match format.unwrap_or("decimal") {
        "decimal" => write!(out, "{n}").ok()?,
        "decimalZero" => write!(out, "{n:02}").ok()?,
        "lowerLetter" | "upperLetter" if n > 0 => {
            let base = if format == Some("lowerLetter") {
                b'a'
            } else {
                b'A'
            };
            let letter = (base + ((n - 1) % 26) as u8) as char;
            let count = (n - 1) / 26 + 1;
            if count > MAX_LABEL_BYTES.saturating_sub(out.len()) as u64 {
                return None;
            }
            out.extend(std::iter::repeat_n(letter, count as usize));
        }
        "lowerRoman" | "upperRoman" if n > 0 => {
            let mut remaining = n;
            let offset = out.len();
            for (value, digits) in [
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
                if remaining / value
                    > MAX_LABEL_BYTES.saturating_sub(out.len()) as u64 / digits.len() as u64
                {
                    return None;
                }
                for _ in 0..remaining / value {
                    out.push_str(digits);
                }
                remaining %= value;
            }
            if format == Some("lowerRoman") {
                out[offset..].make_ascii_lowercase();
            }
        }
        "lowerLetter" | "upperLetter" | "lowerRoman" | "upperRoman" => out.push('0'),
        "none" => {}
        _ => return None,
    }
    Some(())
}

use std::io::{self, Write};

use serde::{Deserialize, Serialize};

use crate::{BlockNodeInput, PresentationBlockInput, PresentationDocument, SlideNode};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExtractionLimits {
    pub max_depth: usize,
    pub max_blocks: usize,
    pub max_words: usize,
    pub max_slides: usize,
    pub max_table_columns: usize,
    pub max_table_cells: usize,
    pub max_assets: usize,
    pub max_asset_bytes: usize,
    pub max_output_bytes: usize,
}

impl Default for ExtractionLimits {
    fn default() -> Self {
        Self {
            max_depth: 64,
            max_blocks: 100_000,
            max_words: 1_000_000,
            max_slides: 10_000,
            max_table_columns: 1024,
            max_table_cells: 1_000_000,
            max_assets: 10_000,
            max_asset_bytes: 128 * 1024 * 1024,
            max_output_bytes: 128 * 1024 * 1024,
        }
    }
}

pub(crate) struct StructureBudget<'a> {
    limits: &'a ExtractionLimits,
    blocks: usize,
    text_bytes: usize,
}

impl<'a> StructureBudget<'a> {
    pub fn new(limits: &'a ExtractionLimits) -> Self {
        Self {
            limits,
            blocks: 0,
            text_bytes: 0,
        }
    }

    pub fn block(&mut self, depth: usize) -> Result<(), String> {
        self.blocks = self.blocks.saturating_add(1);
        if depth > self.limits.max_depth || self.blocks > self.limits.max_blocks {
            return Err("LIMIT: extraction structure".into());
        }
        Ok(())
    }

    pub fn text(&mut self, text: &str, depth: usize) -> Result<(), String> {
        self.text_bytes = self
            .text_bytes
            .saturating_add(text.len().saturating_mul(depth + 1));
        if self.text_bytes > self.limits.max_output_bytes {
            return Err("LIMIT: extraction text".into());
        }
        Ok(())
    }

    pub fn table(&self, rows: usize, columns: usize) -> Result<(), String> {
        if rows > self.limits.max_table_cells
            || columns > self.limits.max_table_columns
            || rows
                .checked_mul(columns)
                .is_none_or(|cells| cells > self.limits.max_table_cells)
        {
            return Err("LIMIT: extraction table".into());
        }
        Ok(())
    }
}

pub(crate) fn validate_model(
    document: &PresentationDocument,
    limits: &ExtractionLimits,
    notes: bool,
) -> Result<(), String> {
    if document.slides.len() > limits.max_slides {
        return Err("LIMIT: extraction slides".into());
    }
    let mut budget = StructureBudget::new(limits);
    let mut pending = Vec::new();
    for slide in &document.slides {
        pending.extend(slide.nodes.iter().map(|node| (node, 0)));
        if notes {
            for note in &slide.notes {
                budget.block(0)?;
                budget.text(&note.text, 0)?;
                for paragraph in &note.paragraphs {
                    budget.block(1)?;
                    for run in &paragraph.runs {
                        budget.text(&run.text, 1)?;
                    }
                }
            }
        }
    }
    while let Some((node, depth)) = pending.pop() {
        budget.block(depth)?;
        match node {
            SlideNode::Group { children, .. } => {
                pending.extend(children.iter().map(|child| (child, depth + 1)))
            }
            SlideNode::Shape { paragraphs, .. } => {
                for paragraph in paragraphs {
                    budget.block(depth + 1)?;
                    for run in &paragraph.runs {
                        budget.text(&run.text, depth + 1)?;
                    }
                }
            }
            SlideNode::Table { rows, .. } => {
                budget.table(rows.len(), rows.iter().map(Vec::len).max().unwrap_or(0))?;
                for cell in rows.iter().flatten() {
                    if cell
                        .merge
                        .as_ref()
                        .is_some_and(|m| m.horizontal || m.vertical)
                    {
                        continue;
                    }
                    budget.block(depth + 1)?;
                    for paragraph in &cell.paragraphs {
                        budget.block(depth + 2)?;
                        for run in &paragraph.runs {
                            budget.text(&run.text, depth + 2)?;
                        }
                    }
                }
            }
            SlideNode::Chart {
                title: Some(title), ..
            } => budget.text(title, depth)?,
            _ => {}
        }
        if pending.len().saturating_add(budget.blocks) > limits.max_blocks {
            return Err("LIMIT: extraction structure".into());
        }
    }
    Ok(())
}

pub(crate) fn validate_input(
    document: &PresentationBlockInput,
    limits: &ExtractionLimits,
    notes: bool,
) -> Result<(), String> {
    if document.slides.len() > limits.max_slides {
        return Err("LIMIT: extraction slides".into());
    }
    let mut budget = StructureBudget::new(limits);
    let mut pending: Vec<(&BlockNodeInput, usize)> = Vec::new();
    for slide in &document.slides {
        pending.extend(slide.nodes.iter().map(|node| (node, 0)));
        if notes {
            for note in &slide.notes {
                budget.block(0)?;
                budget.text(&note.text, 0)?;
                for paragraph in &note.paragraphs {
                    budget.block(1)?;
                    for run in &paragraph.runs {
                        budget.text(&run.text, 1)?;
                    }
                }
            }
        }
    }
    while let Some((node, depth)) = pending.pop() {
        budget.block(depth)?;
        for paragraph in &node.paragraphs {
            budget.block(depth + 1)?;
            for run in &paragraph.runs {
                budget.text(&run.text, depth + 1)?;
            }
        }
        budget.table(
            node.rows.len(),
            node.rows.iter().map(Vec::len).max().unwrap_or(0),
        )?;
        for cell in node.rows.iter().flatten() {
            if cell
                .merge
                .as_ref()
                .is_some_and(|m| m.horizontal || m.vertical)
            {
                continue;
            }
            budget.block(depth + 1)?;
            for paragraph in &cell.paragraphs {
                budget.block(depth + 2)?;
                for run in &paragraph.runs {
                    budget.text(&run.text, depth + 2)?;
                }
            }
        }
        if let Some(title) = &node.title {
            budget.text(title, depth)?;
        }
        pending.extend(node.children.iter().map(|child| (child, depth + 1)));
        if pending.len().saturating_add(budget.blocks) > limits.max_blocks {
            return Err("LIMIT: extraction structure".into());
        }
    }
    Ok(())
}

pub(crate) fn serialized_size(value: &impl Serialize, max: usize) -> Result<usize, String> {
    struct Counter(usize);
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0 = self
                .0
                .checked_sub(bytes.len())
                .ok_or_else(|| io::Error::other("LIMIT: serialized output"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(max);
    serde_json::to_writer(&mut counter, value).map_err(|error| error.to_string())?;
    Ok(max - counter.0)
}

pub(crate) fn check_serialized_size(value: &impl Serialize, max: usize) -> Result<(), String> {
    serialized_size(value, max).map(|_| ())
}

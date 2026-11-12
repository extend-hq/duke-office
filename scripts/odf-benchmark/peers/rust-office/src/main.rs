//! This repository's parsers: `rust-docx` for text documents and `rust-pptx`
//! for presentations and drawings.

#[path = "../../../harness.rs"]
mod harness;

use rust_docx::{BlockDetails, BlockType, DocumentBlock};
use rust_pptx::{ParseLimits, SlideNode, TextParagraph};

/// Main-story text in reading order, the scope of the reference. Blocks
/// carry their story, so header, footer, footnote, endnote and comment
/// stories are left out, as are figures, formulas and text boxes, none of which
/// LibreOffice's text export reliably contains. List and heading numbers are part of it.
fn document_text(bytes: &[u8]) -> Result<String, String> {
    fn push(block: &DocumentBlock, out: &mut String) {
        let body = block.source_refs.first().is_none_or(|source| source.story == "body");
        // Figures, text boxes included, are outside the main story, as in the
        // Word reference benchmark. So are formulas, whose block text is the
        // object's name and not something the reference contains.
        if !body
            || matches!(
                block.r#type,
                BlockType::Header | BlockType::Footer | BlockType::Figure | BlockType::Formula
            )
        {
            return;
        }
        // Read from the typed details: converting every block's details to
        // JSON for one field would time the conversion, not the parser.
        if let BlockDetails::Paragraph(details) = &block.details {
            if let Some(label) = &details.list_label {
                out.push_str(label);
                out.push(' ');
            }
        }
        out.push_str(&block.content);
        out.push('\n');
    }
    // OpenDocument text is built straight into the document model.
    let structured = rust_docx::extract_document_blocks(&rust_docx::parse_odt(bytes)?);
    let mut out = String::new();
    for block in &structured.blocks {
        push(block, &mut out);
    }
    Ok(out)
}

/// Text of every slide object in drawing order, without speaker notes.
fn presentation_text(bytes: &[u8]) -> Result<String, String> {
    fn paragraphs(paragraphs: &[TextParagraph], out: &mut String) {
        for paragraph in paragraphs {
            for run in &paragraph.runs {
                out.push_str(&run.text);
            }
            out.push('\n');
        }
    }
    fn nodes(list: &[SlideNode], out: &mut String) {
        for node in list {
            match node {
                SlideNode::Shape { paragraphs: text, .. } => paragraphs(text, out),
                SlideNode::Group { children, .. } => nodes(children, out),
                // A chart's title, categories and series names are text on the slide.
                SlideNode::Chart { title, series, .. } => {
                    out.extend(title.iter().map(String::as_str));
                    out.push('\n');
                    let categories = series.first().and_then(|series| series.categories.as_ref());
                    for category in categories.into_iter().flatten().filter_map(|category| category.as_str()) {
                        out.push_str(category);
                        out.push('\n');
                    }
                    for name in series.iter().filter_map(|series| series.name.as_deref()) {
                        out.push_str(name);
                        out.push('\n');
                    }
                }
                SlideNode::Table { rows, .. } => {
                    for row in rows {
                        for cell in row {
                            paragraphs(&cell.paragraphs, out);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let document =
        rust_pptx::parse_presentation(bytes, &ParseLimits::server()).map_err(|error| error.to_string())?;
    let mut out = String::new();
    for slide in &document.slides {
        nodes(&slide.nodes, &mut out);
    }
    Ok(out)
}

fn main() {
    harness::run(|bytes, _| match rust_odf::sniff(bytes).map(|kind| kind.family) {
        Some(rust_odf::OdfFamily::Text) => document_text(bytes),
        Some(rust_odf::OdfFamily::Presentation | rust_odf::OdfFamily::Graphics) => presentation_text(bytes),
        _ => Err("not an OpenDocument text, presentation or drawing file".into()),
    });
}

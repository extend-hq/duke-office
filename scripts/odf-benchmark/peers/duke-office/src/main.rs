//! This repository's parsers: `duke-pages` for text documents and `duke-slides`
//! for presentations and drawings.

#[path = "../../../harness.rs"]
mod harness;

fn document_text(bytes: &[u8]) -> Result<String, String> {
    let model = duke_pages::parse_odt(bytes)?;
    Ok(duke_pages::extract_document_blocks(&model).body_text())
}

fn presentation_text(bytes: &[u8]) -> Result<String, String> {
    duke_slides::parse_presentation(bytes, &duke_slides::ParseLimits::server())
        .map(|document| document.body_text())
        .map_err(|error| error.to_string())
}

fn main() {
    harness::run(|bytes, _| match duke_odf::sniff(bytes).map(|kind| kind.family) {
        Some(duke_odf::OdfFamily::Text) => document_text(bytes),
        Some(duke_odf::OdfFamily::Presentation | duke_odf::OdfFamily::Graphics) => presentation_text(bytes),
        _ => Err("not an OpenDocument text, presentation or drawing file".into()),
    });
}

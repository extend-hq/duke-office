use rust_docx::{build_doc_model, package_to_bytes, parse_document_bytes, try_serialize_doc_model};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = args.next().ok_or("Input document is required")?;
    let output = args.next().ok_or("Output model path is required")?;
    let package = parse_document_bytes(&std::fs::read(input)?)?;
    let model = build_doc_model(&package);
    std::fs::write(output, serde_json::to_vec(&model)?)?;
    if let Some(output) = args.next() {
        let mut rebuilt = model;
        for node in &mut rebuilt.nodes {
            if let rust_docx::DocNode::Paragraph(paragraph) = node {
                paragraph.source_xml = None;
                paragraph.source_text_patch = None;
            }
        }
        let exported = try_serialize_doc_model(&rebuilt, Some(&package))?;
        std::fs::write(output, package_to_bytes(&exported)?)?;
    }
    Ok(())
}

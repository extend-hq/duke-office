mod audit;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args[1] == "--audit" {
        let bytes = std::fs::read(&args[2])?;
        std::fs::write(&args[3],serde_json::to_vec(&audit::audit(&bytes)?)?)?;
        return Ok(());
    }
    let bytes = std::fs::read(&args[1])?;
    let model = if rust_docx::is_odf_format(&bytes) && rust_pptx::detect_format(&bytes).is_err() {
        let package = rust_docx::parse_document_bytes(&bytes)?;
        serde_json::to_vec(&rust_docx::build_doc_model(&package))?
    } else {
        serde_json::to_vec(&rust_pptx::parse_presentation(&bytes, &Default::default())?)?
    };
    std::fs::write(&args[2], model)?;
    Ok(())
}

mod audit;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let audit = args.get(1).is_some_and(|arg| arg == "--audit");
    let bundle = args.get(1).is_some_and(|arg| arg == "--bundle");
    if args.len() != if audit || bundle { 4 } else { 3 } {
        return Err("usage: odf-visual [--audit|--bundle] INPUT OUTPUT".into());
    }
    if audit {
        let bytes = std::fs::read(&args[2])?;
        std::fs::write(&args[3], serde_json::to_vec(&audit::audit(&bytes)?)?)?;
        return Ok(());
    }
    if bundle {
        let bytes = std::fs::read(&args[2])?;
        let json = if rust_pptx::detect_format(&bytes).is_ok() {
            serde_json::to_vec(&rust_pptx::parse_presentation_bundle(
                &bytes,
                rust_pptx::ExtractPresentationBundleOptions {
                    include_charts: true,
                    ..Default::default()
                },
            )?)?
        } else {
            serde_json::to_vec(&rust_docx::parse_document_bundle(
                &bytes,
                rust_docx::ExtractDocumentBundleOptions {
                    include_charts: true,
                    ..Default::default()
                },
            )?)?
        };
        std::fs::write(&args[3], json)?;
        return Ok(());
    }
    let bytes = std::fs::read(&args[1])?;
    let model = if rust_docx::is_odf_format(&bytes) && rust_pptx::detect_format(&bytes).is_err() {
        serde_json::to_vec(&rust_docx::parse_odt(&bytes)?)?
    } else {
        serde_json::to_vec(&rust_pptx::parse_presentation(&bytes, &Default::default())?)?
    };
    std::fs::write(&args[2], model)?;
    Ok(())
}

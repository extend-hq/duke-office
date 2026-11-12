#[path = "../../../harness.rs"]
mod harness;

fn main() {
    // The converter picks the format from the file name, so it reads the file itself.
    let converter = docling::DocumentConverter::new();
    harness::run(|_, path| {
        let source = docling::SourceDocument::from_file(path).map_err(|error| error.to_string())?;
        converter
            .convert(source)
            .map(|result| result.document.export_to_markdown())
            .map_err(|error| error.to_string())
    });
}

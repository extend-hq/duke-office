#[path = "../../../harness.rs"]
mod harness;

fn main() {
    harness::run(|bytes, _| {
        let config = docx_to_md::ParserConfig::builder().extract_images(false).build();
        docx_to_md::DocumentContainer::from_bytes(bytes.to_vec(), config)
            .and_then(|document| document.convert_to_md())
            .map_err(|error| error.to_string())
    });
}

#[path = "../../../harness.rs"]
mod harness;

fn main() {
    // The crate reads from a path only, so the file read is inside the timed call.
    harness::run(|_, path| {
        let config = pptx_to_md::ParserConfig::builder().extract_images(false).build();
        pptx_to_md::PresentationContainer::open(path, config)
            .and_then(|mut presentation| presentation.convert_to_md())
            .map_err(|error| error.to_string())
    });
}

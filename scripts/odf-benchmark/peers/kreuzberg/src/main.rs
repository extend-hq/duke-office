#[path = "../../../harness.rs"]
mod harness;

fn main() {
    let mut config = kreuzberg::ExtractionConfig::default();
    config.use_cache = false;
    config.enable_quality_processing = false;
    harness::run(|bytes, _| {
        kreuzberg::extract_bytes_sync(bytes, "application/vnd.oasis.opendocument.text", &config)
            .map(|result| result.content)
            .map_err(|error| error.to_string())
    });
}

//! Measure native chart extraction with generated artwork deferred.
#[path = "harness.rs"]
mod harness;

fn main() {
    harness::run(|bytes, _| {
        let options = rust_pptx::ExtractPresentationBundleOptions {
            include_charts: true,
            include_artwork: false,
            ..Default::default()
        };
        let bundle = rust_pptx::parse_presentation_bundle(bytes, options)?;
        serde_json::to_string(&bundle).map_err(|error| error.to_string())
    });
}

#[path = "../../../harness.rs"]
mod harness;

fn main() {
    let markkit = markkit::Markkit::builder().build();
    harness::run(|bytes, path| {
        let hint = markkit::DocumentHint {
            content_type: None,
            file_name: path.file_name().and_then(|name| name.to_str()),
        };
        markkit
            .extract_document_sync(bytes, hint)
            .map(|extraction| extraction.markdown)
            .map_err(|error| error.to_string())
    });
}

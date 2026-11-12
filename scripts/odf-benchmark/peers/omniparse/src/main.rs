#[path = "../../../harness.rs"]
mod harness;

fn main() {
    harness::run(|bytes, _| {
        match omniparse::extract_from_bytes(bytes, None).map_err(|error| error.to_string())?.content {
            omniparse::Content::Text(text) => Ok(text),
            _ => Err("no text content".to_owned()),
        }
    });
}

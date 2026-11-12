#[path = "../../../harness.rs"]
mod harness;

fn main() {
    harness::run(|bytes, _| {
        anydoc::to_markdown_bytes(bytes, None::<anydoc::Format>).map_err(|error| error.to_string())
    });
}

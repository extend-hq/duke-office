#[path = "../../../harness.rs"]
mod harness;

fn main() {
    harness::run(|bytes, path| {
        let extension = path.extension().and_then(|extension| extension.to_str()).unwrap_or_default();
        if matches!(extension, "odp" | "otp" | "fodp" | "odg" | "otg" | "fodg") {
            litchi::odf::Presentation::from_bytes(bytes.to_vec())
                .and_then(|mut presentation| presentation.text())
                .map_err(|error| error.to_string())
        } else {
            litchi::odf::Document::from_bytes(bytes.to_vec())
                .and_then(|mut document| document.text())
                .map_err(|error| error.to_string())
        }
    });
}

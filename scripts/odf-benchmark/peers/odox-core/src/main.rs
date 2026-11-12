#[path = "../../../harness.rs"]
mod harness;

fn main() {
    // The crate exposes a document model only; this collects every paragraph
    // and heading the model holds.
    fn walk(element: &odox_core::Element, out: &mut String) {
        for child in element.elements() {
            if child.is(&odox_core::Ns::Text, "p") || child.is(&odox_core::Ns::Text, "h") {
                out.push_str(&child.plain_text());
                out.push('\n');
            } else {
                walk(child, out);
            }
        }
    }
    harness::run(|bytes, path| {
        let extension = path.extension().and_then(|extension| extension.to_str()).unwrap_or_default();
        let mut out = String::new();
        if matches!(extension, "odp" | "otp" | "fodp" | "odg" | "otg" | "fodg") {
            let presentation = odox_core::doc::Presentation::read(bytes).map_err(|error| error.to_string())?;
            for slide in presentation.slides() {
                for shape in slide.shapes() {
                    walk(shape, &mut out);
                }
            }
        } else {
            let document = odox_core::doc::TextDocument::read(bytes).map_err(|error| error.to_string())?;
            if let Some(body) = document.body() {
                walk(body, &mut out);
            }
        }
        Ok(out)
    });
}

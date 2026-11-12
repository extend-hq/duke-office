#[path = "../../../harness.rs"]
mod harness;

fn main() {
    harness::run(|bytes, _| {
        let book = ebook_rs::odt::OdtBook::parse(bytes, "").map_err(|error| error.to_string())?;
        let mut out = String::new();
        for section in book.sections() {
            out.push_str(&section.plain_text);
            out.push('\n');
        }
        Ok(out)
    });
}

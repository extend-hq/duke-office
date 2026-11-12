#[path = "../../../harness.rs"]
mod harness;
use quarb_text::Block;

fn main() {
    harness::run(|bytes, _| {
        let blocks = quarb_text_odt::blocks(bytes).map_err(|error| error.to_string())?;
        let mut out = String::new();
        for block in &blocks {
            match block {
                Block::Heading { lemma, .. } => out.push_str(lemma),
                Block::Paragraph { text }
                | Block::Dialogue { text, .. }
                | Block::Speech { text, .. }
                | Block::Text { text }
                | Block::Verbatim { text, .. } => out.push_str(text),
                Block::Table { headers, rows, .. } => {
                    for header in headers.iter().flatten() {
                        out.push_str(header);
                        out.push('\t');
                    }
                    for cell in rows.iter().flatten() {
                        out.push_str(&cell.text);
                        out.push('\t');
                    }
                }
                _ => continue,
            }
            out.push('\n');
        }
        Ok(out)
    });
}

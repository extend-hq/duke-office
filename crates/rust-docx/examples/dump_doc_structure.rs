fn main() {
    let path = std::env::args().nth(1).expect("path");
    let bytes = std::fs::read(&path).expect("read");
    let pkg = if path.ends_with(".doc") {
        rust_docx::parse_document_bytes(&bytes).expect("parse")
    } else {
        rust_docx::parse_docx(&bytes).expect("parse_docx")
    };
    let model = rust_docx::build_doc_model(&pkg);
    for (i, node) in model.nodes.iter().enumerate() {
        match node {
            rust_docx::DocNode::Paragraph(p) => {
                let text: String = p.children.iter().filter_map(|c| match c {
                    rust_docx::ParagraphChildNode::Text(r) => Some(r.text.clone()),
                    _ => None,
                }).collect();
                println!("{i:3} P {:?}", &text.chars().take(60).collect::<String>());
            }
            rust_docx::DocNode::Table(t) => {
                println!("{i:3} T rows={} cols(first)={}", t.rows.len(),
                    t.rows.first().map(|r| r.cells.len()).unwrap_or(0));
            }
        }
    }
}

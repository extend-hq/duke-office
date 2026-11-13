fn main() {
    let path = std::env::args().nth(1).expect("path");
    let bytes = std::fs::read(&path).expect("read");
    let pkg = if path.ends_with(".doc") {
        duke_pages::parse_document_bytes(&bytes).expect("parse")
    } else {
        duke_pages::parse_docx(&bytes).expect("parse_docx")
    };
    let model = duke_pages::build_doc_model(&pkg);
    print!("{}", text(&model));
}
fn text(model: &duke_pages::DocModel) -> String {
    let mut out = String::new();
    for node in &model.nodes { walk(node, &mut out); }
    out
}
fn walk(node: &duke_pages::DocNode, out: &mut String) {
    match node {
        duke_pages::DocNode::Paragraph(p) => {
            for c in &p.children {
                if let duke_pages::ParagraphChildNode::Text(r) = c { out.push_str(&r.text); }
            }
            out.push('\n');
        }
        duke_pages::DocNode::Table(t) => {
            for row in &t.rows { for cell in &row.cells { for n in &cell.nodes {
                match n {
                    duke_pages::TableCellContentNode::Paragraph(p) => {
                        for c in &p.children { if let duke_pages::ParagraphChildNode::Text(r) = c { out.push_str(&r.text); } }
                        out.push('\n');
                    }
                    duke_pages::TableCellContentNode::Table(t2) => walk(&duke_pages::DocNode::Table(*t2.clone()), out),
                }
            } } }
        }
    }
}

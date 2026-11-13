use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::error::Error;

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn visit(node: &Value, drawings: &mut Vec<Value>, refs: &mut Vec<Value>) {
    match node {
        Value::Object(obj) => {
            if let Some(part) = obj
                .get("chartPartName")
                .filter(|_| obj.get("type").and_then(Value::as_str) == Some("image"))
            {
                let src = obj.get("src").and_then(Value::as_str);
                drawings.push(json!({
                    "part": part, "width": obj.get("widthPx"), "height": obj.get("heightPx"),
                    "semantic": hash(&serde_json::to_vec(&obj.get("chartData")).unwrap()),
                    "renderedSvg": src.is_some_and(|s| s.starts_with("data:image/svg+xml")),
                    "imageUrl": src.map(|s| hash(s.as_bytes())),
                    "imageNode": hash(&serde_json::to_vec(node).unwrap()),
                }));
            }
            if let Some(part) = obj.get("details").and_then(|o| o.get("chartPartName")) {
                refs.push(
                    json!({"part": part, "id": obj.get("id"), "sourceRefs":obj.get("sourceRefs"),
                                 "figure":hash(&serde_json::to_vec(node).unwrap())}),
                );
            }
            obj.values().for_each(|x| visit(x, drawings, refs));
        }
        Value::Array(list) => list.iter().for_each(|x| visit(x, drawings, refs)),
        _ => (),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let bytes = std::fs::read(std::env::args_os().nth(1).ok_or("document path required")?)?;
    let package = duke_docx::parse_docx(&bytes)?;
    let model = duke_docx::build_doc_model(&package);
    let structured = duke_docx::extract_document_blocks(&model);
    let mut drawings = vec![];
    let mut refs = vec![];
    visit(&serde_json::to_value(&model)?, &mut drawings, &mut refs);
    visit(
        &serde_json::to_value(&structured)?,
        &mut drawings,
        &mut refs,
    );
    println!(
        "{}",
        json!({"docx":hash(&bytes), "drawings":drawings,"figures":refs})
    );
    Ok(())
}

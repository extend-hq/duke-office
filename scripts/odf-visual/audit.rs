use rust_odf::draw::{Shape, ShapeKind};
use rust_odf::style::Scope;
use rust_odf::text::{Block, Inline, Table};
use rust_odf::{Body, Limits, OdfDocument};
use serde_json::{json, Value};

pub fn audit(bytes: &[u8]) -> Result<Value, Box<dyn std::error::Error>> {
    let document = OdfDocument::parse(bytes, &Limits::default())?;
    let mut drawings = Vec::new();
    let mut charts = Vec::new();
    let mut failures = Vec::new();
    visit_document(&document, &mut drawings, &mut charts, &mut failures, 0);
    Ok(
        json!({"kind":format!("{:?}",document.kind), "drawings":drawings,"charts":charts,"failures":failures}),
    )
}
fn visit_document(
    doc: &OdfDocument,
    drawings: &mut Vec<Value>,
    charts: &mut Vec<Value>,
    failures: &mut Vec<String>,
    depth: usize,
) {
    if depth > 16 {
        failures.push("embedded depth limit".into());
        return;
    }
    fn inlines<'a>(values: &'a [Inline], out: &mut Vec<&'a Shape>) {
        for i in values {
            match i {
                Inline::Shape(s) => out.push(s),
                Inline::Span { children, .. } | Inline::Link { children, .. } => {
                    inlines(children, out)
                }
                _ => {}
            }
        }
    }
    fn table<'a>(t: &'a Table, out: &mut Vec<&'a Shape>) {
        out.extend(&t.shapes);
        for r in &t.rows {
            for c in &r.cells {
                blocks(&c.blocks, out);
            }
        }
    }
    fn blocks<'a>(values: &'a [Block], out: &mut Vec<&'a Shape>) {
        for b in values {
            match b {
                Block::Shape(s) => out.push(s),
                Block::Paragraph(p) => inlines(&p.inlines, out),
                Block::List(l) => {
                    for i in &l.items {
                        blocks(&i.blocks, out);
                    }
                }
                Block::Section(s) => blocks(&s.blocks, out),
                Block::Table(t) => table(t, out),
            }
        }
    }
    let mut shapes = Vec::new();
    match &doc.body {
        Body::Pages(pages) => {
            for p in pages {
                shapes.extend(&p.shapes);
            }
        }
        Body::Text(values) => blocks(values, &mut shapes),
        Body::Spreadsheet(tables) => {
            for t in tables {
                table(t, &mut shapes);
            }
        }
        Body::Chart(chart) => {
            let rendered = rust_office_charts::odf::render_chart(
                chart,
                chart.width.map_or(640.0, |v| v.0 as f64 / 9525.0),
                chart.height.map_or(360.0, |v| v.0 as f64 / 9525.0),
            );
            charts.push(json!({"class":chart.class,"rendered":rendered.is_some(),"data":rust_office_charts::odf::extract_chart_data(chart),"categories":chart.categories,"series":chart.series.iter().map(|s|json!({"name":s.name,"values":s.values,"domains":s.domains})).collect::<Vec<_>>()}));
        }
        _ => {}
    }
    while let Some(shape) = shapes.pop() {
        if let ShapeKind::Group(children) = &shape.kind {
            shapes.extend(children);
            continue;
        }
        let rendered = rust_office_art::odf::render_shape(shape, &doc.styles, Scope::Content);
        let custom = if let ShapeKind::Custom(g) = &shape.kind {
            Some(
                json!({"type":g.shape_type,"hasPath":g.path.is_some(),"evaluated":g.evaluated_paths([shape.width.map_or(21600.0,|v|v.0 as f64/360.0),shape.height.map_or(21600.0,|v|v.0 as f64/360.0)],true,true).is_some()}),
            )
        } else {
            None
        };
        drawings.push(json!({"name":shape.name,"kind":format!("{:?}",std::mem::discriminant(&shape.kind)),"rendered":rendered.is_some(),"custom":custom}));
    }
    for object in doc.objects.values() {
        match object.document(&Limits::default()) {
            Ok(sub) => visit_document(&sub, drawings, charts, failures, depth + 1),
            Err(e) => failures.push(e.to_string()),
        }
    }
}

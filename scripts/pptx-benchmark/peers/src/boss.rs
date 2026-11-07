use std::{collections::{BTreeMap, BTreeSet}, io, sync::Arc};
use pptxboss_core::{Document, ChartData, DiagramData, Comment};
use pptxboss_core::model::{Content, RunKind, Shape, SlideContent, TextBody};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

struct SharedBytes(Arc<[u8]>);
impl pptxboss_core::zip::Source for SharedBytes {
    fn len(&self) -> u64 { self.0.len() as u64 }
    fn as_bytes(&self) -> Option<&[u8]> { Some(&self.0) }
    fn read_at(&self, offset: u64, output: &mut [u8]) -> io::Result<()> {
        let invalid = || io::Error::new(io::ErrorKind::UnexpectedEof, "Input range is out of bounds");
        let start = usize::try_from(offset).map_err(|_| invalid())?;
        let end = start.checked_add(output.len()).ok_or_else(invalid)?;
        output.copy_from_slice(self.0.get(start..end).ok_or_else(invalid)?);
        Ok(())
    }
}

struct Slide {
    content: Option<SlideContent>,
    notes: Option<TextBody>,
    charts: BTreeMap<u32, ChartData>,
    diagrams: BTreeMap<u32, DiagramData>,
    comments: Vec<Comment>,
    images: Vec<(u32, String)>,
    links: BTreeMap<String, String>,
    errors: Vec<String>,
    unknown_elements: u32,
    unknown_graphics: Vec<String>,
}

pub struct Materialized {
    document: Document,
    slides: Vec<Slide>,
    images: BTreeMap<String, Vec<u8>>,
    auxiliary: Vec<SlideContent>,
    auxiliary_errors: Vec<String>,
}

fn retain<T>(result: pptxboss_core::Result<T>, errors: &mut Vec<String>) -> Option<T> {
    match result { Ok(value) => Some(value), Err(error) => { errors.push(error.to_string()); None } }
}

pub fn parse(bytes: &Arc<[u8]>) -> Result<Materialized, String> {
    let mut document = Document::from_source(Arc::new(SharedBytes(Arc::clone(bytes)))).map_err(|e| e.to_string())?;
    document.set_threads(1);
    let mut images = BTreeMap::new();
    let mut slides = Vec::new();
    let mut auxiliary_errors = Vec::new();
    retain(document.core_properties(), &mut auxiliary_errors);
    retain(document.app_properties(), &mut auxiliary_errors);
    let mut auxiliary = Vec::new();
    if let Some(package) = document.package() {
        for part in package.parts() {
            if package.content_type_of(&part.name).is_some_and(|kind| [".slideLayout+xml", ".slideMaster+xml", ".notesMaster+xml", ".handoutMaster+xml"].iter().any(|suffix| kind.ends_with(suffix))) {
                if let Some((content, _)) = retain(document.slide_part(&part.name), &mut auxiliary_errors) { auxiliary.push(content); }
            }
        }
    }
    for index in 0..document.slide_count() {
        let mut output = Slide { content: None, notes: None, charts: BTreeMap::new(), diagrams: BTreeMap::new(), comments: Vec::new(), images: Vec::new(), links: BTreeMap::new(), errors: Vec::new(), unknown_elements: 0, unknown_graphics: Vec::new() };
        if let Some(slide) = retain(document.slide(index), &mut output.errors) {
            output.notes = retain(slide.notes(), &mut output.errors).flatten();
            output.comments = retain(slide.comments(), &mut output.errors).unwrap_or_default();
            let mut link_ids = BTreeSet::new();
            for shape in slide.content.walk() {
                match &shape.content {
                    Content::Chart(Some(id)) => { if let Some(chart) = retain(slide.chart(id), &mut output.errors) { output.charts.insert(shape.id, chart); } },
                    Content::Diagram(Some(id)) => { if let Some(diagram) = retain(slide.diagram(id), &mut output.errors) { output.diagrams.insert(shape.id, diagram); } },
                    Content::Text(body) => collect_links(body, &mut link_ids),
                    Content::Table(table) => for row in &table.rows { for cell in &row.cells { collect_links(&cell.body, &mut link_ids); } },
                    _ => (),
                }
            }
            for id in link_ids {
                if let Some(target) = retain(slide.hyperlink_target(&id), &mut output.errors).flatten() { output.links.insert(id, target); }
            }
            if let Some(references) = retain(slide.images(), &mut output.errors) {
                for reference in references {
                    let key = reference.part.clone().unwrap_or_else(|| format!("unresolved:{index}:{}", reference.rel_id));
                    if !images.contains_key(&key) {
                        if let Some(bytes) = retain(slide.image_bytes(&reference), &mut output.errors) { images.insert(key.clone(), bytes); }
                    }
                    output.images.push((reference.shape_id, key));
                }
            }
            output.unknown_elements = slide.report.unknown_elements;
            output.unknown_graphics = slide.report.unknown_graphics;
            output.content = Some(slide.content);
        }
        slides.push(output);
    }
    Ok(Materialized { document, slides, images, auxiliary, auxiliary_errors })
}

fn collect_links(body: &TextBody, ids: &mut BTreeSet<String>) {
    for paragraph in &body.paragraphs { for run in &paragraph.runs { if let Some(id) = &run.props.hyperlink { ids.insert(id.clone()); } } }
}

fn paragraphs(body: &TextBody, links: &BTreeMap<String, String>) -> Vec<Value> {
    body.paragraphs.iter().map(|paragraph| {
        let runs: Vec<_> = paragraph.runs.iter().map(|run| json!({
            "text": if matches!(run.kind, RunKind::LineBreak) { "\n" } else { &run.text },
            "bold": run.props.bold, "italic": run.props.italic,
            "link": run.props.hyperlink.as_ref().and_then(|id| links.get(id)),
        })).collect();
        json!({"text":paragraph.text(),"runs":runs})
    }).collect()
}

fn walk(shapes: &[Shape], grouped: bool, source: &Slide, output: &mut Value) {
    for shape in shapes {
        if !grouped && !matches!(shape.content, Content::Group(..)) {
            if let Some(transform) = shape.transform { output["shapes"].as_array_mut().unwrap().push(json!({"id":shape.id.to_string(),"transform":[transform.x,transform.y,transform.cx,transform.cy]})); }
        }
        match &shape.content {
            Content::Text(body) => output["paragraphs"].as_array_mut().unwrap().extend(paragraphs(body, &source.links)),
            Content::Group(children, _) => walk(children, true, source, output),
            Content::Table(table) => {
                let mut cells = Vec::new();
                for row in &table.rows { for cell in &row.cells {
                    let ps = paragraphs(&cell.body, &source.links);
                    let text: String = ps.iter().map(|p| p["text"].as_str().unwrap()).collect();
                    output["paragraphs"].as_array_mut().unwrap().extend(ps);
                    cells.push(json!({"text":text,"col_span":cell.grid_span,"row_span":cell.row_span}));
                } }
                output["tables"].as_array_mut().unwrap().push(json!(cells));
            },
            Content::Chart(_) => {
                let series: Vec<Vec<Option<f64>>> = source.charts.get(&shape.id).map(|chart| chart.series.iter().map(|series| series.values.iter().map(|value| value.parse::<f64>().ok().filter(|value| value.is_finite())).collect()).collect()).unwrap_or_default();
                output["charts"].as_array_mut().unwrap().push(json!(series));
            },
            Content::Diagram(_) => if let Some(diagram) = source.diagrams.get(&shape.id) {
                for item in &diagram.items { output["paragraphs"].as_array_mut().unwrap().push(json!({"text":item.text,"runs":[{"text":item.text}]})); }
            },
            _ => (),
        }
    }
}

pub fn serialize(document: Materialized) -> Value {
    let images: BTreeMap<_, _> = document.images.iter().map(|(key, bytes)| (key, format!("{:x}", Sha256::digest(bytes)))).collect();
    let slides: Vec<_> = document.slides.iter().map(|slide| {
        let mut value = json!({"paragraphs":[],"tables":[],"images":[],"charts":[],"shapes":[],"notes":[],"comments":[],"hidden":false,"errors":slide.errors,"unknown_elements":slide.unknown_elements,"unknown_graphics":slide.unknown_graphics});
        if let Some(content) = &slide.content { value["hidden"] = json!(!content.show); walk(&content.shapes, false, slide, &mut value); }
        if let Some(body) = &slide.notes { value["notes"] = json!(paragraphs(body, &BTreeMap::new())); }
        value["comments"] = json!(slide.comments.iter().map(|comment| &comment.text).collect::<Vec<_>>());
        value["images"] = json!(slide.images.iter().map(|(_, key)| images.get(key).map_or("missing", String::as_str)).collect::<Vec<_>>());
        value
    }).collect();
    json!({"slides":slides,"size":document.document.presentation().slide_size.as_ref().map(|size| [size.cx,size.cy]),"unresolved_slides":document.document.defects().unresolved_slides,"auxiliary_parts":document.auxiliary.len(),"auxiliary_errors":document.auxiliary_errors,"adapter":"materialized source model; normalization and payload hashing excluded from parse timing"})
}

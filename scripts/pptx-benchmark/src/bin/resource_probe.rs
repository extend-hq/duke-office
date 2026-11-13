use std::{fs, time::Instant};

use serde_json::json;

#[derive(Default)]
struct ModelSizes {
    nodes: usize,
    source_bytes: usize,
    svg_bytes: usize,
}

impl ModelSizes {
    fn include(&mut self, nodes: &[duke_slides::SlideNode]) {
        for node in nodes {
            self.nodes += 1;
            match node {
                duke_slides::SlideNode::Shape { source_xml, shape_svg, .. } => {
                    self.source_bytes += source_xml.as_ref().map_or(0, String::len);
                    self.svg_bytes += shape_svg.as_ref().map_or(0, String::len);
                }
                duke_slides::SlideNode::Image { source_xml, picture_svg, .. } => {
                    self.source_bytes += source_xml.as_ref().map_or(0, String::len);
                    self.svg_bytes += picture_svg.as_ref().map_or(0, String::len);
                }
                duke_slides::SlideNode::Group { children, .. } => self.include(children),
                duke_slides::SlideNode::Chart { chart_xml, chart_svg, .. } => {
                    self.source_bytes += chart_xml.as_ref().map_or(0, String::len);
                    self.svg_bytes += chart_svg.as_ref().map_or(0, String::len);
                }
                duke_slides::SlideNode::Media { source_xml, poster, .. } => {
                    self.source_bytes += source_xml.as_ref().map_or(0, String::len);
                    if let Some(poster) = poster { self.include(std::slice::from_ref(poster)); }
                }
                _ => {}
            }
        }
    }
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let result = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let bytes = fs::read(&args[1]).expect("read input");
            let mut limits = duke_slides::ParseLimits::default();
            if let Some(value) = args.get(2) {
                limits.max_xml_nodes = value.parse().expect("node limit");
            }
            let start = Instant::now();
            match duke_slides::parse_presentation(&bytes, &limits) {
                Ok(document) => {
                    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
                    let mut sizes = ModelSizes::default();
                    for nodes in document.slides.iter().map(|s| s.nodes.as_slice())
                        .chain(document.layouts.iter().map(|s| s.nodes.as_slice()))
                        .chain(document.masters.iter().map(|s| s.nodes.as_slice())) {
                        sizes.include(nodes);
                    }
                    if let Some(path) = args.get(3) {
                        let mut writer = flate2::write::GzEncoder::new(fs::File::create(path).expect("output"), flate2::Compression::fast());
                        serde_json::to_writer(&mut writer, &document).expect("serialize");
                        writer.finish().expect("finish output");
                    }
                    json!({
                    "status": "ok",
                    "elapsed_ms": elapsed_ms,
                    "slides": document.slides.len(),
                    "layouts": document.layouts.len(),
                    "nodes": sizes.nodes,
                    "source_xml_bytes": sizes.source_bytes,
                    "svg_bytes": sizes.svg_bytes,
                    "asset_bytes": document.assets.values().map(|asset| asset.byte_length).sum::<usize>(),
                    "max_xml_nodes": limits.max_xml_nodes,
                })},
                Err(error) => json!({
                    "status": "error",
                    "elapsed_ms": start.elapsed().as_secs_f64() * 1000.0,
                    "error": error.to_string(),
                    "max_xml_nodes": limits.max_xml_nodes,
                }),
            }
        })
        .expect("spawn parse")
        .join()
        .expect("parse completed");
    println!("{result}");
}

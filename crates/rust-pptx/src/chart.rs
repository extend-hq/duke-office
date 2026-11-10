pub use rust_office_charts::{ChartCategory, ChartData, ChartPoint, ChartSeries};

use crate::{PresentationDocument, SlideNode};

/// Read cached chart data without evaluating formulas or rendering artwork.
pub fn extract_chart_data(document: &PresentationDocument, part_name: &str) -> Option<ChartData> {
    let mut pending: Vec<_> = document
        .slides
        .iter()
        .flat_map(|slide| &slide.nodes)
        .collect();
    while let Some(node) = pending.pop() {
        match node {
            SlideNode::Chart {
                chart_part_name: Some(part),
                chart_xml: Some(xml),
                ..
            } if part == part_name => return rust_office_charts::parse_chart_xml(xml),
            SlideNode::Group { children, .. } => pending.extend(children),
            _ => {}
        }
    }
    None
}

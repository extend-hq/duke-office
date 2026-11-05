mod art;
#[cfg(feature = "extended-charts")]
mod bubble_chart;
mod cartesian_chart;
#[cfg(feature = "extended-charts")]
mod chart_3d;
mod chart_colors;
mod chart_drawing;
mod chart_svg;
#[cfg(feature = "extended-charts")]
mod chartex;
mod colors;
mod font_metrics;
#[cfg(feature = "extended-charts")]
mod pie_chart;
#[cfg(feature = "extended-charts")]
mod radar_stock;
mod util;

use crate::{RenderOptions, RenderedChart};
use chart_colors::ChartColors;
use util::escape_xml_text;
type ThemeColorMap = std::collections::HashMap<String, String>;

/// Render a chart part. Returns `None` when the XML cannot be rendered.
pub fn render_chart(xml: &str, options: &RenderOptions<'_>) -> Option<RenderedChart> {
    let document = roxmltree::Document::parse(xml).ok()?;
    let root = document.root_element();
    let data = match options.data {
        Some(data) => data.clone(),
        None => crate::chart::chart_data_from_root(root)?,
    };
    let colors = ChartColors::new(root, options, &data);
    let width = options
        .width
        .filter(|v| v.is_finite() && *v > 0.)
        .unwrap_or(640.);
    let height = options
        .height
        .filter(|v| v.is_finite() && *v > 0.)
        .unwrap_or(360.);
    let svg = if root.has_tag_name((crate::chart::CHARTEX_NS, "chartSpace")) {
        #[cfg(feature = "extended-charts")]
        {
            chartex::render(root, &data, &colors, width, height, options)
        }
        #[cfg(not(feature = "extended-charts"))]
        {
            None
        }
    } else {
        let chart = crate::chart::child(root, "chart")?;
        let plot = crate::chart::child(chart, "plotArea")?;
        let kind = plot.children().find_map(|node| {
            let tag = node.tag_name().name();
            if crate::chart::chart_node(node, tag) {
                tag.strip_suffix("Chart")
            } else {
                None
            }
        })?;
        let render = match kind {
            "bar" | "line" | "area" | "scatter" => cartesian_chart::render,
            #[cfg(feature = "extended-charts")]
            "pie" | "doughnut" | "ofPie" => pie_chart::render,
            #[cfg(feature = "extended-charts")]
            "area3D" | "bar3D" | "line3D" | "pie3D" | "surface" | "surface3D" => chart_3d::render,
            #[cfg(feature = "extended-charts")]
            "radar" | "stock" => radar_stock::render,
            #[cfg(feature = "extended-charts")]
            "bubble" => bubble_chart::render,
            _ => return None,
        };
        let font = options.font_family;
        render(root, &data, width, height, &colors, font).or_else(|| {
            render_empty_chart(
                &data,
                crate::chart::child(chart, "legend").is_some(),
                width,
                height,
                &colors,
            )
        })
    }?;
    let mut svg = art::expand_effects(colors.with_definitions(svg));
    chart_drawing::append(&mut svg, root, options, &colors, width, height);
    Some(RenderedChart { svg, data })
}

fn render_empty_chart(
    chart: &crate::chart::ChartData,
    legend: bool,
    width: f64,
    height: f64,
    colors: &ChartColors,
) -> Option<String> {
    use std::fmt::Write;
    let series = &chart.series;
    if series.is_empty() || series.iter().any(|s| !s.points.is_empty()) {
        return None;
    }
    let left = 46.0;
    let right = width - (width * 0.22).clamp(64.0, 120.0);
    let top = 36.0;
    let bottom = height - 32.0;
    let title = escape_xml_text(chart.title.as_deref().unwrap_or(""));
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" font-family="sans-serif"><rect width="100%" height="100%" fill="white"/><text x="16" y="22" font-size="15" fill="#111827">{title}</text>"##
    );
    for step in 0..=4 {
        let fraction = step as f64 / 4.0;
        let y = bottom - fraction * (bottom - top);
        let value = fraction;
        write!(svg, r##"<path d="M{left} {y}H{right}" stroke="#e5e7eb"/><text x="{}" y="{}" text-anchor="end" font-size="10">{value:.2}</text>"##, left - 5.0, y + 3.0).unwrap();
    }
    write!(
        svg,
        r##"<path d="M{left} {top}V{bottom}H{right}" fill="none" stroke="#9ca3af"/>"##
    )
    .unwrap();
    if legend {
        for (i, s) in series.iter().enumerate() {
            let y = top + i as f64 * 17.0;
            let name = s
                .name
                .as_deref()
                .map(escape_xml_text)
                .unwrap_or_else(|| format!("Series {}", i + 1));
            let color = colors.fill(i, None);
            colors.begin_hit(&mut svg, "legendEntry", i, None);
            write!(svg, r#"<rect x="{}" y="{y}" width="8" height="8" fill="{color}"/><text x="{}" y="{}" font-size="11">{name}</text>"#,right+10.0,right+22.0,y+8.0).unwrap();
            colors.end_hit(&mut svg);
        }
    }
    svg.push_str("</svg>");
    Some(svg)
}

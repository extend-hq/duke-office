//! Render DrawingML chart parts with colors and data provided by the host document.

mod chart;
mod parse;
pub mod odf;

use std::collections::HashMap;

pub use chart::{parse_chart_xml, ChartCategory, ChartData, ChartPoint, ChartSeries};
pub use parse::render_chart;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartDrawing {
    pub xml: String,
    #[serde(default)]
    pub images: HashMap<String, String>,
}

/// Inputs resolved by the host. Missing dimensions default to 640 by 360 CSS pixels.
#[derive(Clone, Debug, Default)]
pub struct RenderOptions<'a> {
    pub width: Option<f64>,
    pub height: Option<f64>,
    /// Resolved theme token names mapped to RGB hex colors.
    pub theme_colors: Option<&'a HashMap<String, String>>,
    pub font_family: Option<&'a str>,
    pub theme_xml: Option<&'a str>,
    pub theme_override_xml: Option<&'a str>,
    pub style_xml: Option<&'a str>,
    pub color_style_xml: Option<&'a str>,
    /// Embedded raster data URLs keyed by relationship ID within the chart part.
    pub images: Option<&'a HashMap<String, String>>,
    /// Saved drawing parts keyed by relationship ID within the chart part.
    pub drawings: Option<&'a HashMap<String, ChartDrawing>>,
    pub legacy_defaults: bool,
    /// Apply spreadsheet host defaults for omitted chart properties.
    pub spreadsheet_defaults: bool,
    /// Reserve native OpenDocument title and legend space in circular charts.
    pub opendocument_defaults: bool,
    /// Draw circular data series counterclockwise from their starting angle.
    pub pie_counterclockwise: bool,
    /// Emit hit targets that identify the source series and data point.
    pub interactive: bool,
    /// Workbook values in chart XML series order. Point indices preserve sparse data.
    pub data: Option<&'a ChartData>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedChart {
    pub svg: String,
    pub data: ChartData,
}

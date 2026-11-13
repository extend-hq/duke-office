use duke_office_charts::{render_chart, ChartData, ChartDrawing, RenderOptions};
use serde::Deserialize;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Options {
    width: Option<f64>,
    height: Option<f64>,
    theme_colors: Option<HashMap<String, String>>,
    font_family: Option<String>,
    theme_xml: Option<String>,
    theme_override_xml: Option<String>,
    style_xml: Option<String>,
    color_style_xml: Option<String>,
    images: Option<HashMap<String, String>>,
    drawings: Option<HashMap<String, ChartDrawing>>,
    legacy_defaults: bool,
    spreadsheet_defaults: bool,
    interactive: bool,
    data: Option<ChartData>,
}

/// Render a chart using serialized host options. Unsupported input has no output.
#[wasm_bindgen(js_name = renderChartSvg)]
pub fn render_chart_svg(xml: &str, options_json: &str) -> Result<Option<String>, JsError> {
    let options: Options = serde_json::from_str(options_json)?;
    let options = RenderOptions {
        width: options.width,
        height: options.height,
        theme_colors: options.theme_colors.as_ref(),
        font_family: options.font_family.as_deref(),
        theme_xml: options.theme_xml.as_deref(),
        theme_override_xml: options.theme_override_xml.as_deref(),
        style_xml: options.style_xml.as_deref(),
        color_style_xml: options.color_style_xml.as_deref(),
        images: options.images.as_ref(),
        drawings: options.drawings.as_ref(),
        legacy_defaults: options.legacy_defaults,
        spreadsheet_defaults: options.spreadsheet_defaults,
        interactive: options.interactive,
        data: options.data.as_ref(),
        ..Default::default()
    };
    Ok(render_chart(xml, &options).map(|chart| chart.svg))
}

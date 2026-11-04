use std::borrow::Borrow;

use extend_office_charts::{render_chart, RenderOptions};
pub use extend_office_charts::{ChartCategory, ChartData, ChartPoint, ChartSeries};

use crate::package::{collect_relationship_targets, relationship_target_from_parts};
use crate::parse::{svg_data_uri, ParseContext, ParsedStyleSheet};
use crate::OoxmlPackage;

pub(crate) const CHARTEX_NS: &str = "http://schemas.microsoft.com/office/drawing/2014/chartex";

/// Read the literal and cached series data for the chart part referenced by a figure.
/// Formula references are not evaluated. A series without cached points remains empty.
pub fn extract_chart_data(package: &OoxmlPackage, part_name: &str) -> Option<ChartData> {
    extend_office_charts::parse_chart_xml(&package.parts.get(part_name)?.content)
}

pub(crate) fn render_chart_part(
    part_name: &str,
    context: &ParseContext<'_, impl Borrow<ParsedStyleSheet>>,
    width: Option<f64>,
    height: Option<f64>,
) -> Option<(String, ChartData)> {
    let parts = context.parts;
    let part = parts.get(part_name)?;
    let chart_targets = collect_relationship_targets(parts, part_name);
    let related = |kind: &str| {
        let target = chart_targets.get(kind)?.as_ref()?;
        Some(parts.get(target)?.content.as_str())
    };

    let package_targets = collect_relationship_targets(parts, "");
    let main_part = package_targets
        .get("officeDocument")
        .and_then(Option::as_deref)
        .unwrap_or("word/document.xml");
    let theme_part = relationship_target_from_parts(parts, main_part, "theme");
    let theme = theme_part
        .as_ref()
        .and_then(|name| parts.get(name))
        .or_else(|| parts.get("word/theme/theme1.xml"));
    let properties = package_targets
        .get("extended-properties")
        .and_then(Option::as_deref)
        .unwrap_or("docProps/app.xml");
    let styles = context.styles();
    let options = RenderOptions {
        width,
        height,
        theme_colors: Some(&styles.theme_colors),
        font_family: styles.theme_fonts.minor_latin.as_deref(),
        theme_xml: theme.map(|part| part.content.as_str()),
        theme_override_xml: related("themeOverride"),
        style_xml: related("chartStyle"),
        color_style_xml: related("chartColorStyle"),
        legacy_defaults: parts
            .get(properties)
            .is_some_and(|part| uses_legacy_chart_defaults(&part.content)),
        ..Default::default()
    };
    let chart = render_chart(&part.content, &options)?;
    Some((svg_data_uri(&chart.svg), chart.data))
}

fn uses_legacy_chart_defaults(properties: &str) -> bool {
    let Ok(xml) = roxmltree::Document::parse(properties) else {
        return false;
    };
    let version = xml
        .root_element()
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "AppVersion")
        .and_then(|node| node.text())
        .and_then(|value| value.trim().split('.').next()?.parse::<u16>().ok());
    matches!(version, Some(1..=12))
}

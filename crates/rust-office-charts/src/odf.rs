//! Map resolved OpenDocument chart semantics onto the shared chart renderer.

use std::fmt::Write;

use rust_odf::chart::Chart;
use rust_odf::style::{Fill, Props, Stroke};

use crate::{RenderOptions, RenderedChart};

fn escaped(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn property<'a>(props: &'a Props, name: &str) -> Option<&'a str> { props.chart.get(name).map(String::as_str) }

fn kind(class: &str) -> Option<&'static str> {
    Some(match class {
        "bar" => "bar", "line" => "line", "area" => "area", "circle" => "pie", "ring" => "doughnut",
        "scatter" => "scatter", "bubble" => "bubble", "radar" | "filled-radar" => "radar", "stock" => "stock",
        _ => return None,
    })
}

fn paint(out: &mut String, props: &Props, default: Option<&str>, chart: &Chart) {
    let p = &props.graphic;
    out.push_str("<c:spPr>");
    if p.fill == Some(Fill::None) { out.push_str("<a:noFill/>"); }
    else if let Some(gradient) = p.fill_gradient.as_ref().and_then(|name| chart.gradients.get(name)).filter(|_| p.fill == Some(Fill::Gradient)) {
        let start = gradient.start_color.as_deref().unwrap_or("FFFFFF");
        let end = gradient.end_color.as_deref().unwrap_or("000000");
        out.push_str("<a:gradFill><a:gsLst>");
        let stops = if gradient.kind == "axial" { vec![(0,end),(50000,start),(100000,end)] } else { vec![(0,start),(100000,end)] };
        for (position,color) in stops {
            let _ = write!(out,"<a:gs pos=\"{position}\"><a:srgbClr val=\"{color}\"/></a:gs>");
        }
        let angle = (90.0-gradient.angle).rem_euclid(360.0)*60000.0;
        let _ = write!(out,"</a:gsLst><a:lin ang=\"{angle}\" scaled=\"1\"/></a:gradFill>");
    }
    else if let Some(color) = p.fill_color.as_deref().or(default) {
        let _ = write!(out, "<a:solidFill><a:srgbClr val=\"{color}\"><a:alpha val=\"{}\"/></a:srgbClr></a:solidFill>", (p.opacity.unwrap_or(1.0).clamp(0.0, 1.0)*100000.0).round());
    }
    let _ = write!(out, "<a:ln w=\"{}\">", p.stroke_width.map_or(0, |w| w.0));
    if p.stroke == Some(Stroke::None) || (p.stroke.is_none() && p.stroke_color.is_none()) { out.push_str("<a:noFill/>"); }
    else {
        let _ = write!(out, "<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>", p.stroke_color.as_deref().or(default).unwrap_or("000000"));
        if p.stroke == Some(Stroke::Dash) { out.push_str("<a:prstDash val=\"dash\"/>"); }
    }
    out.push_str("</a:ln></c:spPr>");
}

fn numbers(out: &mut String, role: &str, values: &[Option<f64>]) {
    let _ = write!(out, "<c:{role}><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val=\"{}\"/>", values.len());
    for (i, value) in values.iter().enumerate() {
        if let Some(v) = value.filter(|v| v.is_finite()) { let _ = write!(out, "<c:pt idx=\"{i}\"><c:v>{v}</c:v></c:pt>"); }
    }
    let _ = write!(out, "</c:numLit></c:{role}>");
}

fn text_properties(out: &mut String, props: &Props, default_size: f64) {
    let size=props.text.points().unwrap_or(default_size)*100.0;
    let _=write!(out,"<c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz=\"{size}\" b=\"{}\" i=\"{}\">",u8::from(props.text.bold.unwrap_or(false)),u8::from(props.text.italic.unwrap_or(false)));
    if let Some(color)=&props.text.color {let _=write!(out,"<a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill>");}
    let _=write!(out,"<a:latin typeface=\"{}\"/></a:defRPr></a:pPr></a:p></c:txPr>",escaped(props.text.font.as_deref().unwrap_or("Liberation Sans")));
}

fn title(out: &mut String, text: &str, props: &Props) {
    let _ = write!(out, "<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>{}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/>", escaped(text));
    text_properties(out,props,14.0);
    out.push_str("</c:title>");
}

/// Builds the shared renderer's input, without converting the host document.
/// Unsupported chart classes return `None` so the host can retain a saved preview.
pub fn chart_xml(chart: &Chart) -> Option<String> {
    let main_kind = kind(&chart.class)?;
    if chart.series.is_empty() { return None; }
    if property(&chart.plot_style, "three-dimensional") == Some("true")
        || chart.series.iter().any(|s| s.attached_axis.as_deref().is_some_and(|a| a != "primary-y")) {
        return None;
    }
    let mut out = String::from("<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">");
    out.push_str("<c:roundedCorners val=\"0\"/>");
    paint(&mut out, &chart.style, Some("FFFFFF"),chart);
    text_properties(&mut out,&chart.style,10.0);
    out.push_str("<c:chart>");
    if let Some(text) = &chart.title { title(&mut out, text,&chart.title_style); } else { out.push_str("<c:autoTitleDeleted val=\"1\"/>"); }
    out.push_str("<c:plotArea>");
    if let (Some([x,y,w,h]), Some(width), Some(height)) = (chart.plot_bounds, chart.width, chart.height) {
        if width.0 > 0 && height.0 > 0 && w.0 > 0 && h.0 > 0 {
            let _ = write!(out, "<c:layout><c:manualLayout><c:layoutTarget val=\"inner\"/><c:xMode val=\"factor\"/><c:yMode val=\"factor\"/><c:wMode val=\"factor\"/><c:hMode val=\"factor\"/><c:x val=\"{}\"/><c:y val=\"{}\"/><c:w val=\"{}\"/><c:h val=\"{}\"/></c:manualLayout></c:layout>", x.0 as f64/width.0 as f64, y.0 as f64/height.0 as f64, w.0 as f64/width.0 as f64, h.0 as f64/height.0 as f64);
        }
    }
    let horizontal = property(&chart.plot_style, "vertical") == Some("true");
    let grouping = if property(&chart.plot_style, "percentage") == Some("true") { "percentStacked" }
        else if property(&chart.plot_style, "stacked") == Some("true") { "stacked" } else { "clustered" };
    let palette = ["004586", "FF420E", "FFD320", "579D1C", "7E0021", "83CAFF", "314004", "AECF00", "4B1F6F", "FF950E", "C5000B", "0084D1"];
    let series_kind = |class: Option<&str>| kind(if chart.class == "ring" && class == Some("circle") { "ring" } else { class.unwrap_or(&chart.class) });
    let mut groups: Vec<&str> = Vec::new();
    for series in &chart.series {
        let k = series_kind(series.class.as_deref())?;
        if !groups.contains(&k) { groups.push(k); }
    }
    for k in groups {
        let _ = write!(out, "<c:{k}Chart>");
        match k {
            "bar" => { let _ = write!(out, "<c:barDir val=\"{}\"/><c:grouping val=\"{grouping}\"/>", if horizontal { "bar" } else { "col" }); }
            "line" | "area" => { let _ = write!(out, "<c:grouping val=\"{}\"/>", if grouping == "clustered" { "standard" } else { grouping }); }
            "scatter" => out.push_str("<c:scatterStyle val=\"lineMarker\"/>"),
            "radar" => { let _ = write!(out, "<c:radarStyle val=\"{}\"/>", if chart.class == "filled-radar" { "filled" } else { "marker" }); }
            _ => {}
        }
        let _ = write!(out, "<c:varyColors val=\"{}\"/>", u8::from(matches!(k, "pie" | "doughnut")));
        for (index, series) in chart.series.iter().enumerate().filter(|(_, s)| series_kind(s.class.as_deref()) == Some(k)) {
            let _ = write!(out, "<c:ser><c:idx val=\"{index}\"/><c:order val=\"{index}\"/>");
            if let Some(name) = &series.name { let _ = write!(out, "<c:tx><c:v>{}</c:v></c:tx>", escaped(name)); }
            let mut style = series.style.clone();
            style.inherit(&chart.plot_style);
            if matches!(k, "line" | "scatter" | "radar") && style.graphic.stroke.is_none() {
                style.graphic.stroke = Some(Stroke::Solid);
                style.graphic.stroke_width = Some(rust_odf::units::Length(12700));
            }
            paint(&mut out, &style, Some(palette[index % palette.len()]),chart);
            if matches!(k, "line" | "scatter" | "radar") {
                let marker = match property(&style, "symbol-name") {
                    Some("square") => "square", Some("diamond") => "diamond", Some("arrow-up") => "triangle",
                    Some("x") => "x", Some("plus") => "plus", _ => "circle",
                };
                let marker = if matches!(property(&style, "symbol-type"), None | Some("none")) { "none" } else { marker };
                let _ = write!(out, "<c:marker><c:symbol val=\"{marker}\"/><c:size val=\"5\"/></c:marker>");
            }
            for (i, point) in series.points.iter().enumerate() {
                if point.graphic == Default::default() && !matches!(k, "pie" | "doughnut") { continue; }
                let _ = write!(out, "<c:dPt><c:idx val=\"{i}\"/>");
                let mut point_style = point.clone();
                point_style.inherit(&series.style);
                if matches!(k, "pie" | "doughnut") && point.graphic.fill_color.is_none() {
                    point_style.graphic.fill_color = Some(palette[i % palette.len()].to_owned());
                }
                paint(&mut out, &point_style, None,chart);
                out.push_str("</c:dPt>");
            }
            if matches!(k, "scatter" | "bubble") {
                let x = if k == "bubble" { series.domains.get(1) } else { series.domains.first() };
                let automatic = (1..=series.values.len()).map(|v| Some(v as f64)).collect::<Vec<_>>();
                numbers(&mut out, "xVal", x.unwrap_or(&automatic));
                numbers(&mut out, "yVal", if k == "bubble" { series.domains.first().unwrap_or(&series.values) } else { &series.values });
                if k == "bubble" { numbers(&mut out, "bubbleSize", &series.values); }
            } else {
                let _ = write!(out, "<c:cat><c:strLit><c:ptCount val=\"{}\"/>", chart.categories.len());
                for (i, category) in chart.categories.iter().enumerate() { let _ = write!(out, "<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", escaped(category)); }
                out.push_str("</c:strLit></c:cat>"); numbers(&mut out, "val", &series.values);
            }
            let _ = write!(out, "<c:smooth val=\"{}\"/></c:ser>", u8::from(property(&style, "interpolation") == Some("cubic-spline")));
        }
        if k == "bar" {
            let gap = property(&chart.plot_style, "gap-width").and_then(|v| v.parse::<u32>().ok()).unwrap_or(100);
            let _ = write!(out, "<c:gapWidth val=\"{gap}\"/><c:overlap val=\"{}\"/>", if grouping == "clustered" { 0 } else { 100 });
        }
        if k == "doughnut" { out.push_str("<c:holeSize val=\"50\"/>"); }
        if matches!(k, "pie" | "doughnut") {
            let angle = property(&chart.plot_style, "angle-offset").and_then(rust_odf::units::parse_number).unwrap_or(90.0);
            let _ = write!(out, "<c:firstSliceAng val=\"{}\"/>", (90.0 - angle).rem_euclid(360.0));
        }
        if !matches!(k, "pie" | "doughnut") { out.push_str("<c:axId val=\"1\"/><c:axId val=\"2\"/>"); }
        let _ = write!(out, "</c:{k}Chart>");
    }
    if !matches!(main_kind, "pie" | "doughnut") {
        for (dimension, id, cross) in [("x", 1, 2), ("y", 2, 1)] {
            let axis = chart.axes.iter().find(|a| a.dimension == dimension);
            let value_axis = dimension == "y" || matches!(main_kind, "scatter" | "bubble");
            let tag = if value_axis { "valAx" } else { "catAx" };
            let _ = write!(out, "<c:{tag}><c:axId val=\"{id}\"/><c:scaling>");
            if let Some(axis) = axis {
                for (source, target) in [("minimum", "min"), ("maximum", "max")] {
                    if let Some(v) = property(&axis.style, source).and_then(rust_odf::units::parse_number) { let _ = write!(out, "<c:{target} val=\"{v}\"/>"); }
                }
                if property(&axis.style, "logarithmic") == Some("true") { out.push_str("<c:logBase val=\"10\"/>"); }
            }
            let reversed = axis.is_some_and(|a| property(&a.style, "reverse-direction") == Some("true")) ^ (main_kind == "radar" && dimension == "x");
            let _ = write!(out, "<c:orientation val=\"{}\"/></c:scaling><c:delete val=\"{}\"/><c:axPos val=\"{}\"/>", if reversed { "maxMin" } else { "minMax" }, u8::from(axis.is_some_and(|a| property(&a.style, "display-label") == Some("false"))), if (dimension == "x") != horizontal { "b" } else { "l" });
            if let Some(axis) = axis {
                if axis.major_grid { out.push_str("<c:majorGridlines/>"); }
                if let Some(text) = &axis.title { title(&mut out, text,&axis.style); }
                let mut style=axis.style.clone();style.text.inherit(&chart.style.text);
                text_properties(&mut out,&style,10.0);
                if let Some(v) = property(&axis.style, "interval-major").and_then(rust_odf::units::parse_number).filter(|v| *v > 0.0) { let _ = write!(out, "<c:majorUnit val=\"{v}\"/>"); }
            }
            let _ = write!(out, "<c:numFmt formatCode=\"General\" sourceLinked=\"0\"/><c:tickLblPos val=\"nextTo\"/><c:majorTickMark val=\"out\"/><c:minorTickMark val=\"none\"/><c:crossAx val=\"{cross}\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"{}\"/></c:{tag}>", if main_kind == "bar" { "between" } else { "midCat" });
        }
    }
    if matches!(main_kind, "bar" | "line" | "area" | "scatter" | "bubble") {
        paint(&mut out, chart.wall_style.as_ref().unwrap_or(&Props::default()), Some("D9D9D9"),chart);
    }
    out.push_str("</c:plotArea>");
    if chart.has_legend {
        let position = match chart.legend_position.as_deref() { Some("start") => "l", Some("top") => "t", Some("bottom") => "b", _ => "r" };
        let _ = write!(out, "<c:legend><c:legendPos val=\"{position}\"/><c:overlay val=\"0\"/>");
        let mut style=chart.legend_style.clone();style.text.inherit(&chart.style.text);
        text_properties(&mut out,&style,10.0);
        paint(&mut out,&style,None,chart);
        out.push_str("</c:legend>");
    }
    out.push_str("<c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart></c:chartSpace>");
    Some(out)
}

pub fn render_chart(chart: &Chart, width: f64, height: f64) -> Option<RenderedChart> {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 || width > 1_000_000.0 || height > 1_000_000.0 { return None; }
    let xml = chart_xml(chart)?;
    let width=chart.width.map(|v|v.0 as f64/9525.0).filter(|v|*v>0.0 && *v<=1_000_000.0).unwrap_or(width);
    let height=chart.height.map(|v|v.0 as f64/9525.0).filter(|v|*v>0.0 && *v<=1_000_000.0).unwrap_or(height);
    crate::render_chart(&xml, &RenderOptions { width: Some(width), height: Some(height), font_family: chart.style.text.font.as_deref(),
        opendocument_defaults: true,
        pie_counterclockwise: !chart.axes.iter().any(|a| a.dimension == "y" && property(&a.style, "reverse-direction") == Some("true")),
        ..Default::default() })
}

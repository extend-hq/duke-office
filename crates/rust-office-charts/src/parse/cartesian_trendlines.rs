use super::{flag, number, val, Coords, Group};
use crate::chart::{chart_node, child, ChartData, ChartSeries};
use crate::parse::chart_colors::ChartColors;
use crate::parse::chart_svg::{
    font_family, font_size, format_value, text, text_props, text_width_for,
};
use roxmltree::Node;
use std::collections::BTreeMap;
use std::fmt::Write;

pub(super) struct LinearFit {
    pub(super) ends: [(f64, f64); 2],
    slope: f64,
    intercept: f64,
    r_squared: Option<f64>,
}

pub(super) fn date_categories(group: &Group, data: &ChartData) -> BTreeMap<u32, f64> {
    let mut categories = BTreeMap::new();
    if !group.category.is_some_and(|n| chart_node(n, "dateAx")) {
        return categories;
    }
    let Some(plot) = group.node.parent() else {
        return categories;
    };
    for source in super::groups(plot, data).unwrap_or_default() {
        if source.category != group.category {
            continue;
        }
        for series in &data.series[source.start..source.end] {
            for point in &series.points {
                if let Some(value) = point
                    .categories
                    .iter()
                    .find(|c| c.level == 0)
                    .and_then(|c| number(Some(&c.label)))
                {
                    categories.entry(point.index).or_insert(value);
                }
            }
        }
    }
    categories
}

pub(super) fn linear_fit(
    series: &ChartSeries,
    trend: Node,
    scatter: bool,
    dates: &BTreeMap<u32, f64>,
) -> Option<LinearFit> {
    let implicit_x = scatter && trend.parent().is_some_and(|n| child(n, "xVal").is_none());
    let points: Vec<_> = series
        .points
        .iter()
        .filter_map(|p| {
            let x = if scatter && !implicit_x {
                number(p.x.as_deref())?
            } else if !dates.is_empty() {
                *dates.get(&p.index)?
            } else {
                f64::from(p.index) + 1.
            };
            Some((
                x,
                number(if scatter {
                    p.y.as_deref()
                } else {
                    p.value.as_deref()
                })?,
                if scatter { x } else { f64::from(p.index) },
            ))
        })
        .collect();
    if points.len() < 2 {
        return None;
    }
    let count = points.len() as f64;
    let mx = points.iter().map(|p| p.0 / count).sum::<f64>();
    let my = points.iter().map(|p| p.1 / count).sum::<f64>();
    let fixed = number(val(Some(trend), "intercept"));
    let (center_x, center_y) = fixed.map_or((mx, my), |v| (0., v));
    let denominator = points.iter().map(|p| (p.0 - center_x).powi(2)).sum::<f64>();
    if denominator <= 0. || !denominator.is_finite() {
        return None;
    }
    let slope = points
        .iter()
        .map(|p| (p.0 - center_x) * (p.1 - center_y))
        .sum::<f64>()
        / denominator;
    let intercept = center_y - slope * center_x;
    let predicted = |x| slope * (x - center_x) + center_y;
    let mean_prediction = predicted(mx);
    let r_squared = if fixed == Some(0.) {
        let total = points.iter().map(|p| p.1.powi(2)).sum::<f64>();
        (total > 0.).then(|| points.iter().map(|p| predicted(p.0).powi(2)).sum::<f64>() / total)
    } else {
        let total = points.iter().map(|p| (p.1 - my).powi(2)).sum::<f64>();
        let predicted_total = points
            .iter()
            .map(|p| (predicted(p.0) - mean_prediction).powi(2))
            .sum::<f64>();
        let covariance = points
            .iter()
            .map(|p| (p.1 - my) * (predicted(p.0) - mean_prediction))
            .sum::<f64>();
        (total > 0. && predicted_total > 0.).then(|| covariance.powi(2) / total / predicted_total)
    }
    .filter(|v| v.is_finite())
    .map(|v| v.clamp(0., 1.));
    let first = points.iter().min_by(|a, b| a.0.total_cmp(&b.0))?;
    let last = points.iter().max_by(|a, b| a.0.total_cmp(&b.0))?;
    let category_scale = (last.2 - first.2) / (last.0 - first.0);
    let backward = number(val(Some(trend), "backward")).unwrap_or(0.).max(0.);
    let forward = number(val(Some(trend), "forward")).unwrap_or(0.).max(0.);
    let ends = [
        (
            first.2 - backward * category_scale,
            predicted(first.0 - backward),
        ),
        (
            last.2 + forward * category_scale,
            predicted(last.0 + forward),
        ),
    ];
    (slope.is_finite()
        && intercept.is_finite()
        && ends.iter().all(|p| p.0.is_finite() && p.1.is_finite()))
    .then_some(LinearFit {
        ends,
        slope,
        intercept,
        r_squared,
    })
}

fn coefficient(value: f64, code: Option<&str>) -> String {
    if code.is_some_and(|s| !s.is_empty() && !s.eq_ignore_ascii_case("General")) {
        return format_value(value, code);
    }
    if value == 0. {
        return "0".into();
    }
    let exponent = value.abs().log10().floor() as i32;
    if !(-4..5).contains(&exponent) {
        let scientific = format!("{value:.4e}");
        let (mantissa, exponent) = scientific.split_once('e').unwrap();
        let exponent = exponent.parse::<i32>().unwrap();
        format!(
            "{}E{exponent:+03}",
            mantissa.trim_end_matches('0').trim_end_matches('.')
        )
    } else {
        let precision = (4 - exponent).max(0) as usize;
        let value = format!("{value:.precision$}");
        if value.contains('.') {
            value.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            value
        }
    }
}

fn label(
    svg: &mut String,
    trend: Node,
    fit: &LinearFit,
    index: usize,
    ends: [(f64, f64); 2],
    coords: &Coords,
    colors: &ChartColors,
    default_face: &str,
) {
    let show_equation = flag(Some(trend), "dispEq", false);
    let show_r_squared = flag(Some(trend), "dispRSqr", false);
    if !show_equation && !show_r_squared {
        return;
    }
    let node = child(trend, "trendlineLbl");
    let code = node
        .and_then(|n| child(n, "numFmt"))
        .and_then(|n| n.attribute("formatCode"));
    let mut lines = Vec::new();
    if show_equation {
        let slope = coefficient(fit.slope, code);
        let intercept = coefficient(fit.intercept.abs(), code);
        let suffix = if fit.intercept == 0. {
            String::new()
        } else {
            format!(
                " {} {intercept}",
                if fit.intercept < 0. { "-" } else { "+" }
            )
        };
        lines.push(format!("y = {slope}x{suffix}"));
    }
    if show_r_squared {
        lines.push(format!(
            "R² = {}",
            fit.r_squared
                .map_or_else(|| "#N/A".into(), |v| coefficient(v, code))
        ));
    }
    if let Some(tx) = node.and_then(|n| child(n, "tx")) {
        let custom: Vec<_> = tx
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "p")
            .map(|p| {
                p.descendants()
                    .filter(|n| n.is_element() && n.tag_name().name() == "t")
                    .filter_map(|n| n.text())
                    .collect::<String>()
            })
            .collect();
        if !custom.is_empty() {
            lines = custom;
        } else if let Some(value) = tx
            .descendants()
            .find(|n| chart_node(*n, "v"))
            .and_then(|n| n.text())
        {
            lines = value.lines().map(str::to_owned).collect();
        }
    }
    if lines.is_empty() {
        return;
    }
    let root_props = text_props(Some(trend.document().root_element()), false);
    let props = text_props(node, true)
        .or(text_props(node, false))
        .or(root_props);
    let default_size = 40. / 3.;
    #[cfg(feature = "extended-charts")]
    let default_size = colors
        .style_font_size("trendlineLabel")
        .unwrap_or(default_size);
    let size = font_size(props, default_size);
    let face = font_family(props, default_face);
    let bold = props
        .and_then(|n| n.attribute("b"))
        .is_some_and(super::truth);
    let italic = props
        .and_then(|n| n.attribute("i"))
        .is_some_and(super::truth);
    let text_width = lines
        .iter()
        .map(|line| {
            crate::parse::font_metrics::width(line, size, &face, bold, italic)
                .unwrap_or_else(|| text_width_for(line, size, &face))
        })
        .fold(0., f64::max);
    let layout = node
        .and_then(|n| child(n, "layout"))
        .and_then(|n| child(n, "manualLayout"));
    let extent = |key, dimension| {
        number(val(layout, key))
            .filter(|v| *v > 0.)
            .map(|v| v * dimension)
    };
    let line_height = size * 1.2;
    let width = extent("w", coords.canvas.0).unwrap_or(text_width + size * 0.6);
    let height =
        extent("h", coords.canvas.1).unwrap_or(lines.len() as f64 * line_height + size * 0.45);
    let left_to_right = ends[1].0 >= ends[0].0;
    let x = ends[1].0
        + if left_to_right {
            -width / 2. - size * 0.6
        } else {
            width / 2. + size * 0.6
        };
    let default_x = x.clamp(
        coords.rect.left + width.min(coords.rect.width()) / 2.,
        coords.rect.right - width.min(coords.rect.width()) / 2.,
    );
    let default_y = ends[1].1.clamp(
        coords.rect.top + height.min(coords.rect.height()) / 2.,
        coords.rect.bottom - height.min(coords.rect.height()) / 2.,
    );
    let position = |key, mode, default, extent, dimension| {
        number(val(layout, key)).map_or(default, |value| {
            if val(layout, mode) == Some("edge") {
                value * dimension + extent / 2.
            } else {
                default + value * dimension
            }
        })
    };
    let x = position("x", "xMode", default_x, width, coords.canvas.0);
    let y = position("y", "yMode", default_y, height, coords.canvas.1);
    let body = node.and_then(|n| child(n, "txPr").or_else(|| child(n, "tx")));
    let rotation = body
        .and_then(|n| {
            n.descendants()
                .find(|n| n.is_element() && n.tag_name().name() == "bodyPr")
        })
        .and_then(|n| number(n.attribute("rot")))
        .unwrap_or(0.)
        / 60000.;
    let paint = props.and_then(|n| colors.shape_fill(n));
    #[cfg(feature = "extended-charts")]
    let paint = paint.or_else(|| colors.style_text("trendlineLabel", "#000000"));
    let paint = paint.unwrap_or_else(|| "#000000".into());
    write!(
        svg,
        r#"<g transform="translate({x:.2} {y:.2}) rotate({rotation:.2})" fill="{paint}""#
    )
    .unwrap();
    colors.hit_attrs(svg, "trendlineLabel", index, None);
    if bold {
        svg.push_str(r#" font-weight="bold""#);
    }
    if italic {
        svg.push_str(r#" font-style="italic""#);
    }
    svg.push('>');
    if let Some(shape) = node.and_then(|n| child(n, "spPr")) {
        let fill = colors.shape_fill(shape).unwrap_or_else(|| "none".into());
        let stroke = colors.shape_stroke(shape).unwrap_or_else(|| "none".into());
        write!(svg, r#"<rect x="{:.2}" y="{:.2}" width="{width:.2}" height="{height:.2}" fill="{fill}" stroke="{stroke}"/>"#, -width / 2., -height / 2.).unwrap();
    }
    let alignment = body
        .and_then(|n| {
            n.descendants()
                .find(|n| n.is_element() && n.tag_name().name() == "pPr")
        })
        .and_then(|n| n.attribute("algn"));
    let (tx, anchor) = match alignment {
        Some("l") => (-width / 2. + size * 0.3, "start"),
        Some("r") => (width / 2. - size * 0.3, "end"),
        _ => (0., "middle"),
    };
    for (i, line) in lines.iter().enumerate() {
        text(
            svg,
            tx,
            size * 0.375 + (i as f64 - (lines.len() - 1) as f64 / 2.) * line_height,
            anchor,
            size,
            &face,
            line,
        );
    }
    svg.push_str("</g>");
}

pub(super) fn draw(
    svg: &mut String,
    labels: &mut String,
    group: &Group,
    data: &ChartData,
    coords: &Coords,
    colors: &ChartColors,
    default_face: &str,
) {
    if !colors.spreadsheet_defaults {
        return;
    }
    let dates = date_categories(group, data);
    for (index, series) in
        (group.start..group.end).zip(group.node.children().filter(|n| chart_node(*n, "ser")))
    {
        for trend in series.children().filter(|n| {
            chart_node(*n, "trendline") && val(Some(*n), "trendlineType") == Some("linear")
        }) {
            let Some(fit) = linear_fit(&data.series[index], trend, group.scatter, &dates) else {
                continue;
            };
            let ends = fit.ends.map(|(x, y)| coords.point(x, y));
            if ends.iter().all(|p| p.0.is_finite() && p.1.is_finite()) {
                super::trendline_path(svg, trend, colors, index, ends[0], ends[1], true);
                label(
                    labels,
                    trend,
                    &fit,
                    index,
                    ends,
                    coords,
                    colors,
                    default_face,
                );
            }
        }
    }
}

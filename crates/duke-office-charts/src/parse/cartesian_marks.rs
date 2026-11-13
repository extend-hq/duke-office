#[path = "cartesian_labels.rs"]
mod data_labels;

use super::Coords;
use crate::chart::{chart_node, child, ChartSeries};
use crate::parse::chart_colors::ChartColors;
pub(super) use crate::parse::chart_svg::marker;
pub(in crate::parse) use data_labels::DataLabels;
use roxmltree::Node;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

fn val<'a>(node: Node<'a, 'a>, name: &str) -> Option<&'a str> {
    child(node, name)?.attribute("val")
}

fn number(s: Option<&str>) -> Option<f64> {
    s?.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

fn flag(node: Node, name: &str, default: bool) -> bool {
    child(node, name).map_or(default, |n| {
        !matches!(n.attribute("val"), Some("0" | "false" | "off"))
    })
}

fn line_properties<'a>(node: Node<'a, 'a>) -> Option<Node<'a, 'a>> {
    child(node, "spPr")?
        .children()
        .find(|n| n.tag_name().name() == "ln")
}

fn stroke_width(node: Node, default: f64) -> f64 {
    number(line_properties(node).and_then(|n| n.attribute("w")))
        .map_or(default, |w| (w / 9525.).clamp(0., 256.))
}

pub(super) fn invert_negative(series: Node, point: Option<u32>, colors: &ChartColors) -> bool {
    let default = flag(
        series,
        "invertIfNegative",
        !(colors.legacy_defaults() || colors.spreadsheet_defaults),
    );
    point
        .and_then(|index| {
            series.children().find(|n| {
                chart_node(*n, "dPt")
                    && val(*n, "idx").and_then(|v| v.parse::<u32>().ok()) == Some(index)
            })
        })
        .map_or(default, |pt| flag(pt, "invertIfNegative", default))
}

pub(super) fn legend_color(
    group: Node,
    series: Node,
    ordinal: usize,
    point: Option<u32>,
    value: Option<f64>,
    colors: &ChartColors,
    line: bool,
) -> String {
    if line {
        colors.line_at(ordinal, point)
    } else if chart_node(group, "barChart")
        && value.is_some_and(|v| v < 0.)
        && invert_negative(series, point, colors)
    {
        "#ffffff".into()
    } else {
        colors.fill(ordinal, point)
    }
}

#[derive(Default)]
struct Stacks {
    positive: BTreeMap<u32, f64>,
    negative: BTreeMap<u32, f64>,
    totals: BTreeMap<u32, f64>,
}

impl Stacks {
    fn new(series: &[ChartSeries], percent: bool) -> Self {
        let mut stacks = Self::default();
        if percent {
            for point in series.iter().flat_map(|s| &s.points) {
                if let Some(v) = number(point.value.as_deref()) {
                    *stacks.totals.entry(point.index).or_default() += v.abs();
                }
            }
        }
        stacks
    }

    fn span(&mut self, index: u32, value: f64, stacked: bool, percent: bool) -> (f64, f64) {
        if !stacked {
            return (0., value);
        }
        let values = if value < 0. {
            &mut self.negative
        } else {
            &mut self.positive
        };
        let top = values.entry(index).or_default();
        let bottom = *top;
        let fraction = if percent {
            let total = self.totals.get(&index).copied().unwrap_or(0.);
            if total > 0. {
                value / total
            } else {
                0.
            }
        } else {
            value
        };
        *top += fraction;
        (bottom, *top)
    }
}

fn bars(
    svg: &mut String,
    labels: &mut String,
    group: Node,
    nodes: &[Node],
    series: &[ChartSeries],
    first: usize,
    colors: &ChartColors,
    coords: &Coords,
    default_face: &str,
) {
    let grouping = val(group, "grouping");
    let stacked = matches!(grouping, Some("stacked" | "percentStacked"));
    let percent = grouping == Some("percentStacked");
    let mut stacks = Stacks::new(series, percent);
    let gap = number(val(group, "gapWidth"))
        .unwrap_or(150.)
        .clamp(0., 500.)
        / 100.;
    let overlap = number(val(group, "overlap"))
        .unwrap_or(if stacked { 100. } else { 0. })
        .clamp(-100., 100.)
        / 100.;
    let count = series.len() as f64;
    let width = coords.slot_width() / (gap + 1. + (count - 1.) * (1. - overlap));
    // Offset the cluster in category coordinates so both reversed and horizontal axes preserve series order.
    let category_step = (1. - overlap) * width / coords.slot_width().max(f64::MIN_POSITIVE);
    for (j, (serie, &node)) in series.iter().zip(nodes).enumerate() {
        let shift = (j as f64 - (count - 1.) / 2.) * category_step;
        let ordinal = first + j;
        write!(svg, r#"<g data-docx-series="{ordinal}">"#).unwrap();
        let data_labels = DataLabels::new(group, node, default_face, coords.canvas);
        let invert = flag(
            node,
            "invertIfNegative",
            !(colors.legacy_defaults() || colors.spreadsheet_defaults),
        );
        let point_styles: BTreeMap<u32, _> = node
            .children()
            .filter(|n| chart_node(*n, "dPt"))
            .filter_map(|n| Some((val(n, "idx")?.parse().ok()?, n)))
            .collect();
        let base_width = stroke_width(node, 1.);
        for p in &serie.points {
            let Some(v) = number(p.value.as_deref()) else {
                continue;
            };
            let (lo, hi) = stacks.span(p.index, v, stacked, percent);
            let slot = f64::from(p.index) + shift;
            let (a, b) = (
                coords.point(slot, coords.clamp_value(lo)),
                coords.point(slot, coords.clamp_value(hi)),
            );
            let (x, y, w, h) = if coords.is_horizontal() {
                (a.0.min(b.0), a.1 - width / 2., (a.0 - b.0).abs(), width)
            } else {
                (a.0 - width / 2., a.1.min(b.1), width, (a.1 - b.1).abs())
            };
            if let Some(dl) = &data_labels {
                dl.draw(
                    labels,
                    serie,
                    p,
                    b,
                    a,
                    if stacked { "ctr" } else { "outEnd" },
                    coords.is_horizontal(),
                    0.,
                    ordinal,
                    colors,
                );
            }
            if w <= 0. || h <= 0. {
                continue;
            }
            let point_style = point_styles.get(&p.index).copied();
            let inverted =
                v < 0. && point_style.map_or(invert, |n| flag(n, "invertIfNegative", invert));
            let fill = if inverted {
                "#ffffff".to_owned()
            } else {
                colors.fill(ordinal, Some(p.index))
            };
            let stroke = colors
                .outline(ordinal, Some(p.index))
                .unwrap_or(if inverted { "#000000" } else { "none" });
            let sw = point_style.map_or(base_width, |n| stroke_width(n, base_width));
            let body = format!(
                r#"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" fill="{fill}" stroke="{stroke}" stroke-width="{sw:.2}"/>"#
            );
            let properties = point_style
                .and_then(ChartColors::effect_properties)
                .or_else(|| ChartColors::effect_properties(node));
            colors.begin_hit(svg, "bar", ordinal, Some(p.index));
            colors.paint_styled_shape(svg, properties, "dataPoint", &fill, [x, y, w, h], &body);
            colors.end_hit(svg);
        }
        svg.push_str("</g>");
    }
}

#[derive(Clone, Copy)]
struct Sample {
    index: u32,
    top: (f64, f64),
    base: (f64, f64),
    has_value: bool,
}

fn lines(
    svg: &mut String,
    part: &[Sample],
    ordinal: usize,
    node: Node,
    colors: &ChartColors,
    smooth: bool,
) {
    if part.len() < 2 {
        return;
    }
    let sw = stroke_width(node, 3.);
    let line = line_properties(node);
    let dash = colors.line_dash(line, sw);
    let cap = match line.and_then(|n| n.attribute("cap")) {
        Some("rnd") => "round",
        Some("sq") => "square",
        _ => "butt",
    };
    let mut body = String::new();
    for i in 1..part.len() {
        let p = part[i - 1].top;
        let q = part[i].top;
        let paint = colors.line_at(ordinal, Some(part[i].index));
        if paint == "none" {
            continue;
        }
        let (x, y) = p;
        let (xx, yy) = q;
        write!(body,r#"<path stroke="{paint}" stroke-width="{sw:.2}" stroke-dasharray="{dash}" stroke-linecap="{cap}" fill="none" d="M{x:.2} {y:.2}"#).unwrap();
        if smooth && part.len() > 2 {
            let a = part[i.saturating_sub(2)].top;
            let b = part[(i + 1).min(part.len() - 1)].top;
            let f = if i == 1 { 1. / 3. } else { 1. / 6. };
            let g = if i + 1 == part.len() {
                1. / 3.
            } else {
                1. / 6.
            };
            write!(
                body,
                "C{:.2} {:.2} {:.2} {:.2} {xx:.2} {yy:.2}",
                x + (xx - a.0) * f,
                y + (yy - a.1) * f,
                xx - (b.0 - x) * g,
                yy - (b.1 - y) * g
            )
            .unwrap();
        } else {
            write!(body, "L{xx:.2} {yy:.2}").unwrap();
        }
        body.push_str("\"/>");
    }
    if let (Some(first), Some(last)) = (part.first(), part.last()) {
        colors.line_ends(
            &mut body,
            line,
            first.top,
            last.top,
            sw,
            &colors.line_at(ordinal, None),
        );
        let left = part.iter().map(|p| p.top.0).fold(f64::INFINITY, f64::min);
        let top = part.iter().map(|p| p.top.1).fold(f64::INFINITY, f64::min);
        let right = part
            .iter()
            .map(|p| p.top.0)
            .fold(f64::NEG_INFINITY, f64::max);
        let bottom = part
            .iter()
            .map(|p| p.top.1)
            .fold(f64::NEG_INFINITY, f64::max);
        colors.begin_hit(svg, "line", ordinal, None);
        colors.paint_point(
            svg,
            ordinal,
            None,
            "dataPointLine",
            &colors.line_at(ordinal, None),
            [left, top, right - left, bottom - top],
            &body,
        );
        colors.end_hit(svg);
    }
}

fn area(svg: &mut String, part: &[Sample], ordinal: usize, colors: &ChartColors, width: f64) {
    if part.len() < 2 {
        return;
    }
    let mut coords = part
        .iter()
        .map(|s| s.top)
        .chain(part.iter().rev().map(|s| s.base));
    let (x, y) = coords.next().unwrap();
    let fill = colors.fill(ordinal, None);
    let stroke = colors.outline(ordinal, None).unwrap_or("none");
    let mut body = String::new();
    write!(
        body,
        r#"<path fill="{fill}" stroke="{stroke}" stroke-width="{width:.2}" d="M{x:.2} {y:.2}"#
    )
    .unwrap();
    for (x, y) in coords {
        write!(body, "L{x:.2} {y:.2}").unwrap();
    }
    body.push_str("Z\"/>");
    let points = part
        .iter()
        .flat_map(|p| [p.top, p.base])
        .collect::<Vec<_>>();
    let left = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let top = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let right = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let bottom = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    colors.begin_hit(svg, "area", ordinal, None);
    colors.paint_point(
        svg,
        ordinal,
        None,
        "dataPoint",
        &fill,
        [left, top, right - left, bottom - top],
        &body,
    );
    colors.end_hit(svg);
}

fn up_down_paint(group: Node, colors: &ChartColors, up: bool) -> (String, String) {
    let root = group.ancestors().find(|n| chart_node(*n, "chartSpace"));
    let style = root
        .and_then(super::super::chart_colors::classic_style_id)
        .unwrap_or(2)
        % 100;
    let variant = (style - 1) % 8;
    let accent = format!("accent{}", variant.saturating_sub(1));
    let fill = if up && ((33..=40).contains(&style) || style == 42) {
        colors.theme_paint("bg1", None)
    } else if variant < 2 {
        let tint = if style == 42 {
            None
        } else {
            Some(match (variant, up) {
                (0, false) => 0.85,
                (0, true) => 0.25,
                (_, false) => 0.95,
                (_, true) => 0.05,
            })
        };
        colors.theme_paint("tx1", tint)
    } else if up {
        colors.theme_paint(&accent, Some(0.25))
    } else {
        colors.theme_shade(&accent, 0.25)
    };
    let stroke = match style {
        1..=16 => colors.theme_paint("dk1", None),
        33..=34 => colors.theme_paint("tx1", None),
        35..=40 => colors.theme_shade(&accent, 0.25),
        _ => "none".into(),
    };
    (fill, stroke)
}

fn line_adornments(
    svg: &mut String,
    group: Node,
    series: &[ChartSeries],
    colors: &ChartColors,
    coords: &Coords,
) {
    let drop_lines = child(group, "dropLines");
    let high_low_lines = child(group, "hiLowLines");
    let up_down_bars = child(group, "upDownBars");
    if drop_lines.is_none() && high_low_lines.is_none() && up_down_bars.is_none() {
        return;
    }
    if colors.interactive {
        svg.push_str(r#"<g pointer-events="none">"#);
    }
    struct Span {
        low: f64,
        high: f64,
        first: Option<f64>,
        last: Option<f64>,
    }
    let mut points = BTreeMap::new();
    for (ordinal, serie) in series.iter().enumerate() {
        for point in &serie.points {
            let Some(value) = number(point.value.as_deref()) else {
                continue;
            };
            let span = points.entry(point.index).or_insert(Span {
                low: value,
                high: value,
                first: None,
                last: None,
            });
            span.low = span.low.min(value);
            span.high = span.high.max(value);
            if ordinal == 0 {
                span.first = Some(value)
            }
            if ordinal + 1 == series.len() {
                span.last = Some(value)
            }
        }
    }
    let category_axis = group.parent().and_then(|plot| {
        plot.children().find(|n| {
            (chart_node(*n, "catAx") || chart_node(*n, "dateAx"))
                && group
                    .children()
                    .any(|id| chart_node(id, "axId") && id.attribute("val") == val(*n, "axId"))
        })
    });
    let crossing = category_axis
        .and_then(|axis| number(val(axis, "crossesAt")))
        .unwrap_or_else(
            || match category_axis.and_then(|axis| val(axis, "crosses")) {
                Some("max") => coords.values.original(coords.values.high),
                Some("min") => coords.values.original(coords.values.low),
                _ => 0.,
            },
        );
    let baseline = coords.clamp_value(crossing);
    for (node, to_axis) in [(drop_lines, true), (high_low_lines, false)] {
        let Some(node) = node else { continue };
        let stroke = child(node, "spPr")
            .and_then(|p| colors.shape_stroke(p))
            .unwrap_or_else(|| colors.theme_paint("tx1", Some(0.75)));
        let width = stroke_width(node, 2. / 3.);
        for (&index, span) in &points {
            let (low, high) = (span.low, span.high);
            let a = coords.point(
                f64::from(index),
                if to_axis { high.max(baseline) } else { high },
            );
            let b = coords.point(
                f64::from(index),
                if to_axis { low.min(baseline) } else { low },
            );
            write!(svg, r#"<path d="M{:.2} {:.2}L{:.2} {:.2}" stroke="{stroke}" stroke-width="{width:.2}" fill="none"/>"#, a.0, a.1, b.0, b.1).unwrap();
        }
    }
    let Some(bars) = up_down_bars.filter(|_| series.len() >= 2) else {
        if colors.interactive {
            svg.push_str("</g>");
        }
        return;
    };
    let gap = number(val(bars, "gapWidth"))
        .unwrap_or(150.)
        .clamp(0., 500.)
        / 100.;
    let width = coords.slot_width() / (1. + gap);
    for (&index, span) in &points {
        let (Some(first), Some(last)) = (span.first, span.last) else {
            continue;
        };
        if first == last {
            continue;
        }
        let up = last > first;
        let node = child(bars, if up { "upBars" } else { "downBars" });
        let props = node.and_then(|n| child(n, "spPr"));
        let (default_fill, default_stroke) = up_down_paint(group, colors, up);
        let fill = props
            .and_then(|p| colors.shape_fill(p))
            .unwrap_or(default_fill);
        let stroke = props
            .and_then(|p| colors.shape_stroke(p))
            .unwrap_or(default_stroke);
        let stroke_width = node.map_or(2. / 3., |n| stroke_width(n, 2. / 3.));
        let a = coords.point(f64::from(index), first);
        let b = coords.point(f64::from(index), last);
        write!(svg, r#"<rect x="{:.2}" y="{:.2}" width="{width:.2}" height="{:.2}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width:.2}"/>"#, a.0 - width/2., a.1.min(b.1), (b.1-a.1).abs()).unwrap();
    }
    if colors.interactive {
        svg.push_str("</g>");
    }
}

pub(super) fn draw(
    svg: &mut String,
    labels: &mut String,
    group: Node,
    series: &[ChartSeries],
    first_series: usize,
    colors: &ChartColors,
    coords: &Coords,
    blanks: &str,
    default_face: &str,
) {
    let nodes: Vec<_> = group.children().filter(|n| chart_node(*n, "ser")).collect();
    if chart_node(group, "barChart") {
        bars(
            svg,
            labels,
            group,
            &nodes,
            series,
            first_series,
            colors,
            coords,
            default_face,
        );
        return;
    }
    let scatter = chart_node(group, "scatterChart");
    let is_area = chart_node(group, "areaChart");
    let percent = val(group, "grouping") == Some("percentStacked");
    let stacked = percent || val(group, "grouping") == Some("stacked");
    let mut stacks = Stacks::new(series, percent);
    let scatter_style = val(group, "scatterStyle").unwrap_or("marker");
    let zero = blanks == "zero";
    let mut area_anchors = String::new();
    for (j, (serie, &node)) in series.iter().zip(&nodes).enumerate() {
        let ordinal = first_series + j;
        write!(svg, r#"<g data-docx-series="{ordinal}">"#).unwrap();
        let mut segments = Vec::new();
        let mut segment = Vec::new();
        let by_index: BTreeMap<_, _> = serie.points.iter().map(|p| (p.index, p)).collect();
        let mut indices: BTreeSet<u32> = by_index.keys().copied().collect();
        if !scatter && zero {
            // A long run of absent cache indices can be represented by its endpoints.
            let last = if colors.use_cached_point_count {
                super::cached_point_count(node).checked_sub(1)
            } else {
                indices.last().copied()
            };
            if let Some(last) = last {
                indices.insert(0);
                indices.insert(last);
            }
            let mut prior = None;
            for i in indices.clone() {
                if let Some(p) = prior {
                    if i - p > 1 {
                        indices.insert(p + 1);
                        indices.insert(i - 1);
                    }
                }
                prior = Some(i);
            }
        }
        let implicit_x = scatter && child(node, "xVal").is_none();
        let mut previous = None;
        for index in indices {
            if blanks == "gap" && previous.is_some_and(|i| index - i > 1) && !segment.is_empty() {
                segments.push(std::mem::take(&mut segment));
            }
            previous = Some(index);
            let point = by_index.get(&index).copied();
            let v = point.and_then(|p| {
                number(if scatter {
                    p.y.as_deref()
                } else {
                    p.value.as_deref()
                })
            });
            let x = if scatter {
                if implicit_x {
                    Some(f64::from(index) + 1.)
                } else {
                    point.and_then(|p| number(p.x.as_deref()))
                }
            } else {
                Some(f64::from(index))
            };
            if let (Some(x), Some(vv)) = (x, v.or(zero.then_some(0.))) {
                let (lo, hi) = stacks.span(index, vv, stacked, percent);
                segment.push(Sample {
                    index,
                    base: coords.point(x, coords.clamp_value(lo)),
                    top: coords.point(x, hi),
                    has_value: v.is_some(),
                });
            } else if blanks != "span" && !segment.is_empty() {
                segments.push(std::mem::take(&mut segment));
            }
        }
        if !segment.is_empty() {
            segments.push(segment);
        }
        for part in &segments {
            if is_area {
                area(svg, part, ordinal, colors, stroke_width(node, 1.));
            } else if !scatter
                || !matches!(scatter_style, "marker" | "none")
                || line_properties(node).is_some()
                || (colors.spreadsheet_defaults && scatter_style == "marker")
            {
                let default_smooth = !(colors.legacy_defaults() || colors.spreadsheet_defaults)
                    || (scatter && matches!(scatter_style, "smooth" | "smoothMarker"));
                let smooth = flag(node, "smooth", flag(group, "smooth", default_smooth));
                lines(svg, part, ordinal, node, colors, smooth);
            }
        }
        if scatter && colors.spreadsheet_defaults {
            super::error_bars::draw(svg, node, serie, ordinal, colors, coords);
        }
        if let Some(dl) = DataLabels::new(group, node, default_face, coords.canvas) {
            let radius = number(child(node, "marker").and_then(|n| val(n, "size")))
                .unwrap_or(if scatter { 7. } else { 5. })
                .clamp(2., 72.)
                * 2.
                / 3.;
            for sample in segments.iter().flatten() {
                if let Some(point) = by_index.get(&sample.index) {
                    dl.draw(
                        labels,
                        serie,
                        point,
                        sample.top,
                        sample.base,
                        if is_area { "ctr" } else { "r" },
                        false,
                        if is_area { 0. } else { radius },
                        ordinal,
                        colors,
                    );
                }
            }
        }
        if is_area {
            if colors.interactive {
                for sample in segments.iter().flatten().filter(|p| p.has_value) {
                    let (x, y) = sample.top;
                    area_anchors.push_str("<circle");
                    colors.hit_attrs(&mut area_anchors, "point", ordinal, Some(sample.index));
                    write!(area_anchors, r#" data-chart-x="{x:.2}" data-chart-y="{y:.2}" cx="{x:.2}" cy="{y:.2}" r="4" fill="transparent"/>"#).unwrap();
                }
            }
            svg.push_str("</g>");
            continue;
        }
        let series_marker = child(node, "marker");
        let default_visible = if scatter {
            matches!(scatter_style, "marker" | "lineMarker" | "smoothMarker")
        } else {
            flag(group, "marker", true)
        };
        let point_nodes: BTreeMap<_, _> = node
            .children()
            .filter(|n| chart_node(*n, "dPt"))
            .filter_map(|n| Some((val(n, "idx")?.parse::<u32>().ok()?, child(n, "marker")?)))
            .collect();
        let vary = colors.vary_by_point(ordinal);
        let ser_idx = val(node, "idx")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(ordinal);
        for sample in segments.iter().flatten().filter(|p| p.has_value) {
            let point_marker = point_nodes.get(&sample.index).copied();
            let symbol = point_marker
                .and_then(|m| val(m, "symbol"))
                .or_else(|| series_marker.and_then(|m| val(m, "symbol")))
                .unwrap_or(if default_visible { "auto" } else { "none" });
            if colors.interactive {
                let (x, y) = sample.top;
                let kind = if symbol == "none" { "point" } else { "marker" };
                write!(svg, r#"<g data-chart-kind="{kind}" data-chart-series="{ordinal}" data-chart-point="{}" data-chart-x="{x:.2}" data-chart-y="{y:.2}""#, sample.index).unwrap();
                svg.push('>');
                if symbol == "none" {
                    write!(
                        svg,
                        r#"<circle cx="{x:.2}" cy="{y:.2}" r="4" fill="transparent"/></g>"#
                    )
                    .unwrap();
                }
            }
            if symbol == "none" {
                continue;
            }
            let symbols = [
                "diamond", "square", "triangle", "x", "star", "circle", "plus", "dash",
            ];
            let symbol = if symbol == "auto" {
                symbols[(if vary { sample.index as usize } else { ser_idx }) % symbols.len()]
            } else {
                symbol
            };
            let size = number(point_marker.and_then(|m| val(m, "size")))
                .or_else(|| number(series_marker.and_then(|m| val(m, "size"))))
                .unwrap_or(7.)
                .clamp(2., 72.)
                * 2.
                / 3.;
            let fill = colors.marker(ordinal, Some(sample.index));
            let stroke = colors.marker_outline(ordinal, Some(sample.index));
            let sw = point_marker
                .or(series_marker)
                .map_or(1., |n| stroke_width(n, 1.));
            let mut body = String::new();
            marker(&mut body, sample.top, symbol, size, &fill, &stroke, sw);
            let properties = point_marker
                .and_then(ChartColors::effect_properties)
                .or_else(|| series_marker.and_then(ChartColors::effect_properties));
            colors.paint_styled_shape(
                svg,
                properties,
                "dataPointMarker",
                &fill,
                [
                    sample.top.0 - size,
                    sample.top.1 - size,
                    size * 2.,
                    size * 2.,
                ],
                &body,
            );
            if colors.interactive {
                svg.push_str("</g>");
            }
        }
        svg.push_str("</g>");
    }
    svg.push_str(&area_anchors);
    if colors.spreadsheet_defaults && chart_node(group, "lineChart") && !stacked {
        line_adornments(svg, group, series, colors, coords);
    }
}

#[cfg(test)]
mod tests {
    use crate::chart::chart_data_from_root;
    use crate::parse::cartesian_chart::render;
    use crate::parse::chart_colors::ChartColors;
    use crate::RenderOptions;

    fn render_line(blanks: &str) -> String {
        let xml = format!(
            r#"<chartSpace xmlns="http://schemas.openxmlformats.org/drawingml/2006/chart">
            <chart><plotArea><lineChart><ser><idx val="0"/><order val="0"/>
            <cat><strLit><ptCount val="3"/><pt idx="0"><v>A</v></pt><pt idx="1"><v>B</v></pt><pt idx="2"><v>C</v></pt></strLit></cat>
            <val><numLit><ptCount val="3"/><pt idx="0"><v>2</v></pt><pt idx="2"><v>8</v></pt></numLit></val>
            <marker><symbol val="none"/></marker></ser><dLbls><showVal val="1"/></dLbls>
            </lineChart></plotArea><dispBlanksAs val="{blanks}"/></chart></chartSpace>"#
        );
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let root = doc.root_element();
        let data = chart_data_from_root(root).unwrap();
        let colors = ChartColors::new(root, &RenderOptions::default(), &data);
        render(root, &data, 320., 180., &colors, None).unwrap()
    }

    #[test]
    fn sparse_line_knows_which_value_and_label_belong_to_each_category() {
        let mut positions = Vec::new();
        for (mode, paths, labels) in [
            ("gap", 0, vec!["2", "8"]),
            ("span", 1, vec!["2", "8"]),
            ("zero", 2, vec!["2", "0", "8"]),
        ] {
            let svg = render_line(mode);
            let doc = roxmltree::Document::parse(&svg).unwrap();
            let marks = doc
                .descendants()
                .find(|n| n.attribute("clip-path").is_some())
                .unwrap();
            assert_eq!(
                marks
                    .descendants()
                    .filter(|n| n.tag_name().name() == "path")
                    .count(),
                paths
            );
            let text: Vec<_> = doc
                .descendants()
                .filter(|n| n.tag_name().name() == "text")
                .collect();
            assert_eq!(
                text.iter().filter_map(|n| n.text()).collect::<Vec<_>>(),
                labels
            );
            positions.push(
                text.last()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .attribute("transform")
                    .unwrap()
                    .to_owned(),
            );
        }
        assert_eq!(positions[0], positions[1]);
        assert_eq!(positions[1], positions[2]);
    }
}

use super::{flag, number, val, Coords, Extent};
use crate::chart::{chart_node, child, ChartSeries};
use crate::parse::chart_colors::ChartColors;
use roxmltree::Node;
use std::collections::BTreeMap;
use std::fmt::Write;

struct Sample {
    index: u32,
    x: f64,
    y: f64,
}

struct Bar {
    index: u32,
    start: (f64, f64),
    end: (f64, f64),
    cap_start: bool,
    cap_end: bool,
}

fn samples(node: Node, series: &ChartSeries) -> Vec<Sample> {
    let implicit_x = child(node, "xVal").is_none();
    series
        .points
        .iter()
        .filter_map(|p| {
            Some(Sample {
                index: p.index,
                x: if implicit_x {
                    f64::from(p.index) + 1.
                } else {
                    number(p.x.as_deref())?
                },
                y: number(p.y.as_deref())?,
            })
        })
        .collect()
}

fn custom_values(node: Node, side: &str) -> BTreeMap<u32, f64> {
    let Some(source) = child(node, side) else {
        return BTreeMap::new();
    };
    let cache = child(source, "numLit")
        .or_else(|| child(source, "numRef").and_then(|reference| child(reference, "numCache")));
    cache
        .into_iter()
        .flat_map(|cache| cache.children())
        .filter(|n| chart_node(*n, "pt"))
        .filter_map(|n| {
            Some((
                n.attribute("idx")?.parse().ok()?,
                number(child(n, "v")?.text())?.abs(),
            ))
        })
        .collect()
}

fn bars(node: Node, samples: &[Sample]) -> Vec<Bar> {
    let horizontal = val(Some(node), "errDir") == Some("x");
    let kind = val(Some(node), "errValType").unwrap_or("fixedVal");
    let side = val(Some(node), "errBarType").unwrap_or("both");
    let minus = matches!(side, "both" | "minus");
    let plus = matches!(side, "both" | "plus");
    let caps = !flag(Some(node), "noEndCap", false);
    let amount = number(val(Some(node), "val")).unwrap_or(1.).abs();
    let custom_minus = custom_values(node, "minus");
    let custom_plus = custom_values(node, "plus");
    let mut count = 0.;
    let mut mean = 0.;
    let mut variance = 0.;
    if matches!(kind, "stdDev" | "stdErr") {
        for p in samples {
            let value = if horizontal { p.x } else { p.y };
            count += 1.;
            let delta = value - mean;
            mean += delta / count;
            variance += delta * (value - mean);
        }
    }
    let deviation = if count > 1. {
        (variance / (count - 1.)).max(0.).sqrt()
    } else {
        0.
    };
    samples
        .iter()
        .filter_map(|p| {
            let value = if horizontal { p.x } else { p.y };
            let magnitude = match kind {
                "fixedVal" => Some(amount),
                "percentage" => Some(value.abs() * amount / 100.),
                "stdDev" => Some(deviation * amount),
                "stdErr" => Some(deviation * amount / count.max(1.).sqrt()),
                "cust" => None,
                _ => return None,
            };
            let below = minus
                .then(|| magnitude.or_else(|| custom_minus.get(&p.index).copied()))
                .flatten()
                .filter(|n| n.is_finite() && *n > 0.);
            let above = plus
                .then(|| magnitude.or_else(|| custom_plus.get(&p.index).copied()))
                .flatten()
                .filter(|n| n.is_finite() && *n > 0.);
            if below.is_none() && above.is_none() {
                return None;
            }
            let center = if kind == "stdDev" { mean } else { value };
            let low = center - below.unwrap_or(0.);
            let high = center + above.unwrap_or(0.);
            (low.is_finite() && high.is_finite()).then_some(Bar {
                index: p.index,
                start: if horizontal { (low, p.y) } else { (p.x, low) },
                end: if horizontal { (high, p.y) } else { (p.x, high) },
                cap_start: caps && below.is_some(),
                cap_end: caps && above.is_some(),
            })
        })
        .collect()
}

pub(super) fn ranges(node: Node, series: &ChartSeries) -> (Extent, Extent) {
    let mut x = Extent::default();
    let mut y = Extent::default();
    let samples = samples(node, series);
    for error in node.children().filter(|n| chart_node(*n, "errBars")) {
        for bar in bars(error, &samples) {
            x.record(bar.start.0);
            x.record(bar.end.0);
            y.record(bar.start.1);
            y.record(bar.end.1);
        }
    }
    (x, y)
}

pub(super) fn draw(
    svg: &mut String,
    node: Node,
    series: &ChartSeries,
    ordinal: usize,
    colors: &ChartColors,
    coords: &Coords,
) {
    let samples = samples(node, series);
    for error in node.children().filter(|n| chart_node(*n, "errBars")) {
        let line = child(error, "spPr").and_then(|p| {
            p.children()
                .find(|n| n.is_element() && n.tag_name().name() == "ln")
        });
        let paint = line
            .and_then(|n| colors.shape_fill(n))
            .unwrap_or_else(|| "#000000".to_owned());
        let width = line
            .and_then(|n| number(n.attribute("w")))
            .map_or(1., |w| (w / 9525.).clamp(0., 256.));
        if paint == "none" || width == 0. {
            continue;
        }
        let dash = line
            .and_then(|n| {
                n.children()
                    .find(|n| n.is_element() && n.tag_name().name() == "prstDash")
            })
            .and_then(|n| n.attribute("val"));
        let dash = match dash {
            Some("dot" | "sysDot") => format!(r#" stroke-dasharray="{width} {}""#, width * 2.),
            Some("dash" | "sysDash") => {
                format!(r#" stroke-dasharray="{} {}""#, width * 4., width * 3.)
            }
            Some("lgDash") => format!(r#" stroke-dasharray="{} {}""#, width * 8., width * 3.),
            Some("dashDot" | "sysDashDot" | "lgDashDot") => format!(
                r#" stroke-dasharray="{} {} {width} {}""#,
                width * 4.,
                width * 2.,
                width * 2.
            ),
            _ => String::new(),
        };
        for bar in bars(error, &samples) {
            let a = coords.point(bar.start.0, bar.start.1);
            let b = coords.point(bar.end.0, bar.end.1);
            let mut path = format!("M{:.2} {:.2}L{:.2} {:.2}", a.0, a.1, b.0, b.1);
            let horizontal = (b.0 - a.0).abs() > (b.1 - a.1).abs();
            let cap = 3.;
            for (point, enabled) in [(a, bar.cap_start), (b, bar.cap_end)] {
                if enabled {
                    let (dx, dy) = if horizontal { (0., cap) } else { (cap, 0.) };
                    write!(
                        path,
                        "M{:.2} {:.2}L{:.2} {:.2}",
                        point.0 - dx,
                        point.1 - dy,
                        point.0 + dx,
                        point.1 + dy
                    )
                    .unwrap();
                }
            }
            svg.push_str("<path");
            colors.hit_attrs(svg, "errorBar", ordinal, Some(bar.index));
            write!(
                svg,
                r#" d="{path}" fill="none" stroke="{paint}" stroke-width="{width:.2}"{dash}/>"#
            )
            .unwrap();
        }
    }
}

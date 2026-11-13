//! DrawingML pie, doughnut, and split pie plots.
use super::chart_colors::ChartColors;
use super::chart_svg::{
    font_family, font_size, line, number, on, text, text_props, text_width, title_position, val,
};
use crate::chart::{chart_node, child, ChartData, ChartSeries};
use roxmltree::Node;
use std::collections::HashMap;
use std::f64::consts::{PI, TAU};
use std::fmt::Write;

struct Slice {
    index: u32,
    aggregate: bool,
    value: f64,
    defined: bool,
    weight: f64,
    name: String,
    explosion: f64,
}

fn slices(series: &ChartSeries, node: Node) -> Vec<Slice> {
    let explosion = |n| number(val(Some(n), "explosion")).map(|v| v.clamp(0., 400.) / 100.);
    let default_explosion = explosion(node).unwrap_or(0.);
    let overrides: HashMap<_, _> = node
        .children()
        .filter(|n| chart_node(*n, "dPt"))
        .filter_map(|n| Some((val(Some(n), "idx")?.parse::<u32>().ok()?, explosion(n)?)))
        .collect();
    let mut out: Vec<_> = series
        .points
        .iter()
        .map(|p| {
            let value = number(p.value.as_deref());
            Slice {
                index: p.index,
                aggregate: false,
                value: value.unwrap_or(0.),
                defined: value.is_some(),
                weight: value.unwrap_or(0.).abs(),
                name: p
                    .categories
                    .first()
                    .map(|c| c.label.clone())
                    .unwrap_or_else(|| (u64::from(p.index) + 1).to_string()),
                explosion: overrides
                    .get(&p.index)
                    .copied()
                    .unwrap_or(default_explosion),
            }
        })
        .collect();
    // Scale the weights before summing to keep very large finite values renderable.
    let max = out.iter().map(|p| p.weight).fold(0., f64::max);
    if max > 0. {
        for p in &mut out {
            p.weight /= max;
        }
    }
    out
}

#[derive(Clone, Copy)]
struct Circle {
    x: f64,
    y: f64,
    r: f64,
}
impl Circle {
    fn at(self, angle: f64) -> (f64, f64) {
        (self.x + self.r * angle.cos(), self.y + self.r * angle.sin())
    }

    fn displaced(self, distance: f64, angle: f64) -> Self {
        let (x, y) = Self {
            r: distance,
            ..self
        }
        .at(angle);
        Self { x, y, ..self }
    }
}

fn sector(
    svg: &mut String,
    circle: Circle,
    inner: f64,
    start: f64,
    sweep: f64,
    fill: &str,
    stroke: &str,
) {
    if sweep <= 0. || circle.r <= 0. {
        return;
    }
    let (x, y, r) = (circle.x, circle.y, circle.r);
    let (a, b) = circle.at(start);
    let (z, w) = circle.at(start + sweep);
    let inner_circle = Circle { r: inner, ..circle };
    let (u, v) = inner_circle.at(start + sweep);
    let (s, t) = inner_circle.at(start);
    if sweep >= TAU - 1e-10 {
        if inner <= 0. {
            write!(
                svg,
                r#"<circle cx="{x:.3}" cy="{y:.3}" r="{r:.3}" fill="{fill}" stroke="{stroke}"/>"#
            )
            .unwrap();
        } else {
            let (l, k) = circle.at(start + PI);
            let (il, ik) = inner_circle.at(start + PI);
            write!(svg,r#"<path d="M{a:.3} {b:.3}A{r:.3},{r:.3} 0 1 1 {l:.3},{k:.3}A{r:.3},{r:.3} 0 1 1 {a:.3},{b:.3}ZM{s:.3} {t:.3}A{inner:.3},{inner:.3} 0 1 0 {il:.3},{ik:.3}A{inner:.3},{inner:.3} 0 1 0 {s:.3},{t:.3}Z" fill="{fill}" fill-rule="evenodd" stroke="{stroke}"/>"#).unwrap();
        }
        return;
    }
    let long = u8::from(sweep > PI);
    write!(
        svg,
        r#"<path d="M{a:.3},{b:.3}A{r:.3},{r:.3} 0 {long} 1 {z:.3},{w:.3}"#
    )
    .unwrap();
    if inner > 0. {
        write!(
            svg,
            "L{u:.3},{v:.3}A{inner:.3},{inner:.3} 0 {long} 0 {s:.3},{t:.3}"
        )
        .unwrap();
    } else {
        write!(svg, "L{x:.3},{y:.3}").unwrap();
    }
    write!(svg, r#"Z" fill="{fill}" stroke="{stroke}"/>"#).unwrap();
}

struct LabelStyle<'a> {
    options: Node<'a, 'a>,
    series: &'a str,
    size: f64,
    face: &'a str,
    legacy: bool,
    spreadsheet: bool,
    format: Option<&'a str>,
    colors: &'a ChartColors,
    width: f64,
    height: f64,
}

impl LabelStyle<'_> {
    fn write(
        &self,
        svg: &mut String,
        p: &Slice,
        fraction: f64,
        x: f64,
        y: f64,
        max_width: f64,
        color: &str,
        leader_anchor: Option<(f64, f64)>,
        leader_region: Option<(Circle, f64, f64, f64)>,
    ) {
        let over = self.options.children().find(|n| {
            chart_node(*n, "dLbl")
                && val(Some(*n), "idx").and_then(|s| s.parse().ok()) == Some(p.index)
        });
        if on(val(over, "delete")) {
            return;
        }
        let props = text_props(over, true)
            .or_else(|| text_props(over, false))
            .or_else(|| text_props(Some(self.options), false));
        let size = font_size(props, self.size);
        let face = font_family(props, self.face.split(',').next().unwrap_or("Calibri"));
        let paint = props.and_then(|p| self.colors.shape_fill(p));
        let fill = paint
            .as_deref()
            .unwrap_or(if self.spreadsheet { "#595959" } else { "black" });
        let layout = over
            .and_then(|n| child(n, "layout"))
            .and_then(|n| child(n, "manualLayout"));
        let extended_layout = over
            .and_then(|node| {
                node.descendants().find(|n| {
                    n.has_tag_name((
                        "http://schemas.microsoft.com/office/drawing/2012/chart",
                        "layout",
                    ))
                })
            })
            .and_then(|n| child(n, "manualLayout"));
        let explicit_width = number(val(extended_layout, "w").or_else(|| val(layout, "w")))
            .filter(|n| *n > 0.)
            .map(|n| n * self.width);
        let explicit_height = number(val(extended_layout, "h").or_else(|| val(layout, "h")))
            .filter(|n| *n > 0.)
            .map(|n| n * self.height);
        let max_width = explicit_width.unwrap_or(max_width);
        let shown = |name| {
            over.and_then(|o| child(o, name))
                .or_else(|| child(self.options, name))
                .map_or(!self.legacy, |n| {
                    !matches!(n.attribute("val"), Some("0" | "false" | "off"))
                })
        };
        let mut items = Vec::new();
        if shown("showSerName") && !self.series.is_empty() {
            items.push(self.series.to_owned())
        }
        if shown("showCatName") {
            items.push(p.name.clone())
        }
        if shown("showVal") {
            items.push(if self.spreadsheet {
                super::chart_svg::format_value(p.value, self.format)
            } else {
                p.value.to_string()
            })
        }
        if shown("showPercent") {
            items.push(format!("{:.0}%", fraction * 100.))
        }
        let separator = child(self.options, "separator")
            .and_then(|s| s.text())
            .unwrap_or(", ");
        let key = if shown("showLegendKey") {
            size * 0.53
        } else {
            0.
        };
        let mut rows = Vec::new();
        for section in items.join(separator).lines() {
            let mut line = String::new();
            for word in section.split_inclusive([' ', '-']) {
                if !line.is_empty()
                    && text_width(line.trim_end(), size) + text_width(word.trim_end(), size)
                        > max_width
                {
                    rows.push(line.trim_end().to_owned());
                    line.clear();
                }
                line.push_str(word);
            }
            if !line.is_empty() {
                rows.push(line.trim_end().to_owned());
            }
        }
        if rows.is_empty() && key == 0. {
            return;
        }
        let natural_width = rows
            .iter()
            .map(|row| text_width(row, size))
            .fold(0., f64::max);
        let box_width = explicit_width.unwrap_or(natural_width + key);
        let box_height = explicit_height.unwrap_or(size * 1.16 * rows.len() as f64);
        let coordinate = |axis: &str, mode: &str, center: f64, extent: f64, dimension: f64| {
            let Some(value) = number(val(layout, axis)) else {
                return center;
            };
            let origin = if val(layout, mode) == Some("edge") {
                value * dimension
            } else {
                let automatic = center - extent * 0.5;
                let automatic = if explicit_width.is_some() && axis == "x" {
                    automatic.max(0.)
                } else {
                    automatic
                };
                automatic + value * dimension
            };
            origin + extent * 0.5
        };
        let x = coordinate("x", "xMode", x, box_width, self.width);
        let y = coordinate("y", "yMode", y, box_height, self.height);
        let padding_x = size * 0.28;
        let padding_y = size * 0.14;
        let left = x - box_width * 0.5 - padding_x;
        let top = y - box_height * 0.5 - padding_y;
        let outside_slice = leader_region.is_none_or(|(circle, inner, start, sweep)| {
            let dx = x - circle.x;
            let dy = y - circle.y;
            let radius = dx.hypot(dy);
            let angle = (dy.atan2(dx) - start).rem_euclid(TAU);
            radius < inner || radius > circle.r || angle > sweep
        });
        if shown("showLeaderLines") && outside_slice {
            if let Some((start_x, start_y)) = leader_anchor {
                let end_x = if start_x < left {
                    left - 2.
                } else if start_x > left + box_width + padding_x * 2. {
                    left + box_width + padding_x * 2. + 2.
                } else {
                    start_x
                };
                let end_y = y.clamp(top, top + box_height + padding_y * 2.);
                if (start_x - end_x).hypot(start_y - end_y) > size * 0.5 {
                    let line_props =
                        child(self.options, "leaderLines").and_then(|n| child(n, "spPr"));
                    let stroke = line_props
                        .and_then(|p| self.colors.shape_stroke(p))
                        .unwrap_or_else(|| "#a6a6a6".to_owned());
                    let elbow_x = end_x + if end_x < start_x { 5. } else { -5. };
                    write!(svg, r#"<path d="M{start_x:.2} {start_y:.2}L{elbow_x:.2} {end_y:.2}H{end_x:.2}" fill="none" stroke="{stroke}" stroke-width="1"/>"#).unwrap();
                }
            }
        }
        let shape = over
            .and_then(|n| child(n, "spPr"))
            .or_else(|| child(self.options, "spPr"));
        if let Some(background) = shape
            .and_then(|p| self.colors.shape_fill(p))
            .filter(|paint| paint != "none")
        {
            write!(svg, r#"<rect x="{left:.2}" y="{top:.2}" width="{:.2}" height="{:.2}" fill="{background}"/>"#, box_width + padding_x * 2., box_height + padding_y * 2.).unwrap();
        }
        write!(
            svg,
            r#"<g fill="{fill}"{}{}>"#,
            if props
                .and_then(|p| p.attribute("b"))
                .is_some_and(|v| on(Some(v)))
            {
                " font-weight=\"bold\""
            } else {
                ""
            },
            if props
                .and_then(|p| p.attribute("i"))
                .is_some_and(|v| on(Some(v)))
            {
                " font-style=\"italic\""
            } else {
                ""
            }
        )
        .unwrap();
        let start = y + size * 0.32 - rows.len().saturating_sub(1) as f64 * size * 0.58;
        for (i, row) in rows.iter().enumerate() {
            let mut cx = x;
            if i == 0 && key > 0. {
                let left = x - (text_width(row, size) + key + 3.) / 2.;
                write!(svg,r#"<rect x="{left:.2}" y="{:.2}" width="{key:.2}" height="{key:.2}" fill="{color}"/>"#,start-key).unwrap();
                cx += (key + 3.) / 2.;
            }
            text(
                svg,
                cx,
                start + i as f64 * size * 1.16,
                "middle",
                size,
                face.as_str(),
                row,
            );
        }
        svg.push_str("</g>");
    }
}

struct Ring<'a> {
    circle: Circle,
    inner: f64,
    start: f64,
    series: usize,
    colors: &'a ChartColors,
    overall: f64,
    labels: Option<&'a LabelStyle<'a>>,
}

impl Ring<'_> {
    fn draw(&self, svg: &mut String, label_svg: &mut String, slices: &[&Slice]) {
        let sum: f64 = slices.iter().map(|p| p.weight).sum();
        if sum <= 0. {
            return;
        }
        let mut angle = self.start;
        for p in slices {
            let sweep = p.weight / sum * TAU;
            let sector_start = if self.colors.pie_counterclockwise {
                angle - sweep
            } else {
                angle
            };
            let bisector = sector_start + sweep / 2.;
            let circle = self.circle.displaced(self.circle.r * p.explosion, bisector);
            let fill = self.colors.fill(self.series, Some(p.index));
            let stroke = self
                .colors
                .outline(self.series, Some(p.index))
                .unwrap_or("none");
            let point = (!p.aggregate).then_some(p.index);
            if self.colors.interactive {
                let (x, y) = Circle {
                    r: (circle.r + self.inner) / 2.,
                    ..circle
                }
                .at(bisector);
                svg.push_str("<g");
                self.colors.hit_attrs(svg, "slice", self.series, point);
                write!(svg, r#" data-chart-x="{x:.2}" data-chart-y="{y:.2}">"#).unwrap();
            }
            let width = self
                .colors
                .spreadsheet_defaults
                .then(|| self.colors.outline_width(self.series, p.index))
                .flatten();
            let mut body = String::new();
            if let Some(width) = width {
                write!(
                    body,
                    r#"<g stroke-width="{width:.3}" stroke-linejoin="round">"#
                )
                .unwrap();
            }
            sector(
                &mut body,
                circle,
                self.inner,
                sector_start,
                sweep,
                &fill,
                stroke,
            );
            if width.is_some() {
                body.push_str("</g>");
            }
            self.colors.paint_point(
                svg,
                self.series,
                point,
                "dataPoint",
                &fill,
                [
                    circle.x - circle.r,
                    circle.y - circle.r,
                    circle.r * 2.,
                    circle.r * 2.,
                ],
                &body,
            );
            self.colors.end_hit(svg);
            if let Some(labels) = self.labels.filter(|_| p.defined) {
                let placement = val(Some(labels.options), "dLblPos").unwrap_or("bestFit");
                let r = if placement == "outEnd"
                    || (self.inner == 0. && placement == "bestFit" && sweep < 0.25 * PI)
                {
                    circle.r * 1.13
                } else {
                    (circle.r + self.inner)
                        * if labels.spreadsheet && self.inner > 0. {
                            0.5
                        } else {
                            0.58
                        }
                };
                let (mut x, y) = Circle { r, ..circle }.at(bisector);
                let width = circle.r
                    * if labels.spreadsheet {
                        if self.inner > 0. {
                            1.0
                        } else {
                            0.85
                        }
                    } else {
                        0.64
                    };
                if labels.spreadsheet && self.inner == 0. && r > circle.r {
                    x += bisector.cos().signum() * width / 3.;
                }
                self.colors
                    .begin_hit(label_svg, "dataLabel", self.series, point);
                labels.write(
                    label_svg,
                    p,
                    p.weight / self.overall,
                    x,
                    y,
                    width.max(labels.size * 3.),
                    &fill,
                    Some(if r > circle.r {
                        circle.at(bisector)
                    } else {
                        (x, y)
                    }),
                    Some((circle, self.inner, angle, sweep)),
                );
                self.colors.end_hit(label_svg);
            }
            angle += if self.colors.pie_counterclockwise {
                -sweep
            } else {
                sweep
            };
        }
    }
}

fn split<'a>(
    group: Node,
    points: &'a [Slice],
    total: f64,
    spreadsheet: bool,
) -> (Vec<&'a Slice>, Vec<&'a Slice>) {
    let position = number(val(Some(group), "splitPos"));
    let mode = val(Some(group), "splitType").unwrap_or("auto");
    let automatic_count = points.len() as f64 / 3.;
    let automatic_count = if spreadsheet {
        automatic_count.ceil()
    } else {
        automatic_count.round()
    };
    let count = position.unwrap_or(automatic_count.max(1.)).max(0.) as usize;
    let last = points
        .len()
        .saturating_sub(count.min(points.len().saturating_sub(1)));
    let selected: std::collections::HashSet<u32> = child(group, "custSplit")
        .into_iter()
        .flat_map(|n| n.children())
        .filter(|n| chart_node(*n, "secondPiePt"))
        .filter_map(|n| n.attribute("val")?.parse().ok())
        .collect();
    let (mut primary, mut secondary) = (Vec::new(), Vec::new());
    for (i, p) in points.iter().enumerate() {
        let in_second = match mode {
            "val" => p.value < position.unwrap_or(0.),
            "percent" => p.weight / total * 100. < position.unwrap_or(0.),
            "cust" => selected.contains(&p.index),
            _ => i >= last,
        };
        if in_second {
            secondary.push(p);
        } else {
            primary.push(p);
        }
    }
    (primary, secondary)
}

fn draw_split(
    svg: &mut String,
    label_svg: &mut String,
    group: Node,
    points: &[Slice],
    frame: Frame,
    colors: &ChartColors,
    labels: Option<&LabelStyle>,
) {
    let total = points.iter().map(|p| p.weight).sum::<f64>();
    if total <= 0. {
        return;
    }
    let (mut first, second) = split(group, points, total, colors.spreadsheet_defaults);
    let combined = second.iter().map(|p| p.weight).sum::<f64>();
    let total_value = second.iter().map(|p| p.value.abs()).sum::<f64>();
    let explosion = points.iter().map(|p| p.explosion).fold(0., f64::max);
    if first.is_empty() || combined <= 0. || second.is_empty() {
        let circle = Circle {
            x: frame.x,
            y: frame.y,
            r: frame.width.min(frame.height) / 2. / (1. + explosion),
        };
        Ring {
            circle,
            inner: 0.,
            start: -PI / 2.,
            series: 0,
            colors,
            overall: total,
            labels,
        }
        .draw(svg, label_svg, &points.iter().collect::<Vec<_>>());
        return;
    }
    let other = Slice {
        aggregate: true,
        index: points
            .iter()
            .map(|p| p.index)
            .max()
            .unwrap_or(0)
            .saturating_add(1),
        value: total_value,
        defined: true,
        weight: combined,
        name: "Other".into(),
        explosion,
    };
    first.push(&other);
    let bar = val(Some(group), "ofPieType") == Some("bar");
    let scale = number(val(Some(group), "secondPieSize"))
        .unwrap_or(75.)
        .clamp(5., 200.)
        / 100.;
    let gap = number(val(Some(group), "gapWidth"))
        .unwrap_or(if colors.spreadsheet_defaults {
            100.
        } else {
            150.
        })
        .clamp(0., 500.)
        / 100.;
    let units_for = |scale: f64| {
        if bar {
            2. + gap / 2. + scale
        } else {
            2. + gap + scale * 2.
        }
    };
    let visible_labels = labels.is_some_and(|labels| {
        !colors.spreadsheet_defaults
            || labels.options.descendants().any(|n| {
                [
                    "showVal",
                    "showPercent",
                    "showSerName",
                    "showCatName",
                    "showLegendKey",
                ]
                .iter()
                .any(|k| chart_node(n, k))
                    && n.attribute("val").map_or(true, |v| on(Some(v)))
                    || chart_node(n, "tx")
            })
    });
    let label_inset = if visible_labels { 0.125 } else { 0. };
    let space_w = (frame.width - (frame.width + 88. / 3.) * label_inset).max(1.);
    let space_h = (frame.height - (frame.height + 88. / 3.) * label_inset).max(1.);
    let scale = if colors.spreadsheet_defaults
        && scale > 1.
        && space_w / units_for(scale) > space_h / 2. / scale
    {
        2.
    } else {
        scale
    };
    let units = units_for(scale);
    let effective = (space_w / units).min(space_h / 2. / scale.max(1.));
    let radius = effective / (1. + explosion);
    let left = frame.x - units * effective / 2.;
    let main = Circle {
        x: left + effective,
        y: frame.y,
        r: radius,
    };
    let right = left + units * effective;
    let start = PI * combined / total;
    let secondary_radius = scale * radius;
    let secondary = Circle {
        x: right - scale * effective,
        y: frame.y,
        r: secondary_radius,
    };
    let bar_left = right - scale * effective;
    if let Some(lines) = child(group, "serLines") {
        let stroke = child(lines, "spPr")
            .and_then(|p| colors.shape_stroke(p))
            .unwrap_or_else(|| "black".to_owned());
        let shifted = main.displaced(radius * other.explosion, 0.);
        for sign in [-1., 1.] {
            let (x, y) = shifted.at(sign * start);
            let (x2, y2) = if bar {
                (bar_left, frame.y + sign * scale * effective)
            } else {
                let vertical = sign * PI / 2.;
                let mut angle = start;
                let mut edge = secondary.at(vertical);
                for p in &second {
                    let sweep = TAU * p.weight / combined;
                    if (vertical - angle).rem_euclid(TAU) <= sweep {
                        edge = secondary
                            .displaced(secondary_radius * p.explosion, angle + sweep / 2.)
                            .at(vertical);
                        break;
                    }
                    angle += sweep;
                }
                edge
            };
            line(svg, x, y, x2, y2, &stroke);
        }
    }
    let ring = Ring {
        circle: main,
        inner: 0.,
        series: 0,
        colors,
        start,
        overall: total,
        labels,
    };
    ring.draw(svg, label_svg, &first);
    if bar {
        let mut y = frame.y - scale * effective;
        for p in second {
            let h = 2. * scale * effective * p.weight / combined;
            let fill = colors.fill(0, Some(p.index));
            let stroke = colors.outline(0, Some(p.index)).unwrap_or("none");
            let w = scale * effective;
            colors.begin_hit(svg, "slice", 0, Some(p.index));
            write!(svg,r#"<rect x="{bar_left:.3}" y="{y:.3}" width="{w:.3}" height="{h:.3}" fill="{fill}" stroke="{stroke}"/>"#).unwrap();
            colors.end_hit(svg);
            if let Some(labels) = labels {
                colors.begin_hit(label_svg, "dataLabel", 0, Some(p.index));
                labels.write(
                    label_svg,
                    p,
                    p.weight / total,
                    if labels.legacy {
                        bar_left + w + labels.size
                    } else {
                        bar_left + w / 2.
                    },
                    y + h / 2.,
                    w,
                    &fill,
                    None,
                    None,
                );
                colors.end_hit(label_svg);
            }
            y += h;
        }
    } else {
        Ring {
            circle: secondary,
            ..ring
        }
        .draw(svg, label_svg, &second);
    }
}

#[derive(Clone, Copy)]
struct Frame {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

fn plot_frame(plot: Node, width: f64, height: f64) -> Frame {
    let layout = child(plot, "layout").and_then(|n| child(n, "manualLayout"));
    let left = number(val(layout, "x")).unwrap_or(0.).clamp(0., 1.) * width;
    let top = number(val(layout, "y")).unwrap_or(0.).clamp(0., 1.) * height;
    let w = number(val(layout, "w")).map_or(width - 88. / 3., |n| n.clamp(0., 1.) * width);
    let h = number(val(layout, "h")).map_or(height - 88. / 3., |n| n.clamp(0., 1.) * height);
    Frame {
        x: if layout.is_some() {
            left + w / 2.
        } else {
            width / 2.
        },
        y: if layout.is_some() {
            top + h / 2.
        } else {
            height / 2.
        },
        width: w.max(1.),
        height: h.max(1.),
    }
}

fn legend_grid(
    widths: impl Iterator<Item = f64>,
    width: f64,
    size: f64,
) -> Option<(usize, f64, usize)> {
    let spacing = size * (0.527 + 0.95);
    let (count, longest, total) = widths.fold((0usize, 0f64, 0f64), |(n, max, sum), w| {
        (n + 1, max.max(w), sum + w + spacing)
    });
    if count == 0 || total <= width - 2. * size {
        return None;
    }
    let column_width = longest + spacing;
    let columns = ((width - 2. * size) / column_width).floor().max(1.) as usize;
    Some((columns, column_width, count.div_ceil(columns)))
}

fn legend(
    svg: &mut String,
    node: Node,
    entries: &[(usize, u32, &str)],
    colors: &ChartColors,
    width: f64,
    height: f64,
    size: f64,
    face: &str,
) {
    let pos = val(Some(node), "legendPos").unwrap_or("r");
    let side = matches!(pos, "l" | "r" | "tr");
    let hidden: std::collections::HashSet<u32> = node
        .children()
        .filter(|n| chart_node(*n, "legendEntry") && on(val(Some(*n), "delete")))
        .filter_map(|n| val(Some(n), "idx")?.parse().ok())
        .collect();
    let items: Vec<_> = entries
        .iter()
        .filter(|(_, idx, _)| !hidden.contains(idx))
        .map(|e| {
            let advance = text_width(e.2, size);
            let advance = if colors.spreadsheet_defaults && face.contains("Aptos Narrow") {
                advance * 0.82
            } else {
                advance
            };
            (e, advance)
        })
        .collect();
    let odf_side = colors.opendocument_defaults
        && matches!(pos, "l" | "r")
        && child(node, "layout")
            .and_then(|n| child(n, "manualLayout"))
            .is_none();
    let key = size * if odf_side { 0.6 } else { 0.527 };
    let layout = odf_side.then(|| {
        super::odf_text::LegendLayout::side(
            width,
            height,
            size,
            face,
            key,
            items.iter().map(|(e, w)| {
                super::odf_text::label_width(e.2, size, face, false, false).unwrap_or(*w)
            }),
            pos == "l",
        )
    });
    let step = layout.as_ref().map_or(size * 1.76, |l| l.step);
    let max_name = items.iter().map(|(_, w)| *w).fold(0., f64::max);
    let total = items
        .iter()
        .map(|(_, w)| w + key + size * 0.95)
        .sum::<f64>();
    let grid = (colors.spreadsheet_defaults && !side)
        .then(|| legend_grid(items.iter().map(|(_, w)| *w), width, size))
        .flatten();
    let mut x = if side {
        if pos == "l" {
            size
        } else {
            width - size - max_name - key - size * 0.225
        }
    } else {
        (width - total) / 2. + size * 0.32
    };
    if let Some((columns, column_width, _)) = grid {
        x = (width - columns as f64 * column_width) / 2. + size * 0.32;
    }
    let start_x = x;
    let n = if side {
        items.len().min((height / step).floor() as usize)
    } else {
        items.len()
    };
    let mut y = if side {
        if pos == "tr" {
            size * 1.5
        } else {
            (height - (n.saturating_sub(1) as f64) * step) / 2.
        }
    } else if pos == "t" {
        size * 1.48
    } else {
        height
            - size
                * if colors.spreadsheet_defaults {
                    1.68
                } else {
                    1.48
                }
    };
    if let Some((_, _, rows)) = grid {
        if pos != "t" {
            y -= rows.saturating_sub(1) as f64 * step;
        }
    }
    if let Some(l) = &layout {
        x = l.left + l.padding_x;
        y = l.top + l.padding_y + l.line_height / 2.;
        l.frame(svg, Some(node), colors);
    }
    for (i, (&(series, index, label), w)) in items.into_iter().take(n).enumerate() {
        colors.begin_hit(svg, "legendEntry", series, None);
        let fill = colors.fill(series, Some(index));
        let stroke = colors.outline(series, Some(index)).unwrap_or("none");
        write!(svg,r#"<rect x="{x:.2}" y="{:.2}" width="{key:.2}" height="{key:.2}" fill="{fill}" stroke="{stroke}"/>"#,y-key/2.).unwrap();
        text(
            svg,
            x + key + layout.as_ref().map_or(size * 0.225, |l| l.text_gap),
            y + layout
                .as_ref()
                .map_or(size * 0.3, |l| l.ascent - l.line_height / 2.),
            "start",
            size,
            face,
            label,
        );
        colors.end_hit(svg);
        if side {
            y += step;
        } else if let Some((columns, column_width, _)) = grid {
            x = start_x + ((i + 1) % columns) as f64 * column_width;
            if (i + 1) % columns == 0 {
                y += step;
            }
        } else {
            x += w + key + size * 0.95;
        }
    }
}

pub(super) fn render(
    root: Node,
    data: &ChartData,
    width: f64,
    height: f64,
    colors: &ChartColors,
    minor_latin: Option<&str>,
) -> Option<String> {
    let chart = child(root, "chart")?;
    let plot = child(chart, "plotArea")?;
    let group = plot.children().find(|n| {
        ["pieChart", "doughnutChart", "ofPieChart"]
            .iter()
            .any(|k| chart_node(*n, k))
    })?;
    let kind = group.tag_name().name().strip_suffix("Chart")?;
    let nodes: Vec<_> = group.children().filter(|n| chart_node(*n, "ser")).collect();
    let ring_series = nodes.len();
    let scatter = child(plot, "scatterChart").filter(|_| colors.spreadsheet_defaults);
    if let Some(overlay) = scatter {
        let (rings, series) = data.series.split_at_checked(ring_series)?;
        let expected = [group, overlay];
        let groups = plot
            .children()
            .filter(|n| n.is_element() && n.tag_name().name().ends_with("Chart"));
        if kind != "doughnut"
            || !groups.eq(expected)
            || child(chart, "legend").is_some()
            || rings.iter().any(|s| s.chart_type != kind)
            || series.iter().any(|s| s.chart_type != "scatter")
            || series.len() != overlay.children().filter(|n| chart_node(*n, "ser")).count()
        {
            return None;
        }
    } else if data.series.is_empty() || data.series.iter().any(|s| s.chart_type != kind) {
        return None;
    }
    let base_props = text_props(Some(root), false);
    let size = font_size(base_props, 40. / 3.);
    let face = font_family(base_props, minor_latin.unwrap_or("Calibri"));
    let mut frame = plot_frame(plot, width, height);
    let legacy = colors.legacy_defaults();
    let paint = child(root, "spPr");
    let bg = paint
        .and_then(|n| colors.shape_fill(n))
        .unwrap_or_else(|| "white".to_string());
    let border = paint
        .and_then(|n| colors.shape_stroke(n))
        .unwrap_or_else(|| "#868686".to_string());
    let corner = if !val(Some(root), "roundedCorners").map_or(!legacy, |v| on(Some(v))) {
        0.
    } else {
        40. / 3.
    };
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}"><rect width="{width}" height="{height}" rx="{corner:.2}" fill="{bg}" stroke="{border}"/>"#
    );
    let mut label_svg = String::new();
    let count = if kind == "doughnut" { nodes.len() } else { 1 };
    let sets: Vec<_> = data
        .series
        .iter()
        .zip(nodes.iter().copied())
        .take(count)
        .map(|(s, n)| slices(s, n))
        .collect();
    let mut ring_order: Vec<_> = (0..sets.len()).collect();
    ring_order.sort_by_key(|i| {
        val(Some(nodes[*i]), "order")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(*i)
    });
    let first_index = *ring_order.first()?;
    let first = &sets[first_index];
    let legend_node = child(chart, "legend");
    let legend_props = text_props(legend_node, false).or(base_props);
    let legend_size = font_size(legend_props, size);
    let legend_face = font_family(legend_props, minor_latin.unwrap_or("Calibri"));
    let mut entries: Vec<_> = if sets.len() > 1 && !colors.vary_by_point(first_index) {
        ring_order
            .iter()
            .map(|&idx| {
                (
                    idx,
                    idx as u32,
                    data.series[idx].name.as_deref().unwrap_or(""),
                )
            })
            .collect()
    } else {
        first
            .iter()
            .map(|p| (first_index, p.index, p.name.as_str()))
            .collect()
    };
    if colors.spreadsheet_defaults && colors.use_cached_point_count && sets.len() == 1 {
        let count = child(nodes[first_index], "cat")
            .and_then(|n| n.descendants().find(|p| chart_node(*p, "ptCount")))
            .and_then(|n| n.attribute("val")?.parse::<u32>().ok())
            .unwrap_or(0);
        let known: std::collections::HashSet<_> = entries.iter().map(|e| e.1).collect();
        // A styled blank category still has a legend key.
        entries.extend(
            nodes[first_index]
                .children()
                .filter(|n| chart_node(*n, "dPt"))
                .filter_map(|n| val(Some(n), "idx")?.parse::<u32>().ok())
                .filter(|i| *i < count && !known.contains(i))
                .map(|i| (first_index, i, "")),
        );
        entries.sort_by_key(|entry| entry.1);
    }
    let title = child(chart, "title");
    let title_props = text_props(title, true).or_else(|| text_props(title, false));
    let title_font = font_size(
        title_props,
        super::chart_svg::title_size_default(
            colors,
            root,
            size,
            if legacy || colors.spreadsheet_defaults {
                24.
            } else {
                size
            },
        ),
    );
    let unstyled_title =
        colors.spreadsheet_defaults && super::chart_svg::title_uses_automatic_font(root);
    let title_face = font_family(
        title_props,
        if unstyled_title {
            "Arial"
        } else {
            minor_latin.unwrap_or("Calibri")
        },
    );
    if (legacy || colors.spreadsheet_defaults || colors.opendocument_defaults)
        && child(plot, "layout")
            .and_then(|n| child(n, "manualLayout"))
            .is_none()
    {
        if title.is_some() && !on(val(title, "overlay")) {
            let leading = if colors.spreadsheet_defaults || title_face.contains("Cambria") {
                1.17
            } else {
                1.22
            };
            let h = 12. + title_font * leading;
            frame.y += h / 2.;
            frame.height -= h;
        }
        if legend_node.is_some() && !on(val(legend_node, "overlay")) {
            let pos = val(legend_node, "legendPos").unwrap_or("r");
            if matches!(pos, "l" | "r" | "tr") {
                let longest = entries
                    .iter()
                    .map(|e| text_width(e.2, legend_size))
                    .fold(0., f64::max);
                let w = longest + legend_size * 1.055 + 13.333;
                frame.x += if pos == "l" { w / 2. } else { -w / 2. };
                frame.width -= w;
            } else {
                let rows = if colors.spreadsheet_defaults {
                    legend_grid(
                        entries.iter().map(|e| {
                            text_width(e.2, legend_size)
                                * if legend_face.contains("Aptos Narrow") {
                                    0.82
                                } else {
                                    1.
                                }
                        }),
                        width,
                        legend_size,
                    )
                    .map_or(1, |(_, _, rows)| rows)
                } else {
                    1
                };
                let h = legend_size * 1.76 * rows as f64
                    + if colors.spreadsheet_defaults { 10. } else { 8. };
                frame.y += if pos == "t" { h / 2. } else { -h / 2. };
                frame.height -= h;
            }
        }
        frame.width = frame.width.max(1.);
        frame.height = frame.height.max(1.);
    }
    let explosion = sets
        .iter()
        .flatten()
        .map(|p| p.explosion)
        .fold(0., f64::max);
    let visible_labels = nodes.iter().any(|node| {
        child(*node, "dLbls")
            .or_else(|| child(group, "dLbls"))
            .is_some_and(|labels| {
                labels.descendants().any(|n| {
                    [
                        "showVal",
                        "showPercent",
                        "showSerName",
                        "showCatName",
                        "showLegendKey",
                    ]
                    .iter()
                    .any(|k| chart_node(n, k))
                        && n.attribute("val").map_or(true, |v| on(Some(v)))
                        || chart_node(n, "tx")
                })
            })
    });
    let automatic = child(plot, "layout")
        .and_then(|n| child(n, "manualLayout"))
        .is_none();
    let r = if colors.spreadsheet_defaults {
        let inset = if visible_labels && automatic {
            0.125
        } else {
            0.
        };
        (frame.width - width * inset)
            .min(frame.height - height * inset)
            .max(1.)
            / 2.
    } else {
        frame.width.min(frame.height) / 2. / (1. + explosion)
    };
    let hole = if kind == "doughnut" {
        number(val(Some(group), "holeSize"))
            .unwrap_or(50.)
            .clamp(10., 90.)
            / 100.
    } else {
        0.
    };
    let start = number(val(Some(group), "firstSliceAng"))
        .unwrap_or(0.)
        .to_radians()
        - PI / 2.;
    for (ring, i) in ring_order.into_iter().enumerate() {
        write!(svg, r#"<g data-docx-series="{i}">"#).unwrap();
        let points = &sets[i];
        let series = &data.series[i];
        let all_labels = child(nodes[i], "dLbls").or_else(|| child(group, "dLbls"));
        let props = if colors.spreadsheet_defaults {
            text_props(all_labels, false)
        } else {
            None
        };
        let label_size = font_size(props, size);
        let label_face = font_family(props, minor_latin.unwrap_or("Calibri"));
        let labels = all_labels.map(|options| LabelStyle {
            options,
            series: series.name.as_deref().unwrap_or(""),
            size: label_size,
            face: if colors.spreadsheet_defaults {
                &label_face
            } else {
                &face
            },
            legacy: legacy || colors.spreadsheet_defaults,
            spreadsheet: colors.spreadsheet_defaults,
            colors,
            width,
            height,
            format: child(nodes[i], "val")
                .and_then(|node| node.descendants().find(|n| chart_node(*n, "formatCode")))
                .and_then(|n| n.text()),
        });
        if kind == "ofPie" {
            draw_split(
                &mut svg,
                &mut label_svg,
                group,
                points,
                frame,
                colors,
                labels.as_ref(),
            );
        } else {
            let band = r * (1. - hole) / count as f64;
            let inner = r * hole + band * ring as f64;
            Ring {
                circle: Circle {
                    x: frame.x,
                    y: frame.y,
                    r: inner + band,
                },
                inner,
                series: i,
                start,
                colors,
                labels: labels.as_ref(),
                overall: points.iter().map(|p| p.weight).sum(),
            }
            .draw(&mut svg, &mut label_svg, &points.iter().collect::<Vec<_>>());
        }
        svg.push_str("</g>");
    }
    if let Some(scatter) = scatter {
        let side = frame.width.min(frame.height);
        super::cartesian_chart::scatter_overlay(
            &mut svg,
            &mut label_svg,
            scatter,
            &data.series[ring_series..],
            ring_series,
            colors,
            [frame.x - side / 2., frame.y - side / 2., side, side],
            (width, height),
            minor_latin.unwrap_or("Calibri"),
        )?;
    }
    svg.push_str(&label_svg);
    if title.is_some() {
        let props = title_props;
        let label = data
            .title
            .as_deref()
            .or_else(|| {
                if legacy || (colors.spreadsheet_defaults && count == 1) {
                    data.series.first()?.name.as_deref()
                } else {
                    None
                }
            })
            .unwrap_or("Chart Title");
        let font = title_font;
        let bold = props
            .and_then(|n| n.attribute("b"))
            .map_or(!unstyled_title, |a| a != "0" && a != "false");
        let paint = title_props
            .filter(|_| colors.spreadsheet_defaults)
            .and_then(|props| colors.shape_fill(props));
        if let Some(paint) = &paint {
            write!(svg, r#"<g fill="{paint}">"#).unwrap();
        }
        if bold {
            svg.push_str("<g font-weight=\"bold\">");
        }
        let (x, y, anchor) = title_position(
            title.filter(|_| colors.spreadsheet_defaults),
            width,
            height,
            font,
            10.4 + font * 0.925,
        );
        text(&mut svg, x, y, anchor, font, &title_face, label);
        if bold {
            svg.push_str("</g>");
        }
        if paint.is_some() {
            svg.push_str("</g>");
        }
    }
    if let Some(node) = legend_node {
        let paint = legend_props
            .filter(|_| colors.spreadsheet_defaults)
            .and_then(|props| colors.shape_fill(props));
        if let Some(paint) = &paint {
            write!(svg, r#"<g fill="{paint}">"#).unwrap();
        }
        legend(
            &mut svg,
            node,
            &entries,
            colors,
            width,
            height,
            legend_size,
            &legend_face,
        );
        if paint.is_some() {
            svg.push_str("</g>");
        }
    }
    svg.push_str("</svg>");
    Some(svg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_slices_have_nonzero_area() {
        let circle = Circle {
            x: 50.,
            y: 50.,
            r: 40.,
        };
        let mut disk = String::new();
        sector(&mut disk, circle, 0., 0., TAU, "red", "none");
        assert_eq!(
            roxmltree::Document::parse(&disk)
                .unwrap()
                .root_element()
                .tag_name()
                .name(),
            "circle"
        );
        let mut annulus = String::new();
        sector(&mut annulus, circle, 20., 0., TAU, "red", "none");
        let svg = roxmltree::Document::parse(&annulus).unwrap();
        assert_eq!(
            svg.root_element()
                .attribute("d")
                .unwrap()
                .matches('A')
                .count(),
            4
        );
        assert_eq!(svg.root_element().attribute("fill-rule"), Some("evenodd"));
    }

    #[test]
    fn custom_subplots_use_source_point_indices() {
        let doc=roxmltree::Document::parse(r#"<ofPieChart xmlns="http://schemas.openxmlformats.org/drawingml/2006/chart"><splitType val="cust"/><custSplit><secondPiePt val="7"/></custSplit></ofPieChart>"#).unwrap();
        let points = [1, 4, 7].map(|index| Slice {
            index,
            aggregate: false,
            value: 4.,
            defined: true,
            weight: 1.,
            name: String::new(),
            explosion: 0.,
        });
        let (main, second) = split(doc.root_element(), &points, 3., false);
        assert_eq!(main.iter().map(|p| p.index).collect::<Vec<_>>(), [1, 4]);
        assert_eq!(second.iter().map(|p| p.index).collect::<Vec<_>>(), [7]);
    }
}

use super::val;
use crate::chart::{chart_node, child, ChartPoint, ChartSeries};
use crate::parse::chart_colors::ChartColors;
use crate::parse::chart_svg::{
    font_family, font_size, format_value, number, text, text_props, text_width,
};
use roxmltree::Node;
use std::collections::BTreeMap;
use std::fmt::Write;

pub(in crate::parse) struct DataLabels<'a> {
    root_props: Option<Node<'a, 'a>>,
    defaults: [BTreeMap<&'a str, Node<'a, 'a>>; 2],
    overrides: [BTreeMap<u32, Node<'a, 'a>>; 2],
    source_format: Option<&'a str>,
    bubble_format: Option<&'a str>,
    default_face: &'a str,
    canvas: (f64, f64),
}

impl<'a> DataLabels<'a> {
    pub(in crate::parse) fn new(
        group: Node<'a, 'a>,
        series: Node<'a, 'a>,
        default_face: &'a str,
        canvas: (f64, f64),
    ) -> Option<Self> {
        let defaults = [child(series, "dLbls"), child(group, "dLbls")];
        if defaults.iter().all(Option::is_none) {
            return None;
        }
        let overrides = defaults.map(|node| {
            node.into_iter()
                .flat_map(|n| n.children())
                .filter(|n| chart_node(*n, "dLbl"))
                .filter_map(|n| Some((val(n, "idx")?.parse().ok()?, n)))
                .collect()
        });
        let source_format = child(series, "val")
            .or_else(|| child(series, "yVal"))
            .and_then(|n| n.descendants().find(|c| chart_node(*c, "formatCode")))
            .and_then(|n| n.text());
        let bubble_format = child(series, "bubbleSize")
            .and_then(|n| n.descendants().find(|c| chart_node(*c, "formatCode")))
            .and_then(|n| n.text());
        let defaults = defaults.map(|node| {
            node.into_iter()
                .flat_map(|n| n.children())
                .filter(|n| {
                    n.is_element() && chart_node(*n, n.tag_name().name()) && !chart_node(*n, "dLbl")
                })
                .map(|n| (n.tag_name().name(), n))
                .collect()
        });
        let root_props = text_props(Some(group.document().root_element()), false);
        Some(Self {
            root_props,
            defaults,
            overrides,
            source_format,
            bubble_format,
            default_face,
            canvas,
        })
    }

    pub(in crate::parse) fn draw(
        &self,
        svg: &mut String,
        series: &ChartSeries,
        point: &ChartPoint,
        end: (f64, f64),
        start: (f64, f64),
        default_pos: &str,
        horizontal: bool,
        marker_radius: f64,
        ordinal: usize,
        colors: &ChartColors,
    ) {
        let prop = |name| {
            (0..2).find_map(|i| {
                self.overrides[i]
                    .get(&point.index)
                    .and_then(|&n| child(n, name))
                    .or_else(|| self.defaults[i].get(name).copied())
            })
        };
        let show = |name, default| {
            prop(name).map_or(default, |n| {
                !matches!(n.attribute("val"), Some("false" | "0" | "off"))
            })
        };
        let number_format = |source: Option<&'a str>| {
            let node = prop("numFmt");
            let explicit = node.and_then(|n| n.attribute("formatCode"));
            if node
                .is_some_and(|n| matches!(n.attribute("sourceLinked"), Some("1" | "true" | "on")))
            {
                source.or(explicit)
            } else {
                explicit.or(source)
            }
        };
        if show("delete", false) {
            return;
        }
        let rich = prop("tx");
        let label = if let Some(tx) = rich {
            if let Some(body) = child(tx, "rich") {
                body.children()
                    .filter(|n| n.tag_name().name() == "p")
                    .map(|p| {
                        p.descendants()
                            .filter(|n| n.is_element())
                            .filter_map(|n| match n.tag_name().name() {
                                "t" => n.text(),
                                "br" => Some("\n"),
                                _ => None,
                            })
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                tx.descendants()
                    .find(|n| chart_node(*n, "v"))
                    .and_then(|n| n.text())
                    .unwrap_or("")
                    .to_owned()
            }
        } else {
            let mut fields = Vec::new();
            if show("showSerName", false) {
                fields.push(
                    series
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("Series {}", ordinal + 1)),
                );
            }
            if show("showCatName", false) {
                let label = point
                    .categories
                    .iter()
                    .min_by_key(|c| c.level)
                    .map(|c| c.label.clone())
                    .or_else(|| point.x.clone())
                    .unwrap_or_else(|| (u64::from(point.index) + 1).to_string());
                fields.push(label);
            }
            if show(
                "showVal",
                prop("showCatName").is_none() && prop("showSerName").is_none(),
            ) {
                let value = point.value.as_deref().or(point.y.as_deref()).unwrap_or("0");
                let code = number_format(self.source_format);
                fields.push(
                    number(Some(value)).map_or_else(|| value.to_owned(), |v| format_value(v, code)),
                );
            }
            if show("showBubbleSize", false) {
                if let Some(value) = point.bubble_size.as_deref() {
                    let code = number_format(self.bubble_format);
                    fields.push(
                        number(Some(value))
                            .map_or_else(|| value.to_owned(), |v| format_value(v, code)),
                    );
                }
            }
            fields.join(prop("separator").and_then(|n| n.text()).unwrap_or(", "))
        };
        let key = show("showLegendKey", false);
        if label.is_empty() && !key {
            return;
        }
        let root_props = self.root_props;
        let text_body = prop("tx").or_else(|| prop("txPr"));
        let props = text_body.and_then(|n| {
            n.descendants()
                .find(|c| c.is_element() && matches!(c.tag_name().name(), "rPr" | "defRPr"))
        });
        let size = font_size(props, font_size(root_props, 40. / 3.));
        let family = font_family(props.or(root_props), self.default_face);
        let bubble = series.chart_type == "bubble";
        let measure = |text: &str| {
            if bubble {
                crate::parse::font_metrics::width(text, size, &family, false, false)
                    .unwrap_or_else(|| text_width(text, size))
            } else {
                text_width(text, size)
            }
        };
        let layout = prop("layout").and_then(|n| child(n, "manualLayout"));
        let extended_layout = prop("extLst")
            .and_then(|n| {
                n.descendants().find(|n| {
                    n.has_tag_name((
                        "http://schemas.microsoft.com/office/drawing/2012/chart",
                        "layout",
                    ))
                })
            })
            .and_then(|n| child(n, "manualLayout"));
        let extent = |axis, dimension| {
            number(
                extended_layout
                    .and_then(|n| val(n, axis))
                    .or_else(|| layout.and_then(|n| val(n, axis))),
            )
            .filter(|n| *n > 0.)
            .map(|n| n * dimension)
        };
        let explicit_width = extent("w", self.canvas.0);
        let explicit_height = extent("h", self.canvas.1);
        let key_width = if key {
            if bubble {
                56. / 3.
            } else {
                size * 0.53 + 3.
            }
        } else {
            0.
        };
        let mut rows = Vec::new();
        for paragraph in label.split('\n') {
            if bubble && rich.is_none() && explicit_width.is_none() {
                let mut row = String::new();
                for word in paragraph.split_whitespace() {
                    let next = if row.is_empty() {
                        word.to_owned()
                    } else {
                        format!("{row} {word}")
                    };
                    let key = if rows.is_empty() { key_width } else { 0. };
                    if !row.is_empty() && measure(&next) + key > 400. / 3. {
                        rows.push(row);
                        row = word.to_owned();
                    } else {
                        row = next;
                    }
                }
                rows.push(row);
            } else {
                rows.push(paragraph.to_owned());
            }
        }
        let w = rows
            .iter()
            .enumerate()
            .map(|(i, s)| measure(s) + if i == 0 { key_width } else { 0. })
            .fold(0., f64::max);
        let line_height = size * if bubble { 1.2205 } else { 1.16 };
        let h = if bubble {
            line_height * rows.len() as f64 + 4.
        } else {
            size + (rows.len() - 1) as f64 * line_height
        };
        let box_width = explicit_width.unwrap_or(w + if bubble { 8. } else { 0. });
        let box_height = explicit_height.unwrap_or(h);
        let ox = box_width / 2.
            + if explicit_width.is_some() {
                0.
            } else if bubble {
                8. / 3.
            } else {
                8.
            }
            + marker_radius;
        let oy = box_height / 2. + if explicit_height.is_some() { 0. } else { 7.2 } + marker_radius;
        let (dx, dy) = if horizontal {
            (if end.0 < start.0 { -ox } else { ox }, 0.)
        } else {
            (0., if end.1 > start.1 { oy } else { -oy })
        };
        let pos = prop("dLblPos")
            .and_then(|n| n.attribute("val"))
            .unwrap_or(default_pos);
        let pos = if end == start && default_pos != "r" {
            "outEnd"
        } else {
            pos
        };
        let (x, y) = match pos {
            "r" => (end.0 + ox, end.1),
            "l" => (end.0 - ox, end.1),
            "t" => (end.0, end.1 - oy),
            "b" => (end.0, end.1 + oy),
            "outEnd" => (end.0 + dx, end.1 + dy),
            "inEnd" => (end.0 - dx, end.1 - dy),
            "inBase" => (start.0 + dx, start.1 + dy),
            "ctr" if default_pos == "r" => end,
            _ => ((end.0 + start.0) / 2., (end.1 + start.1) / 2.),
        };
        let coordinate = |axis, mode, center, extent, dimension| {
            let Some(value) = layout.and_then(|n| number(val(n, axis))) else {
                return center;
            };
            if layout.and_then(|n| val(n, mode)) == Some("edge") {
                value * dimension + extent / 2.
            } else {
                center + value * dimension
            }
        };
        let x = coordinate("x", "xMode", x, box_width, self.canvas.0);
        let y = coordinate("y", "yMode", y, box_height, self.canvas.1);
        let rot = text_body
            .and_then(|n| n.descendants().find(|c| c.tag_name().name() == "bodyPr"))
            .and_then(|n| number(n.attribute("rot")))
            .unwrap_or(0.)
            / 60000.;
        let paint = props
            .and_then(|n| colors.shape_fill(n))
            .or_else(|| root_props.and_then(|n| colors.shape_fill(n)))
            .unwrap_or_else(|| "#000000".into());
        write!(
            svg,
            r#"<g transform="translate({x:.2} {y:.2}) rotate({rot:.2})" fill="{paint}""#
        )
        .unwrap();
        if props
            .and_then(|n| n.attribute("b"))
            .is_some_and(|v| matches!(v, "1" | "true" | "on"))
        {
            svg.push_str(r#" font-weight="bold""#);
        }
        if props
            .and_then(|n| n.attribute("i"))
            .is_some_and(|v| matches!(v, "1" | "true" | "on"))
        {
            svg.push_str(r#" font-style="italic""#);
        }
        colors.hit_attrs(svg, "dataLabel", ordinal, Some(point.index));
        svg.push('>');
        if let Some(shape) = prop("spPr") {
            let fill = colors.shape_fill(shape).unwrap_or_else(|| "none".into());
            let stroke = colors.shape_stroke(shape).unwrap_or_else(|| "none".into());
            write!(svg,r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{fill}" stroke="{stroke}"/>"#,-box_width/2.-2.,-box_height/2.-2.,box_width+4.,box_height+4.).unwrap();
        }
        let first_y = size * 0.375 - (rows.len() - 1) as f64 * line_height / 2.;
        if key {
            let fill = colors.fill(ordinal, Some(point.index));
            let side = size * 0.53;
            let x = -(measure(&rows[0]) + key_width) / 2.
                + if bubble { (key_width - side) / 2. } else { 0. };
            let y = first_y - side;
            if series.chart_type == "bubble" {
                let body = format!(
                    r#"<circle cx="{:.2}" cy="{:.2}" r="{:.2}" fill="{fill}"/>"#,
                    x + side / 2.,
                    y + side / 2.,
                    side / 2.
                );
                colors.paint_point(
                    svg,
                    ordinal,
                    Some(point.index),
                    "dataPoint",
                    &fill,
                    [x, y, side, side],
                    &body,
                );
            } else {
                write!(svg, r#"<rect x="{x:.2}" y="{y:.2}" width="{side:.2}" height="{side:.2}" fill="{fill}"/>"#).unwrap();
            }
        }
        for (i, row) in rows.iter().enumerate() {
            text(
                svg,
                if i == 0 || bubble { key_width / 2. } else { 0. },
                first_y + line_height * i as f64,
                "middle",
                size,
                &family,
                row,
            );
        }
        svg.push_str("</g>");
    }
}

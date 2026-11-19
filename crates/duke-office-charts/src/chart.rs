use roxmltree::Node;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartData {
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub axis_titles: Vec<String>,
    pub series: Vec<ChartSeries>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartSeries {
    pub chart_type: String,
    pub name: Option<String>,
    pub points: Vec<ChartPoint>,
}

/// Source indices are zero-based. Values retain their original spelling and precision.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartPoint {
    pub index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<Arc<str>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<ChartCategory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bubble_size: Option<String>,
}

/// A label at its stored hierarchy level, starting at zero for the leaf level.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartCategory {
    pub level: usize,
    pub label: String,
}

pub(crate) const CHARTEX_NS: &str = "http://schemas.microsoft.com/office/drawing/2014/chartex";

pub(crate) fn cx_child<'a>(parent: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    parent
        .children()
        .find(|n| n.has_tag_name((CHARTEX_NS, name)))
}

pub(crate) fn chart_node(node: Node, name: &str) -> bool {
    node.tag_name().name() == name
        && matches!(
            node.tag_name().namespace(),
            Some(
                "http://schemas.openxmlformats.org/drawingml/2006/chart"
                    | "http://purl.oclc.org/ooxml/drawingml/chart"
            )
        )
}

pub(crate) fn child<'a>(node: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    node.children().find(|n| chart_node(*n, name))
}

fn label_text(owner: Node, mut field_value: impl FnMut(&str) -> Option<String>) -> Option<String> {
    let tx = child(owner, "tx")?;
    if let Some(rich) = child(tx, "rich") {
        let field_cache = label_field_cache(owner);
        let drawing = |node: Node, name| {
            node.tag_name().name() == name
                && matches!(
                    node.tag_name().namespace(),
                    Some(
                        "http://schemas.openxmlformats.org/drawingml/2006/main"
                            | "http://purl.oclc.org/ooxml/drawingml/main"
                    )
                )
        };
        let mut text = String::new();
        for paragraph in rich.children().filter(|&n| drawing(n, "p")) {
            let before = text.len();
            if !text.is_empty() {
                text.push('\n');
            }
            let start = text.len();
            let mut missing_field = false;
            for run in paragraph.children().filter(Node::is_element) {
                if drawing(run, "br") {
                    text.push('\n');
                } else if drawing(run, "r") || drawing(run, "fld") {
                    let value = run
                        .children()
                        .find(|&n| drawing(n, "t"))
                        .and_then(|n| n.text())
                        .unwrap_or_default();
                    if drawing(run, "fld") {
                        if let Some(cached) = run.attribute("id").and_then(|id| field_cache.get(id))
                        {
                            text.push_str(cached);
                            continue;
                        }
                        if let Some(kind) = run.attribute("type").filter(|&kind| {
                            value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) == Some(kind)
                        }) {
                            if let Some(display) = field_value(kind) {
                                text.push_str(&display);
                            } else {
                                missing_field = true;
                            }
                            continue;
                        }
                    }
                    text.push_str(value);
                }
            }
            if missing_field && text.len() == start {
                text.truncate(before);
            }
        }
        return (!text.trim().is_empty()).then_some(text);
    }
    let value = tx.descendants().find(|&n| chart_node(n, "v"))?.text()?;
    (!value.is_empty()).then(|| value.to_owned())
}

fn label_field_cache<'a>(owner: Node<'a, 'a>) -> BTreeMap<&'a str, &'a str> {
    const NS: &str = "http://schemas.microsoft.com/office/drawing/2012/chart";
    let Some(extensions) = child(owner, "extLst") else {
        return BTreeMap::new();
    };
    extensions
        .descendants()
        .filter(|n| n.has_tag_name((NS, "dlblFTEntry")))
        .filter_map(|entry| {
            let id = entry
                .children()
                .find(|n| n.has_tag_name((NS, "txfldGUID")))?
                .text()?;
            let cache = entry
                .children()
                .find(|n| n.has_tag_name((NS, "dlblFieldTableCache")))?;
            Some((
                id,
                child(child(cache, "pt")?, "v")?.text().unwrap_or_default(),
            ))
        })
        .collect()
}

fn deleted(node: Node) -> bool {
    child(node, "delete")
        .is_some_and(|d| !matches!(d.attribute("val"), Some("0" | "false" | "off")))
}

enum PointLabel<'a> {
    Text(Arc<str>),
    Fields(Node<'a, 'a>),
}

fn point_labels<'a>(group: Node<'a, 'a>) -> BTreeMap<u32, Option<PointLabel<'a>>> {
    group
        .children()
        .filter(|&n| chart_node(n, "dLbl"))
        .filter_map(|label| {
            let index = child(label, "idx")?.attribute("val")?.parse().ok()?;
            if deleted(label) {
                return Some((index, None));
            }
            child(label, "tx")?;
            let mut has_fields = false;
            let text = label_text(label, |_| {
                has_fields = true;
                None
            });
            let label = if has_fields {
                Some(PointLabel::Fields(label))
            } else {
                text.map(|s| PointLabel::Text(Arc::from(s)))
            };
            Some((index, label))
        })
        .collect()
}

fn cached_format<'a>(series: Node<'a, 'a>, channel: &str) -> Option<&'a str> {
    child(series, channel)?
        .descendants()
        .find(|&n| chart_node(n, "formatCode"))?
        .text()
}

struct LabelValues<'a> {
    series: Option<&'a str>,
    value_format: Option<&'a str>,
    bubble_format: Option<&'a str>,
    pie_total: Option<f64>,
}

impl PointLabel<'_> {
    fn resolve(&self, point: &ChartPoint, values: &LabelValues) -> Option<Arc<str>> {
        let node = match self {
            Self::Text(value) => return Some(Arc::clone(value)),
            Self::Fields(node) => *node,
        };
        let format = [Some(node), node.parent_element()]
            .into_iter()
            .flatten()
            .find_map(|n| child(n, "numFmt"))
            .filter(|n| !matches!(n.attribute("sourceLinked"), Some("1" | "true")))
            .and_then(|n| n.attribute("formatCode"));
        let number = |raw: &str, code| {
            raw.parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .map(|n| crate::parse::format_value(n, code).trim().to_owned())
                .unwrap_or_else(|| raw.to_owned())
        };
        label_text(node, |kind| match kind {
            "VALUE" => Some(number(
                point.value.as_deref().or(point.y.as_deref())?,
                format.or(values.value_format),
            )),
            "BUBBLESIZE" => Some(number(
                point.bubble_size.as_deref()?,
                format.or(values.bubble_format),
            )),
            "CATEGORYNAME" => point.categories.first().map(|c| c.label.clone()),
            "SERIESNAME" => values.series.map(str::to_owned),
            "PERCENTAGE" => {
                let value = point.value.as_deref()?.parse::<f64>().ok()?;
                let share = value.abs() / values.pie_total?;
                let code = format.filter(|c| c.contains('%')).unwrap_or("0%");
                Some(crate::parse::format_value(share, Some(code)))
            }
            _ => None,
        })
        .map(Arc::from)
    }
}

fn series_data(
    series: Node,
    chart_type: &str,
    group_labels: &BTreeMap<u32, Option<PointLabel>>,
) -> ChartSeries {
    let name = child(series, "tx")
        .and_then(|tx| tx.descendants().find(|n| chart_node(*n, "v")))
        .and_then(|n| n.text())
        .map(str::to_owned);
    let mut points = BTreeMap::new();
    for channel in series.children().filter(Node::is_element) {
        let role = channel.tag_name().name();
        if !chart_node(channel, role)
            || !matches!(role, "cat" | "val" | "xVal" | "yVal" | "bubbleSize")
        {
            continue;
        }
        let Some(source) = channel.children().find(Node::is_element) else {
            continue;
        };
        let cache = source
            .children()
            .find(|n| {
                ["strCache", "numCache", "multiLvlStrCache"]
                    .iter()
                    .any(|name| chart_node(*n, name))
            })
            .unwrap_or(source);
        let mut levels: Vec<_> = cache.children().filter(|n| chart_node(*n, "lvl")).collect();
        if levels.is_empty() {
            levels.push(cache);
        }
        for (level, source) in levels.into_iter().enumerate() {
            for pt in source.children().filter(|n| chart_node(*n, "pt")) {
                let Some(index) = pt.attribute("idx").and_then(|s| s.parse::<u32>().ok()) else {
                    continue;
                };
                let value = child(pt, "v").map(|v| v.text().unwrap_or_default().to_owned());
                let point = points.entry(index).or_insert_with(|| ChartPoint {
                    index,
                    ..Default::default()
                });
                match role {
                    "cat" => {
                        if let Some(label) = value {
                            point.categories.push(ChartCategory { level, label });
                        }
                    }
                    "val" => point.value = value,
                    "xVal" => point.x = value,
                    "yVal" => point.y = value,
                    "bubbleSize" => point.bubble_size = value,
                    _ => unreachable!(),
                }
            }
        }
    }
    let local = child(series, "dLbls");
    if !local.is_some_and(deleted) && (local.is_some() || !group_labels.is_empty()) {
        let pie_total = if matches!(chart_type, "pie" | "pie3D" | "doughnut") {
            points
                .values()
                .filter_map(|p| p.value.as_deref())
                .try_fold(0.0, |sum, v| Some(sum + v.parse::<f64>().ok()?.abs()))
                .filter(|n| n.is_finite() && *n > 0.0)
        } else {
            None
        };
        let values = LabelValues {
            series: name.as_deref(),
            value_format: cached_format(series, "val").or_else(|| cached_format(series, "yVal")),
            bubble_format: cached_format(series, "bubbleSize"),
            pie_total,
        };
        let local_labels = local.map(point_labels).unwrap_or_default();
        for (&index, label) in &local_labels {
            if label.is_some() {
                points.entry(index).or_insert_with(|| ChartPoint {
                    index,
                    ..Default::default()
                });
            }
        }
        for (index, point) in &mut points {
            point.label = local_labels
                .get(index)
                .or_else(|| group_labels.get(index))
                .and_then(|label| label.as_ref()?.resolve(point, &values));
        }
    }
    ChartSeries {
        chart_type: chart_type.to_owned(),
        name,
        points: points.into_values().collect(),
    }
}

/// Read chart series from literal and cached values without evaluating formulas.
pub fn parse_chart_xml(content: &str) -> Option<ChartData> {
    let xml = roxmltree::Document::parse(content).ok()?;
    chart_data_from_root(xml.root_element())
}

pub(crate) fn chart_data_from_root(root: Node) -> Option<ChartData> {
    if root.has_tag_name((CHARTEX_NS, "chartSpace")) {
        return extended_data(root);
    }
    let chart = child(root, "chart")?;
    let plot = child(chart, "plotArea")?;
    let title = child(chart, "title").and_then(|n| label_text(n, |_| None));
    let axis_titles = plot
        .children()
        .filter(|&n| {
            ["catAx", "dateAx", "valAx", "serAx"]
                .iter()
                .any(|name| chart_node(n, name))
                && !deleted(n)
        })
        .filter_map(|axis| child(axis, "title").and_then(|n| label_text(n, |_| None)))
        .collect();
    let mut series = Vec::new();
    for group in plot.children().filter(Node::is_element) {
        if let Some(kind) = group.tag_name().name().strip_suffix("Chart") {
            let group_labels = child(group, "dLbls")
                .filter(|&n| !deleted(n))
                .map(point_labels)
                .unwrap_or_default();
            series.extend(
                group
                    .children()
                    .filter(|n| chart_node(*n, "ser"))
                    .map(|n| series_data(n, kind, &group_labels)),
            );
        }
    }
    Some(ChartData {
        title,
        subtitle: None,
        axis_titles,
        series,
    })
}

fn extended_text(node: Node) -> Option<String> {
    let value = cx_child(node, "tx")
        .and_then(|tx| cx_child(tx, "txData"))
        .and_then(|d| cx_child(d, "v"))
        .and_then(|v| v.text());
    if let Some(value) = value.filter(|v| !v.is_empty()) {
        return Some(value.to_owned());
    }
    let rich = cx_child(node, "tx")
        .and_then(|tx| cx_child(tx, "rich"))
        .or_else(|| cx_child(node, "txPr"))?;
    let text: String = rich
        .descendants()
        .filter(|n| {
            n.tag_name().name() == "t"
                && matches!(
                    n.tag_name().namespace(),
                    Some(
                        "http://schemas.openxmlformats.org/drawingml/2006/main"
                            | "http://purl.oclc.org/ooxml/drawingml/main"
                    )
                )
        })
        .filter_map(|n| n.text())
        .collect();
    (!text.is_empty()).then_some(text)
}

pub(crate) fn map_color_dimension<'a>(data: Node<'a, 'a>) -> Option<Node<'a, 'a>> {
    data.children().find(|n| {
        (n.has_tag_name((CHARTEX_NS, "numDim")) && n.attribute("type") == Some("colorVal"))
            || (n.has_tag_name((CHARTEX_NS, "strDim")) && n.attribute("type") == Some("colorStr"))
    })
}

fn extended_points(data: Node, layout: &str) -> Vec<ChartPoint> {
    let mut points = BTreeMap::new();
    let color_dimension = if layout == "regionMap" {
        map_color_dimension(data)
    } else {
        None
    };
    for dimension in data.children().filter(|n| {
        n.has_tag_name((CHARTEX_NS, "strDim")) || n.has_tag_name((CHARTEX_NS, "numDim"))
    }) {
        let role = dimension.attribute("type").unwrap_or_default();
        let role = if Some(dimension) == color_dimension {
            "val"
        } else if color_dimension.is_some() && matches!(role, "val" | "size") {
            continue;
        } else {
            role
        };
        if !matches!(role, "cat" | "val" | "size" | "x" | "y") {
            continue;
        }
        for (level, cache) in dimension
            .children()
            .filter(|n| n.has_tag_name((CHARTEX_NS, "lvl")))
            .enumerate()
        {
            for pt in cache
                .children()
                .filter(|n| n.has_tag_name((CHARTEX_NS, "pt")))
            {
                let Some(index) = pt.attribute("idx").and_then(|v| v.parse::<u32>().ok()) else {
                    continue;
                };
                let point = points.entry(index).or_insert_with(|| ChartPoint {
                    index,
                    ..Default::default()
                });
                let value = pt.text().unwrap_or_default().to_owned();
                match role {
                    "cat" => point.categories.push(ChartCategory {
                        level,
                        label: value,
                    }),
                    "val" | "size" => point.value = Some(value),
                    "x" => point.x = Some(value),
                    "y" => point.y = Some(value),
                    _ => unreachable!(),
                }
            }
        }
    }
    points.into_values().collect()
}

fn extended_data(root: Node) -> Option<ChartData> {
    let chart = cx_child(root, "chart")?;
    let plot = cx_child(cx_child(chart, "plotArea")?, "plotAreaRegion")?;
    let data = cx_child(root, "chartData");
    let datasets: BTreeMap<_, _> = data
        .into_iter()
        .flat_map(|d| d.children())
        .filter(|n| n.has_tag_name((CHARTEX_NS, "data")))
        .filter_map(|n| Some((n.attribute("id")?.parse::<u32>().ok()?, n)))
        .collect();
    let series = plot
        .children()
        .filter(|n| n.has_tag_name((CHARTEX_NS, "series")))
        .filter_map(|s| {
            let layout = s.attribute("layoutId")?;
            let id = cx_child(s, "dataId")
                .and_then(|d| d.attribute("val"))
                .and_then(|id| id.parse::<u32>().ok());
            Some(ChartSeries {
                chart_type: layout.to_owned(),
                name: extended_text(s),
                points: id
                    .and_then(|id| datasets.get(&id))
                    .map(|&d| extended_points(d, layout))
                    .unwrap_or_default(),
            })
        })
        .collect();
    Some(ChartData {
        title: cx_child(chart, "title").and_then(extended_text),
        subtitle: None,
        axis_titles: cx_child(chart, "plotArea")?
            .children()
            .filter(|n| {
                n.has_tag_name((CHARTEX_NS, "axis"))
                    && !matches!(n.attribute("hidden"), Some("1" | "true"))
            })
            .filter_map(|axis| cx_child(axis, "title").and_then(extended_text))
            .collect(),
        series,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_cache_uses_data_ids_and_preserves_sparse_hierarchy_levels() {
        let xml = r#"<x:chartSpace xmlns:x="http://schemas.microsoft.com/office/drawing/2014/chartex" xmlns:t="http://purl.oclc.org/ooxml/drawingml/main">
            <x:chartData><x:data id="9">
                <x:strDim type="cat"><x:lvl ptCount="4"><x:pt idx="1">A</x:pt><x:pt idx="3">B</x:pt></x:lvl><x:lvl ptCount="4"><x:pt idx="1">Group</x:pt></x:lvl></x:strDim>
                <x:numDim type="size"><x:lvl ptCount="4"><x:pt idx="3">12.00</x:pt></x:lvl></x:numDim>
            </x:data></x:chartData>
            <x:chart><x:title><x:tx><x:txData><x:v>Title</x:v></x:txData></x:tx><x:txPr><t:p><t:r><t:t>Title</t:t></t:r></t:p></x:txPr></x:title>
            <x:plotArea><x:plotAreaRegion><x:series layoutId="treemap"><x:tx><x:txData><x:v>Series</x:v></x:txData></x:tx><x:dataId val="9"/></x:series><x:series layoutId="treemap"><x:dataId val="33"/></x:series></x:plotAreaRegion></x:plotArea>
            </x:chart></x:chartSpace>"#;
        let chart = parse_chart_xml(xml).unwrap();
        assert_eq!(chart.title.as_deref(), Some("Title"));
        let series = &chart.series[0];
        assert_eq!(series.name.as_deref(), Some("Series"));
        assert_eq!(series.chart_type, "treemap");
        assert_eq!(
            series.points.iter().map(|p| p.index).collect::<Vec<_>>(),
            [1, 3]
        );
        assert_eq!(
            series.points[0].categories[1],
            ChartCategory {
                level: 1,
                label: "Group".into()
            }
        );
        assert_eq!(series.points[0].value, None);
        assert_eq!(series.points[1].value.as_deref(), Some("12.00"));
        assert!(chart.series[1].points.is_empty());
    }
}

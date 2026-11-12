//! Stored chart data, resolved styles and axis declarations.

use crate::style::{Family, Gradient, Props, Scope, StyleSheet};
use crate::text::Ctx;
use crate::units::{parse_length, parse_number, Length};
use crate::xml::{attr, child, elements, is, owned, text_content, Node, CHART, OFFICE, SVG, TABLE, TEXT};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chart {
    pub class: String,
    pub title: Option<String>,
    pub title_style: Props,
    pub subtitle: Option<String>,
    pub has_legend: bool,
    pub legend_position: Option<String>,
    pub legend_style: Props,
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub style: Props,
    pub gradients: std::collections::BTreeMap<String, Gradient>,
    pub plot_style: Props,
    pub wall_style: Option<Props>,
    /// Saved coordinate region, in chart-local units.
    pub plot_bounds: Option<[Length; 4]>,
    pub axes: Vec<ChartAxis>,
    pub categories: Vec<String>,
    pub series: Vec<ChartSeries>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChartAxis {
    pub dimension: String,
    pub name: Option<String>,
    pub title: Option<String>,
    pub style: Props,
    pub major_grid: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChartSeries {
    pub name: Option<String>,
    pub class: Option<String>,
    pub values: Vec<Option<f64>>,
    /// Domains in stored order; bubble charts store Y before X.
    pub domains: Vec<Vec<Option<f64>>>,
    pub style: Props,
    pub points: Vec<Props>,
    pub attached_axis: Option<String>,
}

#[derive(Clone)]
struct Cell {
    text: String,
    value: Option<f64>,
}

fn local_table(table: Node<'_, '_>, ctx: &mut Ctx<'_>) -> Vec<Vec<Cell>> {
    fn rows(parent: Node<'_, '_>, ctx: &mut Ctx<'_>, out: &mut Vec<Vec<Cell>>) {
        if !ctx.enter() { return; }
        for node in elements(parent) {
            if !ctx.count() { break; }
            if is(node, TABLE, "table-row") {
                let mut row = Vec::new();
                for cell in elements(node).filter(|cell| is(*cell, TABLE, "table-cell") || is(*cell, TABLE, "covered-table-cell")) {
                    let text: Vec<String> = elements(cell).filter(|p| is(*p, TEXT, "p")).map(text_content).collect();
                    let data = Cell { text: text.join(" "), value: attr(cell, OFFICE, "value").and_then(parse_number) };
                    for _ in 0..ctx.repeat(cell, "number-columns-repeated") {
                        if !ctx.count() { ctx.leave(); return; }
                        row.push(data.clone());
                    }
                }
                for _ in 0..ctx.repeat(node, "number-rows-repeated") {
                    for _ in &row {
                        if !ctx.count() { ctx.leave(); return; }
                    }
                    if !ctx.count() { ctx.leave(); return; }
                    out.push(row.clone());
                }
            } else if is(node, TABLE, "table-header-rows") || is(node, TABLE, "table-rows") || is(node, TABLE, "table-row-group") {
                rows(node, ctx, out);
            }
        }
        ctx.leave();
    }
    let mut out = Vec::new();
    rows(table, ctx, &mut out);
    out
}

fn cell_reference(reference: &str) -> Option<(usize, usize)> {
    let cell = reference.rsplit('.').next()?;
    let mut column = 0usize;
    let mut row = 0usize;
    let mut letters = 0;
    let mut digits = false;
    for ch in cell.chars() {
        match ch {
            '$' => {}
            'A'..='Z' if !digits => {
                column = column.checked_mul(26)?.checked_add(ch as usize - 'A' as usize + 1)?;
                letters += 1;
            }
            '0'..='9' if letters > 0 => {
                digits = true;
                row = row.checked_mul(10)?.checked_add(ch as usize - '0' as usize)?;
            }
            _ => return None,
        }
    }
    (letters > 0 && row > 0).then(|| (column - 1, row - 1))
}

fn range<'t>(grid: &'t [Vec<Cell>], address: &str) -> Vec<Option<&'t Cell>> {
    let mut quoted = false;
    let split = address.char_indices().find_map(|(index, ch)| {
        if ch == '\'' { quoted = !quoted; }
        (ch == ':' && !quoted).then_some(index)
    });
    let (first, last) = split.map_or((address, address), |index| (&address[..index], &address[index + 1..]));
    let (Some(first), Some(last)) = (cell_reference(first), cell_reference(last)) else { return Vec::new(); };
    let width = grid.iter().map(Vec::len).max().unwrap_or(0);
    let mut cells = Vec::new();
    for row in grid.iter().take(first.1.max(last.1).saturating_add(1)).skip(first.1.min(last.1)) {
        for column in first.0.min(last.0)..first.0.max(last.0).saturating_add(1).min(width) {
            cells.push(row.get(column));
        }
    }
    cells
}

fn heading(parent: Node<'_, '_>, name: &str) -> Option<String> {
    let lines: Vec<String> = elements(child(parent, CHART, name)?)
        .filter(|paragraph| is(*paragraph, TEXT, "p"))
        .map(text_content).collect();
    Some(lines.join(" ")).filter(|text| !text.trim().is_empty())
}

pub(crate) fn parse_chart(body: Node<'_, '_>, ctx: &mut Ctx<'_>, styles: &StyleSheet) -> Chart {
    let Some(chart) = child(body, CHART, "chart") else { return Chart::default(); };
    let class = |node: Node<'_, '_>| attr(node, CHART, "class").map(|s| s.rsplit(':').next().unwrap_or(s).to_owned());
    let props = |node: Node<'_, '_>| styles.resolve(Scope::Content, Family::Chart, attr(node, CHART, "style-name"));
    let grid = child(chart, TABLE, "table").map(|table| local_table(table, ctx)).unwrap_or_default();
    let mut result = Chart {
        class: class(chart).unwrap_or_default(), title: heading(chart, "title"), subtitle: heading(chart, "subtitle"),
        title_style: child(chart, CHART, "title").map(props).unwrap_or_default(),
        legend_style: child(chart, CHART, "legend").map(props).unwrap_or_default(),
        gradients: styles.gradients.clone(),
        has_legend: child(chart, CHART, "legend").is_some(),
        legend_position: child(chart, CHART, "legend").and_then(|n| owned(n, CHART, "legend-position")),
        width: attr(chart, SVG, "width").and_then(parse_length), height: attr(chart, SVG, "height").and_then(parse_length),
        style: props(chart), ..Default::default()
    };
    if let Some(title) = child(chart, CHART, "title") {
        if let Some(name) = title.descendants().find_map(|n| attr(n, TEXT, "style-name")) {
            let mut text = styles.resolve(Scope::Content, Family::Text, Some(name)).text;
            text.inherit(&result.title_style.text);
            result.title_style.text = text;
        }
    }
    let Some(plot) = child(chart, CHART, "plot-area") else { return result; };
    result.plot_style = props(plot);
    result.wall_style = child(plot, CHART, "wall").map(props);
    result.plot_bounds = child(plot, CHART, "coordinate-region").and_then(|region| {
        Some([attr(region, SVG, "x").and_then(parse_length)?, attr(region, SVG, "y").and_then(parse_length)?,
            attr(region, SVG, "width").and_then(parse_length)?, attr(region, SVG, "height").and_then(parse_length)?])
    });
    let by_rows = result.plot_style.chart.get("series-source").map(String::as_str) == Some("rows");
    let mut shared_domains: Vec<Vec<Option<f64>>> = Vec::new();
    for node in elements(plot) {
        if !ctx.count() { break; }
        if is(node, CHART, "axis") {
            if let Some(address) = child(node, CHART, "categories").and_then(|n| attr(n, TABLE, "cell-range-address")) {
                result.categories = range(&grid, address).into_iter().map(|c| c.map_or_else(String::new, |c| c.text.clone())).collect();
            }
            result.axes.push(ChartAxis {
                dimension: owned(node, CHART, "dimension").unwrap_or_default(), name: owned(node, CHART, "name"),
                title: heading(node, "title"), style: props(node),
                major_grid: elements(node).any(|n| is(n, CHART, "grid") && attr(n, CHART, "class") == Some("major")),
            });
        } else if is(node, CHART, "series") {
            let index = result.series.len() + 1;
            let fallback: Vec<Option<&Cell>> = if by_rows {
                grid.get(index).into_iter().flat_map(|row| row.iter().skip(1).map(Some)).collect()
            } else { grid.iter().skip(1).map(|row| row.get(index)).collect() };
            let values = attr(node, CHART, "values-cell-range-address").map(|address| range(&grid, address)).unwrap_or(fallback)
                .into_iter().map(|c| c.and_then(|c| c.value)).collect();
            let name = match attr(node, CHART, "label-cell-address") {
                Some(address) => range(&grid, address).first().and_then(|c| c.map(|c| c.text.clone())),
                None => if by_rows { grid.get(index).and_then(|r| r.first()) } else { grid.first().and_then(|r| r.get(index)) }.map(|c| c.text.clone()),
            };
            let mut domains: Vec<Vec<Option<f64>>> = elements(node).filter(|n| is(*n, CHART, "domain"))
                .map(|n| attr(n, TABLE, "cell-range-address").map(|a| range(&grid, a).into_iter().map(|c| c.and_then(|c| c.value)).collect()).unwrap_or_default()).collect();
            for (i, domain) in domains.iter().enumerate() {
                if shared_domains.len() <= i { shared_domains.push(domain.clone()); }
            }
            if domains.len() < shared_domains.len() { domains.extend_from_slice(&shared_domains[domains.len()..]); }
            let mut points = Vec::new();
            for point in elements(node).filter(|n| is(*n, CHART, "data-point")) {
                let repeat = attr(point, CHART, "repeated").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).min(ctx.limits.max_repeat);
                for _ in 0..repeat {
                    if !ctx.count() { break; }
                    points.push(props(point));
                }
            }
            result.series.push(ChartSeries { name: name.filter(|n| !n.is_empty()), class: class(node), values, domains,
                style: props(node), points, attached_axis: owned(node, CHART, "attached-axis") });
        }
    }
    if result.categories.is_empty() {
        result.categories = if by_rows { grid.first().into_iter().flat_map(|r| r.iter().skip(1)).map(|c| c.text.clone()).collect() }
            else { grid.iter().skip(1).filter_map(|r| r.first()).map(|c| c.text.clone()).collect() };
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cell_references() {
        assert_eq!(cell_reference("local-table.$B$2"), Some((1, 1)));
        assert_eq!(cell_reference(".AA10"), Some((26, 9)));
        assert_eq!(cell_reference("local-table.$1"), None);
        assert_eq!(cell_reference("B"), None);
        assert_eq!(cell_reference("b2"), None);
    }
}

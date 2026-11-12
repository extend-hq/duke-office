//! Charts: the title, categories and series an embedded chart document stores.

use crate::text::Ctx;
use crate::units::parse_number;
use crate::xml::{attr, child, elements, is, text_content, Node, CHART, OFFICE, TABLE, TEXT};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chart {
    /// `chart:class` without its prefix: `bar`, `line`, `circle`, `ring`, `area`, `scatter`, ...
    pub class: String,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub has_legend: bool,
    pub categories: Vec<String>,
    pub series: Vec<ChartSeries>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChartSeries {
    pub name: Option<String>,
    /// A series-specific class in a combined chart.
    pub class: Option<String>,
    /// One value per category; a cell without a number is `None`.
    pub values: Vec<Option<f64>>,
}

struct Cell {
    text: String,
    value: Option<f64>,
}

/// The chart's own copy of its data, as rows of cells.
fn local_table(table: Node<'_, '_>, ctx: &mut Ctx<'_>) -> Vec<Vec<Cell>> {
    fn rows(parent: Node<'_, '_>, ctx: &mut Ctx<'_>, out: &mut Vec<Vec<Cell>>) {
        for node in elements(parent) {
            if !ctx.count() {
                return;
            }
            if is(node, TABLE, "table-row") {
                let mut row = Vec::new();
                for cell in elements(node).filter(|cell| is(*cell, TABLE, "table-cell")) {
                    if !ctx.count() {
                        return;
                    }
                    let text: Vec<String> = elements(cell)
                        .filter(|paragraph| is(*paragraph, TEXT, "p"))
                        .map(text_content)
                        .collect();
                    row.push(Cell {
                        text: text.join(" "),
                        value: attr(cell, OFFICE, "value").and_then(parse_number),
                    });
                }
                out.push(row);
            } else if matches!(node.tag_name().name(), "table-header-rows" | "table-rows") {
                rows(node, ctx, out);
            }
        }
    }
    let mut out = Vec::new();
    rows(table, ctx, &mut out);
    out
}

/// Parses one end of a range such as `local-table.$B$2` into a zero-based column and row.
fn cell_reference(reference: &str) -> Option<(usize, usize)> {
    let cell = reference.rsplit('.').next()?;
    let mut column = 0usize;
    let mut row = 0usize;
    let mut letters = 0;
    for ch in cell.chars() {
        match ch {
            '$' => {}
            'A'..='Z' if row == 0 => {
                column = column.checked_mul(26)?.checked_add(ch as usize - 'A' as usize + 1)?;
                letters += 1;
            }
            '0'..='9' if letters > 0 => {
                row = row.checked_mul(10)?.checked_add(ch as usize - '0' as usize)?;
            }
            _ => return None,
        }
    }
    (letters > 0 && row > 0).then(|| (column - 1, row - 1))
}

/// The cells a range such as `local-table.$B$2:.$B$5` covers, row by row.
fn range<'t>(grid: &'t [Vec<Cell>], address: &str) -> Vec<&'t Cell> {
    let mut ends = address.split(':');
    let Some(first) = ends.next().and_then(cell_reference) else {
        return Vec::new();
    };
    let last = ends.next().and_then(cell_reference).unwrap_or(first);
    let mut cells = Vec::new();
    for row in first.1.min(last.1)..=first.1.max(last.1) {
        for column in first.0.min(last.0)..=first.0.max(last.0) {
            if let Some(cell) = grid.get(row).and_then(|row| row.get(column)) {
                cells.push(cell);
            }
        }
    }
    cells
}

fn heading(parent: Node<'_, '_>, name: &str) -> Option<String> {
    let lines: Vec<String> = elements(child(parent, CHART, name)?)
        .filter(|paragraph| is(*paragraph, TEXT, "p"))
        .map(text_content)
        .collect();
    Some(lines.join(" ")).filter(|text| !text.trim().is_empty())
}

/// Reads `office:chart`.
pub(crate) fn parse_chart(body: Node<'_, '_>, ctx: &mut Ctx<'_>) -> Chart {
    let Some(chart) = child(body, CHART, "chart") else {
        return Chart::default();
    };
    let class = |node: Node<'_, '_>| {
        attr(node, CHART, "class").map(|class| class.rsplit(':').next().unwrap_or(class).to_owned())
    };
    let grid = child(chart, TABLE, "table")
        .map(|table| local_table(table, ctx))
        .unwrap_or_default();
    let mut result = Chart {
        class: class(chart).unwrap_or_default(),
        title: heading(chart, "title"),
        subtitle: heading(chart, "subtitle"),
        has_legend: child(chart, CHART, "legend").is_some(),
        categories: Vec::new(),
        series: Vec::new(),
    };
    let Some(plot) = child(chart, CHART, "plot-area") else {
        return result;
    };
    for node in elements(plot) {
        if is(node, CHART, "axis") {
            if let Some(address) = child(node, CHART, "categories")
                .and_then(|categories| attr(categories, TABLE, "cell-range-address"))
            {
                result.categories = range(&grid, address).into_iter().map(|cell| cell.text.clone()).collect();
            }
        } else if is(node, CHART, "series") {
            // Without addresses, series follow the table's columns after the label column.
            let column = result.series.len() + 1;
            let values = match attr(node, CHART, "values-cell-range-address") {
                Some(address) => range(&grid, address).into_iter().map(|cell| cell.value).collect(),
                None => grid.iter().skip(1).map(|row| row.get(column).and_then(|cell| cell.value)).collect(),
            };
            let name = match attr(node, CHART, "label-cell-address") {
                Some(address) => range(&grid, address).first().map(|cell| cell.text.clone()),
                None => grid.first().and_then(|row| row.get(column)).map(|cell| cell.text.clone()),
            };
            result.series.push(ChartSeries {
                name: name.filter(|name| !name.is_empty()),
                class: class(node),
                values,
            });
        }
    }
    if result.categories.is_empty() {
        result.categories = grid
            .iter()
            .skip(1)
            .filter_map(|row| row.first())
            .map(|cell| cell.text.clone())
            .collect();
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

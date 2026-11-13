use super::*;

pub(super) fn draw(
    out: &mut String,
    plot: Node,
    data: &ChartData,
    colors: &ChartColors,
    rect: Box2,
    canvas: (f64, f64),
    size: f64,
    face: &str,
) {
    if !colors.opendocument_defaults {
        return;
    }
    let Some(table) = child(plot, "dTable") else {
        return;
    };
    let count = data
        .series
        .iter()
        .flat_map(|s| &s.points)
        .map(|p| u64::from(p.index) + 1)
        .max()
        .unwrap_or(0);
    if count == 0 || count > 512 {
        return;
    }
    let props = text_props(Some(table), false);
    let size = font_size(props, size);
    let face = font_family(props, face.split(',').next().unwrap_or(face));
    let cell = rect.width() / count as f64;
    let gap = size * 0.2;
    let left = size * 0.5;
    let names_width = (rect.left - left - gap).max(size);
    let keys = flag(Some(table), "showKeys", false);
    let key_width = if keys { size } else { 0. };
    let mut categories = BTreeMap::new();
    for p in data.series.iter().flat_map(|s| &s.points) {
        if let Some(cat) = p.categories.first() {
            categories.entry(p.index).or_insert_with(|| {
                wrap_label(&cat.label, size, (cell - 2. * gap).max(size), &face)
            });
        }
    }
    let header_height =
        categories.values().map(Vec::len).max().unwrap_or(1) as f64 * size * 1.1 + gap;
    let mut y = rect.bottom + gap;
    for (index, lines) in &categories {
        for (row, label) in lines.iter().enumerate() {
            text(
                out,
                rect.left + (f64::from(*index) + 0.5) * cell,
                y + size * (0.9 + row as f64 * 1.1),
                "middle",
                size,
                &face,
                label,
                false,
            );
        }
    }
    let top = y;
    y += header_height;
    let props = child(table, "spPr");
    let stroke = props
        .and_then(|p| colors.shape_stroke(p))
        .unwrap_or_else(|| "#868686".into());
    let mut row_edges = vec![y];
    for (i, series) in data.series.iter().take(256).enumerate() {
        if y >= canvas.1 {
            break;
        }
        let name = series
            .name
            .clone()
            .unwrap_or_else(|| format!("Series {}", i + 1));
        let lines = wrap_label(&name, size, (names_width - key_width).max(size), &face);
        let height = size * (lines.len() as f64 * 1.1 + 0.5);
        if keys {
            write!(
                out,
                r#"<rect x="{left:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{}"/>"#,
                y + size * 0.3,
                size * 0.6,
                size * 0.6,
                colors.fill(i, None)
            )
            .unwrap();
        }
        for (row, label) in lines.iter().enumerate() {
            text(
                out,
                left + key_width,
                y + size * (0.9 + row as f64 * 1.1),
                "start",
                size,
                &face,
                label,
                false,
            );
        }
        for point in &series.points {
            if let Some(raw) = point.value.as_deref() {
                let value = number(Some(raw)).map_or_else(
                    || raw.into(),
                    |v| super::super::chart_svg::format_value(v, Some("General")),
                );
                text(
                    out,
                    rect.left + (f64::from(point.index) + 0.5) * cell,
                    y + size * 0.9,
                    "middle",
                    size,
                    &face,
                    &value,
                    false,
                );
            }
        }
        y += height;
        row_edges.push(y);
    }
    if flag(Some(table), "showHorzBorder", false) {
        for edge in &row_edges {
            line(out, left, *edge, rect.right, *edge, &stroke);
        }
    }
    if flag(Some(table), "showVertBorder", false) {
        for column in 0..=count {
            let x = rect.left + column as f64 * cell;
            line(out, x, top, x, y, &stroke);
        }
    }
    if flag(Some(table), "showOutline", false) {
        write!(out, r#"<path d="M{:.2} {top:.2}H{:.2}V{y:.2}H{left:.2}V{:.2}H{:.2}Z" fill="none" stroke="{stroke}"/>"#, rect.left, rect.right, row_edges[0], rect.left).unwrap();
    }
}

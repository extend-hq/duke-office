# rust-office-charts

by Extend

Render native Office Open XML chart parts to SVG in Rust. The crate also reads cached chart values into a serializable model that preserves series order, sparse point indices and category hierarchy.

```rust
use rust_office_charts::{render_chart, RenderOptions};

fn render_slide_chart(xml: &str, width: f64, height: f64) -> Option<String> {
    let chart = render_chart(xml, &RenderOptions {
        width: Some(width),
        height: Some(height),
        ..Default::default()
    })?;
    Some(chart.svg)
}
```

The host opens the Office package and supplies the chart XML, size in CSS pixels, resolved theme colors, font and related chart style parts. The default uses the chart's cached values. A spreadsheet can supply current workbook values through `RenderOptions::data` in source series order, and set `spreadsheet_defaults` for omitted spreadsheet properties. Unsupported layouts return `None` so the host can use its own fallback.

Set `interactive` to emit hit targets for two-dimensional Cartesian charts. The SVG identifies a bar, line, marker, point anchor or legend entry with `data-chart-kind` and its zero-based series ordinal with `data-chart-series`. Point targets carry the source cache index in `data-chart-point`. Line marker and point anchor groups also carry `data-chart-x` and `data-chart-y` in SVG user coordinates. Hosts handle pointer events and selection state.

Default builds include the classic chart families, 3D and surface plots, and supported ChartEx types. Consumers that use only two-dimensional bar, line, area and scatter charts can disable default features to omit the additional renderers and map cache decoder.

Extracted from [extend-hq/react-docx](https://github.com/extend-hq/react-docx/tree/185025994fb74825506ff024c37fe67283d6fc6a). Licensed under the [MIT License](LICENSE).

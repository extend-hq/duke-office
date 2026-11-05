# rust-office-charts

Render native Office Open XML chart parts to SVG in Rust. The crate also reads cached chart values into a serializable model that preserves series order, sparse point indices and category hierarchy.

```rust
use extend_office_charts::{render_chart, RenderOptions};

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

Supported font stacks include Carlito for Calibri, Arimo for Arial, Tinos for Times New Roman, Cousine for Courier New, Intos for Aptos and Intos Narrow for Aptos Narrow. The shared `@extend-ai/office-fonts` package in this repository supplies browser loading, portable SVG embedding and native font assets. The host must load fonts before measuring or displaying text; generating an SVG does not install fonts. Selected Cartesian title, axis and legend layouts and ChartEx text use advance widths from the same six bundled font families, with existing estimates retained for other families or missing glyphs. These advances do not perform kerning, ligature substitution or complex-script shaping, and other chart layout paths still use estimates. Each metric table retains its font license alongside it.

Set `interactive` to emit hit targets across the classic and supported ChartEx families. Targets carry a `data-chart-kind` and a zero-based source series ordinal in `data-chart-series`. Point targets carry the source cache index in `data-chart-point`. Markers and point anchors can also carry `data-chart-x` and `data-chart-y` in SVG user coordinates. Legend entries use `legendEntry`. Hosts handle pointer events and selection state by finding the closest target ancestor.

Aggregate geometry, including histogram bins, box summaries, merged hierarchy branches and the combined split-pie slice, selects its source series. A surface facet selects the nearest projected source vertex. Surface color-band legends represent value ranges spanning series and have no source-series target. Interactive mode preserves visible rendering, and the default output omits these targets.

Default builds include the classic chart families, 3D and surface plots, and supported ChartEx types. Consumers that use only two-dimensional bar, line, area and scatter charts can disable default features to omit the additional renderers and map cache decoder.

Maps use the saved Office boundary cache, including its hierarchy, projection and attribution. The optional `map-fallback` feature adds offline country and US-state boundaries for spreadsheet charts without a usable cache. This fallback does not run for DOCX. An uncached location outside the atlas renders the chart frame and legend with an SVG description explaining that boundaries are unavailable. The browser `/full` entry includes this feature; the compact entry does not load its data.

Extracted from [extend-hq/react-docx](https://github.com/extend-hq/react-docx/tree/185025994fb74825506ff024c37fe67283d6fc6a). Licensed under the [MIT License](LICENSE).

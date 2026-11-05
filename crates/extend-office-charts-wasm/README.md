# extend-office-charts-wasm

WebAssembly bindings for rendering Office Open XML charts to SVG. The default
build includes two-dimensional bar, line, area and scatter charts. Enable
`extended-charts` for the other chart families supported by the Rust renderer.

The [browser package](https://github.com/extend-hq/extend-office) exposes the
default build at `@extend-ai/extend-office-charts` and the expanded build at
`@extend-ai/extend-office-charts/full`. Both export `loadOfficeCharts(source?)`
and return the same renderer interface. The expanded build loads only when its
entry point is used. Native Rust callers can use `extend-office-charts` directly.

The full browser build enables `map-fallback`: saved Office map geometry is preferred, with an offline country and US-state atlas available for spreadsheet charts that omit it. Other uncached subdivisions have no geometry; the SVG describes that limitation. The atlas and its license are bundled with the renderer, without a JavaScript geo library or a geocoding network request.

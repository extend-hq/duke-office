# Optional offline map boundaries

The `map-fallback` feature uses these boundaries only for spreadsheet charts without usable saved map geometry. Saved Office geometry always takes precedence. DOCX rendering does not use the fallback. The atlas covers countries and the 50 US states plus the District of Columbia; it does not geocode addresses, postal codes, counties or other subdivisions. Unknown locations are not assigned invented geometry.

The asset is derived from `world-atlas` 2.0.2 (Natural Earth, 1:50m countries) and `us-atlas` 3.0.1 (US Census, states at 1:10m). Their ISC notice is preserved in LICENSE. The source packages and datasets are available at https://github.com/topojson/world-atlas and https://github.com/topojson/us-atlas. Input and output SHA-256 hashes are recorded in manifest.json.

Regenerate with `python3 scripts/build-map-atlas.py <world-atlas/countries-50m.json> <us-atlas/states-10m.json>` from the repository root. Polygon rings retain their coordinates at 1e-5 degree precision and use even-odd filling; the renderer handles projection and display-pixel decimation. The binary is raw DEFLATE containing a versioned length-prefixed format, decoded once lazily. No JavaScript geometry or projection library is required at runtime.

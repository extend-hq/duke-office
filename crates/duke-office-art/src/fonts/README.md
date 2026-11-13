# Portable outline fonts

These fonts retain all original glyphs and OpenType shaping tables. TrueType hinting instructions and device-metric tables are omitted because the renderer outputs vector contours. Rebuild them with `python3 scripts/build-art-fonts.py` with FontTools installed. Each family directory retains its license and records upstream inputs, processing, and SHA-256 hashes of generated font files. The Carlito derivative uses the internal family name `OfficeArtSans` to respect its reserved font name.

The default outline renderer maps Arial/Helvetica to Arimo, Times New Roman to Tinos, Courier New to Cousine, and Calibri to the Carlito derivative. Aptos also falls back to the Carlito derivative, which is not metrically identical to Aptos. Unknown families use Arimo. `outline_text_with_font` accepts caller-supplied OpenType bytes when exact font matching is required. Missing glyphs return no outline so the caller can preserve its text fallback.

Line metrics select the OS/2 typographic metrics when `USE_TYPO_METRICS` is set, otherwise the Windows ascent/descent. See the [Microsoft OpenType OS/2 specification](https://learn.microsoft.com/en-us/typography/opentype/spec/os2).

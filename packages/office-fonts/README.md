# Office fonts

Shared font fallbacks for document, spreadsheet, presentation and chart renderers. Regular, bold, italic and bold italic are included under the SIL Open Font License.

| Requested font | Bundled fallback |
| --- | --- |
| Calibri | Carlito |
| Arial | Arimo |
| Times New Roman | Tinos |
| Courier New | Cousine |

Aptos and Aptos Narrow have no bundled substitutes. They use host-provided fonts or the caller's generic font stack, which can change text widths and line wrapping.

```js
import { loadOfficeFonts } from "@extend-ai/office-fonts";

await loadOfficeFonts(document, ["Calibri", "Times New Roman"]);
```

Call this before measuring, laying out or paginating content. Each family is a separate lazy chunk. Existing font-face definitions and locally installed original fonts take precedence; a fallback payload is downloaded only when a requested face needs it. Repeated calls share loads per document, including across bundled copies. Failed loads and removed faces can be retried. Imports are safe without a DOM. The default family is Calibri for backwards compatibility; passing an empty list loads nothing.

`officeFontFallback(family)` returns the corresponding fallback name for constructing font stacks. It does not register or load a font. For CSS-managed loading, import the family's stylesheet, such as `@extend-ai/office-fonts/tinos.css`, and wait for `document.fonts.load()` before measuring. The bundler must copy relative font URLs.

Inline SVG shares document fonts. SVG images and exported SVGs are separate documents: `await embedOfficeFonts(svg)` embeds the supported families and styles referenced by generated SVG markup. It leaves SVGs without supported font declarations unchanged. Each SVG carries its own font data; use inline SVG when possible. This function is not a sanitizer and expects generated SVG markup with font-family declarations, not arbitrary HTML or externally styled SVG.

Sharp uses Fontconfig instead of SVG web fonts. On Linux, set `FONTCONFIG_FILE` to `require.resolve("@extend-ai/office-fonts/fonts.conf")` before importing Sharp, or install the bundled TTF assets into the runtime image and the aliases from that configuration into Fontconfig. The supplied configuration preserves system fonts and adds this bundle without changing the machine's installation.

All sources are pinned by upstream commit or release and SHA-256 hashes in each font directory's `source.json`. Arimo uses static 400 and 700 instances of the upstream variable fonts. WOFF2 assets losslessly encode the bundled TTFs. Redistribution notices are collected in [FONT-LICENSES.txt](FONT-LICENSES.txt).

The selected families' printable ASCII advances match the locally installed Office originals in all four styles. Font versions, non-Latin coverage, glyph outlines, kerning, vertical metrics and layout algorithms can still differ. In particular, the native chart tables measure advances, not full OpenType shaping. There are no automatic aliases for Calibri Light, Cambria, Aptos, Aptos Narrow, Aptos Display, Aptos Serif or symbol fonts. Modern Caladea did not match the tested Cambria widths closely enough to enable by default. CJK and complex-script documents may require additional original or embedded fonts.

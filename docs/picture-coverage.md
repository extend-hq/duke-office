# Office picture rendering coverage

`rust-office-art` is shared by `rust-docx` and `rust-docx-wasm`. The parser resolves relationships and theme colors, preserves original assets and ordered presentation properties, and passes picture appearance to the shared renderer. The viewer consumes the resulting image and paint bounds; `presentation.transformApplied` prevents applying a second rotation or flip.

This is a measured implementation, not full OfficeArt or pixel-identical Word compatibility. Parsing a property, rendering it, preserving a stored preview, and proving visual equivalence are separate capabilities.

| Feature family | Current behavior | Remaining limitation |
| --- | --- | --- |
| Embedded, linked and SVG alternative images | Resolve available assets and retain source relationships | Missing SVG alternatives recover a raster asset; Word can display an error instead |
| Source crop and stretch destination | Signed and percentage rectangles | Invalid or inverted rectangles use preserved fallback |
| Rotation, flips and grouped transforms | Shared SVG and separate paint bounds; nested affine composition | Text and effects in arbitrary transformed groups need broader comparison |
| Geometry clipping | Rectangle, ellipse, rounded rectangle, triangle, diamond, delay and numeric single paths | Full preset geometry, formula evaluation and multipath geometry remain unsupported |
| Color and alpha | Grayscale, bilevel, luminance, duotone, HSL, tint, color replacement and alpha operations | Software/hardware precision and color-space propagation can differ |
| Borders and ordinary effects | Solid/dashed lines, shadows, glow, soft edges and reflection | Compound/gradient/pattern borders, preset shadows and several effect parameters remain limited |
| Artistic picture extensions | Display the processed bitmap and preserve original image/effect settings | Regenerating artistic effects from the original is not implemented |
| Effect graphs and 3D | Preserve source properties and warn when falling back | General effect DAG evaluation and 3D lighting/extrusion are not implemented |
| Raster normalization | Bounded PNG, JPEG, GIF, BMP, TIFF and WebP decoding where needed | First frame only; no complete PICT, JPEG XR, EPS, WMF or EMF rendering coverage |
| Legacy VML pictures | Crop, rotation/flips, opacity, grayscale and bilevel | Gain, black level, gamma and chromakey remain preserved with warnings |
| Export | Retain unchanged source appearance and relationships through reconstruction | Editing arbitrary effects is outside this renderer API |

Microsoft specifies that an embedded picture with artistic image properties can contain the processed bitmap in the main blip and the original JPEG XR image in the extension layer. Displaying that saved bitmap avoids applying effects twice. Linked and metafile blips have different rules. [MS-ODRAWXML, Pictures](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-odrawxml/f10da8b6-40f3-4527-9c3d-9f5c2e9cff45).

## Native evidence

The local investigation used Word for Mac 16.106.1, build 16.106.26021521, its OfficeArt and Gfx frameworks, REA function analysis, and native Word screen/PDF output. The inventory found 51 OfficeArt model classes, 65 GEL effect interfaces, 17 blip effect choices and 27 extension tokens. These overlapping inventories are evidence of code presence, not counts of independently supported features.

Targeted function analysis established luminance coefficients, grayscale weights, the 240-step HLS conversion, native HSL parameter mapping, shortest-arc tint interpolation, and alpha/color processing. Order-sensitive native captures confirmed preprocessing of threshold and color-key operations before the remaining blip effects. The main Word executable analysis timed out; the smaller rendering frameworks provided the relevant function evidence. Proprietary binary slices and decompiler output are retained locally and are not distributed with the crate.

## Reproduction and interpretation

The scripts in `scripts/image-fidelity` generate a deterministic 40-case picture matrix, capture the installed Word output and compare fixed page regions with browser screenshots. Each picture has a fixed layout frame and each comparison includes overflow. Source and reference hashes prevent substituting changed inputs. Blank and missing cases remain in the report denominator.

Run the native `image_fidelity` example to extract the model and optionally export a reconstructed document. `prepare-pages.py` builds fixed pages from native output. `prepare-capture.py` generates a Playwright CLI capture function for either those pages or the React DOCX playground. The comparison reports ink overlap and RGB agreement within 16 levels over the union of ink. White margins do not inflate color agreement. The plain picture establishes raster-sampling differences between Word's PDF output and the browser.

The native matrix includes a blip blur that is blank in both Word's UI and PDF export; the SVG renderer draws it. This remains an explicit divergence. The missing-SVG-alternative case also differs intentionally because the parser recovers an available raster image. Neither case is counted as equivalent. A separate 10-case native matrix checks effect ordering and omitted HSL parameters. These bounded matrices do not cover every codec, shape preset, effect combination, Office version or platform.

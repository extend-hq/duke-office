# Office Art rendering notes

The November 6, 2026 implementation work combined the ECMA-376 text-warp definitions, native Word PDF measurements, and targeted REA2 analysis of Word for Mac 16.106.1. Native analysis establishes individual operations; it does not establish complete rendering parity. The [comparison gallery](office-art-comparisons.md) retains visible differences and uses the same input and page coordinates for both renderers.

## Text geometry

Office evaluates preset text geometry in a 65,536-unit square before scaling to the text frame. Envelope presets deform outlines between paired paths. Single-path presets place individual glyphs rigidly along a curve, using the curve tangent for rotation. These paths require distinct mapping operations.

The native line mapper computes a fit from path length divided by source-line width, with growth disabled when the relevant flag is absent. It uses glyph bounds and source-line alignment when locating glyphs, and a fractional vertical anchor within each glyph. The upstream host supplies the source lines and can choose different sizing for path text. Static analysis of the mapper alone does not determine that host's complete font-fitting policy.

The DOCX adapter's legacy path-text sizing is inferred from native PDF glyph measurements: path glyphs retain point-sized outlines while the guide frame uses the nominal pixel advance. This inference is separate from the observed shared mapping algorithm and has narrower validation than the standard envelope definitions.

The implementation uses Rustybuzz shaping and portable licensed font outlines. Font metrics respect OpenType's typographic-metrics selection flag. Caller-supplied font bytes are supported by the shared outline API; the DOCX adapter currently uses bundled fallback faces. Font substitution, wrapped or mixed-format WordArt, and host-specific fitting remain limits.

The current mapper handles single baselines and paired envelopes. Presets with additional regions (`textButton`, `textButtonPour`, `textDeflateInflate`, and `textDeflateInflateDeflate`) retain fallback rendering. A manual sweep covered every preset at left, center and right alignment; unsupported combinations remained explicit rather than being silently replaced with another warp.

## Three-dimensional shapes and text

The shared renderer projects bounded contour meshes, including extrusion and bevel surfaces, through preset cameras. Light-rig directions are transformed into the scene's coordinate system. Native material observations distinguish diffuse response, specular color and exponent, and self-emission. Plastic, metal, soft metal, powder and legacy materials therefore cannot be represented by a single darkened extrusion color.

Native material IDs were cross-checked against Microsoft's [material enumeration](https://learn.microsoft.com/en-us/office/vba/api/office.msopresetmaterial). Camera and light-rig tables were checked against public preset data. The implementation uses face shading; native per-pixel response, Fresnel effects, antialiased edges and some bevel profiles can differ. Embedded 3D models still use their saved Office preview rather than this contour renderer.

## Ink

InkML coordinates retain their declared physical resolution. Brush dimensions and normalized pressure determine the stroke envelope; fitting all traces to their bounding rectangle destroys both placement and pressure fidelity. Native pen construction confirmed a brush-dimension multiplier of `0.25 + 1.5 × normalized_pressure`.

Native pencil rendering uses repeated textured stamps with pressure-dependent opacity and a circular brush envelope. Its internal texture atlas is not redistributed. A bounded 300 dpi procedural grain image preserves the reconstructed physical dimensions and pressure response while remaining an approximation of Office's texture.

## Evidence boundaries

REA2 used Ghidra 12.1.4 against the ARM64 OfficeArt and graphics frameworks. The selected OfficeArt slice had SHA-256 `a909fb65a4e287cffdfd4d837905080b91183471c4fb06366ae8642eed016deb`. Focused raw-code queries retained source offsets and parent hashes; these omit external callees, data and relocations and use a generic ARM64 ABI. Decompiled text is an interpretation of shipped instructions, not original source.

The investigation retained local evidence bundles for preset construction (`ev_817d62014bdfbdfbdae3f62fe532e5075d8a58dd33a51098dbe6262d3a236d21`), path mapping (`ev_e952aee684fd18bb7d896cd34308cc04cab8d906a25f0fb912e5e30eb9fcf378`), glyph transforms (`ev_4627bb7c4e8c6700051cc000d4de996338e10f00e8f1d587d88f3e62f5e9246a`), material response (`ev_8a1977cdd3b34eccaf37082a4de8d87ac4472f45592fe2dbf5cb481bef076299`) and ink pressure (`ev_54721e0fec1bec71b3a1e8c7ed99fcedcd1bd4670cd0b5849604ec384c5b0e15`). Those IDs identify local analysis records, not public downloads. Proprietary binaries, decompiled code and texture assets are excluded from the repository and packages.

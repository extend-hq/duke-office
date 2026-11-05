# rust-office-art

Shared Office picture rendering for native Rust and WebAssembly. The renderer consumes DrawingML picture XML and a resolved image source, and produces SVG with separate paint bounds and layout dimensions.

`render_picture` accepts a `PictureContext` that resolves DrawingML colors against the caller's theme. `RenderedPicture.svg` contains the SVG text; `paint_bounds` describes any pixels outside the original layout box. Keep the original image bytes and drawing XML for export.

Implemented appearance includes signed cropping, stretch fill rectangles, raster tiling with embedded DPI, rotation, flips, line borders, alpha and color operations, shadows, glow, soft edges, and reflections. Geometry support covers rectangles, ellipses, rounded rectangles, triangles, diamonds, flowchart delays, and a bounded single custom path using numeric move, line, quadratic, and cubic commands. Theme colors are resolved by the caller.

HSL, tint, exact color replacement, and alpha ceiling use bounded raster processing. The color transforms preserve native Office software arithmetic, including its 240-step HLS conversion and preprocessing order. Native software and hardware output can differ by a few color levels. Glow and soft-edge SVG approximations require further visual comparison across geometry and radius combinations.

`normalize_raster` converts a base64 image data URI to PNG, including TIFF images that browsers cannot display directly. Decoding supports PNG, JPEG, GIF, BMP, TIFF, and WebP; it applies stored orientation and uses the first image/frame. Limits are 16,384 pixels per dimension, 16,777,216 total pixels, 128 MiB of decoder allocation, and 96 MiB of input base64 text. Encoded PNG output is limited to 72 MiB. Rendering uses the same limits when raster color processing is required.

The cached bitmap in a DrawingML blip preserves an already-rendered artistic appearance. Regenerating artistic effects from an original image is not implemented. Other unsupported features include three-dimensional lighting, general effect graphs, preset shadows, arbitrary geometry formulas and multiple custom paths, compound/gradient/pattern borders, and nonlinear raster operations combined with blip blur. Unsupported, invalid, over-limit, or unchanged pictures return `None`; callers should retain the original representation and report unsupported decorated pictures. This API does not assert complete Word parity.

The crate has no operating-system, application-automation or browser dependencies. Both native callers and WebAssembly use the same rendering implementation.

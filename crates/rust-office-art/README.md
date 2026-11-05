# rust-office-art

Shared Office drawing rendering for native Rust and WebAssembly. The renderer consumes DrawingML picture XML and a resolved image source, and produces SVG with separate paint bounds and layout dimensions.

`render_picture` accepts a `PictureContext` that resolves DrawingML colors against the caller's theme. `RenderedPicture.svg` contains the SVG text; `paint_bounds` describes any pixels outside the original layout box. Keep the original image bytes and drawing XML for export.

Implemented picture appearance includes signed cropping, stretch fill rectangles, raster tiling with embedded DPI, rotation, flips, line borders, alpha and color operations, shadows, glow, soft edges, and reflections. Geometry supports all 187 DrawingML presets and bounded custom guide formulas, elliptical arcs, multiple paths, and quadratic/cubic commands. The preset definitions are distributed with Apache POI license, notice, revision and hash metadata.

Tile sizing uses embedded resolution with a 72 DPI fallback when resolution is missing. The tested `blipFill/@dpi` and `a14:useLocalDpi` settings do not override intrinsic tile size, matching native Word 16.106.1 and Excel 16.107.3 on macOS. Intrinsic tile dimensions currently support PNG, JPEG/JFIF, GIF and BMP; TIFF/WebP normalization does not establish their intrinsic tile resolution. A 12-case Word control confirms physical tile dimensions, while a 48-case Excel replay retains residual differences in high-resolution tile sampling and alignment. These checks do not establish all-version or all-platform parity.

Normal dot, dash and dash-dot outlines use distinct segment spacing from their system variants. Pictures and shapes share the preset mapping and use round joins when no join is specified. Rectangular picture borders follow an outward-offset centerline; nonrectangular picture borders still use an outside-stroke approximation.

`render_shape(properties_xml, style_xml, width, height, context)` renders shape and connector geometry, supported fills and outlines, arrow markers, local rotation/flips, and shared effects. Text layout remains the caller's responsibility. `PictureContext` resolves colors, optional theme style references, and image relationships. Bitmap fills reuse the picture pipeline before geometry clipping. Zero-axis connectors retain their geometry but use a minimum one-pixel logical paint frame to prevent division by zero in consumers.

`smartart::parse_smartart_data` reads semantic points and connections; `smartart::parse_smartart_drawing` reads the saved drawing tree, preserving ordered shapes, connectors, pictures and groups. DOCX and PPTX adapters compose those drawings using the shared renderer and their own text layout. This does not implement the SmartArt data-to-layout engine. Missing saved drawings use explicitly reported caller fallbacks.

HSL, tint, exact color replacement, and alpha ceiling use bounded raster processing. The color transforms preserve native Office software arithmetic, including its 240-step HLS conversion and preprocessing order. Native software and hardware output can differ by a few color levels. Glow and soft-edge SVG approximations require further visual comparison across geometry and radius combinations.

`normalize_raster` converts a base64 image data URI to PNG, including TIFF images that browsers cannot display directly. Decoding supports PNG, JPEG, GIF, BMP, TIFF, and WebP; it applies stored orientation and uses the first image/frame. Limits are 16,384 pixels per dimension, 16,777,216 total pixels, 128 MiB of decoder allocation, and 96 MiB of input base64 text. Encoded PNG output is limited to 72 MiB. Rendering uses the same limits when raster color processing is required.

The cached bitmap in a DrawingML blip preserves an already-rendered artistic appearance. Regenerating artistic effects from an original image is not implemented. Other unsupported features include three-dimensional lighting, general effect graphs, preset shadows, compound outlines, some path gradients and pattern fills, detached fill rotation, and nonlinear raster operations combined with blip blur. Warped WordArt text and full SmartArt layout generation are not implemented. Unsupported, invalid, over-limit, or unchanged pictures return `None`; callers should retain the original representation and report unsupported decorated pictures. This API does not assert complete Word parity.

The crate has no operating-system, application-automation or browser dependencies. Both native callers and WebAssembly use the same rendering implementation.

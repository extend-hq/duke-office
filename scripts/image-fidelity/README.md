# Picture appearance comparison

Generate a fixed picture matrix and capture it with the installed Microsoft Word:

```sh
python3 scripts/image-fidelity/generate.py --output artifacts/word-image-parity-20261105
python3 scripts/image-fidelity/capture.py --directory artifacts/word-image-parity-20261105
python3 scripts/image-fidelity/compare.py --reference-directory artifacts/word-image-parity-20261105 --candidate-directory /absolute/path/to/pages --output /absolute/path/to/comparison --label 'React DOCX'
```

The generator records deterministic input hashes and one case per 432 × 432 pixel page. Each picture has a 192 × 128 pixel layout frame at (120, 120), using 96 pixels per inch. Compare the full 336 × 336 pixel region beginning at (48, 48) so rotation, borders, shadows and reflection remain visible. Do not normalize each image to its own ink bounds; doing so hides geometry differences.

The capture opens only the matrix, disables external-link updates while it is open, restores the prior application settings, and closes only the opened document. Existing native references cannot be overwritten. The receipt records Word version, input and output hashes, page count, and rasterizer DPI.

Candidate pages must have the same 432 × 432 pixel dimensions and use `page-01.png` through `page-40.png`; a different name pattern can be passed with `--candidate-pattern`. The comparison retains missing, size-mismatched, and blank outputs in the report. It verifies the native PNG hashes before scoring and produces a per-case JSON ledger and HTML gallery with difference images. Color agreement is evaluated over the union of visible ink, avoiding inflated scores from white page margins.

This is a synthetic appearance matrix. It is not an exhaustive image-format corpus or a claim of Word rendering parity. Compare each output to the retained Word pages and report unsupported cases without excluding them from the denominator.

Extract the native model with `cargo run -p rust-docx --example image_fidelity -- INPUT.docx MODEL.json ROUNDTRIP.docx`. The optional third path reconstructs the document after clearing paragraph source XML, exercising image source preservation.

Use `prepare-pages.py --directory DIR --model MODEL.json` to produce fixed browser pages from the native model. Serve the directory locally, then generate a CLI function with `prepare-capture.py --directory DIR --target parser --base-url URL`. For the running React DOCX playground use `--target viewer`. Run the generated function with `playwright-cli run-code --filename PATH`. Both capture routes use the same page dimensions and retain all cases.

`compare.py --reference-receipt RECEIPT.json` selects a separately recorded rasterization of the same Word PDF. The receipt lists exact page paths and hashes; the input DOCX and PDF hashes are also checked. Retain both reports when comparing rasterizers. The original Splash rasterizer can resample an integer-sized bitmap into one extra output pixel; Cairo avoids that artifact in the unchanged-picture control. This is a reference rasterization distinction, not an image alignment or scoring adjustment.

## Shapes, connectors and manual fixtures

Generate the 16-page native shape matrix, the broader manual atlas, or the saved SmartArt probes independently:

```sh
python3 scripts/image-fidelity/generate-shape-matrix.py --output artifacts/word-shape-parity-20261105
python3 scripts/image-fidelity/generate_office_art_atlas.py --output artifacts/office-art-fixtures-20261105
python3 scripts/image-fidelity/generate_smartart_docx.py --output artifacts/office-art-fixtures-20261105/smartart
```

The 589-case atlas distinguishes implemented rendering, preserved properties, fallbacks and unmeasured behavior. It includes seven editable Word documents and a machine-readable manifest. Its case count is not a native agreement score. The SmartArt probes exercise saved geometry, semantic text, namespace aliases and independent text transforms; they do not establish full SmartArt layout parity.

`gallery.py` builds picture/group or shape/connector galleries from unchanged native references and captured React pages. Supply the actual runtime build provenance with `--capture-proof`, the matching version with `--runtime-version`, and either `--runtime-source registry` or `--runtime-source workspace`. Picture galleries also require `--group-capture-proof`. The tool verifies the input, served WASM and every candidate page against those receipts. The gallery rescales both columns equally for display, while measurements use the original fixed-coordinate crops. Keep full per-case reports alongside the representative gallery. Viewer captures use a 1440 × 1600 viewport so a complete Letter page fits below the application controls.

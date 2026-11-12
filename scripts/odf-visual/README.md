# OpenDocument chart and drawing evaluation

This harness measures native Rust artwork against LibreOffice PDF output using the same full-page ink-union metric as the Office Art harness. It also checks the public chart extraction bundles independently of rendering. It does not measure complete Writer pagination or Calc sheet layout.

Requirements: Rust, Python with Pillow, LibreOffice, Poppler (`pdftocairo`), the existing `react-pptx` viewer checkout and its esbuild binary, and `playwright-cli`. The browser uses a 432 × 432 CSS-pixel viewport at device scale 1; native PDF pages are rasterized at 96 dpi. The render function waits for fonts and images. A fixed browser and viewer bundle must be used for both versions.

## Inputs and native reference

Run from the repository root, with a new output directory:

```sh
python3 scripts/odf-visual/generate.py output/odf-visual-run
/Applications/LibreOffice.app/Contents/MacOS/soffice -env:UserInstallation=file:///tmp/odf-visual-run-profile --headless --convert-to pdf --outdir output/odf-visual-run/reference output/odf-visual-run/visual.odp
pdftocairo -png -r 96 output/odf-visual-run/reference/visual.pdf output/odf-visual-run/reference/page
```

The default matrix has 15 art probes and 11 chart probes, repeated across packaged and flat text, presentation, drawing, and spreadsheet containers. `--supplemental` generates a separate six-case matrix for transforms, opacity, logical coordinates and lazy equations. Normalize Poppler page suffixes to two digits for matrices with fewer than ten pages. Keep blank native pages in the report; they are flagged rather than silently discarded.

To isolate real embedded charts from a corpus without using saved preview images:

```sh
python3 scripts/odf-visual/collect_charts.py CORPUS output/odf-visual-run/visual.odp output/odf-chart-corpus
```

This preserves embedded chart content and styles, fits each chart uniformly into a fixed page, and records source hashes and object identities. It does not copy other embedded assets or recreate the surrounding host document. Export this derived presentation to PDF using the same commands before comparing it.

## Rust models and browser captures

```sh
cargo build --manifest-path scripts/odf-visual/Cargo.toml
scripts/odf-visual/target/debug/odf-visual INPUT OUTPUT_JSON
```

Freeze the baseline executable and model **before** changing the parser. Save the candidate model as `candidate.json` and the baseline model as `baseline.json` inside the evaluation directory. Record binary hashes, source revision and dirty source hashes; a later checkout is not evidence of what the earlier executable contained. The initial November baseline used the model-only snapshot helper before audit and bundle modes were added.

Prepare the existing viewer, supplying its local paths:

```sh
python3 scripts/odf-visual/prepare_viewer.py output/odf-visual-run --viewer-repo VIEWER_REPO --esbuild ESBUILD_BINARY --url http://127.0.0.1:8879/ --captures output/playwright/odf-visual-run
python3 -m http.server 8879 --bind 127.0.0.1 --directory output/odf-visual-run
```

In another terminal, use `playwright-cli open` and then `playwright-cli run-code --filename output/odf-visual-run/capture.js` in the same browser session. The capture script returns the browser version and device scale. The model comes from the native Rust executable; this does not validate a published WASM package.

```sh
python3 scripts/odf-visual/compare.py output/odf-visual-run --baseline output/playwright/odf-visual-run/baseline --candidate output/playwright/odf-visual-run/candidate --baseline-model output/odf-visual-run/baseline.json --candidate-model output/odf-visual-run/candidate.json --browser-version BROWSER_VERSION --reference-version LIBREOFFICE_VERSION
```

`compare.py` reuses `scripts/image-fidelity/compare.py`: RGB agreement within 16 over the union of ink below 247, plus ink intersection-over-union and channel error. White page margins do not inflate agreement. It performs no object registration, alignment, resizing, masks or cropping. Missing captures and size mismatches score zero. Blank references are flagged and remain in the aggregate. The first comparison seals reference PDF/PNG hashes; later runs reject changed references. Reports retain input, model and capture hashes and show reference, baseline, candidate and absolute difference side by side.

## Data extraction and availability

```sh
python3 scripts/odf-visual/verify_data.py scripts/odf-visual/target/debug/odf-visual output/odf-visual-run output/odf-visual-run/data-contract
python3 scripts/odf-visual/audit_corpus.py scripts/odf-visual/target/debug/odf-visual CORPUS output/odf-visual-run/corpus-audit.json
```

The data check uses public ODT/ODP/ODG bundles and the shared ODF API for ODS. It checks identical chart data across eight containers, explicitly checks scatter and bubble domains, then verifies exact numeric spellings and sparse indices with unsupported chart artwork. These are evaluation probes, not new unit tests.

The corpus audit reports parse failures, embedded-object failures, evaluated custom geometry and available SVGs. Successful parsing or SVG production is **not** a visual-fidelity score. Preserve the failures and the source manifest.

## Source observations

[`libreoffice-sources.json`](libreoffice-sources.json) pins the implementation files inspected to understand native behavior at LibreOffice commit `1f77d10d6938fd34972958f64b2bcfa54f8b1ba5` (26.2.2.2). The Rust implementation is independently written; these sources are behavioral references, not vendored implementation dependencies. Source observations cover enhanced-path commands and lazy expressions, transforms, gradient angles, local chart tables, domain ordering, and pie-axis orientation. Native rendering remains the visual oracle.

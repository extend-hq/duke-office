# PowerPoint package relationships

The local `relationships2` build resolves the main presentation through the package's `officeDocument` relationship. Layouts, masters, themes, notes, table styles, comments, comment authors and image/audio/video assets resolve through their relationship roles rather than assuming a particular folder. Core and extended properties also follow package relationships. Relative paths and package-absolute paths are supported; source-part and asset identities retain the resolved package names for downstream consumers.

The relationship parser decodes each relationship's attributes once, retaining target, type and external mode in one record. Chart, diagram, control and comment discovery reuse those records instead of reparsing relationship XML. Existing missing-type compatibility fallbacks remain for conventional structural paths. Conventional media-folder discovery also remains for compatibility. External relationships are not treated as embedded parts by the new structural lookups.

Both transitional and strict Office relationship prefixes are recognized. Modern comments use their separate Microsoft relationship type; their existing text extraction is preserved. This does not implement typed modern comment threads, mentions, resolution state or replies.

The implementation follows the package structure described in Microsoft's [PresentationML documentation](https://learn.microsoft.com/en-us/office/open-xml/presentation/structure-of-a-presentationml-document), the [main-part relationship requirements](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/32be961f-d71a-4812-913f-b675c79aa88a), and the [modern Comment Part contract](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-pptx/b85a9293-bdca-4c6b-a554-8f3918db9791).

## Validation

- All 1,156 existing full canonical models and exact failure statuses/errors match the previous validated `comments3` build.
- Twenty-four package relocation variants preserve the complete model after reversing only relocated part identities. The harness independently checks that every non-relationship/non-content-type part remains byte-identical. Cases cover the root alone, related parts alone, the entire package, relative and absolute targets, charts, notes, images, themes, layouts, metadata, and strict/transitional classic and modern comment content.
- Six negative checks verify that external and unrelated relationship types cannot load internal layout, notes or comment parts solely because their paths look conventional.
- All 30 focused cases have identical native/WASM compact models and complete artwork hashes.
- On the 687-input native/WASM cohort, all statuses match and 624 of 625 successful models/artwork sets match exactly. The previously recorded curved-3D numerical mismatch remains a strict failure.
- All 392 existing Rust checks pass. No new unit tests were added.

The first candidate lost modern comment text on four inputs; its failed audit remains archived. The corrected candidate preserves those outputs. An incomplete audit interrupted by disk exhaustion is also archived separately from the successful complete rerun.

This is scoped relationship-resolution coverage. It does not establish complete OPC conformance, validation of every URI or XML namespace rule, full strict-OOXML support, complete preservation of unused extension parts, or native PowerPoint rendering fidelity. No accuracy reference or benchmark denominator was changed.

Reproducible probes live in [relationship_probes.py](../scripts/pptx-benchmark/relationship_probes.py). Frozen source, executable identities and full receipts are under `output/pptx-benchmark-20261106/experiments/relationships2/`.

The fresh five-parser comparison is `expanded-relationships`: 630 attempted packages, 628 reference packages, and 611 common nonempty timing inputs. Duke PPTX measures 1.159 ms median and 15.432 ms p95; source feature aggregates are exactly unchanged from `expanded-comments`. The paired candidate/reference estimate is 0.996× (95% interval 0.987–1.015×), so no speed gain is established. [Complete comparison](../output/pptx-benchmark-20261106/report/expanded.html).

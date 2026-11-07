# DrawingML text fields

A text run can carry an optional `field` object. `text` remains the saved cached value. The parser does not replace it with the current date, a computed slide number, or another generated value.

| Property | Meaning |
| --- | --- |
| `id` | Persisted field token, retained when present. |
| `type` | Open application-defined string, including reserved slide/date types. |
| `hasCachedText` | Whether a qualified text element was present; distinguishes missing content from a saved empty value. |
| `paragraphProperties` | Explicit field paragraph settings, including supported defaults for runs. |
| `sourcePart` | Package part containing the field. |
| `sourceXml` | Field subtree with a wrapper carrying its in-scope namespace bindings. |

Strict and transitional DrawingML namespaces are supported, independently of prefix spelling. Foreign elements and attributes with colliding local names do not become field content. XML literal line endings and attribute whitespace are normalized before entity decoding; character references retain their decoded characters. Field-only notes survive even when their cached values are absent or empty. SmartArt fragments may be reserialized by the shared reader; character references preserve the original text and attribute values through that step. Empty fields also survive semantic SmartArt fallback.

The run style resolves inherited defaults, then the field's `pPr/defRPr`, then the field's explicit `rPr`. The additive `TextRunProperties` model exposes the same supported run properties without requiring text. The React PPTX model carries the field metadata alongside the resolved run style.

Field paragraph metadata includes alignment, level, bullets, spacing, margins, indent, tabs, bidi/line-break settings, font alignment, bullet follow-text flags and internal bullet-image references. Unsupported properties and extensions remain in `sourceXml`. Field-level paragraph placement and bullet overrides still require downstream layout integration; preserving those values is not a claim of rendered fidelity. The parser does not currently evaluate field update types.

The contract follows [DrawingML fields](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.field?view=openxml-3.0.1) and [Microsoft's field-type clarifications](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/209a8afb-4ce6-4ad9-ad6b-f18da263502e). The run-default precedence also agrees with [LibreOffice's field insertion implementation](https://github.com/LibreOffice/core/blob/master/oox/source/drawingml/textfield.cxx).

Validation receipts belong to the frozen experiment. Source verification checks emitted fields against the original namespace-qualified XML, and checks complete non-field models and payloads against the predecessor. It does not count every field in unused or unsupported package parts as recalled, and it does not substitute for a rendering comparison.

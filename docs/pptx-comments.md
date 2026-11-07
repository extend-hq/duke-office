# PowerPoint comment model

Classic PresentationML comments retain their text, including empty and whitespace-only strings. XML literal line endings normalize to LF; character references retain their decoded characters. Author names and initials follow XML attribute whitespace normalization. Direct comment and property elements must use a supported PresentationML namespace.

| Field | Meaning |
| --- | --- |
| `id` | Author/index identity within a slide; ordinal fallback for missing source identity |
| `authorId`, `sourceIndex` | Original classic author identifier and comment index |
| `author`, `authorInitials` | Values resolved from the related comment-author part, when available |
| `sourcePart` | Package part containing the comment |
| `modifiedAt` | Original `dt` value, without clock-based regeneration |
| `position` | Original `x`, `y`, with `unit: "eighthPoint"` |

`createdAt` is not populated from `dt`: the classic format specifies last modification time. The older unqualified `x`/`y` fields remain unset. Convert the explicit position to points by dividing by eight, or to EMUs by multiplying by 1,587.5. Comment display policy and overlay appearance belong to the consumer; this extraction does not establish rendered placement fidelity.

Classic comment and author parts resolve by their relationship types, including nonstandard package paths and strict OOXML relationship namespaces. External relationships are not fetched. Missing authors retain the source author ID without inventing a display name. Authors load lazily only when a slide has a comment part.

Unrecognized comment namespaces retain the preceding text-only fallback where available. Modern comments have the typed contract below. Mention/reaction semantics, classic threading extensions and a viewer comment interface remain incomplete.

The contract follows Microsoft's [classic comment definition](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.presentation.comment?view=openxml-3.0.1) and [PowerPoint position-unit interpretation](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/34ada90e-9cb8-4efb-b20e-027eed8c75e3). Source and boundary checks are reproduced by `comments_review.py`, `comments_probes.py` and `comments_budget_probes.py` in the benchmark tooling.

## Modern comment contract

Modern comments use `http://schemas.microsoft.com/office/powerpoint/2018/8/main`. A comment can anchor to a slide, drawing element, text range or explicit unknown anchor. Its optional position is relative to the first anchored object's upper-left corner. Replies form an ordered list, and the comment body is a DrawingML text body. Scheduling, assignees, completion and title are separate properties. These must not be flattened into classic coordinates or conflated with comment text. See [CT_Comment](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-pptx/161bc2c9-98fc-46b7-852b-ba7ee77e2e54).

Replies retain their own identity, author, creation timestamp, status and rich text body; they do not become unrelated top-level comments. See [CT_CommentReply](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-pptx/bcc2b4ae-7571-4ecc-8e94-982f69f793db). Status has three defined states: active, resolved and closed. See [ST_CommentStatus](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-pptx/bd806e3a-96cf-43aa-9618-81b317c34c61).

The [complete namespace schema](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-pptx/af0dc8d7-ee58-435b-80fb-72f2b351b689) gives status an active default, completion a zero-percent default, and empty defaults for assignees and title. Authors have a GUID identity, name, optional initials, user identity and provider identity. An absent text body is permitted. Empty comments and replies therefore need preservation independently of text extraction.

The implementation resolves modern author relationships separately from classic authors and loads each author family only when needed. Threads remain top-level `SlideComment` values; their ordered `replies` retain separate identities, authors and creation timestamps. `paragraphs` uses the existing rich-text model and resolves hyperlinks through the comment part's own relationships. Missing and empty bodies remain comments, and a parent's text excludes reply text. Existing classic serialization is unchanged.

The optional `modern` object exposes `status`, author user/provider identities, ordered `anchors`, `positionEmu`, `task`, and qualified extension XML. Status follows XML token whitespace normalization, defaults to `active`, and preserves unknown values. Anchor kinds are `slide`, `drawingElement`, `textRange` and `unknown`. Each moniker preserves its namespace, local name, qualified attributes and ordered children; namespaced attribute keys use `{namespace}localName`. Nested text-range context survives. This is structured anchor identity, not a resolved viewer selection.

`positionEmu.x/y` are relative to the first anchored object's top-left corner. Integer coordinates and `mm`, `cm`, `in`, `pt`, `pc`, `pi` measures convert to EMUs without rounding fractional EMUs; `sourceX/sourceY` retain the original coordinate strings. These values never populate the classic eighth-point `position` field. Task values preserve start/due dates, assignee identifiers, title and original completion text, with a parsed finite completion fraction when available. The schema's empty title/assignee, active status and zero-percent completion defaults are explicit. Reply metadata does not invent thread anchors or task properties.

Author/comment extension lists retain namespace-scoped XML, including unknown content. Raw extension preservation is not typed mention, reaction or task-extension support. Those semantics and consumer anchor resolution still need their own specification review and source checks. Current headline source-recall scores do not measure these capabilities.

The source inventory of 1,156 established inputs finds five declared modern comment parts across four presentations: five threads, two replies, four drawing-element anchors and one text-range anchor. Six comment/reply statuses are active and one is resolved. The sampled modern bodies contain seven rich-text runs, with no task attributes or extension lists. Twenty-two declared modern-author parts contain 190 author records, including parts without corresponding observed comment threads. These counts establish corpus coverage only; they do not prove parser support or exercise absent feature combinations. Five inventory inputs fail package/content-type inspection, and 288 are non-ZIP containers. The 317 reserved inputs remain uninspected. See the [inventory receipt](../output/pptx-benchmark-20261106/comment-inventory-modern/summary.json).

Anchor lists are ordered identity paths, rather than interchangeable shape IDs. Drawing-element paths can terminate in shapes, groups, graphic frames, connectors, pictures or ink. Text-range paths include character-range identity, and slide paths include their owning document identity. The consumer must resolve the complete path before positioning an annotation. See the [drawing-element](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-odrawxml/145d263b-da8a-4cad-b076-1e51a9d149bf), [text-range](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-odrawxml/7ac2b0dc-426e-4c42-a3f1-7be68542dbbe) and [slide](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-pptx/d50cf27f-f9d4-482b-9c44-076b2576ab75) moniker contracts.


## Validation

The retained `modern-comments2` source audit passes all 1,156 established inputs: 1,152 complete outputs/errors remain exact, and four change only in source-verified comments. The five observed threads and two replies retain ownership, authors, body boundaries and full ordered anchors. All 156 SDK tuning outputs/errors remain exact. Nineteen focused probes cover rich formatting and links, empty bodies/replies, strict DrawingML, all anchor categories, qualified unknown extensions, author/comment relationship modes, all six physical units, status normalization and depth boundaries. Native and the actual WASM bridge agree on every probe, including the expected over-depth rejection.

The 687-input previous-WASM regression has 686 exact outputs/errors and one native-matched comment change. All four real modern-comment presentations additionally match native/WASM compact models and all 1,214 artwork hashes; three are outside the previous WASM cohort. The existing curved-3D numerical difference elsewhere remains a strict failure. All 53 existing Rust checks and React model/viewer typechecks pass. The original trial and a stale-binary audit attempt are preserved and excluded from final-build evidence.

The 40-input paired tuning pilot gives a 1.014× median ratio against `xml-recent1` (95% interval 1.003–1.027×), with effectively unchanged summed medians. It contains none of the four comment-bearing inputs. A separate interleaved measurement of those four changes summed medians from 58.52 to 58.80 ms (+0.47%). The change is retained for accuracy, without a general speed-gain claim. See the [decision and receipts](../output/pptx-benchmark-20261106/experiments/modern-comments2/decision.json).

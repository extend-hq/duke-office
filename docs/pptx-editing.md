# Object editing metadata

The local OOXML parser adds optional `editing` metadata to slide nodes. It retains object locks, connector endpoints and qualified nonvisual property XML for downstream consumers. Geometry, text, artwork and existing object identities keep their previous representation. Legacy binary presentations do not yet expose this contract.

`editing.kind` identifies the original object family: `shape`, `connector`, `picture`, `group`, `graphicFrame` or `contentPart`. This can differ from the display node type: a SmartArt graphic frame can produce a group, and a graphic frame can produce an image fallback.

| Property | Meaning | Default when omitted in source |
| --- | --- | --- |
| `locks` | Explicit restrictions allowed by the object family | Each supported restriction is false |
| `textBox` | Shape is a text box (`txBox`) | False |
| `preferRelativeResize` | Picture prefers resizing relative to its original size | True |
| `isComment` | Content part represents an ink annotation | True |
| `connector.start` / `connector.end` | Target shape ID and connection-site index | Unconnected |
| `sourceXml` | Nonvisual property subtree with its in-scope namespace bindings | Present on every emitted `editing` object |

Optional booleans preserve an explicit `false` separately from absence. Values accept XML Schema boolean spellings (`true`, `false`, `1`, `0`) and XML whitespace. Invalid values are not coerced to false. Consumers can apply the schema defaults without losing the distinction between explicit and default behavior.

The common lock family includes grouping, selection, rotation, aspect-ratio changes, movement, resizing, point editing, adjustment handles, arrowheads and shape type. Shapes additionally support text-edit restrictions; pictures support crop restrictions. Group and graphic-frame locks use their narrower schema-defined attribute sets. Group locks add ungrouping, while graphic-frame locks add drilldown. Unsupported attributes remain available in the retained subtree when an editing object is emitted. Unknown extension content is preserved, not executed.

Connector endpoints contain `shapeId` and `connectionSite`, both unsigned 32-bit integers. The [XML Schema integer lexical rules](https://www.w3.org/TR/xmlschema11-2/#nonNegativeInteger) permit leading zeroes, a plus sign and a minus sign on zero. A missing or malformed required attribute leaves that endpoint unresolved; the retained source still contains its declaration. Connection-site indices refer to the target geometry's connection-site table, not to coordinates or the order of shapes on the slide. The parser does not reroute connector geometry.

The React presentation model exports `resolveConnectorEndpoint(nodes, connector, endpoint)`. It searches nested groups and matches the target's numeric `sourceId` within the connector's `sourcePart`. Display IDs can have inheritance prefixes without changing source identity. Missing targets, missing source-part identity and duplicate target IDs return `undefined`. Callers should provide the node forest for the relevant rendered slide or original master/layout, rather than concatenating duplicate copies of those forests.

This metadata does not enforce editing restrictions in the read-only React viewer, compute connection-site coordinates, or establish native application rendering parity. It describes the source object and supports later editing and interaction work.

The implementation follows ECMA-376 Part 1 §20.1.2.2.36 (start connection), §20.1.2.2.11 (connector locks), and the DrawingML nonvisual property schemas. Microsoft documents [shape locks](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.shapelocks?view=openxml-3.0.1), [connector locks](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.connectionshapelocks?view=openxml-3.0.1), and the [Office presentation extensions](https://officeprotocoldoc.z19.web.core.windows.net/files/MS-PPTX/%5BMS-PPTX%5D.pdf).

The independent qualified-source audit covers 1,312 distinct inputs. It verifies 95,166 emitted editing objects and 524 connector endpoints, including inherited instances. On 911 inputs only the new editing data changes; all other complete model values, artwork and payloads remain exact. The other 401 outputs or errors remain completely exact. These counts describe emitted instances, not distinct source objects or a feature-recall denominator.

Twenty-three focused packages cover all six object families, Strict and Transitional namespaces, default and shadowed prefixes, foreign lookalikes, explicit false/invalid booleans, unsigned integer boundaries and missing endpoints. Fifty-two checks execute the built React endpoint resolver. The native/WASM regression covers 711 inputs: every previous WASM field, error and 63,716 artwork hashes stay exact, and 49,567 retained editing-source hashes match native. The prior strict native/WASM artwork-size discrepancy remains a reported failure on one of 649 successful models.

Source-audit receipts and the initial feature-cost measurements are under `output/pptx-benchmark-20261106/experiments/object-editing1`. The following bounded-cache trial is under `object-editing-cache1`. The headline literal-feature matrix does not score this editing contract; it retains its separately identified comparison build and measurement scope.

The retained cache owns at most eight parsed property subtrees and caps their accounted storage at 256 KiB per parse, plus bounded container overhead. It compares complete qualified XML, returns owned copies, bypasses oversized entries and expires with the parse. The 1,312-input native regression, 711-input WASM regression and 23 focused packages remain exact; 1,528 standalone checks cover reuse, mutation, ownership, eviction and bounds. The 40-input pilot shows a 1.031× median speed ratio versus the uncached implementation, with effectively unchanged summed medians. A separate direct pilot still measures about 2.9% greater median latency than the pre-feature parser. No general throughput or fastest-parser claim follows from these results.

The following retained `editing-namespaces1` optimization reuses sorted, escaped namespace declarations until their scope changes and constructs each qualified source wrapper at its final size. Complete output and error behavior remains exact on all 1,312 established/SDK inputs, 711 prior-WASM inputs and 39 focused native/WASM property/scope packages. All 32 existing Rust checks pass. A prior namespace-cache trial before this editing contract had no reliable gain and was reverted; it remains archived.

With unchanged code after the 40-input pilot, the full 610-input tuning follow-up measures a 1.024× median speed ratio versus `object-editing-cache1` (95% interval 1.020–1.030×), reducing summed per-input median latency by 1.92%. A separate direct 610-input comparison against the pre-feature `xml-indexed1` build measures 0.975× (0.969–0.979×), or about 2.6% greater median latency for the complete richer contract. These direct measurements supersede extrapolation from pilots; ratios from separate runs are not multiplied. The [decision and receipts](../output/pptx-benchmark-20261106/experiments/editing-namespaces1/decision.json) retain every measurement.

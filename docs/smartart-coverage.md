# SmartArt coverage

Saved SmartArt drawings use the shared OfficeArt shape, connector, picture, group, paint, and transform pipeline. Support is determined by the drawing primitives in the saved cache, rather than a list of diagram layout names. The semantic data, layout, style, colors, cached drawing, and relationships remain preserved separately.

The November 2026 coverage pass used 229 native PowerPoint-authored gallery fixtures covering 176 layout types, plus five hierarchy edge fixtures. Their layout IDs include all 134 entries in the inspected Word installation's SmartArt gallery. The upstream corpus records that PowerPoint COM generated the data and cached drawing parts. The fixtures and their provenance are available in the [upstream SmartArt gallery](https://github.com/ChristopherVR/ooxml/tree/3c850a8a1558264bdb27932435a48c794b531ed6/src/core/pptx/__tests__/fixtures/smartart-gallery); the source repository is Apache-2.0 licensed. No upstream layout interpreter code was adopted.

| Family | Layout types | Gallery cases |
| --- | ---: | ---: |
| List | 30 | 40 |
| Process | 40 | 48 |
| Cycle | 9 | 17 |
| Hierarchy | 15 | 23 |
| Relationship | 30 | 35 |
| Matrix | 3 | 3 |
| Pyramid | 4 | 10 |
| Picture | 25 | 27 |
| Timeline | 5 | 7 |
| Meet the Team | 4 | 6 |
| Text Card | 11 | 13 |
| **Total** | **176** | **229** |

For the DOCX adapter pass, the cached drawing bytes were transplanted unchanged into Word packages with their themes and diagram parts; only host relationships were rewritten. All 229 diagrams produced an SVG, all 1,878 saved shapes passed through OfficeArt, and no missing-cache or reduced-rendering warnings remained. The cached text was verified both for presence and for exact character counts after whitespace normalization, with no missing or invented characters. Chromium decoded all 229 SVGs and each produced visible pixels. The PPTX runtime parsed all 234 original presentations and rendered all 2,340 saved shapes through OfficeArt, including 1,340 independent saved text frames, without presentation or slide warnings. Its 14,527 non-whitespace cached text characters were retained with no additions or omissions.

This establishes the tested saved-drawing type paths and browser decodability. It does not establish native Word pixel parity for every layout. Native Word captures remain a separate oracle. DOCX paragraph spacing, bullets, mixed run formatting, and precise text fitting still have fidelity limits in the generic text-box SVG adapter. Word wrapping uses bounded font estimates rather than Word's typography engine, particularly affecting very small text frames. Picture-family placeholders in the gallery do not independently establish every embedded-photo behavior.

The pass fixed several generic failures. Non-integral pixel dimensions could create invalid fractional EMU group extents; empty neutral 3D-shape declarations could unnecessarily trigger a reduced-rendering fallback; and a data-part image relationship could shadow a host-owned saved drawing with the same relationship ID. The adapters now retain the original integer frame extents, accept only empty neutral 3D-shape declarations, and try the bounded valid cached-drawing candidates. DOCX text inherits the theme font and color from its saved font reference, and respects saved wrapping and independent text frames. Both adapters treat cached text as authoritative: empty or absent text bodies stay empty, while semantic labels are used only when the entire cached drawing is unavailable. This avoids adding labels to decorative and child shapes that share a semantic association with another rendered shape.

A diagram without a supported cached drawing still displays its semantic text with an explicit layout-fallback warning. Executing arbitrary DiagramML layout programs is not implemented. An inventory of the local Word installation found 135 layout definitions and 134 gallery entries, using ten layout algorithm kinds. The cached corpus covers 134 definition IDs; the additional older definition is outside that gallery and has not been separately captured. Local cache-free capture inputs were generated from those installed definitions, but opening the initial control did not establish native document identity, so no native materialization or parity claim is made for them. Installed template XML stays in local artifacts.

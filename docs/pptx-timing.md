# PowerPoint transition and timing model

The local parser exposes `transition` and `timing` on slides, layouts, and masters. The corresponding TypeScript interfaces are exported by the presentation-model package. These fields describe saved presentation behavior; the current React viewer does not execute the animation graph or animate slide transitions.

## Ownership and identity

Each object belongs to the part that declared it. Layout and master timing is retained separately instead of being copied into every slide. A consumer must apply inheritance and master-relative playback rules when constructing an effective timeline.

Shape targets use the source shape identifier scoped to the owning part. Match it to `sourceId` and `sourcePart`, including within groups; a rendered node's prefixed `id` may differ. Timing conditions reference `AnimationTime.id` through `timeNodeId`. Their order, event, delay, runtime-node trigger, and target are retained. No implicit timeline is synthesized from document order.

## Transition fields

`SlideTransition` contains the effect, speed category, optional explicit duration, click/automatic advancement, and start/stop sound action. The classic transition defaults are applied for direction, orientation, black-screen routing, and wheel spokes. Sound data includes the relationship identifier, normalized target, external flag, available embedded asset identifier, name, and built-in marker.

The parser selects markup-compatibility branches using namespace URIs rather than prefix spelling. It recognizes the strict and transitional core namespaces and the transition extension namespaces documented by [Microsoft](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-pptx/22ebe6b5-2ade-43d9-977a-98fa194725c2). Extended effects retain their qualified name and explicit parameters. Extension-specific defaults and visual execution are not fully implemented.

## Timing graph

The structured model covers the [13 classic timing-node kinds](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.presentation.timetypelisttype?view=openxml-2.19.0): parallel, sequence, exclusive, general animation, color, effect, motion, rotation, scale, command, set, audio, and video. Source element names remain the `kind` values.

Common timing data includes duration and repetitions; acceleration, deceleration, and reversal; preset identifiers; fill, restart and synchronization modes; start/end conditions; iteration; ordinary children and subordinate nodes. Sequential nodes also carry previous/next conditions and actions. Behaviors retain the property names they modify, their target, accumulation/addition modes, transform mode, and runtime context.

Animation parameters retain typed keyframe values and formulas, colors and color deltas, motion paths and point types, rotation/scale data, effect filters, commands, and set values. Media timing includes volume, mute, slide count, stopped-state visibility, narration, and fullscreen flags. Builds cover paragraph, diagram, chart, and graphical-object grouping plus timing templates. Targets distinguish slides, sounds, shapes, ink, text ranges, subshapes, chart elements, and diagram elements.

## Units and defaults

| Value | Public model representation |
| --- | --- |
| Durations, condition delays, automatic advancement | Milliseconds; timing values may be `"indefinite"` |
| Repeat count | Number of repetitions, or `"indefinite"`; source thousandths are divided by 1,000 |
| Speed, acceleration, deceleration, volume, iteration percentages | Fractions; both integer and percent-form source encodings are normalized |
| Keyframe time | Fraction of simple duration, or `"indefinite"` |
| Motion and scale points | Source percentage coordinates expressed as fractions |
| Rotation and hue deltas | Degrees |
| RGB delta channels; HSL saturation/lightness deltas | Fractions |
| Text target range | Source inclusive start/end indexes, with paragraph/character kind |

Missing fields remain absent unless the schema supplies a default. In particular, an absent duration is not converted to zero, and the speed category does not invent a transition duration. Source formulas, motion-path programs, filters and commands are retained without evaluation or execution. These units follow the locally indexed ECMA-376 Part 1 timing definitions and schemas; repeat-count units are specified in §19.5.33, printed page 2639.

## Preservation and limits

The retained `sourceXml` is a well-formed namespace wrapper containing the original selected candidate subtree, including its compatibility branches. The wrapper supplies inherited namespace declarations. This supports further interpretation without claiming that raw XML alone is complete feature support. Timing extensions outside the recognized structured vocabulary are listed in `unsupported`.

Both the original part and the scoped animation parse use configured XML resource limits. Source retention counts against the existing retained-text budget. Embedded sound payloads are resolved even outside the conventional media directory. External sound targets are recorded without fetching them.

The current scope does not establish PowerPoint playback fidelity, resolved preset-effect programs, full extension semantics, legacy binary animation support, or complete master/layout timeline inheritance. Those remain separate work in the [parser audit](pptx-parser-audit.md).

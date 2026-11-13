# OpenDocument merge validation — 2026-11-13

The integration includes `5add612` (regression recovery and parsing latency) and `f170bdc` (radar and long-label layout). Their measurements are recorded in the [recovery report](opendocument-regression-recovery-20261113.md) and [radar report](opendocument-radar-layout-20261113.md).

Before integration, all 246 source hashes, five frozen executable hashes and 74 evidence hashes in `output/odf-radar-layout-20261113/candidate-source.json` match the validated candidate. The existing 469 passing workspace tests, WASM check and parity/latency measurements therefore cover the unchanged Rust source. These checks do not constitute a new benchmark run.

The checked-in browser bundles were stale relative to those sources. Both variants were rebuilt with the repository's `npm run build:browser` command, Rust 1.93.1, the locked dependencies and the configured wasm-opt optimization. `node browser/check.mjs` passes, including source-digest, compressed-payload, rendering and chart-data checks. Package versions and third-party notices are unchanged.

| Browser WASM variant | Previous bytes | Rebuilt bytes | Previous gzip bytes | Rebuilt gzip bytes |
| --- | ---: | ---: | ---: | ---: |
| Lite | 2,188,906 | 7,482,601 | 689,043 | 3,631,924 |
| Full | 3,051,481 | 8,344,715 | 1,281,321 | 4,225,831 |

Gzip sizes use level 9. The radar text-outline path makes bundled font data reachable in the browser build. This is a material download-size tradeoff; the native warm-parsing measurements do not measure browser download, startup or cold font initialization. Browser latency was not benchmarked during integration.

Integration uses a `[skip ci]` merge commit because pushes to main otherwise trigger automatic crate publication. No versions are bumped, no tags are pushed and no release workflow is dispatched. React packages remain unpublished.

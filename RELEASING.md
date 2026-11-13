# Publishing Rust crates

CI and both release scripts accept only `duke-*` workspace packages and reject direct dependencies on the retired Office crate names. Run `python3 scripts/release.py check-workspace` to verify this policy without packaging or contacting a registry.

`duke-pages` and `duke-pages-wasm` share a release version: bump both whenever either package changes. The release check rejects mismatched versions. Other crates have independent Cargo versions. For a change to a crate's packaged source or manifest, bump the version in the same PR and run `cargo update --workspace` to refresh the lockfile. Update local dependency requirements when the new version falls outside their range. Rebuild the browser chart distribution when its Rust renderer or bindings change.

On pull requests and pushes affecting the Rust workspace, CI packages the workspace and compares each existing version with its verified crates.io archive. Changing published contents without changing the version fails the check and lists the differing paths. The comparison ignores Cargo's checkout metadata, original unnormalized manifest and generated library lockfile.

A push to `main` with new versions runs native and WebAssembly checks, then publishes all pending crates in dependency order. Cargo waits for dependencies to appear in the index before uploading their dependents. A retry checks the index again and skips completed uploads. To verify a clean checkout locally, run `python3 scripts/release.py dry-run`. To recover an interrupted release, run **Release crates** on `main` with the dry-run option cleared.

Configure a crates.io trusted publisher for each crate with repository owner `extend-hq`, repository `duke-office`, workflow `release.yml` and environment `crates-io`. Restrict that GitHub environment to `main`. Publish the first version of each new crate with an API token that permits creating those crate names, then register this publisher under each crate's crates.io settings. Configure the new names before relying on the automated release workflow.

This workflow uses the Cargo project's short-lived publisher credentials and serial release jobs, together with the branch and manifest triggers used in the Leo workspace. See the [Cargo release workflow](https://github.com/rust-lang/cargo/blob/master/.github/workflows/release.yml), [Leo release workflow](https://github.com/ProvableHQ/leo/blob/master/.github/workflows/publish-crates.yml) and [crates.io trusted publishing guide](https://crates.io/docs/trusted-publishing).

## Retired crate names

The project has deprecated these packages in favor of their Duke replacements. They are absent from the workspace and are excluded from ongoing CI and release automation.

| Retired crate | Replacement |
| --- | --- |
| `rust-docx` | `duke-pages` |
| `rust-docx-wasm` | `duke-pages-wasm` |
| `rust-pptx` | `duke-slides` |
| `rust-pptx-wasm` | `duke-slides-wasm` |
| `rust-odf` | `duke-odf` |
| `rust-office-art` | `duke-office-art` |
| `rust-office-charts` | `duke-office-charts` |
| `rust-office-charts-wasm` | `duke-office-charts-wasm` |

As checked on 2026-11-13, crates.io has no independent deprecation setting. Its crate settings API only updates trusted-publishing restrictions, and its version settings API updates yanking. Cargo's `maintenance = "deprecated"` badge is [not displayed by crates.io](https://doc.rust-lang.org/cargo/reference/manifest.html#the-badges-section). A visible migration notice in a legacy crate's description and README requires publishing a new version under that name. That one-time migration release must be handled separately from this workspace after the release hold is lifted. No such version has been published as part of this change.

Existing versions remain available. Yanking is not a deprecation notice: it changes dependency resolution and can prevent consumers without an existing lockfile from building. See the [Cargo publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html#cargo-yank) and the registry's [crate settings](https://github.com/rust-lang/crates.io/blob/main/src/controllers/krate/update.rs) and [version settings](https://github.com/rust-lang/crates.io/blob/main/src/controllers/version/update.rs) APIs.

## Browser distribution

Each published crate also gets a GitHub release named `<crate>-v<version>`, with its `.crate` archive, registry link, API documentation and source link. The release workflow creates these after Cargo finishes publishing. To recover missing GitHub releases from a packaged checkout, run `python3 scripts/github_releases.py`.

The spreadsheet viewer installs the small browser tarball from GitHub Releases. After a browser package change, update its `package.json` version, rebuild with `npm run build:browser`, and check with `npm run check:browser`. Push a `charts-browser-v<version>` tag pointing to a commit on `main`. The browser release workflow verifies the tag, packs the JS, types and WASM, and attaches the tarball to that release. Consumers pin the release asset URL and its package-manager integrity hash.

## Shared Office fonts

The shared font package is distributed as a versioned GitHub Release tarball. Bump `packages/office-fonts/package.json` when its packaged contents change, validate the package, and push an `office-fonts-v<version>` tag on `main`. The font release workflow attaches the packed package. React viewers pin this release URL and its lockfile integrity hash, then bundle the selected lazy font chunks and redistribution notices.

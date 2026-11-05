# Publishing Rust crates

`rust-docx` and `rust-docx-wasm` share a release version: bump both whenever either package changes. The release check rejects mismatched versions. Other crates have independent Cargo versions. For a change to a crate's packaged source or manifest, bump the version in the same PR and run `cargo update --workspace` to refresh the lockfile. Update local dependency requirements when the new version falls outside their range. Rebuild the browser chart distribution when its Rust renderer or bindings change.

On pull requests and pushes affecting the Rust workspace, CI packages the workspace and compares each existing version with its verified crates.io archive. Changing published contents without changing the version fails the check and lists the differing paths. The comparison ignores Cargo's checkout metadata, original unnormalized manifest and generated library lockfile.

A push to `main` with new versions runs native and WebAssembly checks, then publishes all pending crates in dependency order. Cargo waits for dependencies to appear in the index before uploading their dependents. A retry checks the index again and skips completed uploads. To verify a clean checkout locally, run `python3 scripts/release.py dry-run`. To recover an interrupted release, run **Release crates** on `main` with the dry-run option cleared.

Configure a crates.io trusted publisher for each crate with repository owner `extend-hq`, repository `rust-office`, workflow `release.yml` and environment `crates-io`. Restrict that GitHub environment to `main`. After the first publication of a new crate, its owner can register this publisher under its crates.io settings.

This workflow uses the Cargo project's short-lived publisher credentials and serial release jobs, together with the branch and manifest triggers used in the Leo workspace. See the [Cargo release workflow](https://github.com/rust-lang/cargo/blob/master/.github/workflows/release.yml), [Leo release workflow](https://github.com/ProvableHQ/leo/blob/master/.github/workflows/publish-crates.yml) and [crates.io trusted publishing guide](https://crates.io/docs/trusted-publishing).

## Browser distribution

Each published crate also gets a GitHub release named `<crate>-v<version>`, with its `.crate` archive, registry link, API documentation and source link. The release workflow creates these after Cargo finishes publishing. To recover missing GitHub releases from a packaged checkout, run `python3 scripts/github_releases.py`.

The spreadsheet viewer installs the small browser tarball from GitHub Releases. After a browser package change, update its `package.json` version, rebuild with `npm run build:browser`, and check with `npm run check:browser`. Push a `charts-browser-v<version>` tag pointing to a commit on `main`. The browser release workflow verifies the tag, packs the JS, types and WASM, and attaches the tarball to that release. Consumers pin the release asset URL and its package-manager integrity hash.

## Shared Office fonts

The shared font package is distributed as a versioned GitHub Release tarball. Bump `packages/office-fonts/package.json` when its packaged contents change, validate the package, and push an `office-fonts-v<version>` tag on `main`. The font release workflow attaches the packed package. React viewers pin this release URL and its lockfile integrity hash, then bundle the selected lazy font chunks and redistribution notices.

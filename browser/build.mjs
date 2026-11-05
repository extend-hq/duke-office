import { sourceDigest } from "./source-digest.mjs";
import { thirdPartyNotices } from "./notices.mjs";
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { gzipSync } from "node:zlib";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("../", import.meta.url)));
const rustflags = [
  ...(process.env.CARGO_ENCODED_RUSTFLAGS?.split("\x1f") ?? process.env.RUSTFLAGS?.split(/\s+/) ?? []),
  `--remap-path-prefix=${homedir()}=/home/builder`,
  `--remap-path-prefix=${root}=.`,
].join("\x1f");
const crate = resolve(root, "crates/rust-office-charts-wasm");
const generated = resolve(root, "browser/generated");
const digest = sourceDigest();
for (const extended of [false, true]) {
  const output = extended ? resolve(generated, "full") : generated;
  mkdirSync(output, { recursive: true });
  const build = spawnSync(
    "wasm-pack",
    ["build", crate, "--target", "web",
      "--release", "--no-typescript", "--no-pack", "--out-dir", output,
      "--out-name", "office_charts", "--", "--locked",
      ...(extended ? ["--features", "map-fallback"] : [])],
    {
      stdio: "inherit",
      env: {
        ...process.env,
        CARGO_PROFILE_RELEASE_OPT_LEVEL: "z",
        CARGO_PROFILE_RELEASE_CODEGEN_UNITS: "1",
        CARGO_PROFILE_RELEASE_LTO: "true",
        CARGO_PROFILE_RELEASE_PANIC: "abort",
        CARGO_PROFILE_RELEASE_STRIP: "true",
        CARGO_INCREMENTAL: "0",
        CARGO_ENCODED_RUSTFLAGS: rustflags,
      },
    },
  );
  if (build.error) throw build.error;
  if (build.status !== 0) process.exit(build.status ?? 1);
  const compressed = gzipSync(readFileSync(resolve(output, "office_charts_bg.wasm")), { level: 9 });
  writeFileSync(resolve(output, "payload.js"), `export default "${compressed.toString("base64")}";\n`);
  rmSync(resolve(output, ".gitignore"), { force: true });
}
if (digest !== sourceDigest()) throw new Error("Renderer inputs changed during build; rerun the build.");
writeFileSync(resolve(generated, "sources.sha256"), digest + "\n");
writeFileSync(resolve(root, "THIRD-PARTY-NOTICES.txt"), thirdPartyNotices(root));

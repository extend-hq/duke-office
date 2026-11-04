import { sourceDigest } from "./source-digest.mjs";
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
const crate = resolve(root, "crates/extend-office-charts-wasm");
const output = resolve(root, "browser/generated");
mkdirSync(output, { recursive: true });
const build = spawnSync(
  "wasm-pack",
  ["build", crate, "--target", "web",
    "--release", "--no-typescript", "--no-pack", "--out-dir", output,
    "--out-name", "office_charts", "--", "--locked"],
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
const binary = resolve(output, "office_charts_bg.wasm");
const compressed = gzipSync(readFileSync(binary), { level: 9 });
writeFileSync(resolve(output, "payload.js"), `export default "${compressed.toString("base64")}";\n`);
rmSync(resolve(output, ".gitignore"), { force: true });
writeFileSync(resolve(output, "sources.sha256"), sourceDigest() + "\n");

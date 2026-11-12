import { createHash } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));

function rustSources(dir) {
  return readdirSync(resolve(root, dir), { withFileTypes: true }).flatMap((entry) => {
    const path = dir + "/" + entry.name;
    return entry.isDirectory() ? rustSources(path) : [path];
  });
}

export function sourceDigest() {
  const files = ["Cargo.toml", "Cargo.lock", "browser/build.mjs", "browser/source-digest.mjs", "browser/notices.mjs"];
  for (const crate of ["rust-odf", "rust-office-art", "rust-office-charts", "rust-office-charts-wasm"]) {
    files.push(`crates/${crate}/Cargo.toml`, ...rustSources(`crates/${crate}/src`));
  }
  const hash = createHash("sha256");
  for (const file of files.sort()) {
    hash.update(file).update("\0").update(readFileSync(resolve(root, file))).update("\0");
  }
  return hash.digest("hex");
}

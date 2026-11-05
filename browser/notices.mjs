import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";

export function thirdPartyNotices(root) {
  const result = spawnSync("cargo", ["metadata", "--locked", "--offline", "--format-version", "1", "--filter-platform", "wasm32-unknown-unknown"], { cwd: root, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
  if (result.status !== 0) throw Error(result.stderr || "Cannot read renderer dependency metadata.");
  const metadata = JSON.parse(result.stdout);
  const graph = new Map(metadata.resolve.nodes.map(node => [node.id, node]));
  const reached = new Set();
  const visit = id => {
    if (reached.has(id)) return;
    reached.add(id);
    for (const dependency of graph.get(id)?.deps ?? []) {
      if (dependency.dep_kinds.some(kind => kind.kind !== "dev")) visit(dependency.pkg);
    }
  };
  visit(metadata.packages.find(pkg => pkg.name === "rust-office-charts-wasm").id);
  return metadata.packages.filter(pkg => reached.has(pkg.id)).sort((a, b) => a.name.localeCompare(b.name)).map(pkg => {
    const directory = dirname(pkg.manifest_path);
    const extra = pkg.name === "rust-office-art" ? ["src/geometry_data/LICENSE.apache-poi", "src/geometry_data/NOTICE.apache-poi", "src/geometry_data/SOURCE.md"] : [];
    const paths = [...new Set([pkg.license_file, "LICENSE", "LICENSE-MIT", "LICENSE-APACHE", "LICENSE.txt", "LICENSE.md", "COPYING", ...extra].filter(Boolean))];
    const text = paths.map(path => join(directory, path)).filter(existsSync).map(path => readFileSync(path, "utf8").replace(/[ \t]+$/gm, "")).join("\n\n");
    return `${pkg.name} ${pkg.version}\n${pkg.license ?? ""}\n${pkg.repository ?? ""}\n\n${text}`;
  }).join("\n\n--------------------\n\n").trimEnd() + "\n";
}

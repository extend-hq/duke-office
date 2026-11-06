import { spawnSync } from "node:child_process";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";

export function thirdPartyNotices(root) {
  const result = spawnSync("cargo", ["metadata", "--locked", "--offline", "--all-features", "--format-version", "1", "--filter-platform", "wasm32-unknown-unknown"], { cwd: root, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
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
  const notices = metadata.packages.filter(pkg => reached.has(pkg.id)).sort((a, b) => a.name.localeCompare(b.name)).map(pkg => {
    const directory = dirname(pkg.manifest_path);
    const extra = pkg.name === "rust-office-art" ? [
      "src/geometry_data/LICENSE.apache-poi", "src/geometry_data/NOTICE.apache-poi",
      "src/geometry_data/LICENSE.ecma", "src/geometry_data/SOURCE.md",
      ...["arimo", "carlito", "tinos", "cousine"].map(family => `src/fonts/${family}/LICENSE`),
    ] : [];
    const discovered = readdirSync(directory, { withFileTypes: true })
      .filter(entry => entry.isFile() && /^(?:licen[cs]e|copying|copyright|notice|unlicense)(?:$|[._-]|s(?:$|[._-]))/i.test(entry.name))
      .map(entry => entry.name).sort();
    const paths = [...new Set([pkg.license_file, ...discovered, ...extra].filter(Boolean))];
    const text = paths.map(path => readFileSync(join(directory, path), "utf8").replace(/\r\n/g, "\n").replace(/[ \t]+$/gm, "")).join("\n\n");
    if (!text.trim()) throw Error(`Missing license text for ${pkg.name} ${pkg.version}.`);
    return `${pkg.name} ${pkg.version}\n${pkg.license ?? ""}\n${pkg.repository ?? ""}\n\n${text}`;
  }).join("\n\n--------------------\n\n").trimEnd() + "\n";
  return "Scope: non-development Cargo dependencies of rust-office-charts-wasm for wasm32-unknown-unknown, with all features enabled.\n"
    + "This is not a workspace-wide dependency notice inventory.\n"
    + "Bundled font and map data notices are in FONT-DATA-LICENSE and MAP-DATA-LICENSE.\n\n"
    + notices;
}

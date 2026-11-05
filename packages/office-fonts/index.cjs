exports.OFFICE_FALLBACK_FONT = "Carlito";
exports.loadOfficeFonts = (target, families) => import("./index.js").then((m) => m.loadOfficeFonts(target, families));
exports.embedOfficeFonts = (svg) => import("./index.js").then((m) => m.embedOfficeFonts(svg));
const families = require("./families.json");
exports.officeFontFallback = (family) => {
  const primary = family.split(",")[0].trim().replace(/^["']|["']$/g, "").toLowerCase();
  return families.find((entry) => [entry.original, entry.family].some((name) => name.toLowerCase() === primary))?.family;
};

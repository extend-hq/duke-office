import { families } from "./families.js";

const registry = Symbol.for("@extend-ai/office-fonts/loads/v2");
const loads = globalThis[registry] ??= new WeakMap();
const normalize = (family) => family.trim().replace(/^["']|["']$/g, "").toLowerCase();
const primary = (family) => family.split(",")[0];
const lookup = new Map(families.flatMap((family) =>
  [family.original, family.family].map((name) => [normalize(name), family])));
const variants = [
  { style: "normal", weight: "400", suffix: "" },
  { style: "normal", weight: "700", suffix: " Bold" },
  { style: "italic", weight: "400", suffix: " Italic" },
  { style: "italic", weight: "700", suffix: " Bold Italic" },
];

export function officeFontFallback(family) {
  return lookup.get(normalize(primary(family)))?.family;
}

function matches(face, family, variant) {
  const weight = face.weight === "normal" ? "400" : face.weight === "bold" ? "700" : face.weight;
  const range = weight.split(/\s+/).map(Number);
  return normalize(face.family) === normalize(family) && face.style === variant.style
    && Number(variant.weight) >= range[0] && Number(variant.weight) <= (range[1] ?? range[0]);
}

export async function loadOfficeFonts(target = globalThis.document, requestedFamilies = ["Calibri"]) {
  const FontFaceClass = target?.defaultView?.FontFace ?? globalThis.FontFace;
  if (!target?.fonts || !FontFaceClass) return;
  const fonts = target.fonts;
  let entries = loads.get(fonts);
  if (!entries) loads.set(fonts, entries = new Map());
  const selected = new Set([...requestedFamilies].map((name) => normalize(primary(name))));
  await Promise.all([...selected].map(async (key) => {
    const definition = lookup.get(key);
    if (!definition) return;
    const previous = entries.get(key);
    if (previous && (!previous.ready || previous.faces.every((face) => fonts.has(face)))) return previous.promise;
    const family = key === normalize(definition.original) ? definition.original : definition.family;
    const state = { ready: false, faces: [], promise: undefined };
    state.promise = (async () => {
      const added = [];
      try {
        const results = await Promise.allSettled(variants.map(async (variant, index) => {
          const existing = [...fonts].filter((face) => matches(face, family, variant) && face.status !== "error");
          if (existing.length) {
            const loaded = await Promise.allSettled(existing.map((face) => face.load()));
            const available = loaded.flatMap((result) => result.status === "fulfilled" ? [result.value] : []);
            if (available.length) return available;
          }
          let face;
          if (family === definition.original) {
            try {
              face = await new FontFaceClass(family, `local("${family}${variant.suffix}")`, variant).load();
            } catch {
              face = undefined;
            }
          }
          if (!face) {
            const { faces: assets } = await definition.load();
            face = new FontFaceClass(family, `url("${assets[index].source}")`, variant);
            fonts.add(face);
            added.push(face);
            await face.load();
          } else {
            fonts.add(face);
            added.push(face);
          }
          return [face];
        }));
        const failure = results.find((result) => result.status === "rejected");
        if (failure) throw failure.reason;
        state.faces = results.flatMap((result) => result.value);
        state.ready = true;
      } catch (error) {
        for (const face of added) fonts.delete(face);
        entries.delete(key);
        throw error;
      }
    })();
    entries.set(key, state);
    return state.promise;
  }));
}

export async function embedOfficeFonts(svg) {
  const opening = /^\s*(?:<\?xml[^?]*\?>\s*)?<svg\b[^>]*>/i.exec(svg);
  if (!opening || /data-office-fonts=/.test(svg)) return svg;
  const names = new Set();
  for (const match of svg.matchAll(/\bfont-family\s*(?:=\s*"([^"]*)"|=\s*'([^']*)'|:\s*([^;}]+))/gi)) {
    const name = primary((match[1] ?? match[2] ?? match[3]).replace(/&quot;|&apos;/g, ""));
    const definition = lookup.get(normalize(name));
    if (definition) names.add(normalize(name));
  }
  if (!names.size) return svg;
  const bold = /font-weight\s*(?:=\s*["']|:\s*)(?:bold|[5-9]00)/i.test(svg);
  const italic = /font-style\s*(?:=\s*["']|:\s*)(?:italic|oblique)/i.test(svg);
  const rules = await Promise.all([...names].map(async (name) => {
    const definition = lookup.get(name);
    const family = name === normalize(definition.original) ? definition.original : definition.family;
    const { faces } = await definition.load();
    return faces.flatMap(({ source, style, weight }, index) => {
      if ((!bold && weight === "700") || (!italic && style === "italic")) return [];
      const local = family === definition.original ? `local("${family}${variants[index].suffix}"),` : "";
      return [`@font-face{font-family:"${family}";src:${local}url("${source}") format("woff2");font-style:${style};font-weight:${weight}}`];
    }).join("");
  }));
  return svg.slice(0, opening[0].length) + `<defs data-office-fonts="v2"><style>${rules.join("")}</style></defs>` + svg.slice(opening[0].length);
}

export const OFFICE_FALLBACK_FONT = "Carlito";

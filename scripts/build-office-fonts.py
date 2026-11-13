import base64
import json
import struct
from pathlib import Path

from fontTools.ttLib import TTFont

root = Path(__file__).resolve().parents[1]
package = root / "packages/office-fonts"
destination = root / "crates/duke-office-charts/src/parse/font_metrics"
destination.mkdir(exist_ok=True)
families = json.loads((package / "families.json").read_text())
licenses = []
imports = []
config = ['<?xml version="1.0"?>', '<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">', '<fontconfig>', '  <include ignore_missing="yes">/etc/fonts/fonts.conf</include>', '  <dir prefix="relative">fonts</dir>', '  <cachedir prefix="xdg">fontconfig</cachedir>']
styles = [("Regular", "normal", "400", ""), ("Bold", "normal", "700", " Bold"), ("Italic", "italic", "400", " Italic"), ("BoldItalic", "italic", "700", " Bold Italic")]

for family in families:
    key, original, fallback, prefix = (family[k] for k in ("id", "original", "family", "prefix"))
    faces, css = [], []
    fonts = [TTFont(package / "fonts" / key / f"{prefix}-{style}.ttf", recalcTimestamp=False) for style, *_ in styles]
    codepoints = sorted(set.intersection(*(set(font.getBestCmap()) for font in fonts)))
    metrics = bytearray(struct.pack("<HH", fonts[0]["head"].unitsPerEm, len(codepoints)))
    metrics += b"".join(struct.pack("<I", cp) for cp in codepoints)
    for font, (name, style, weight, suffix) in zip(fonts, styles):
        assert font["head"].unitsPerEm == fonts[0]["head"].unitsPerEm
        cmap = font.getBestCmap()
        metrics += b"".join(struct.pack("<H", font["hmtx"][cmap[cp]][0]) for cp in codepoints)
        path = package / "fonts" / key / f"{prefix}-{name}.woff2"
        font.flavor = "woff2"
        font.save(path)
        source = "data:font/woff2;base64," + base64.b64encode(path.read_bytes()).decode()
        faces.append(dict(source=source, style=style, weight=weight))
        for alias in [original, fallback]:
            sources = (f'local("{original}{suffix}"), ' if alias == original else "") + f'url("./fonts/{key}/{prefix}-{name}.woff2") format("woff2")'
            css.append(f'@font-face {{\n  font-family: "{alias}";\n  src: {sources};\n  font-style: {style};\n  font-weight: {weight};\n  font-display: swap;\n}}\n')
    (package / f"{key}-data.js").write_text("export const faces = " + json.dumps(faces, separators=(",", ":")) + ";\n")
    (package / f"{key}.css").write_text("\n".join(css))
    (destination / f"{key}.bin").write_bytes(metrics)
    license_text = (package / "fonts" / key / "LICENSE").read_text()
    (destination / f"{key}-LICENSE").write_text(license_text)
    licenses.append(f"{fallback}\n{'=' * len(fallback)}\n{license_text}\n")
    imports.append(f'  {{ id: "{key}", family: "{fallback}", original: "{original}", load: () => import("./{key}-data.js") }}')
    config.append(f'  <alias binding="same"><family>{original}</family><accept><family>{fallback}</family></accept></alias>')
    print(f"{fallback}: {len(codepoints)} characters, {len(metrics)} metric bytes")

config.append('</fontconfig>')
(package / "fontconfig-aliases.conf").write_text("<fontconfig>\n" + "\n".join(line for line in config if line.startswith("  <alias")) + "\n</fontconfig>\n")
(package / "fonts.conf").write_text("\n".join(config) + "\n")
(package / "families.js").write_text("export const families = [\n" + ",\n".join(imports) + "\n];\n")
(package / "FONT-LICENSES.txt").write_text("\n".join(licenses))
(root / "FONT-DATA-LICENSE").write_text("\n".join(licenses))

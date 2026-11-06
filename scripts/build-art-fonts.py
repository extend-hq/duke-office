"""Rebuild portable outline fonts, preserving the complete glyph repertoire."""
from pathlib import Path
import hashlib
import json
import shutil
from fontTools.ttLib import TTFont

root = Path(__file__).resolve().parents[1]
source = root / "packages/office-fonts/fonts"
destination = root / "crates/rust-office-art/src/fonts"
for family in ("arimo", "carlito", "tinos", "cousine"):
    target = destination / family
    target.mkdir(parents=True, exist_ok=True)
    output_hashes = {}
    for path in sorted((source / family).glob("*.ttf")):
        font = TTFont(path, recalcTimestamp=False)
        for tag in ("DSIG", "LTSH", "hdmx", "VDMX", "cvt ", "fpgm", "prep", "gasp"):
            if tag in font:
                del font[tag]
        if "glyf" in font:
            for glyph in font["glyf"].glyphs.values():
                glyph.removeHinting()
        if family == "carlito":
            for record in font["name"].names:
                if record.nameID in (1, 3, 4, 6, 16, 18, 21):
                    renamed = record.toUnicode().replace("Carlito", "OfficeArtSans")
                    record.string = renamed.encode(record.getEncoding())
        name = path.name.replace("Carlito", "OfficeArtSans") if family == "carlito" else path.name
        font.save(target / name)
        output_hashes[name] = hashlib.sha256((target / name).read_bytes()).hexdigest()
    shutil.copyfile(source / family / "LICENSE", target / "LICENSE")
    provenance = {
        "input": json.loads((source / family / "source.json").read_text()),
        "processing": "Hinting instructions and device-metric tables removed; complete glyph repertoire and shaping retained."
        + (" Reserved font names renamed to OfficeArtSans." if family == "carlito" else ""),
        "generator": "scripts/build-art-fonts.py",
        "files": output_hashes,
    }
    (target / "source.json").write_text(json.dumps(provenance, indent=2) + "\n")

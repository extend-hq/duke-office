"""Compose a documentation gallery from a completed native comparison run."""

import argparse
import csv
import hashlib
import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont, ImageOps


GROUPS = {
    "bars-lines-areas": [(15, "Clustered columns"), (16, "Horizontal bars"), (17, "Lines"), (21, "Overlapping areas"), (109, "Stacked positive and negative values"), (112, "Percent bars and zeroes"), (115, "Stacked areas"), (116, "Secondary-axis combination")],
    "pies-and-rings": [(18, "Pie"), (19, "Doughnut"), (72, "Exploded pie and labels"), (73, "3D pie"), (74, "Large doughnut hole"), (75, "Bar-of-pie"), (81, "Pie-of-pie"), (140, "Multiple rings")],
    "scatter-radar-stock": [(85, "Scatter lines and markers"), (86, "Smooth scatter"), (87, "Bubbles"), (98, "Radar"), (100, "Filled radar"), (101, "Scatter with error bars"), (106, "Open-high-low-close"), (108, "Volume-open-high-low-close")],
    "modern-charts": [(43, "Sunburst"), (44, "Treemap"), (45, "Waterfall"), (46, "Funnel"), (47, "Histogram"), (48, "Pareto"), (49, "Box and whisker"), (50, "Filled map")],
    "three-dimensional": [(31, "Stacked 3D area"), (33, "Exploded 3D pie"), (37, "Stacked cones"), (38, "Stacked cylinders"), (39, "Horizontal pyramids"), (41, "Cylindrical columns"), (42, "Pyramid columns"), (78, "Surface")],
    "formatting-and-effects": [(123, "Area bevels"), (124, "Column bevels"), (126, "Line shadows"), (133, "Radar line effects"), (148, "Doughnut bevels and shadows"), (152, "Bubble bevels and labels"), (153, "Filled radar bevels"), (157, "Trendline equation and R-squared")],
}


def compose(rows, cases, output, width=360, height=235):
    gutter, header = 20, 72
    cell_width, cell_height = width * 2 + gutter, height + header
    canvas = Image.new("RGB", (cell_width * 2 + gutter, ((len(cases) + 1) // 2) * cell_height + gutter), "#edf1f5")
    draw = ImageDraw.Draw(canvas)
    font = ImageFont.load_default(size=16)
    small = ImageFont.load_default(size=13)
    for i, (ordinal, label) in enumerate(cases):
        row = rows[ordinal]
        x, y = gutter + i % 2 * cell_width, gutter + i // 2 * cell_height
        draw.text((x, y), label, fill="#18212c", font=font)
        for j, (caption, path) in enumerate([("Microsoft Excel", row["reference"]["reference"]), ("React XLSX / Rust", row["png"])]):
            left = x + j * width
            draw.text((left, y + 27), caption, fill="#4a5765", font=small)
            image = Image.open(path).convert("RGB")
            thumbnail = ImageOps.contain(image, (width - 8, height - 8), Image.Resampling.LANCZOS)
            canvas.paste("white", (left, y + 49, left + width - 8, y + 49 + height))
            canvas.paste(thumbnail, (left, y + 49 + (height - thumbnail.height) // 2))
    canvas.save(output, optimize=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    report = json.loads(args.report.read_text())
    rows = {row["ordinal"]: row for row in report["rows"]}
    args.output.mkdir(parents=True, exist_ok=True)
    for name, cases in GROUPS.items():
        compose(rows, cases, args.output / f"excel-chart-{name}.png")
    overview = [case for cases in GROUPS.values() for case in cases[:4]]
    compose(rows, overview, args.output / "excel-chart-overview.png", 270, 175)
    fields = ["ordinal", "chartType", "referenceKind", "meanAbsoluteError", "changedPixels", "pixelCount", "exact", "effectCount"]
    with (args.output / "excel-chart-comparison.csv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=fields)
        writer.writeheader()
        writer.writerows({key: row.get(key) for key in fields} for row in rows.values())
    provenance = {
        "date": "2026-11-05",
        "reportSha256": hashlib.sha256(args.report.read_bytes()).hexdigest(),
        "rendererWasmSha256": report["environment"]["wasmSha256"],
        "fontArchiveSha256": report["environment"]["fontPackage"]["archiveSha256"],
        "referenceManifestSha256": report["comparisonReferencesSha256"],
        "summary": report["summary"],
        "galleryOrdinals": GROUPS,
    }
    (args.output / "excel-chart-provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")


if __name__ == "__main__":
    main()

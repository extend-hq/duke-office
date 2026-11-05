import argparse
import hashlib
import json
import re
import shutil
import subprocess
import unicodedata
from collections import Counter
from pathlib import Path
from xml.etree import ElementTree as ET

import numpy as np
from PIL import Image

p = argparse.ArgumentParser(description="Pair Excel exports with chart parts and measure the resulting browser renders.")
p.add_argument("step", choices=["prepare", "report"])
p.add_argument("root", type=Path)
p.add_argument("--references", type=Path, nargs="*", default=[])
p.add_argument("--run", type=Path)
p.add_argument("--baseline", type=Path, help="Previously scored report.json with the same jobs, Chromium and fonts")
args = p.parse_args()
root = args.root.resolve()


def read(path):
    return json.loads(path.read_text())


def digest(path):
    with Path(path).open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def prepare():
    rows = read(root / "inputs.json")
    for source in args.references:
        for ref in read(source):
            on_sheet = [r for r in rows if r["workbookSha256"] == ref["workbookSha256"] and r["sheet"] == ref["sheet"]]
            matched = [r for r in on_sheet if (r["part"] == ref["part"] if ref.get("part") else r["name"] == ref["name"])]
            basis = "part" if ref.get("part") else "object name"
            if not matched and not ref.get("part") and len(on_sheet) == 1 and on_sheet[0]["sheetType"] == "chartsheet":
                matched, basis = on_sheet, "single chart on chartsheet"
            if len(matched) > 1 and ref.get("chartOrdinal"):
                matched = [r for r in matched if r["chartOrdinal"] == int(ref["chartOrdinal"])]
                basis = "object name + drawing ordinal"
            if len(matched) > 1:
                print(f"Skipping ambiguous native reference for {ref['sheet']}: no unique object identity")
                continue
            if not matched:
                continue
            if ref.get("unavailable"):
                if "reference" not in matched[0]:
                    matched[0]["unavailable"] = ref["unavailable"]
                    for key, dest in (("widthPt", "cssWidth"), ("heightPt", "cssHeight")):
                        if ref.get(key, 0) > 0:
                            matched[0][dest] = ref[key] * 4 / 3
                continue
            path = (source.parent / ref["png"]).resolve()
            with Image.open(path) as im:
                w, h = im.size
            dpi = float(ref.get("dpi", 72))
            if not 0 < dpi <= 1200:
                raise ValueError(f"Invalid capture DPI for {path}")
            cw = float(ref.get("widthPt", w * 72 / dpi)) * 4 / 3
            ch = float(ref.get("heightPt", h * 72 / dpi)) * 4 / 3
            if not (cw > 0 and ch > 0):
                raise ValueError(f"Invalid capture dimensions for {path}")
            matched[0].update(reference=str(path), referenceSha256=digest(path),
                              pdfSha256=digest(path.with_suffix(".pdf")) if path.with_suffix(".pdf").exists() else None,
                              referenceMatch=basis, pixels=[w, h], cssWidth=cw, cssHeight=ch, dpi=dpi,
                              dimensionSource="chart object" if "widthPt" in ref else f"{dpi:g} dpi export",
                              aspectMismatch=max(abs(cw * dpi / 96 - w), abs(ch * dpi / 96 - h)) > 2,
                              referenceSource=ref.get("source", source.name), excelVersion=ref.get("excelVersion"),
                              derived=bool(ref.get("derived") or ref.get("referenceClass") == "derived"),
                              captureWorkbookSha256=ref.get("derivedSha256", ref.get("derivedWorkbookSha256", ref["workbookSha256"])))
    save(root / "jobs.json", rows)
    print(sum("reference" in r for r in rows), "/", len(rows), "paired")


def image(path):
    with Image.open(path) as im:
        rgba = im.convert("RGBA")
        bg = Image.new("RGBA", rgba.size, "white")
        return np.asarray(Image.alpha_composite(bg, rgba).convert("RGB"), dtype=np.int16)


def labels(reference, drawn):
    pdf = Path(reference).with_suffix(".pdf")
    if not pdf.is_file() or not shutil.which("pdftotext"):
        return None
    p = subprocess.run(["pdftotext", "-layout", str(pdf), "-"], capture_output=True, timeout=30)
    if p.returncode:
        return None

    def tokens(text):
        return Counter(re.findall(r"[^\s]+", unicodedata.normalize("NFKC", text).replace("−", "-")))

    expected = tokens(p.stdout.decode("utf-8", errors="replace"))
    if not expected:
        return None
    actual = tokens(" ".join(drawn))
    return {"match": actual == expected, "missing": dict(expected - actual), "extra": dict(actual - expected),
            "nativeTokens": sum(expected.values()), "candidateTokens": sum(actual.values())}


def measure(ref, candidate, diff):
    a, b = image(ref), image(candidate)
    if a.shape != b.shape:
        raise ValueError("candidate and reference dimensions differ")
    delta = np.abs(a - b)
    mask = np.any(delta != 0, axis=2)
    magnitude = delta.max(axis=2)
    heat = np.stack([255 - magnitude // 4, 255 - magnitude, 255 - magnitude], axis=2).astype("uint8")
    Image.fromarray(heat).save(diff)
    Image.fromarray(~mask).save(diff.with_name(diff.name.replace("-diff.png", "-mask.png")))
    ink_a, ink_b = a.min(axis=2) < 242, b.min(axis=2) < 242
    union = np.logical_or(ink_a, ink_b)
    return dict(exact=not mask.any(), differingPixels=int(mask.sum()), pixels=int(mask.size),
                rgbMae=round(float(delta.mean()), 4),
                inkIoU=round(float(np.logical_and(ink_a, ink_b).sum() / max(1, union.sum())), 6))


def chart_details(job):
    doc = ET.fromstring(read(root / "cases" / (job["id"] + ".json"))["xml"])
    title = doc.find("{*}chart/{*}title/{*}tx")
    text = "".join((n.text or "") for n in title.iter() if n.tag.rsplit("}", 1)[-1] in ("t", "v")) if title is not None else ""
    plot = doc.find("{*}chart/{*}plotArea")
    tags = []
    if plot is not None:
        for node in plot:
            for key in ("barDir", "grouping", "scatterStyle", "radarStyle", "ofPieType", "shape"):
                prop = node.find("{*}" + key)
                if prop is not None and prop.get("val"):
                    tags.append(prop.get("val"))
        for key, label in (("dropLines", "drop lines"), ("hiLowLines", "high-low lines"), ("upDownBars", "up/down bars"),
                           ("dTable", "data table"), ("trendline", "trendline"), ("errBars", "error bars"),
                           ("manualLayout", "manual layout"), ("logBase", "log axis"), ("dateAx", "date axis"),
                           ("multiLvlStrRef", "hierarchical categories")):
            if plot.find(".//{*}" + key) is not None:
                tags.append(label)
        if len(plot.findall("{*}valAx")) > (2 if "scatter" in job["kinds"] else 1):
            tags.append("secondary value axis")
    return text, list(dict.fromkeys(tags))


def report():
    run = args.run.resolve()
    rendered = read(run / "renders.json")
    if rendered["jobsSha256"] != hashlib.sha256((root / "jobs.json").read_bytes()).hexdigest():
        raise ValueError("Reference manifest or corpus changed; regenerate this render before scoring")
    indexed = {r["id"]: r for r in rendered["rows"]}
    rows = []
    for source in read(root / "jobs.json"):
        r = dict(source, **{k: v for k, v in indexed[source["id"]].items() if k != "id"})
        r["title"], r["features"] = chart_details(r)
        if r.get("reference") and r["rendered"]:
            if digest(r["reference"]) != r["referenceSha256"]:
                raise ValueError(f"Native picture changed since prepare: {r['reference']}")
            if r.get("pdfSha256") and digest(Path(r["reference"]).with_suffix(".pdf")) != r["pdfSha256"]:
                raise ValueError(f"Native text source changed since prepare: {r['reference']}")
            r["labelTokens"] = labels(r["reference"], r["labels"])
            r["metrics"] = measure(r["reference"], run / (r["id"] + ".png"), run / (r["id"] + "-diff.png"))
        rows.append(r)
    counts = dict(charts=len(rows), workbooks=len(set(r["workbookSha256"] for r in rows)),
                  rendered=sum(r["rendered"] for r in rows), references=sum("reference" in r for r in rows),
                  compared=sum("metrics" in r for r in rows), missingReference=sum("reference" not in r for r in rows),
                  exact=sum(r.get("metrics", {}).get("exact", False) for r in rows),
                  unresolvedInputs=sum(bool(r["unresolved"]) for r in rows))
    result = dict(counts=counts, **{k: rendered[k] for k in ("engineSha256", "jobsSha256", "browser", "fontManifest")}, rows=rows)
    if rendered.get("completedAt"):
        result["completedAt"] = rendered["completedAt"]
    if args.baseline:
        baseline = read(args.baseline)
        if any(result[k] != baseline[k] for k in ("jobsSha256", "browser", "fontManifest")):
            raise ValueError("Before/after metrics require the same dimensions, references, fonts and browser")
        previous = {r["id"]: r for r in baseline["rows"]}
        for r in rows:
            m = previous[r["id"]].get("metrics")
            if m and r.get("metrics"):
                r["before"] = m
                r["diffPixelChange"] = r["metrics"]["differingPixels"] - m["differingPixels"]
        result["baselineEngineSha256"] = baseline["engineSha256"]
    books = read(root / "workbooks.json")
    counts.update(inputPaths=sum(len(b["aliases"]) for b in books), distinctInputs=len(books),
                  unreadable=sum("error" in b for b in books), zeroCharts=sum(not b["charts"] and "error" not in b for b in books))
    def aggregate(items):
        compared = [r["metrics"] for r in items if r.get("metrics")]
        pairs = [(r["before"], r["metrics"]) for r in items if r.get("before") and r.get("metrics")]
        prior = [a for a, _ in pairs]
        def weighted(results):
            total = sum(m["pixels"] for m in results)
            return round(sum(m["rgbMae"] * m["pixels"] for m in results) / total, 4) if total else None
        return dict(charts=len(items), compared=len(compared), exact=sum(m["exact"] for m in compared),
                    differingPixels=sum(m["differingPixels"] for m in compared),
                    pixels=sum(m["pixels"] for m in compared), weightedRgbMae=weighted(compared),
                    beforeWeightedRgbMae=weighted(prior), pairedAfterWeightedRgbMae=weighted([b for _, b in pairs]), beforeCompared=len(prior),
                    improvedMae=sum(a["rgbMae"] > b["rgbMae"] for a, b in pairs),
                    worseMae=sum(a["rgbMae"] < b["rgbMae"] for a, b in pairs),
                    unchangedMae=sum(a["rgbMae"] == b["rgbMae"] for a, b in pairs),
                    textMatch=sum(bool(r.get("labelTokens") and r["labelTokens"]["match"]) for r in items),
                    textMeasured=sum(r.get("labelTokens") is not None for r in items))
    kinds = sorted({"+".join(r["kinds"]) for r in rows})
    result["summary"] = aggregate(rows)
    result["directReferences"] = aggregate([r for r in rows if r.get("reference") and not r.get("derived")])
    result["derivedReferences"] = aggregate([r for r in rows if r.get("derived")])
    result["families"] = {k: aggregate([r for r in rows if "+".join(r["kinds"]) == k]) for k in kinds}
    save(run / "report.json", result)
    from report_html import html_report
    (run / "report.html").write_text(html_report(run, root, result))
    print(counts)


if args.step == "prepare":
    prepare()
else:
    if args.run is None:
        p.error("--run is required to generate the report")
    report()

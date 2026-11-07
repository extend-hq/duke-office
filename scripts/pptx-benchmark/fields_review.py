import argparse
import collections
import gzip
import hashlib
import json
from pathlib import Path
import zipfile
from xml.etree import ElementTree as ET

from artwork_math_gate import artwork_hashes
from feedback import OUT
from object_gate import invoke, changes


DRAWING = {"http://schemas.openxmlformats.org/drawingml/2006/main", "http://purl.oclc.org/ooxml/drawingml/main"}


def signature(node):
    return (node.tag, tuple(sorted(node.attrib.items())), node.text or "", tuple((signature(child), child.tail or "") for child in node))


def visit(value, location=""):
    if isinstance(value, dict):
        if isinstance(value.get("field"), dict) and "text" in value:
            yield location, value
        for key, child in value.items():
            if key not in {"data", "bytes"}:
                yield from visit(child, location + "/" + key)
    elif isinstance(value, list):
        for index, child in enumerate(value):
            yield from visit(child, location + "/" + str(index))


def verify_properties(prop, actual):
    prefix = prop.tag.rsplit("}", 1)[0] + "}"
    child = lambda name: prop.find(prefix + name)
    checks = 0
    for source, target in [("marL", "marginLeftEmu"), ("marR", "marginRightEmu"), ("indent", "indentEmu"), ("defTabSz", "defaultTabSizeEmu"), ("lvl", "level")]:
        if source in prop.attrib:
            assert actual.get(target) == int(prop.get(source)), (source, actual)
            checks += 1
    for source, target in [("rtl", "rtl"), ("eaLnBrk", "eastAsianLineBreak"), ("latinLnBrk", "latinLineBreak"), ("hangingPunct", "hangingPunctuation")]:
        if source in prop.attrib:
            assert actual.get(target) == (prop.get(source) in ("1", "true")), (source, actual)
            checks += 1
    for source, target in [("buNone", "bulletNone"), ("buClrTx", "bulletColorFollowText"), ("buFontTx", "bulletFontFollowText"), ("buSzTx", "bulletSizeFollowText")]:
        assert actual.get(target) == (True if child(source) is not None else None), (source, actual)
        checks += child(source) is not None
    if "algn" in prop.attrib:
        expected = {"l": "left", "ctr": "center", "r": "right", "just": "justify", "justLow": "justify", "dist": "distributed", "thaiDist": "distributed"}.get(prop.get("algn"))
        assert actual.get("alignment") == expected
        checks += 1
    if "fontAlgn" in prop.attrib:
        assert actual.get("fontAlignment") == prop.get("fontAlgn")
        checks += 1
    for source, target in [("lnSpc", "lineSpacing"), ("spcBef", "spaceBefore"), ("spcAft", "spaceAfter")]:
        element = child(source)
        if element is not None:
            points, percent = element.find(prefix + "spcPts"), element.find(prefix + "spcPct")
            if points is not None:
                assert actual[target] == dict(value=float(points.get("val")) / 100, unit="points")
            elif percent is not None:
                raw = percent.get("val")
                assert actual[target] == dict(value=float(raw[:-1]) / 100 if raw.endswith("%") else float(raw) / 100000, unit="percent")
            checks += 1
    tabs = child("tabLst")
    if tabs is not None:
        expected = [dict(positionEmu=int(tab.get("pos")), alignment={"ctr": "center", "r": "right", "dec": "decimal"}.get(tab.get("algn"), "left")) for tab in tabs.findall(prefix + "tab")]
        assert actual.get("tabStops") == expected
        checks += len(expected)
    defaults = child("defRPr")
    assert ("defaultRunProperties" in actual) == (defaults is not None)
    if defaults is not None:
        run = actual["defaultRunProperties"]
        for source, target, scale in [("sz", "fontSizePt", 100), ("spc", "characterSpacingPt", 100), ("kern", "kerningThresholdPt", 100), ("baseline", "baseline", 1000)]:
            if source in defaults.attrib:
                assert run.get(target) == float(defaults.get(source)) / scale
                checks += 1
        for source, target in [("b", "bold"), ("i", "italic")]:
            if source in defaults.attrib:
                assert run.get(target) == (defaults.get(source) in ("1", "true"))
                checks += 1
        for source, target in [("lang", "language"), ("altLang", "alternativeLanguage")]:
            if source in defaults.attrib:
                assert run.get(target) == defaults.get(source)
                checks += 1
        for source, target in [("latin", "fontFamily"), ("ea", "eastAsianFontFamily"), ("cs", "complexScriptFontFamily"), ("sym", "symbolFontFamily")]:
            font = defaults.find(prefix + source)
            if font is not None and not font.get("typeface", "").startswith("+"):
                assert run.get(target) == font.get("typeface")
                checks += 1
    return checks


def verify_sources(path, document):
    counts = collections.Counter()
    cache = {}
    with zipfile.ZipFile(path) as archive:
        for location, run in visit(document):
            field = run["field"]
            part = field["sourcePart"]
            if part not in cache:
                root = ET.fromstring(archive.read(part))
                cache[part] = {signature(n) for n in root.iter() if n.tag in {"{" + ns + "}fld" for ns in DRAWING}}
            wrapper = ET.fromstring(field["sourceXml"])
            assert len(wrapper) == 1, location
            source = wrapper[0]
            assert signature(source) in cache[part], (location, part, "source subtree differs")
            prefix = source.tag.rsplit("}", 1)[0] + "}"
            assert field.get("id") == source.get("id")
            assert field.get("type") == source.get("type")
            texts = source.findall(prefix + "t")
            assert field["hasCachedText"] == bool(texts)
            assert run["text"] == "".join("".join(n.itertext()) for n in texts), (location, "cached text differs")
            prop = source.find(prefix + "pPr")
            assert ("paragraphProperties" in field) == (prop is not None), location
            counts["fields"] += 1
            counts["empty_cached"] += bool(texts) and not run["text"]
            counts["missing_cached"] += not texts
            counts["paragraph_properties"] += prop is not None
            counts["property_checks"] += verify_properties(prop, field["paragraphProperties"]) if prop is not None else 0
            counts["strict"] += source.tag.startswith("{http://purl.oclc.org/")
    return counts


def strip_fields(document):
    for location, run in visit(document):
        del run["field"]
    return document


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--label", required=True)
    parser.add_argument("--manifest", required=True, type=Path)
    args = parser.parse_args()
    run = OUT / "experiments" / args.label
    proof = json.loads((run / "parity-summary.json").read_text())
    digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    raw = run / "parity.jsonl"
    assert digest(raw) == proof["records_sha256"] and digest(args.manifest) == proof["manifest_sha256"]
    candidate, baseline = run / "candidate", Path(proof["baseline_path"])
    assert digest(candidate) == proof["candidate_sha256"] and digest(baseline) == proof["baseline_sha256"]
    rows = {row["id"]: row for row in json.loads(args.manifest.read_text())}
    records = [json.loads(line) for line in raw.read_text().splitlines()]
    destination = run / "fields-reviewed.jsonl"
    assert not destination.exists(), "Preserve existing audit receipts."
    totals, reviewed = collections.Counter(), []
    with destination.open("w", buffering=1) as stream:
        for index, record in enumerate(records):
            row = rows[record["id"]]
            assert digest(Path(row["path"])) == row["sha256"]
            current = dict(record, passed=record["equal"])
            if not record["equal"]:
                assert record["status"] == "ok"
                old, before = invoke(baseline, row, run / "before.json.gz")
                new, after = invoke(candidate, row, run / "after.json.gz")
                assert old["status"] == new["status"] == "ok" and new["full_sha256"] == record["full_sha256"]
                current["source_counts"] = dict(verify_sources(row["path"], after))
                totals.update(current["source_counts"])
                current["artwork_hashes"] = artwork_hashes(after)
                assert artwork_hashes(before) == current["artwork_hashes"], "Artwork changed."
                strip_fields(after)
                current["differences"] = changes(before, after)
                current["passed"] = not current["differences"]
            stream.write(json.dumps(current) + "\n")
            reviewed.append(current)
            if not current["passed"]:
                print("REVIEW", row["id"], current["differences"][:15], flush=True)
            elif index % 25 == 0:
                print(index, len(records), flush=True)
    summary = dict(total=len(records), passed=sum(r["passed"] for r in reviewed), exact=sum(r["equal"] for r in reviewed), source_counts=dict(totals), candidate_sha256=proof["candidate_sha256"], baseline_sha256=proof["baseline_sha256"], baseline_path=str(baseline), records_sha256=digest(destination), raw_records_sha256=digest(raw), manifest_sha256=digest(args.manifest), harness_sha256=digest(Path(__file__)), scope="Namespace-aware original-source validation of all emitted field identities, open type strings, cached text and cache presence, scoped field source XML, and supported paragraph/run properties. Full non-field model and payloads compared exactly; any residual differences require separate review. This is not source-wide recall or rendered fidelity.")
    (run / "fields-reviewed-summary.json").write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary))
    assert summary["passed"] == summary["total"]


if __name__ == "__main__":
    main()

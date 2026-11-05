import argparse
import json
from pathlib import Path
from xml.etree.ElementTree import ParseError
from zipfile import BadZipFile

from workbooks import extract, sha256

p = argparse.ArgumentParser(description="Resolve the charts attached to XLSX worksheets and chartsheets.")
p.add_argument("--out", type=Path, required=True)
p.add_argument("inputs", type=Path, nargs="+")
args = p.parse_args()
(args.out / "cases").mkdir(parents=True, exist_ok=True)
paths = sorted({q for src in args.inputs for q in (src.rglob("*.xlsx") if src.is_dir() else [src])
                if q.is_file() and not q.name.startswith("~$")})
rows, books = [], {}
for path in paths:
    digest = sha256(path)
    if digest in books:
        books[digest]["aliases"].append(str(path.resolve()))
        continue
    book = books[digest] = dict(workbook=path.name, workbookSha256=digest, aliases=[str(path.resolve())], charts=0)
    try:
        charts = list(extract(path))
    except (BadZipFile, ParseError, KeyError, ValueError, StopIteration) as exc:
        book["error"] = f"{type(exc).__name__}: {exc}"
        continue
    for i, (info, xml, options) in enumerate(charts):
        case = digest[:16] + "-" + str(i + 1)
        (args.out / "cases" / (case + ".json")).write_text(json.dumps(dict(xml=xml, options=options)))
        rows.append(dict(id=case, workbook=path.name, workbookSha256=digest, **info))
        book["charts"] += 1
for name, data in (("inputs", rows), ("workbooks", list(books.values()))):
    (args.out / (name + ".json")).write_text(json.dumps(data, indent=2) + "\n")
print(f"{len(rows)} attached charts across {len(books)} distinct workbooks ({len(paths)} input paths)")

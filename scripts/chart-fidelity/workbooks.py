"""Extract chart parts, related styles, and missing range caches from XLSX packages."""
import hashlib
import posixpath
import re
from pathlib import Path
from xml.etree import ElementTree as ET
from zipfile import ZipFile

CX = "http://schemas.microsoft.com/office/drawing/2014/chartex"
REF = re.compile(r"(?:'((?:[^']|'')+)'|([^!]+))!\$?([A-Z]{1,3})\$?([1-9]\d*)(?::\$?([A-Z]{1,3})\$?([1-9]\d*))?", re.I)


def local(tag):
    return tag.rsplit("}", 1)[-1]


def rel_id(node):
    return next((v for k, v in node.attrib.items() if k.endswith("}id")), None)


def relationships(package, part):
    base, name = posixpath.split(part)
    path = posixpath.join(base, "_rels", name + ".rels")
    if path not in package.namelist():
        return {}
    return {r.get("Id"): (r.get("Type").split("/")[-1],
                           posixpath.normpath(posixpath.join(base, r.get("Target"))).lstrip("/"))
            for r in ET.fromstring(package.read(path)) if r.get("TargetMode") != "External"}


def sha256(path):
    with open(path, "rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def col_index(label):
    result = 0
    for c in label.upper():
        result = result * 26 + ord(c) - 64
    return result


def rich_text(node):
    return "".join(t.text or "" for t in node.findall("{*}t") + node.findall("{*}r/{*}t"))


class Workbook:
    def __init__(self, package):
        self.package = package
        part = next(path for kind, path in relationships(package, "").values() if kind == "officeDocument")
        self.rels = relationships(package, part)
        self.root = ET.fromstring(package.read(part))
        self.sheets = {s.get("name"): self.rels[rel_id(s)][1] for s in self.root.findall("{*}sheets/{*}sheet")}
        self.names = {n.get("name"): n.text for n in self.root.findall("{*}definedNames/{*}definedName")
                      if "localSheetId" not in n.attrib}
        self.strings = []
        self.sheet_values = {}
        self.options = dict(interactive=True, spreadsheetDefaults=True)
        for kind, path in self.rels.values():
            if kind == "sharedStrings":
                self.strings = [rich_text(n) for n in ET.fromstring(package.read(path)).findall("{*}si")]
            if kind == "theme":
                self.options["themeXml"] = package.read(path).decode("utf-8-sig")
                theme = ET.fromstring(self.options["themeXml"])
                colors = {}
                for c in theme.findall(".//{*}clrScheme/*"):
                    if len(c):
                        value = c[0].get("lastClr") or c[0].get("val")
                        if value and re.fullmatch("[a-fA-F0-9]{6}", value):
                            colors[local(c.tag)] = value
                self.options["themeColors"] = colors
                font = theme.find(".//{*}minorFont/{*}latin")
                if font is not None and font.get("typeface"):
                    self.options["fontFamily"] = font.get("typeface")

    def cells(self, sheet):
        if sheet not in self.sheet_values:
            root = ET.fromstring(self.package.read(self.sheets[sheet]))
            out = {}
            for c in root.findall("{*}sheetData/{*}row/{*}c"):
                pos = re.fullmatch(r"([A-Z]+)(\d+)", c.get("r", ""))
                if not pos:
                    continue
                value = c.findtext("{*}v")
                if c.get("t") == "s" and value is not None:
                    value = self.strings[int(value)]
                elif c.get("t") == "inlineStr" and c.find("{*}is") is not None:
                    value = rich_text(c.find("{*}is"))
                if value is None and c.find("{*}f") is not None:
                    value = NotImplemented
                out[col_index(pos[1]), int(pos[2])] = value
            self.sheet_values[sheet] = out
        return self.sheet_values[sheet]

    def range(self, formula, direction=None):
        expression = self.names.get(formula, formula) or ""
        match = REF.fullmatch(expression.lstrip("="))
        if not match:
            raise ValueError(f"unsupported chart reference: {formula}")
        sheet = (match[1] or match[2]).replace("''", "'")
        if sheet not in self.sheets:
            raise ValueError(f"missing worksheet: {sheet}")
        left, top = col_index(match[3]), int(match[4])
        right, bottom = col_index(match[5] or match[3]), int(match[6] or match[4])
        if not (left <= right <= 16384 and top <= bottom <= 1048576
                and (right - left + 1) * (bottom - top + 1) <= 1_000_000):
            raise ValueError(f"unsupported chart range extent: {formula}")
        cells = self.cells(sheet)
        matrix = [[cells.get((c, r)) for c in range(left, right + 1)] for r in range(top, bottom + 1)]
        if any(v is NotImplemented for row in matrix for v in row):
            raise ValueError(f"formula result not cached: {formula}")
        by_row = direction == "row" or (direction is None and top == bottom)
        return list(reversed(matrix if by_row else list(zip(*matrix))))

    def hydrate(self, xml):
        root = ET.fromstring(xml)
        hydrated, unresolved = 0, []
        for parent in root.iter():
            kind = local(parent.tag)
            ex = kind in ("strDim", "numDim", "txData") and parent.tag.startswith("{" + CX + "}")
            classic = kind in ("strRef", "numRef", "multiLvlStrRef")
            if not (ex or classic):
                continue
            f = parent.find("{*}f")
            if f is None or not f.text:
                continue
            cache = {"numRef": "numCache", "strRef": "strCache", "multiLvlStrRef": "multiLvlStrCache"}.get(kind)
            if (ex and (parent.find("{*}lvl") is not None or parent.find("{*}v") is not None)
                    or classic and parent.find("{*}" + cache) is not None and parent.findall(".//{*}pt")):
                continue
            q = parent.tag.split("}")[0] + "}"
            try:
                levels = self.range(f.text, f.get("dir"))
                if kind == "txData":
                    if len(levels) != 1 or len(levels[0]) != 1:
                        raise ValueError(f"non-scalar chart label: {f.text}")
                    ET.SubElement(parent, q + "v").text = levels[0][0]
                else:
                    if not ex and kind != "multiLvlStrRef" and len(levels) != 1:
                        raise ValueError(f"non-vector classic range: {f.text}")
                    if classic:
                        storage = parent.find("{*}" + cache)
                        if storage is None:
                            storage = ET.SubElement(parent, q + cache)
                        count = storage.find("{*}ptCount")
                        if count is None:
                            count = ET.SubElement(storage, q + "ptCount")
                        count.set("val", str(len(levels[0])))
                    for values in levels:
                        if ex:
                            level = ET.SubElement(parent, q + "lvl", ptCount=str(len(values)))
                        elif kind == "multiLvlStrRef":
                            level = ET.SubElement(storage, q + "lvl")
                        else:
                            level = storage
                        for i, value in enumerate(values):
                            if value is not None:
                                pt = ET.SubElement(level, q + "pt", idx=str(i))
                                if not ex:
                                    pt = ET.SubElement(pt, q + "v")
                                pt.text = value
                hydrated += 1
            except ValueError as exc:
                unresolved.append(str(exc))
        return ET.tostring(root, encoding="unicode") if hydrated else xml, hydrated, sorted(set(unresolved))

    def charts(self):
        for sheet, path in self.sheets.items():
            ordinal = 0
            links = relationships(self.package, path)
            if not any(kind == "drawing" for kind, part in links.values()):
                continue
            worksheet = ET.fromstring(self.package.read(path))
            for drawing in worksheet.findall("{*}drawing"):
                drawing_part = links[rel_id(drawing)][1]
                drawing_links = relationships(self.package, drawing_part)
                for anchor in ET.fromstring(self.package.read(drawing_part)):
                    seen = set()
                    for frame in anchor.findall(".//{*}graphicFrame"):
                        nv = frame.find("{*}nvGraphicFramePr/{*}cNvPr")
                        for node in frame.findall(".//{*}graphicData/{*}chart"):
                            rel = drawing_links.get(rel_id(node))
                            if not rel or rel[0].lower() not in ("chart", "chartex") or rel[1] in seen:
                                continue
                            part = rel[1]
                            seen.add(part)
                            xml, added, missing = self.hydrate(self.package.read(part).decode("utf-8-sig"))
                            ch = ET.fromstring(xml)
                            groups = [local(g.tag).removesuffix("Chart") for g in ch.findall(".//{*}plotArea/*")
                                      if local(g.tag).endswith("Chart")]
                            groups += [g.get("layoutId") for g in ch.findall(".//{*}plotAreaRegion/{*}series")]
                            options = self.options.copy()
                            for kind, rp in relationships(self.package, part).values():
                                key = {"chartStyle": "styleXml", "chartColorStyle": "colorStyleXml",
                                       "themeOverride": "themeOverrideXml"}.get(kind)
                                if key:
                                    options[key] = self.package.read(rp).decode("utf-8-sig")
                            size = anchor.find("{*}ext")
                            if size is not None and all(float(size.get(k, 0)) > 0 for k in ("cx", "cy")):
                                options.update(width=float(size.get("cx")) / 9525, height=float(size.get("cy")) / 9525)
                            ordinal += 1
                            yield dict(sheet=sheet, chartOrdinal=ordinal, sheetType=local(worksheet.tag), part=part,
                                       name=nv.get("name", "") if nv is not None else "",
                                       kinds=list(dict.fromkeys(groups)), hydrated=added, unresolved=missing), xml, options


def extract(path):
    with ZipFile(path) as z:
        types = ET.fromstring(z.read("[Content_Types].xml"))
        if any(n.get("ContentType", "").lower().endswith((".chart+xml", ".chartex+xml")) for n in types):
            yield from Workbook(z).charts()

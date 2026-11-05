"""Write a self-contained local index alongside chart captures."""
import os
import json
from pathlib import Path
from html import escape
from urllib.parse import quote


def html_report(run, root, result):
    counts = result["counts"]
    completed = result.get("completedAt")
    updated = f'<p><small>Rendered <time datetime="{escape(completed)}">{escape(completed)}</time></small></p>' if completed else ""
    books = {b["workbookSha256"]: b for b in json.loads((root / "workbooks.json").read_text())}

    def url(path):
        return quote(os.path.relpath(path, run))

    def pic(src, label):
        return f'<figure><figcaption>{label}</figcaption><a href="{src}"><img loading="lazy" src="{src}" alt="{label}"></a></figure>'

    def token_counts(items):
        return escape(", ".join(k if n == 1 else f"{k} (×{n})" for k, n in items.items()) or "none")

    cards = []
    def worst_first(row):
        metrics = row.get("metrics")
        if not metrics:
            return (1, 0, 0, row["id"])
        return (0, -metrics["differingPixels"] / metrics["pixels"], -metrics["rgbMae"], row["id"])

    for row in sorted(result["rows"], key=worst_first):
        ref = url(row["reference"]) if row.get("reference") else ""
        png = row["id"] + ".png"
        workbook_path = books[row["workbookSha256"]]["aliases"][0]
        pdf_link = (' · <a href="' + url(Path(row["reference"]).with_suffix(".pdf")) + '">Native vector export</a>'
                    if row.get("pdfSha256") else "")
        m = row.get("metrics")
        missing_reason = escape(row.get("unavailable", "Native reference unavailable"))
        native_label = "Excel · reconstructed workbook" if row.get("derived") else "Microsoft Excel"
        badge = "Exact" if m and m["exact"] else "Different" if m else "Missing reference" if not ref else "Unsupported"
        details = (f'{m["differingPixels"] / m["pixels"]:.1%} pixels differ ({m["differingPixels"]:,} / {m["pixels"]:,}) · MAE {m["rgbMae"]:.2f} · ink overlap {m["inkIoU"]:.1%}' if m else "")
        before = row.get("before")
        if before:
            details += (f' · previous MAE {before["rgbMae"]:.2f}, ink overlap {before["inkIoU"]:.1%}'
                        f' · differing pixels Δ {row["diffPixelChange"]:+,}')
        if row.get("aspectMismatch"):
            details += " · object and export sizes differ"
        words = row.get("labelTokens")
        labels = ('Native PDF has no extractable label text' if ref else 'No native PDF') if not words else (
            f'{"Matching" if words["match"] else "Different"} text tokens · {words["nativeTokens"]} native, {words["candidateTokens"]} candidate.'
            f'<p>Missing: {token_counts(words["missing"])}<br>Extra: {token_counts(words["extra"])}</p>')
        compare = ""
        if m:
            compare = f'''<details><summary>Wipe, difference image, and text comparison</summary>
<section><figure><figcaption>Wipe · Excel left, Office Charts right</figcaption><div class="stack">
<img loading="lazy" src="{png}" alt="Office Charts"><img loading="lazy" src="{ref}" alt="Excel" class="wipe"></div>
<input type="range" min="0" max="100" value="50" aria-label="Excel crop percentage"
oninput="this.previousElementSibling.style.setProperty('--split',this.value+'%')"></figure>
{pic(row["id"] + "-diff.png", "Pixel difference · white matches, red differs")}</section>
<p><a href="{row["id"]}-mask.png">Binary difference mask</a> (black means at least one channel differs by ≥1).</p><p>{labels}</p></details>'''
        identity = f'{row["workbook"]} / {row["sheet"]} / {row["name"]} {row["part"]} {" ".join(row["kinds"])}'
        identity += " " + row.get("title", "") + " " + " ".join(row.get("features", []))
        heading = row.get("title") or row["name"] or "Chartsheet"
        subtype = ", ".join(row["kinds"] + row.get("features", []))
        if row.get("unresolved"):
            details += " · unresolved input"
        notices = "".join(f'<p>{escape(notice)}</p>' for notice in row.get("notices", []))
        cards.append(f'<article id="{row["id"]}" data-status="{badge}" data-query="{escape(identity.casefold())}">'
                     f'<div class="eyebrow"><span title="{escape(workbook_path)}">{escape(row["workbook"])}</span> · {escape(row["sheet"])}</div>'
                     f'<h2>{escape(heading)} <span>{escape(subtype)}</span></h2>'
                     f'<small>{escape(row["name"])} · {escape(row["part"])} · {escape(row.get("referenceMatch", "no native match"))} · {escape(row.get("referenceSource", "no export"))}{pdf_link}</small>'
                     f'<p><b>{badge}</b> {details}</p>{notices}<section>{pic(ref, native_label) if ref else "<p>" + missing_reason + "</p>"}'
                     f'{pic(png, "Office Charts") if row["rendered"] else "<p>" + escape(row["error"]) + "</p>"}</section>{compare}</article>')

    def sample(label, g):
        if not g["compared"]:
            return ""
        pair = (f' On the {g["beforeCompared"]} charts rendered by both revisions, MAE went from '
                f'{g["beforeWeightedRgbMae"]:.2f} to {g["pairedAfterWeightedRgbMae"]:.2f}.'
                if g["beforeCompared"] else "")
        return (f'<p><b>{label}:</b> {g["compared"]} image pairs, {g["exact"]} exact; '
                f'{g["differingPixels"]:,}/{g["pixels"]:,} pixels differ ({g["differingPixels"] / g["pixels"]:.1%}).{pair}</p>')

    families = []
    for kind, g in result["families"].items():
        mae = g["weightedRgbMae"]
        value = f"{mae:.2f}" if mae is not None else "—"
        families.append(f'<tr><th scope="row">{escape(kind)}</th><td>{g["charts"]}</td><td>{g["compared"]}</td><td>{g["exact"]}</td><td>{value}</td><td>{g["textMatch"]}/{g["textMeasured"]}</td></tr>')
    return f'''<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Excel chart fidelity</title>
<style>
*{{box-sizing:border-box}}body{{font:15px/1.5 system-ui;margin:3vw auto;padding:0 3vw;color:#192528;max-width:1550px}}
h1{{font-size:2.5em;line-height:1.2}}article{{padding:22px 0;border-top:1px solid #bbc8cb}}h2,p,figure{{margin:5px 0}}
section{{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1fr);gap:18px}}img{{display:block;max-width:100%;height:auto}}
.eyebrow,small,figcaption,h2 span{{color:#51686d}}.eyebrow{{font-size:.85rem;font-weight:650}}h2 span{{font-size:.9rem;font-weight:450}}
input,select{{font:inherit;padding:5px 9px}}input[type=range]{{width:100%}}.stack{{display:inline-grid;max-width:100%;--split:50%}}
.stack img{{grid-area:1/1}}.wipe{{clip-path:inset(0 calc(100% - var(--split)) 0 0)}}summary{{cursor:pointer}}
table{{border-collapse:collapse}}td,th{{padding:3px 12px;text-align:right;border-bottom:1px solid #dce5e4}}th:first-child{{text-align:left}}details,nav{{padding:10px 0}}nav{{display:flex;gap:10px;align-items:center;flex-wrap:wrap}}[hidden]{{display:none}}
@media(max-width:700px){{section{{grid-template-columns:1fr}}}}</style>
<h1>Excel chart fidelity</h1>{updated}<p><strong>{counts["exact"]} exact pixel matches out of {counts["compared"]} comparisons.</strong>
Office Charts rendered {counts["rendered"]}/{counts["charts"]} charts; {counts["references"]} have Excel references, including {result["directReferences"]["charts"]} original-workbook captures
and {result["derivedReferences"]["charts"]} from reconstructed workbooks.</p>
{sample("Original workbooks", result["directReferences"])}{sample("Reconstructed workbooks", result["derivedReferences"])}
<p>Scanned {counts["inputPaths"]} workbook paths ({counts["distinctInputs"]} unique files). This report includes charts from {counts["workbooks"]} workbooks.
The input inventory includes {counts["zeroCharts"]} without charts and {counts["unreadable"]} that could not be opened.
<a href="{url(root / 'workbooks.json')}">Input inventory and aliases</a> · <a href="report.json">Raw results</a></p>
<details><summary>Coverage and error by chart family</summary><table><thead><tr><th>Family</th><th>Inputs</th><th>Compared</th><th>Exact</th><th>Pixel-weighted MAE</th><th>PDF text matches / measured</th></tr></thead><tbody>{"".join(families)}</tbody></table></details>
<details><summary>How to read the results</summary><p>Both images use the native export's resolution. Excel supplied the reference;
Chromium {escape(result["browser"])} drew the full WASM renderer's SVG using {len(result["fontManifest"])} locally supplied font faces.
The reference and candidate are composited onto white. No crop matching, translation, stretching, or fallback chart renderer.
RGB MAE is mean absolute component error (0–255); ink overlap is intersection over union of pixels with any channel below 242.
PDF text tokens are case-sensitive and count repeated occurrences; extraction may split or reorder labels, or omit text drawn as paths.
Duplicate workbooks are counted once by bytes. Separate sheets saved as independent workbooks remain separate observations.
No native image means fidelity is unknown. A matching image does not certify interactions.</p>
<p>Renderer SHA‑256: <code>{result["engineSha256"]}</code></p></details>
<nav><input id="q" type="search" placeholder="Filter by chart, sheet, or workbook" aria-label="Filter charts"><select id="s" aria-label="Comparison status">
<option>All charts</option><option>Different</option><option>Exact</option><option>Missing reference</option><option>Unsupported</option></select>
<output>{len(cards)} charts</output><span>Highest differing-pixel percentage first; unscored charts last.</span></nav>{"".join(cards)}
<script>const q=document.querySelector('#q'),s=document.querySelector('#s');function filter(){{
let n=0;for(const a of document.querySelectorAll('article')){{a.hidden=!(a.dataset.query.includes(q.value.toLowerCase())&&(s.selectedIndex===0||a.dataset.status===s.value));n+=!a.hidden;}}
document.querySelector('output').textContent=n+' charts';}}q.oninput=s.onchange=filter;</script>'''

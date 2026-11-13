use super::escape_html;
use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

const SIDES: [&str; 4] = ["top", "right", "bottom", "left"];
#[derive(Default)]
pub(super) struct Css(Vec<(String, String)>);
impl Css {
    fn set(&mut self, key: &str, value: Option<String>) {
        if let Some(value) = value {
            if let Some(entry) = self.0.iter_mut().find(|(k, _)| k == key) {
                entry.1 = value;
            } else {
                self.0.push((key.into(), value));
            }
        }
    }
    fn literal(&mut self, key: &str, value: &str) {
        self.set(key, Some(value.into()));
    }
    fn flag(&mut self, key: &str, value: &Value, yes: &str, no: &str) {
        self.set(
            key,
            value.as_bool().map(|v| if v { yes } else { no }.into()),
        );
    }
    fn when(&mut self, key: &str, condition: bool, value: &str) {
        if condition {
            self.literal(key, value);
        }
    }
    pub(super) fn attribute(&self) -> String {
        if self.0.is_empty() {
            String::new()
        } else {
            format!(
                " style=\"{}\"",
                escape_html(
                    &self
                        .0
                        .iter()
                        .map(|(k, v)| format!("{k}:{v}"))
                        .collect::<Vec<_>>()
                        .join(";")
                )
            )
        }
    }
    fn borders(&mut self, borders: &Value) {
        for side in SIDES {
            self.set(&format!("border-{side}"), border(&borders[side]));
            self.set(
                &format!("padding-{side}"),
                pt(&borders[side]["spacePt"], 1.),
            );
        }
    }
}
fn numeric(v: &Value, min: f64, max: f64) -> Option<f64> {
    v.as_f64()
        .filter(|n| n.is_finite() && *n >= min && *n <= max)
}
fn rounded(n: f64) -> String {
    format!("{:.3}", n)
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}
fn pt(v: &Value, divisor: f64) -> Option<String> {
    numeric(v, -40_000., 40_000.).map(|n| format!("{}pt", rounded(n / divisor)))
}
fn color(v: &Value) -> Option<String> {
    let v = v.as_str()?;
    let hex = v.strip_prefix('#').unwrap_or(v);
    if hex.bytes().all(|c| c.is_ascii_hexdigit())
        && (hex.len() == 6 || (v.starts_with('#') && matches!(hex.len(), 3 | 8)))
    {
        return Some(format!("#{hex}"));
    }
    Some(
        match v {
            "black" => "#000000",
            "white" => "#ffffff",
            "red" => "#ff0000",
            "green" => "#008000",
            "blue" => "#0000ff",
            "yellow" => "#ffff00",
            "cyan" => "#00ffff",
            "magenta" => "#ff00ff",
            "darkBlue" => "#000080",
            "darkCyan" => "#008080",
            "darkGreen" => "#008000",
            "darkMagenta" => "#800080",
            "darkRed" => "#800000",
            "darkYellow" => "#808000",
            "darkGray" => "#808080",
            "lightGray" => "#c0c0c0",
            "auto" => "currentColor",
            _ => return None,
        }
        .into(),
    )
}
fn border(v: &Value) -> Option<String> {
    if v.is_null() {
        return None;
    }
    let kind = v["type"].as_str().unwrap_or("").to_lowercase();
    if matches!(kind.as_str(), "nil" | "none") {
        return Some("none".into());
    }
    let line = if kind.contains("double") || kind == "triple" {
        "double"
    } else if kind.contains("dash") {
        "dashed"
    } else if kind.contains("dot") {
        "dotted"
    } else if matches!(kind.as_str(), "inset" | "outset") {
        &kind
    } else {
        "solid"
    };
    Some(format!(
        "{}pt {line} {}",
        numeric(&v["sizeEighthPt"], 0., 768.)
            .map(|n| n / 8.)
            .unwrap_or(0.75),
        color(&v["color"]).unwrap_or("currentColor".into())
    ))
}
fn family(v: &Value) -> Option<String> {
    static VALID: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[\p{L}\p{N} ._,-]{1,200}$").unwrap());
    let v = v.as_str()?;
    VALID.is_match(v).then(|| {
        v.split(',')
            .map(|n| format!("\"{}\"", n.trim()))
            .collect::<Vec<_>>()
            .join(",")
    })
}
fn text_css(s: &Value) -> Css {
    let mut css = Css::default();
    css.flag("font-weight", &s["bold"], "700", "400");
    css.flag("font-style", &s["italic"], "italic", "normal");
    if !s["underline"].is_null() || !s["strike"].is_null() {
        let mut decoration = Vec::new();
        if s["underline"] == true {
            decoration.push("underline");
        }
        if s["strike"] == true {
            decoration.push("line-through");
        }
        css.literal(
            "text-decoration-line",
            &if decoration.is_empty() {
                "none".into()
            } else {
                decoration.join(" ")
            },
        );
    }
    css.flag("text-transform", &s["allCaps"], "uppercase", "none");
    css.flag("font-variant-caps", &s["smallCaps"], "small-caps", "normal");
    css.set("color", color(&s["color"]));
    css.set(
        "background-color",
        color(&s["backgroundColor"]).or_else(|| color(&s["highlight"])),
    );
    css.set("font-family", family(&s["fontFamily"]));
    let scripted = matches!(
        s["verticalAlign"].as_str(),
        Some("superscript" | "subscript")
    );
    let size = numeric(&s["fontSizePt"], 0.5, 400.);
    css.set(
        "font-size",
        size.map(|n| format!("{}pt", rounded(n * if scripted { 0.65 } else { 1. })))
            .or_else(|| scripted.then(|| "0.65em".into())),
    );
    css.when(
        "vertical-align",
        scripted,
        if s["verticalAlign"] == "superscript" {
            "super"
        } else {
            "sub"
        },
    );
    css.set("letter-spacing", pt(&s["characterSpacingTwips"], 20.));
    css.set(
        "font-stretch",
        numeric(&s["characterScalePercent"], 50., 200.).map(|n| format!("{n}%")),
    );
    css.set(
        "font-kerning",
        numeric(&s["kerningMinFontSizePt"], 0., 400.)
            .zip(size)
            .map(|(min, size)| if size >= min { "normal" } else { "none" }.into()),
    );
    let mut features = Vec::new();
    if let Some(ligatures) = s["ligatures"].as_str() {
        let ligatures = ligatures.to_lowercase();
        for (name, kind) in [
            ("liga", "standard"),
            ("clig", "contextual"),
            ("hlig", "historical"),
            ("dlig", "discretional"),
        ] {
            features.push(format!(
                "\"{name}\" {}",
                u8::from(ligatures == "all" || ligatures.contains(kind))
            ));
        }
    }
    if let Some(v) = s["contextualAlternates"].as_bool() {
        features.push(format!("\"calt\" {}", u8::from(v)));
    }
    css.set(
        "font-feature-settings",
        (!features.is_empty()).then(|| features.join(",")),
    );
    css.flag("direction", &s["rightToLeft"], "rtl", "ltr");
    css.when("unicode-bidi", !s["rightToLeft"].is_null(), "isolate");
    css.set("border", border(&s["runBorder"]));
    css.set("padding", pt(&s["runBorder"]["spacePt"], 1.));
    css.when("box-decoration-break", !s["runBorder"].is_null(), "clone");
    css
}

pub(super) fn text_html(text: &str, style: &Value, link: Option<&str>) -> String {
    static EAST: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"[\p{Han}\p{Hiragana}\p{Katakana}\p{Hangul}\p{Bopomofo}]").unwrap()
    });
    static CS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"[\p{Arabic}\p{Hebrew}\p{Devanagari}\p{Thai}]").unwrap());
    static INHERIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\s\p{M}\p{P}]").unwrap());
    let mut segments: Vec<(String, &str)> = Vec::new();
    for c in text.chars() {
        let value = c.to_string();
        let script = if EAST.is_match(&value) {
            "EastAsia"
        } else if style["complexScript"] == true
            || style["rightToLeft"] == true
            || CS.is_match(&value)
        {
            "Cs"
        } else if INHERIT.is_match(&value) && !segments.is_empty() {
            segments.last().unwrap().1
        } else if c.is_ascii() {
            "Ascii"
        } else {
            "HAnsi"
        };
        if let Some((text, previous)) = segments
            .last_mut()
            .filter(|(_, previous)| *previous == script)
        {
            text.push(c);
            let _ = previous;
        } else {
            segments.push((value, script));
        }
    }
    let mut html = String::new();
    for (text, script) in segments {
        let mut s = style.as_object().cloned().unwrap_or_default();
        let font = style
            .get(format!("resolvedFontFamily{script}"))
            .filter(|v| !v.is_null())
            .or_else(|| {
                style
                    .get(format!("fontFamily{script}"))
                    .filter(|v| !v.is_null())
            })
            .unwrap_or(&style["fontFamily"]);
        s.insert("fontFamily".into(), font.clone());
        if script == "Cs" && !style["fontSizeCsPt"].is_null() {
            s.insert("fontSizePt".into(), style["fontSizeCsPt"].clone());
        }
        let attributes = text_css(&Value::Object(s)).attribute();
        let language = if script == "Cs" {
            style.get("languageBidi")
        } else if script == "EastAsia" {
            style.get("languageEastAsia")
        } else {
            None
        }
        .filter(|v| !v.is_null())
        .unwrap_or(&style["language"]);
        let lang = language
            .as_str()
            .filter(|v| {
                !v.is_empty()
                    && v.len() <= 35
                    && v.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
            })
            .map(|v| format!(" lang=\"{v}\""))
            .unwrap_or_default();
        let escaped = escape_html(&text)
            .replace("\r\n", "\n")
            .replace(['\r', '\n', '\u{b}', '\u{2028}', '\u{2029}'], "<br>");
        if attributes.is_empty() && lang.is_empty() {
            html.push_str(&escaped);
        } else {
            html.push_str(&format!("<span{attributes}{lang}>{escaped}</span>"));
        }
    }
    for (property, tag) in [
        ("bold", "strong"),
        ("italic", "em"),
        ("underline", "u"),
        ("strike", "s"),
    ] {
        if style[property] == true {
            html = format!("<{tag}>{html}</{tag}>");
        }
    }
    if let Some(link) = link.filter(|v| super::safe_link(v)) {
        html = format!("<a href=\"{}\">{html}</a>", escape_html(link));
    }
    html
}
pub(super) fn paragraph(s: &Value) -> String {
    let mut c = Css::default();
    c.literal("white-space", "pre-wrap");
    c.set(
        "text-align",
        s["align"]
            .as_str()
            .filter(|v| matches!(*v, "left" | "right" | "center" | "justify"))
            .map(str::to_owned),
    );
    c.set("background-color", color(&s["backgroundColor"]));
    for (key, part, field) in [
        ("margin-top", "spacing", "beforeTwips"),
        ("margin-bottom", "spacing", "afterTwips"),
        ("margin-left", "indent", "leftTwips"),
        ("margin-right", "indent", "rightTwips"),
    ] {
        c.set(key, pt(&s[part][field], 20.));
    }
    if !s["indent"]["firstLineTwips"].is_null() || !s["indent"]["hangingTwips"].is_null() {
        c.set(
            "text-indent",
            pt(
                &Value::from(
                    s["indent"]["firstLineTwips"].as_f64().unwrap_or(0.)
                        - s["indent"]["hangingTwips"].as_f64().unwrap_or(0.),
                ),
                20.,
            ),
        );
    }
    c.set(
        "line-height",
        numeric(&s["spacing"]["lineTwips"], 1., 40000.).map(|n| {
            if matches!(s["spacing"]["lineRule"].as_str(), Some("exact" | "atLeast")) {
                format!("{}pt", rounded(n / 20.))
            } else {
                rounded(n / 240.)
            }
        }),
    );
    for (key, flag, value) in [
        ("break-after", "keepNext", "avoid"),
        ("break-inside", "keepLines", "avoid"),
        ("break-before", "pageBreakBefore", "page"),
        ("orphans", "widowControl", "2"),
        ("widows", "widowControl", "2"),
    ] {
        c.when(key, s[flag] == true, value);
    }
    c.borders(&s["borders"]);
    c.attribute()
}
fn width(s: &Value) -> Option<String> {
    if s["preferredWidth"]["type"] == "pct" {
        if let Some(n) = numeric(&s["preferredWidth"]["value"], 0., 100.) {
            return Some(format!("{n}%"));
        }
    }
    pt(
        if s["preferredWidth"]["type"] == "dxa" {
            &s["preferredWidth"]["value"]
        } else {
            &s["widthTwips"]
        },
        20.,
    )
}
pub(super) fn table(s: &Value) -> String {
    let mut c = Css::default();
    c.literal(
        "border-collapse",
        if s["cellSpacingTwips"].as_f64().unwrap_or(0.) != 0. {
            "separate"
        } else {
            "collapse"
        },
    );
    c.set("border-spacing", pt(&s["cellSpacingTwips"], 20.));
    c.when("table-layout", s["layout"] == "fixed", "fixed");
    c.set("width", width(s));
    c.set(
        "margin-left",
        if matches!(s["alignment"].as_str(), Some("center" | "right")) {
            Some("auto".into())
        } else {
            pt(&s["indentTwips"], 20.)
        },
    );
    c.when("margin-right", s["alignment"] == "center", "auto");
    c.when("direction", s["bidiVisual"] == true, "rtl");
    c.borders(&s["borders"]);
    c.attribute()
}
pub(super) fn columns(s: &Value) -> String {
    let Some(columns) = s["columnWidthsTwips"].as_array().filter(|c| !c.is_empty()) else {
        return String::new();
    };
    format!(
        "<colgroup>{}</colgroup>",
        columns
            .iter()
            .map(|v| {
                let mut c = Css::default();
                c.set("width", pt(v, 20.));
                format!("<col{}>", c.attribute())
            })
            .collect::<String>()
    )
}
pub(super) fn row(s: &Value) -> String {
    let mut c = Css::default();
    c.set("background-color", color(&s["backgroundColor"]));
    c.set("height", pt(&s["heightTwips"], 20.));
    c.when("break-inside", s["cantSplit"] == true, "avoid");
    c.attribute()
}
pub(super) fn cell(s: &Value, table: &Value) -> String {
    let mut c = Css::default();
    c.set("background-color", color(&s["backgroundColor"]));
    c.set(
        "vertical-align",
        s["verticalAlign"]
            .as_str()
            .and_then(|v| match v {
                "center" => Some("middle"),
                "top" | "bottom" => Some(v),
                _ => None,
            })
            .map(str::to_owned),
    );
    c.when("white-space", s["noWrap"] == true, "nowrap");
    c.set(
        "writing-mode",
        s["textDirection"]
            .as_str()
            .and_then(|v| {
                if v.starts_with("tb") {
                    Some("vertical-rl")
                } else if v == "btLr" {
                    Some("vertical-lr")
                } else {
                    None
                }
            })
            .map(str::to_owned),
    );
    c.set("width", width(s));
    for side in SIDES {
        let key = format!("{side}Twips");
        c.set(
            &format!("padding-{side}"),
            pt(
                s["marginTwips"]
                    .get(&key)
                    .unwrap_or(&table["cellMarginTwips"][&key]),
                20.,
            ),
        );
    }
    c.borders(&s["borders"]);
    c.attribute()
}

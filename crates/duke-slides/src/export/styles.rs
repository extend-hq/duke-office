use duke_office_art::export::{escape_html, safe_link};
use serde_json::Value;

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .filter(|value| value.is_finite() && value.abs() <= 1_000_000_000.)
}

fn color(value: &Value) -> Option<String> {
    let text = value.as_str().or_else(|| value["value"].as_str())?;
    let hex = text.strip_prefix('#').unwrap_or(text);
    (matches!(hex.len(), 3 | 6 | 8) && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| format!("#{hex}"))
}

fn family(value: &Value) -> Option<String> {
    let text = value.as_str()?;
    (text.len() <= 200
        && !text.is_empty()
        && text
            .chars()
            .all(|ch| ch.is_alphanumeric() || " ._,-".contains(ch)))
    .then(|| {
        text.split(',')
            .map(|part| format!("'{}'", part.trim()))
            .collect::<Vec<_>>()
            .join(",")
    })
}

pub(super) fn attribute(values: &[String]) -> String {
    if values.is_empty() {
        String::new()
    } else {
        format!(" style=\"{}\"", escape_html(&values.join(";")))
    }
}

pub(super) fn text_html(text: &str, style: &Value, link: Option<&str>) -> String {
    let mut css = vec!["white-space:pre-wrap".into()];
    for (field, property, yes, no) in [
        ("bold", "font-weight", "700", "400"),
        ("italic", "font-style", "italic", "normal"),
    ] {
        if let Some(value) = style[field].as_bool() {
            css.push(format!("{property}:{}", if value { yes } else { no }));
        }
    }
    let decorations: Vec<_> = [("underline", "underline"), ("strike", "line-through")]
        .into_iter()
        .filter(|(key, _)| style[*key] == true)
        .map(|(_, value)| value)
        .collect();
    if !decorations.is_empty() {
        css.push(format!("text-decoration-line:{}", decorations.join(" ")));
    }
    if let Some(value) = color(&style["color"]) {
        css.push(format!("color:{value}"));
    }
    if let Some(value) = family(&style["fontFamily"]) {
        css.push(format!("font-family:{value}"));
    }
    if let Some(value) = number(&style["fontSizePt"]).filter(|n| *n > 0. && *n <= 4000.) {
        css.push(format!("font-size:{value}pt"));
    }
    if let Some(value) = number(&style["characterSpacingPt"]) {
        css.push(format!("letter-spacing:{value}pt"));
    }
    if let Some(value) = number(&style["baseline"]) {
        css.push(format!("vertical-align:{value}%"));
    }
    if let Some(value) = style["rightToLeft"].as_bool() {
        css.push(format!("direction:{}", if value { "rtl" } else { "ltr" }));
    }
    let lang = style["language"]
        .as_str()
        .map(|lang| format!(" lang=\"{}\"", escape_html(lang)))
        .unwrap_or_default();
    let content = format!(
        "<span{}{lang}>{}</span>",
        attribute(&css),
        escape_html(text).replace('\n', "<br>")
    );
    match link.filter(|link| safe_link(link)) {
        Some(link) => format!("<a href=\"{}\">{content}</a>", escape_html(link)),
        None => content,
    }
}

pub(super) fn paragraph(style: &Value) -> String {
    let mut css = Vec::new();
    if let Some(value @ ("left" | "right" | "center" | "justify")) = style["alignment"].as_str() {
        css.push(format!("text-align:{value}"));
    }
    if let Some(rtl) = style["rtl"].as_bool() {
        css.push(format!("direction:{}", if rtl { "rtl" } else { "ltr" }));
    }
    for (field, property) in [
        ("marginLeftEmu", "margin-left"),
        ("indentEmu", "text-indent"),
    ] {
        if let Some(value) = number(&style[field]) {
            css.push(format!("{property}:{}pt", value / 12700.));
        }
    }
    for (field, property) in [
        ("spaceBefore", "margin-top"),
        ("spaceAfter", "margin-bottom"),
        ("lineSpacing", "line-height"),
    ] {
        if let Some(value) = number(&style[field]["value"]) {
            let unit = match style[field]["unit"].as_str() {
                Some("points") => "pt",
                Some("percent") => "em",
                _ => continue,
            };
            css.push(format!("{property}:{value}{unit}"));
        }
    }
    attribute(&css)
}

pub(super) fn cell(style: &Value) -> String {
    let mut css = Vec::new();
    if style["fill"]["type"] == "solid" {
        if let Some(value) = color(&style["fill"]["color"]) {
            css.push(format!("background-color:{value}"));
        }
    }
    for side in ["top", "right", "bottom", "left"] {
        if let Some(value) = number(&style["textInsets"][side]) {
            css.push(format!("padding-{side}:{}pt", value / 12700.));
        }
        let border = &style["borders"][side];
        if !border.is_null() {
            let width = number(&border["width"]).filter(|n| *n >= 0.).unwrap_or(1.);
            let color = color(&border["color"]).unwrap_or_else(|| "currentColor".into());
            let kind = match border["dash"].as_str() {
                Some("dash" | "lgDash" | "sysDash") => "dashed",
                Some("dot" | "sysDot") => "dotted",
                _ => "solid",
            };
            css.push(format!("border-{side}:{width}px {kind} {color}"));
        }
    }
    if let Some(value @ ("top" | "middle" | "bottom")) = style["verticalAlignment"].as_str() {
        css.push(format!("vertical-align:{value}"));
    }
    attribute(&css)
}

pub(super) fn row(height: &Value) -> String {
    number(height)
        .filter(|n| *n > 0.)
        .map(|height| attribute(&[format!("height:{}pt", height / 12700.)]))
        .unwrap_or_default()
}

use super::util::escape_xml_text;
use crate::chart::child;
use roxmltree::Node;
use std::fmt::Write;

pub(super) fn val<'a>(node: Option<Node<'a, 'a>>, name: &str) -> Option<&'a str> {
    child(node?, name)?.attribute("val")
}

pub(super) fn number(s: Option<&str>) -> Option<f64> {
    s?.trim().parse::<f64>().ok().filter(|n| n.is_finite())
}

#[cfg(feature = "extended-charts")]
pub(super) fn on(s: Option<&str>) -> bool {
    matches!(s, Some("1" | "true" | "on"))
}

pub(super) fn text_props<'a>(node: Option<Node<'a, 'a>>, rich: bool) -> Option<Node<'a, 'a>> {
    let node = node?;
    let root = if rich {
        child(node, "tx")
    } else {
        child(node, "txPr")
    }?;
    root.descendants()
        .find(|n| n.is_element() && matches!(n.tag_name().name(), "rPr" | "defRPr"))
}

pub(super) fn font_size(props: Option<Node>, default: f64) -> f64 {
    number(props.and_then(|p| p.attribute("sz")))
        .map(|n| (n / 75.).clamp(1., 512.))
        .unwrap_or(default)
}

pub(super) fn font_family(props: Option<Node>, default: &str) -> String {
    let name = props
        .and_then(|p| p.children().find(|n| n.tag_name().name() == "latin"))
        .and_then(|n| n.attribute("typeface"))
        .filter(|s| !s.starts_with('+') && !s.is_empty())
        .unwrap_or(default);
    let fallback = if matches!(name, "Cambria" | "Times New Roman" | "Georgia") {
        "serif"
    } else {
        "sans-serif"
    };
    format!("{}, {fallback}", escape_xml_text(name))
}

pub(super) fn title_position(
    title: Option<Node>,
    width: f64,
    height: f64,
    size: f64,
    default_y: f64,
) -> (f64, f64, &'static str) {
    let layout = title
        .and_then(|n| child(n, "layout"))
        .and_then(|n| child(n, "manualLayout"));
    let x = number(val(layout, "x")).map(|v| width * v + size * 0.2);
    let y = number(val(layout, "y")).map(|v| height * v + size * 0.915);
    (
        x.unwrap_or(width / 2.),
        y.unwrap_or(default_y),
        if x.is_some() { "start" } else { "middle" },
    )
}

pub(super) fn title_lines(label: &str, size: f64, width: f64) -> Vec<String> {
    if text_width(label, size) <= width {
        return vec![label.to_owned()];
    }
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in label.split_whitespace() {
        let next = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if !line.is_empty() && text_width(&next, size) > width {
            lines.push(line);
            line = word.to_owned();
        } else {
            line = next;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

pub(super) fn text_width(s: &str, size: f64) -> f64 {
    // The model has no platform font shaper. Keep these advances in ems so
    // margins track the text size even when the browser substitutes a font.
    size * s
        .chars()
        .map(|c| match c {
            '0'..='9' => 0.554,
            'i' | 'l' | '.' | ',' | ' ' => 0.25,
            '-' | 't' | 'f' => 0.335,
            'r' => 0.415,
            'M' | 'W' | 'm' | 'w' => 0.8,
            'S' => 0.50,
            'o' => 0.531,
            'h' | 'u' | 'n' => 0.552,
            'A'..='Z' => 0.68,
            _ if c.is_ascii() => 0.54,
            _ => 1.,
        })
        .sum::<f64>()
}

pub(super) fn line(svg: &mut String, x: f64, y: f64, x2: f64, y2: f64, stroke: &str) {
    write!(
        svg,
        r#"<path d="M{x:.2} {y:.2}L{x2:.2} {y2:.2}" fill="none" stroke="{stroke}"/>"#
    )
    .unwrap();
}

pub(super) fn text(
    svg: &mut String,
    x: f64,
    y: f64,
    anchor: &str,
    size: f64,
    family: &str,
    label: &str,
) {
    write!(svg, r#"<text x="{x:.2}" y="{y:.2}" text-anchor="{anchor}" font-size="{size:.2}" font-family="{family}">{}</text>"#, escape_xml_text(label)).unwrap();
}

pub(super) fn marker(
    svg: &mut String,
    (x, y): (f64, f64),
    symbol: &str,
    radius: f64,
    fill: &str,
    outline: &str,
    sw: f64,
) {
    let r = radius;
    write!(svg, r#"<g transform="translate({x:.2} {y:.2})" fill="{fill}" stroke="{outline}" stroke-width="{sw:.2}" stroke-linejoin="round">"#).unwrap();
    let neg = -r;
    let shape = match symbol {
        "circle" | "dot" => format!(r#"<circle r="{r:.2}"/>"#),
        "square" => format!(
            r#"<rect x="{neg:.2}" y="{neg:.2}" width="{w:.2}" height="{w:.2}"/>"#,
            w = 2. * r
        ),
        "diamond" => format!(r#"<path d="M0 {neg:.2}L{r:.2} 0 0 {r:.2} {neg:.2} 0Z"/>"#),
        "triangle" => format!(r#"<path d="M0 {neg:.2}L{r:.2} {r:.2} {neg:.2} {r:.2}Z"/>"#),
        "plus" => format!(r#"<path d="M{neg:.2} 0H{r:.2}M0 {neg:.2}V{r:.2}" fill="none"/>"#),
        "star" => format!(
            r#"<path d="M{neg:.2} {neg:.2}L{r:.2} {r:.2}M{neg:.2} {r:.2}L{r:.2} {neg:.2}M0 {neg:.2}V{r:.2}" fill="none"/>"#
        ),
        "x" => format!(
            r#"<path d="M{neg:.2} {neg:.2}L{r:.2} {r:.2}M{neg:.2} {r:.2}L{r:.2} {neg:.2}" fill="none"/>"#
        ),
        "dash" => format!(r#"<path d="M{neg:.2} 0H{r:.2}" fill="none"/>"#),
        _ => String::new(),
    };
    svg.push_str(&shape);
    svg.push_str("</g>");
}

pub(super) fn format_value(value: f64, code: Option<&str>) -> String {
    let code = code.unwrap_or("General");
    if code == "General" || code.is_empty() {
        return value.to_string();
    }
    let (integer, fraction) = code.split_once('.').unwrap_or((code, ""));
    if integer == "#,##0" && fraction.len() <= 12 && fraction.bytes().all(|c| c == b'0') {
        let precision = fraction.len();
        // Chart labels round decimal midpoints away from zero.
        let midpoint = value
            .to_string()
            .split_once('.')
            .is_some_and(|(_, digits)| digits.len() == precision + 1 && digits.ends_with('5'));
        let rounded = if midpoint {
            let scale = 10_f64.powi(precision as i32);
            ((value.abs() * scale).floor() + 1.).copysign(value) / scale
        } else {
            value
        };
        let mut label = format!("{rounded:.precision$}");
        let digits_start = usize::from(label.starts_with('-'));
        let mut end = label.find('.').unwrap_or(label.len());
        while end > digits_start + 3 {
            end -= 3;
            label.insert(end, ',');
        }
        return label;
    }
    let decimals = code.split_once('.').map_or(0, |(_, frac)| {
        frac.chars().take_while(|c| *c == '0').count().min(12)
    });
    if code.to_ascii_uppercase().contains("E+") {
        let scientific = format!("{value:.decimals$E}");
        if let Some((mantissa, exp)) = scientific.split_once('E') {
            if let Ok(exp) = exp.parse::<i32>() {
                return format!("{mantissa}E{exp:+03}");
            }
        }
    }
    if code.contains('%') {
        return format!("{:.decimals$}%", value * 100.);
    }
    if code.contains('0') && code.chars().all(|c| matches!(c, '0' | '.')) {
        return format!("{value:.decimals$}");
    }
    value.to_string()
}

pub(super) fn date_label(serial: f64, epoch_1904: bool, code: &str) -> Option<String> {
    if !(-657434. ..=2958465.).contains(&serial) || code.len() > 64 {
        return None;
    }
    let mut serial = serial.floor() as i32;
    if epoch_1904 {
        serial += 1462;
    }
    let (year, month, day) = if serial == 60 {
        // Excel's 1900 date system includes an extra leap day.
        (1900, 2, 29)
    } else {
        let days = serial + i32::from(serial < 60) - 36526;
        let mut year = 2000 + 400 * days.div_euclid(146097);
        let mut day = days.rem_euclid(146097);
        let leap = |y: i32| y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
        while day >= 365 + i32::from(leap(year)) {
            day -= 365 + i32::from(leap(year));
            year += 1;
        }
        let mut month = 1;
        for length in [
            31,
            28 + i32::from(leap(year)),
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ] {
            if day < length {
                break;
            }
            day -= length;
            month += 1;
        }
        (year, month, day + 1)
    };
    let code = code.to_ascii_lowercase();
    let mut chars = code.chars().peekable();
    let mut label = String::new();
    let mut has_date = false;
    while let Some(c) = chars.next() {
        if c == '\\' {
            label.push(chars.next()?);
            continue;
        }
        if !matches!(c, 'm' | 'd' | 'y') {
            if matches!(c, '/' | '-' | '.' | ' ') {
                label.push(c);
                continue;
            }
            return None;
        }
        let mut count = 1;
        while chars.next_if_eq(&c).is_some() {
            count += 1;
        }
        match (c, count) {
            ('m', 1) => write!(label, "{month}").unwrap(),
            ('m', 2) => write!(label, "{month:02}").unwrap(),
            ('d', 1) => write!(label, "{day}").unwrap(),
            ('d', 2) => write!(label, "{day:02}").unwrap(),
            ('y', 2) => write!(label, "{:02}", year.rem_euclid(100)).unwrap(),
            ('y', 4) => write!(label, "{year:04}").unwrap(),
            _ => return None,
        }
        has_date = true;
    }
    has_date.then_some(label)
}

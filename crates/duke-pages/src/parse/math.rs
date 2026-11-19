use crate::model::ImageRunNode;

fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn math_tokens(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        let tag = if ch.is_numeric()
            || (ch == '.' && chars.peek().is_some_and(|(_, c)| c.is_ascii_digit()))
        {
            let mut decimal = ch == '.';
            while let Some((_, c)) = chars.peek() {
                if c.is_numeric() {
                    chars.next();
                } else if *c == '.' && !decimal {
                    decimal = true;
                    chars.next();
                } else {
                    break;
                }
            }
            "mn"
        } else if ch.is_alphabetic() {
            "mi"
        } else if ch.is_whitespace() {
            while chars.next_if(|(_, c)| c.is_whitespace()).is_some() {}
            "mtext"
        } else {
            "mo"
        };
        let end = chars.peek().map_or(text.len(), |(pos, _)| *pos);
        result.push_str(&format!("<{tag}>{}</{tag}>", escaped(&text[start..end])));
    }
    result
}

pub fn parse_math(xml: &str) -> Option<ImageRunNode> {
    let wrapped = format!("<root xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\" xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">{xml}</root>");
    let document = roxmltree::Document::parse(&wrapped).ok()?;
    fn child<'a, 'input>(
        node: roxmltree::Node<'a, 'input>,
        name: &str,
    ) -> Option<roxmltree::Node<'a, 'input>> {
        node.children()
            .find(|n| n.is_element() && n.tag_name().name() == name)
    }
    fn value(node: roxmltree::Node<'_, '_>, property: &str, default: &str) -> String {
        node.children()
            .filter(|n| n.tag_name().name().ends_with("Pr"))
            .flat_map(|n| n.children())
            .find(|n| n.tag_name().name() == property)
            .and_then(|n| n.attributes().find(|a| a.name() == "val"))
            .map(|a| a.value().to_string())
            .unwrap_or_else(|| default.into())
    }
    fn render(node: roxmltree::Node<'_, '_>) -> String {
        if !node.is_element() {
            return String::new();
        }
        let name = node.tag_name().name();
        if name.ends_with("Pr") {
            return String::new();
        }
        let children = || node.children().map(render).collect::<String>();
        let arg = |name| {
            child(node, name)
                .map(render)
                .unwrap_or_else(|| "<mrow/>".into())
        };
        match name {
            "t" => {
                let text = node.text().unwrap_or_default();
                if let Some(run) = node.parent_element() {
                    if let Some(normal) = child(run, "rPr").and_then(|props| child(props, "nor")) {
                        if !normal.attributes().any(|a| {
                            a.name() == "val" && matches!(a.value(), "0" | "false" | "off")
                        }) {
                            return format!("<mtext>{}</mtext>", escaped(text));
                        }
                    }
                }
                if node
                    .ancestors()
                    .any(|parent| parent.is_element() && parent.tag_name().name() == "fName")
                {
                    return format!("<mi>{}</mi>", escaped(text));
                }
                math_tokens(text)
            }
            "f" => {
                let (num, den) = (arg("num"), arg("den"));
                match value(node, "type", "bar").as_str() {
                    "noBar" => format!(r#"<mfrac linethickness="0">{num}{den}</mfrac>"#),
                    "skw" => format!(r#"<mfrac bevelled="true">{num}{den}</mfrac>"#),
                    "lin" => format!("<mrow>{num}<mo>/</mo>{den}</mrow>"),
                    _ => format!("<mfrac>{num}{den}</mfrac>"),
                }
            }
            "sSub" => format!("<msub>{}{}</msub>", arg("e"), arg("sub")),
            "sSup" => format!("<msup>{}{}</msup>", arg("e"), arg("sup")),
            "sSubSup" => format!(
                "<msubsup>{}{}{}</msubsup>",
                arg("e"),
                arg("sub"),
                arg("sup")
            ),
            "sPre" => format!(
                "<mmultiscripts>{}<mprescripts/>{}{}</mmultiscripts>",
                arg("e"),
                arg("sub"),
                arg("sup")
            ),
            "rad" => {
                if value(node, "degHide", "0") == "1"
                    || child(node, "deg")
                        .is_none_or(|n| !n.descendants().any(|n| n.tag_name().name() == "t"))
                {
                    format!("<msqrt>{}</msqrt>", arg("e"))
                } else {
                    format!("<mroot>{}{}</mroot>", arg("e"), arg("deg"))
                }
            }
            "d" => {
                let args = node
                    .children()
                    .filter(|n| n.tag_name().name() == "e")
                    .map(render)
                    .collect::<Vec<_>>()
                    .join(&format!(
                        "<mo>{}</mo>",
                        escaped(&value(node, "sepChr", "|"))
                    ));
                let open = escaped(&value(node, "begChr", "("));
                let close = escaped(&value(node, "endChr", ")"));
                if open == "(" && close == ")" {
                    format!(r#"<mfenced open="(" close=")" separators="">{args}</mfenced>"#)
                } else {
                    format!("<mrow><mo>{open}</mo>{args}<mo>{close}</mo></mrow>")
                }
            }
            "nary" => {
                let op = format!("<mo>{}</mo>", escaped(&value(node, "chr", "∫")));
                let visible = |name: &str, hide: &str| {
                    !matches!(value(node, hide, "0").as_str(), "1" | "true" | "on")
                        && child(node, name).is_some_and(|n| n.children().any(|c| c.is_element()))
                };
                let sub = visible("sub", "subHide");
                let sup = visible("sup", "supHide");
                let side = value(node, "limLoc", "") == "subSup";
                let op = match (sub, sup) {
                    (false, false) => op,
                    (true, true) => {
                        let tag = if side { "msubsup" } else { "munderover" };
                        format!("<{tag}>{op}{}{}</{tag}>", arg("sub"), arg("sup"))
                    }
                    _ => {
                        let (tag, name) = match (side, sub) {
                            (true, true) => ("msub", "sub"),
                            (true, false) => ("msup", "sup"),
                            (false, true) => ("munder", "sub"),
                            (false, false) => ("mover", "sup"),
                        };
                        format!("<{tag}>{op}{}</{tag}>", arg(name))
                    }
                };
                format!("<mrow>{op}{}</mrow>", arg("e"))
            }
            "limLow" => format!("<munder>{}{}</munder>", arg("e"), arg("lim")),
            "limUpp" => format!("<mover>{}{}</mover>", arg("e"), arg("lim")),
            "acc" | "bar" | "groupChr" => {
                let tag = if value(node, "pos", "top") == "bot" {
                    "munder"
                } else {
                    "mover"
                };
                format!(
                    "<{tag}>{}<mo>{}</mo></{tag}>",
                    arg("e"),
                    escaped(&value(node, "chr", if name == "acc" { "̂" } else { "¯" }))
                )
            }
            "m" => format!("<mtable>{}</mtable>", children()),
            "mr" => format!(
                "<mtr>{}</mtr>",
                node.children()
                    .filter(|n| n.tag_name().name() == "e")
                    .map(|n| format!("<mtd>{}</mtd>", render(n)))
                    .collect::<String>()
            ),
            "eqArr" => format!(
                "<mtable>{}</mtable>",
                node.children()
                    .filter(|n| n.tag_name().name() == "e")
                    .map(|n| format!("<mtr><mtd>{}</mtd></mtr>", render(n)))
                    .collect::<String>()
            ),
            "phant" => format!("<mphantom>{}</mphantom>", arg("e")),
            _ => format!("<mrow>{}</mrow>", children()),
        }
    }
    let math = document.root_element().first_element_child()?;
    let text = math
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "t")
        .filter_map(|n| n.text())
        .collect::<String>();
    let math_ml = format!(
        "<math xmlns=\"http://www.w3.org/1998/Math/MathML\">{}</math>",
        render(math)
    );
    serde_json::from_value(serde_json::json!({"type":"image", "semanticKind":"formula", "mathMl":math_ml, "alt":text, "sourceXml":xml, "widthPx": (text.chars().count().max(1) * 9) as f64, "heightPx":28.0})).ok()
}

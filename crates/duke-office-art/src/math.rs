use std::collections::BTreeSet;

use roxmltree::{Document, Node};
use serde::{Deserialize, Serialize};

const MATH_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";
const STRICT_MATH_NS: &str = "http://purl.oclc.org/ooxml/officeDocument/math";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MathAttribute {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MathElement {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<MathAttribute>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<MathElement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficeMath {
    pub text: String,
    pub display: bool,
    pub source_xml: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<MathElement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub justification: Option<String>,
    pub math_ml: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size_pt: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unsupported: Vec<String>,
}

fn math(node: Node<'_, '_>) -> bool {
    matches!(node.tag_name().namespace(), Some(MATH_NS | STRICT_MATH_NS))
}

fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|node| node.is_element() && math(*node) && node.tag_name().name() == name)
}

fn property<'a, 'input>(node: Node<'a, 'input>, key: &str) -> Option<&'a str> {
    node.children()
        .filter(|node| node.is_element() && math(*node) && node.tag_name().name().ends_with("Pr"))
        .flat_map(|node| node.children())
        .find(|node| node.is_element() && node.tag_name().name() == key)
        .and_then(|node| node.attributes().find(|a| a.name() == "val"))
        .map(|a| a.value())
}

fn property_node<'a, 'input>(node: Node<'a, 'input>, key: &str) -> Option<Node<'a, 'input>> {
    node.children()
        .filter(|n| n.is_element() && math(*n) && n.tag_name().name().ends_with("Pr"))
        .flat_map(|n| n.children())
        .find(|n| n.is_element() && math(*n) && n.tag_name().name() == key)
}
fn flag(node: Node<'_, '_>, key: &str, default: bool) -> bool {
    property_node(node, key)
        .map(|n| {
            n.attributes()
                .find(|a| a.name() == "val")
                .map(|a| on(a.value()))
                .unwrap_or(true)
        })
        .unwrap_or(default)
}

fn on(value: &str) -> bool {
    matches!(value, "1" | "true" | "on")
}
fn escape(value: &str) -> String {
    crate::xml::escape_xml_text(value)
}

fn element(node: Node<'_, '_>, depth: usize) -> Option<MathElement> {
    if depth > 64 {
        return None;
    }
    let text = node
        .children()
        .filter(Node::is_text)
        .filter_map(|node| node.text())
        .collect::<String>();
    Some(MathElement {
        name: node.tag_name().name().into(),
        namespace: node.tag_name().namespace().map(str::to_owned),
        attributes: node
            .attributes()
            .map(|a| MathAttribute {
                name: a.name().into(),
                namespace: a.namespace().map(str::to_owned),
                value: a.value().into(),
            })
            .collect(),
        children: node
            .children()
            .filter(Node::is_element)
            .map(|n| element(n, depth + 1))
            .collect::<Option<_>>()?,
        text: (!text.is_empty()).then_some(text),
    })
}

struct Renderer {
    unsupported: BTreeSet<String>,
    font_size: Option<f64>,
    display: bool,
}

impl Renderer {
    fn children(&mut self, node: Node<'_, '_>) -> String {
        node.children()
            .filter(Node::is_element)
            .map(|n| self.render(n))
            .collect()
    }
    fn arg(&mut self, node: Node<'_, '_>, name: &str) -> String {
        child(node, name)
            .map(|n| self.render(n))
            .unwrap_or_else(|| "<mrow/>".into())
    }
    fn render(&mut self, node: Node<'_, '_>) -> String {
        let name = node.tag_name().name();
        if name.ends_with("Pr") {
            return String::new();
        }
        if !math(node) {
            self.unsupported.insert(format!(
                "{{{}}}{name}",
                node.tag_name().namespace().unwrap_or_default()
            ));
            return self.children(node);
        }
        match name {
            "t" => {
                let text = node.text().unwrap_or_default();
                let parent = node.parent_element().unwrap_or(node);
                if flag(parent, "nor", false) {
                    return format!("<mtext>{}</mtext>", escape(text));
                }
                let mut result = String::new();
                let mut token = String::new();
                let mut last = 0;
                for c in text.chars() {
                    let kind = if c.is_alphabetic() {
                        1
                    } else if c.is_numeric() || c == '.' {
                        2
                    } else if c.is_whitespace() {
                        3
                    } else {
                        4
                    };
                    if !token.is_empty() && kind != last {
                        result.push_str(&Self::token(&token, last));
                        token.clear();
                    }
                    token.push(c);
                    last = kind;
                }
                if !token.is_empty() {
                    result.push_str(&Self::token(&token, last));
                }
                result
            }
            "r" => {
                let body = self.children(node);
                let mut attributes = String::new();
                let variant = match property(node, "sty") {
                    Some("b") => Some("bold"),
                    Some("bi") => Some("bold-italic"),
                    Some("i") => Some("italic"),
                    Some("p") => Some("normal"),
                    _ => None,
                };
                if let Some(variant) = variant {
                    attributes.push_str(&format!(" mathvariant=\"{variant}\""));
                }
                let drawing = node
                    .children()
                    .find(|n| n.is_element() && !math(*n) && n.tag_name().name() == "rPr");
                if let Some(drawing) = drawing {
                    if let Some(size) = drawing
                        .attribute("sz")
                        .and_then(|v| v.parse::<f64>().ok())
                        .filter(|v| v.is_finite() && *v > 0.0)
                    {
                        if let Some(base) = self.font_size {
                            attributes
                                .push_str(&format!(" mathsize=\"{}em\"", size / 100.0 / base));
                        }
                    }
                    if let Some(color) = drawing
                        .descendants()
                        .find(|n| n.is_element() && n.tag_name().name() == "srgbClr")
                        .and_then(|n| n.attribute("val"))
                        .filter(|s| s.len() == 6 && s.bytes().all(|b| b.is_ascii_hexdigit()))
                    {
                        attributes.push_str(&format!(" mathcolor=\"#{color}\""));
                    }
                }
                format!("<mstyle{attributes}>{body}</mstyle>")
            }
            "f" => {
                let (num, den) = (self.arg(node, "num"), self.arg(node, "den"));
                match property(node, "type").unwrap_or("bar") {
                    "noBar" => format!("<mfrac linethickness=\"0\">{num}{den}</mfrac>"),
                    "skw" => format!("<mfrac bevelled=\"true\">{num}{den}</mfrac>"),
                    "lin" => format!("<mrow>{num}<mo>/</mo>{den}</mrow>"),
                    _ => format!("<mfrac>{num}{den}</mfrac>"),
                }
            }
            "sSub" => format!(
                "<msub>{}{}</msub>",
                self.arg(node, "e"),
                self.arg(node, "sub")
            ),
            "sSup" => format!(
                "<msup>{}{}</msup>",
                self.arg(node, "e"),
                self.arg(node, "sup")
            ),
            "sSubSup" => format!(
                "<msubsup>{}{}{}</msubsup>",
                self.arg(node, "e"),
                self.arg(node, "sub"),
                self.arg(node, "sup")
            ),
            "sPre" => format!(
                "<mmultiscripts>{}<mprescripts/>{}{}</mmultiscripts>",
                self.arg(node, "e"),
                self.arg(node, "sub"),
                self.arg(node, "sup")
            ),
            "rad" => {
                if flag(node, "degHide", false)
                    || child(node, "deg").is_none_or(|n| {
                        !n.descendants().any(|n| {
                            n.is_element()
                                && n.tag_name().name() == "t"
                                && n.text().is_some_and(|t| !t.is_empty())
                        })
                    })
                {
                    format!("<msqrt>{}</msqrt>", self.arg(node, "e"))
                } else {
                    format!(
                        "<mroot>{}{}</mroot>",
                        self.arg(node, "e"),
                        self.arg(node, "deg")
                    )
                }
            }
            "d" => {
                let separator = format!(
                    "<mo separator=\"true\">{}</mo>",
                    escape(property(node, "sepChr").unwrap_or("|"))
                );
                let arguments = node
                    .children()
                    .filter(|n| n.is_element() && math(*n) && n.tag_name().name() == "e")
                    .map(|n| self.render(n))
                    .collect::<Vec<_>>()
                    .join(&separator);
                format!(
                    "<mrow><mo fence=\"true\">{}</mo>{arguments}<mo fence=\"true\">{}</mo></mrow>",
                    escape(property(node, "begChr").unwrap_or("(")),
                    escape(property(node, "endChr").unwrap_or(")"))
                )
            }
            "nary" => {
                let op = format!(
                    "<mo largeop=\"true\">{}</mo>",
                    escape(property(node, "chr").unwrap_or("∫"))
                );
                let sub = !flag(node, "subHide", false);
                let sup = !flag(node, "supHide", false);
                let under = property(node, "limLoc")
                    .map(|v| v == "undOvr")
                    .unwrap_or_else(|| property_node(node, "limLoc").is_some() || self.display);
                let operator = match (sub, sup) {
                    (true, true) => {
                        let tag = if under { "munderover" } else { "msubsup" };
                        format!(
                            "<{tag}>{op}{}{}</{tag}>",
                            self.arg(node, "sub"),
                            self.arg(node, "sup")
                        )
                    }
                    (true, false) => {
                        let tag = if under { "munder" } else { "msub" };
                        format!("<{tag}>{op}{}</{tag}>", self.arg(node, "sub"))
                    }
                    (false, true) => {
                        let tag = if under { "mover" } else { "msup" };
                        format!("<{tag}>{op}{}</{tag}>", self.arg(node, "sup"))
                    }
                    _ => op,
                };
                format!("<mrow>{operator}{}</mrow>", self.arg(node, "e"))
            }
            "limLow" => format!(
                "<munder>{}{}</munder>",
                self.arg(node, "e"),
                self.arg(node, "lim")
            ),
            "limUpp" => format!(
                "<mover>{}{}</mover>",
                self.arg(node, "e"),
                self.arg(node, "lim")
            ),
            "acc" | "bar" | "groupChr" => {
                let tag = if property(node, "pos").unwrap_or(if name == "groupChr" {
                    "bot"
                } else {
                    "top"
                }) == "bot"
                {
                    "munder"
                } else {
                    "mover"
                };
                let default = if name == "acc" {
                    "̂"
                } else if name == "groupChr" {
                    "⏟"
                } else {
                    "¯"
                };
                let accent = if tag == "munder" {
                    "accentunder"
                } else {
                    "accent"
                };
                format!(
                    "<{tag} {accent}=\"true\">{}<mo stretchy=\"true\">{}</mo></{tag}>",
                    self.arg(node, "e"),
                    escape(property(node, "chr").unwrap_or(default))
                )
            }
            "m" => format!("<mtable>{}</mtable>", self.children(node)),
            "mr" => format!(
                "<mtr>{}</mtr>",
                node.children()
                    .filter(|n| n.is_element() && math(*n) && n.tag_name().name() == "e")
                    .map(|n| format!("<mtd>{}</mtd>", self.render(n)))
                    .collect::<String>()
            ),
            "eqArr" => format!(
                "<mtable>{}</mtable>",
                node.children()
                    .filter(|n| n.is_element() && math(*n) && n.tag_name().name() == "e")
                    .map(|n| format!("<mtr><mtd>{}</mtd></mtr>", self.render(n)))
                    .collect::<String>()
            ),
            "func" => format!(
                "<mrow>{}<mo>⁡</mo>{}</mrow>",
                self.arg(node, "fName"),
                self.arg(node, "e")
            ),
            "phant" => {
                let mut attrs = String::new();
                for (flag_name, attribute) in [
                    ("zeroWid", "width"),
                    ("zeroAsc", "height"),
                    ("zeroDesc", "depth"),
                ] {
                    if flag(node, flag_name, false) {
                        attrs.push_str(&format!(" {attribute}=\"0px\""));
                    }
                }
                let body = self.arg(node, "e");
                let body = if attrs.is_empty() {
                    body
                } else {
                    format!("<mpadded{attrs}>{body}</mpadded>")
                };
                if flag(node, "show", false) {
                    body
                } else {
                    format!("<mphantom>{body}</mphantom>")
                }
            }
            "oMath" | "oMathPara" | "num" | "den" | "e" | "sub" | "sup" | "deg" | "lim"
            | "fName" | "box" => format!("<mrow>{}</mrow>", self.children(node)),
            _ => {
                self.unsupported.insert(name.into());
                format!("<mrow>{}</mrow>", self.children(node))
            }
        }
    }
    fn token(text: &str, kind: u8) -> String {
        let tag = match kind {
            1 => "mi",
            2 => "mn",
            3 => "mtext",
            _ => "mo",
        };
        format!("<{tag}>{}</{tag}>", escape(text))
    }
}

pub fn parse_office_math(xml: &str) -> Option<OfficeMath> {
    if xml.len() > 1024 * 1024 {
        return None;
    }
    let document = Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 16384,
        },
    )
    .ok()?;
    let root = document.descendants().find(|n| {
        n.is_element() && math(*n) && matches!(n.tag_name().name(), "oMath" | "oMathPara")
    })?;
    let structure = element(root, 0)?;
    let text = root
        .descendants()
        .filter(|n| n.is_element() && math(*n) && n.tag_name().name() == "t")
        .filter_map(|n| n.text())
        .collect();
    let font_size = root
        .descendants()
        .filter(|n| n.is_element() && !math(*n) && n.tag_name().name() == "rPr")
        .find_map(|n| n.attribute("sz").and_then(|v| v.parse::<f64>().ok()))
        .filter(|v| v.is_finite() && *v > 0.0)
        .map(|v| v / 100.0);
    let display = root.tag_name().name() == "oMathPara";
    let mut renderer = Renderer {
        unsupported: BTreeSet::new(),
        font_size,
        display,
    };
    let math_ml = format!(
        "<math xmlns=\"http://www.w3.org/1998/Math/MathML\" display=\"{}\">{}</math>",
        if display { "block" } else { "inline" },
        renderer.render(root)
    );
    Some(OfficeMath {
        text,
        display,
        source_xml: xml.into(),
        root: Some(structure),
        justification: property(root, "jc").map(str::to_owned),
        math_ml,
        font_size_pt: font_size,
        unsupported: renderer.unsupported.into_iter().collect(),
    })
}

impl OfficeMath {
    pub fn unparsed(source_xml: String, text: String, display: bool) -> Self {
        Self {
            text,
            display,
            source_xml,
            root: None,
            justification: None,
            math_ml: String::new(),
            font_size_pt: None,
            unsupported: vec!["source-invalid-or-exceeds-limit".into()],
        }
    }
}

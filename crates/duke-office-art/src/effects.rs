use std::fmt::Write;

use crate::xml::{direct_child_tag, extract_direct_child_tag_ranges, get_attribute_ref};
use crate::{blip_filter, PictureContext};

struct Graph<'a, C> {
    context: &'a C,
    output: String,
    remaining: usize,
    next_id: usize,
}

impl<C: PictureContext> Graph<'_, C> {
    fn result(&mut self) -> String {
        let id = format!("art-effect-{}", self.next_id);
        self.next_id += 1;
        id
    }

    fn container(&mut self, xml: &str, input: &str, depth: usize) -> Option<String> {
        if depth > 16 {
            return None;
        }
        let children = extract_direct_child_tag_ranges(xml);
        match get_attribute_ref(xml, "type").unwrap_or("sib") {
            "tree" => {}
            "sib" if children.len() <= 1 => {}
            _ => return None,
        }
        let mut current = input.to_string();
        for child in children {
            if self.remaining == 0 {
                return None;
            }
            self.remaining -= 1;
            let node = &xml[child.start..child.end];
            current = match child.tag_name {
                "a:cont" => self.container(node, &current, depth + 1)?,
                "a:blend" | "a:alphaMod" => {
                    let nested = direct_child_tag(node, "a:cont")?;
                    if extract_direct_child_tag_ranges(node).len() != 1 {
                        return None;
                    }
                    let branch = self.container(nested, &current, depth + 1)?;
                    let result = self.result();
                    if child.tag_name == "a:alphaMod" {
                        let _ = write!(
                            self.output,
                            r#"<feComposite in="{current}" in2="{branch}" operator="in" result="{result}"/>"#
                        );
                    } else {
                        let mode = match get_attribute_ref(node, "blend")? {
                            "over" => "normal",
                            "mult" => "multiply",
                            "screen" => "screen",
                            "darken" => "darken",
                            "lighten" => "lighten",
                            _ => return None,
                        };
                        let _ = write!(
                            self.output,
                            r#"<feBlend in="{branch}" in2="{current}" mode="{mode}" result="{result}"/>"#
                        );
                    }
                    result
                }
                "a:clrRepl" | "a:grayscl" | "a:alphaModFix" | "a:lum" => {
                    if child.tag_name == "a:clrRepl"
                        && self.context.resolve_color(node)?.opacity.unwrap_or(1.0) != 1.0
                    {
                        return None;
                    }
                    let filter = blip_filter(&format!("<a:blip>{node}</a:blip>"), self.context)?;
                    let wrapped = format!("<filter>{filter}</filter>");
                    let mut previous = current;
                    for primitive in extract_direct_child_tag_ranges(&wrapped) {
                        let raw = &wrapped[primitive.start..primitive.end];
                        let end =
                            raw.find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')?;
                        let result = self.result();
                        let _ = write!(
                            self.output,
                            r#"{} in="{previous}" result="{result}"{}"#,
                            &raw[..end],
                            &raw[end..]
                        );
                        previous = result;
                    }
                    previous
                }
                _ => return None,
            };
        }
        Some(current)
    }
}

pub(crate) fn render(xml: &str, context: &impl PictureContext) -> Option<String> {
    if xml.len() > 64 * 1024 {
        return None;
    }
    let mut graph = Graph {
        context,
        output: String::new(),
        remaining: 128,
        next_id: 0,
    };
    let result = graph.container(xml, "SourceGraphic", 0)?;
    if result == "SourceGraphic" {
        return None;
    }
    Some(graph.output)
}

use super::{ParseLimits, XmlBudget};
use quick_xml::Reader;
use std::io::BufRead;

/// Consumes a coordinate-only DrawingML subtree after checking names, nesting,
/// attribute syntax and the same XML budgets used when building the full tree.
/// Other XML spellings and extensions go through the regular reader.
pub(super) fn skip_children(
    reader: &mut Reader<&[u8]>,
    root: &[u8],
    outer_depth: usize,
    budget: &mut XmlBudget,
    limits: &ParseLimits,
) {
    let input = *reader.get_ref();
    if let Some((consumed, used)) = scan(input, root, outer_depth, *budget, limits) {
        reader.stream().consume(consumed);
        *budget = used;
    }
}

fn scan(
    input: &[u8],
    root: &[u8],
    outer_depth: usize,
    mut used: XmlBudget,
    limits: &ParseLimits,
) -> Option<(usize, XmlBudget)> {
    let prefix = root.strip_suffix(b"custGeom")?;
    let mut elements = [root; 6];
    let mut depth = 0;
    let mut rest = input;
    loop {
        let space = rest
            .iter()
            .take_while(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
            .count();
        used.text_bytes = used.text_bytes.saturating_add(space);
        rest = &rest[space..];
        let tag = rest.strip_prefix(b"<")?;
        let end = memchr::memchr(b'>', tag)?;
        let body = &tag[..end];
        if let Some(name) = body.strip_prefix(b"/") {
            if elements[depth] != name {
                return None;
            }
            if depth == 0 {
                if used.nodes > limits.max_xml_nodes
                    || used.text_bytes.saturating_add(used.source_xml_bytes)
                        > limits.max_xml_text_bytes
                    || used.check_attributes(limits).is_err()
                {
                    return None;
                }
                return Some((input.len() - rest.len(), used));
            }
            depth -= 1;
        } else {
            let empty = body.strip_suffix(b"/");
            let body = empty.unwrap_or(body);
            let name_end = memchr::memchr(b' ', body).unwrap_or(body.len());
            let name = &body[..name_end];
            let local = name.strip_prefix(prefix)?;
            if !matches!(
                local,
                b"avLst"
                    | b"gdLst"
                    | b"gd"
                    | b"ahLst"
                    | b"ahXY"
                    | b"ahPolar"
                    | b"cxnLst"
                    | b"cxn"
                    | b"pos"
                    | b"rect"
                    | b"pathLst"
                    | b"path"
                    | b"moveTo"
                    | b"lnTo"
                    | b"arcTo"
                    | b"quadBezTo"
                    | b"cubicBezTo"
                    | b"pt"
                    | b"close"
            ) {
                return None;
            }
            count_attributes(&body[name_end..], &mut used)?;
            used.nodes = used.nodes.saturating_add(1);
            if empty.is_none() {
                if outer_depth + depth >= limits.max_xml_depth {
                    return None;
                }
                depth += 1;
                *elements.get_mut(depth)? = name;
            }
        }
        rest = &tag[end + 1..];
    }
}

fn count_attributes(mut raw: &[u8], used: &mut XmlBudget) -> Option<()> {
    while !raw.is_empty() {
        raw = raw.strip_prefix(b" ")?;
        let equals = memchr::memchr(b'=', raw)?;
        let name = &raw[..equals];
        if !name.first()?.is_ascii_alphabetic()
            || !name.iter().all(u8::is_ascii_alphanumeric)
            || name == b"xmlns"
        {
            return None;
        }
        let (&quote, value) = raw[equals + 1..].split_first()?;
        if !matches!(quote, b'"' | b'\'') {
            return None;
        }
        let end = memchr::memchr(quote, value)?;
        if memchr::memchr2(b'&', b'<', &value[..end]).is_some() {
            return None;
        }
        used.attributes = used.attributes.saturating_add(1);
        used.attribute_bytes = used.attribute_bytes.saturating_add(name.len() + end);
        raw = &value[end + 1..];
    }
    Some(())
}

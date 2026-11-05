pub(crate) struct TaggedRange {
    pub start: usize,
    pub end: usize,
    pub tag_name: String,
}

fn token_end(xml: &str, start: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, byte) in xml.as_bytes().iter().copied().enumerate().skip(start) {
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
        } else if matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
        } else if byte == b'>' {
            return Some(offset + 1);
        }
    }
    None
}

struct Token<'a> {
    start: usize,
    end: usize,
    name: &'a str,
    close: bool,
    empty: bool,
}

fn tokens(xml: &str) -> impl Iterator<Item = Token<'_>> {
    let mut cursor = 0;
    std::iter::from_fn(move || loop {
        let start = cursor + xml.get(cursor..)?.find('<')?;
        let tail = &xml[start..];
        let skip = if tail.starts_with("<!--") {
            Some(("-->", 3))
        } else if tail.starts_with("<![CDATA[") {
            Some(("]]>", 3))
        } else if tail.starts_with("<?") {
            Some(("?>", 2))
        } else {
            None
        };
        if let Some((suffix, length)) = skip {
            cursor = start + tail.find(suffix)? + length;
            continue;
        }
        let end = token_end(xml, start)?;
        cursor = end;
        if tail.starts_with("<!") {
            continue;
        }
        let close = tail.starts_with("</");
        let name_start = start + if close { 2 } else { 1 };
        let name_length = xml[name_start..end]
            .find(|ch: char| ch.is_ascii_whitespace() || ch == '/' || ch == '>')?;
        let name = &xml[name_start..name_start + name_length];
        let empty = xml[start..end - 1].trim_end().ends_with('/');
        return Some(Token {
            start,
            end,
            name,
            close,
            empty,
        });
    })
}

pub(crate) fn extract_direct_child_tag_ranges(xml: &str) -> Vec<TaggedRange> {
    let mut tags = tokens(xml);
    let Some(root) = tags.next() else {
        return Vec::new();
    };
    if root.empty || root.close {
        return Vec::new();
    }
    let mut depth = 0;
    let mut pending = None;
    let mut output = Vec::new();
    for token in tags {
        if token.close {
            if depth == 0 {
                break;
            }
            depth -= 1;
            if depth == 0 {
                if let Some((name, start)) = pending.take() {
                    output.push(TaggedRange {
                        start,
                        end: token.end,
                        tag_name: name,
                    });
                }
            }
        } else {
            if depth == 0 {
                if token.empty {
                    output.push(TaggedRange {
                        start: token.start,
                        end: token.end,
                        tag_name: token.name.to_string(),
                    });
                } else {
                    pending = Some((token.name.to_string(), token.start));
                }
            }
            if !token.empty {
                depth += 1;
            }
        }
    }
    output
}

pub(crate) fn direct_child_tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let tag = extract_direct_child_tag_ranges(xml)
        .into_iter()
        .find(|tag| tag.tag_name.eq_ignore_ascii_case(name))?;
    Some(&xml[tag.start..tag.end])
}

pub(crate) fn extract_balanced_tag_slices<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let mut depth = 0;
    let mut start = 0;
    let mut output = Vec::new();
    for token in tokens(xml) {
        if !token.name.eq_ignore_ascii_case(name) {
            continue;
        }
        if token.close {
            if depth == 0 {
                continue;
            }
            depth -= 1;
            if depth == 0 {
                output.push(&xml[start..token.end]);
            }
        } else if token.empty {
            if depth == 0 {
                output.push(&xml[token.start..token.end]);
            }
        } else {
            if depth == 0 {
                start = token.start;
            }
            depth += 1;
        }
    }
    output
}

pub(crate) fn get_attribute_ref<'a>(xml: &'a str, attribute: &str) -> Option<&'a str> {
    let first = tokens(xml).next()?;
    let xml = &xml[first.start..first.end];
    let bytes = xml.as_bytes();
    let mut cursor = 0;
    while let Some(offset) = bytes[cursor..].iter().position(|byte| *byte == b'=') {
        let equals = cursor + offset;
        let mut end = equals;
        while end > 0 && bytes[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        let mut start = end;
        while start > 0
            && !bytes[start - 1].is_ascii_whitespace()
            && !matches!(bytes[start - 1], b'<' | b'>' | b'/' | b'"' | b'\'')
        {
            start -= 1;
        }
        let mut quote_start = equals + 1;
        while quote_start < bytes.len() && bytes[quote_start].is_ascii_whitespace() {
            quote_start += 1;
        }
        let quote = *bytes.get(quote_start)?;
        if !matches!(quote, b'"' | b'\'') {
            cursor = equals + 1;
            continue;
        }
        let value_start = quote_start + 1;
        let value_end = value_start
            + bytes[value_start..]
                .iter()
                .position(|byte| *byte == quote)?;
        if bytes[start..end].eq_ignore_ascii_case(attribute.as_bytes()) {
            return Some(&xml[value_start..value_end]);
        }
        cursor = value_end + 1;
    }
    None
}

pub(crate) fn escape_xml_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            _ => output.push(character),
        }
    }
    output
}

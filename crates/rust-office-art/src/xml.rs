use std::{cell::RefCell, collections::{HashMap, hash_map::RandomState}, hash::BuildHasher};

const MAX_CACHE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy)]
struct CachedRange {
    start: usize,
    end: usize,
    name_start: usize,
    name_end: usize,
}

#[derive(Default)]
struct ChildCache {
    ranges: HashMap<u64, Vec<usize>>,
    entries: Vec<(String, Vec<CachedRange>)>,
    hasher: RandomState,
    bytes: usize,
    recent: [Option<usize>; 4],
}

impl ChildCache {
    fn remember(&mut self, index: usize) {
        let position = self.recent.iter().position(|slot| *slot == Some(index))
            .unwrap_or(self.recent.len() - 1);
        self.recent[..=position].rotate_right(1);
        self.recent[0] = Some(index);
    }
}

thread_local! {
    static CHILD_CACHE: RefCell<Option<ChildCache>> = const { RefCell::new(None) };
}

pub(crate) fn with_cached_xml<T>(parse: impl FnOnce() -> T) -> T {
    struct Restore(Option<ChildCache>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CHILD_CACHE.with(|slot| *slot.borrow_mut() = self.0.take());
        }
    }
    let previous = CHILD_CACHE.with(|slot| slot.replace(Some(ChildCache::default())));
    let _restore = Restore(previous);
    parse()
}

pub(crate) struct TaggedRange<'a> {
    pub start: usize,
    pub end: usize,
    pub tag_name: &'a str,
}

fn token_end(xml: &str, start: usize) -> Option<usize> {
    let bytes = xml.as_bytes();
    let mut cursor = start;
    while cursor < bytes.len() {
        let offset = cursor + memchr::memchr3(b'\'', b'"', b'>', &bytes[cursor..])?;
        if bytes[offset] == b'>' {
            return Some(offset + 1);
        }
        let value_start = offset + 1;
        cursor = value_start + memchr::memchr(bytes[offset], &bytes[value_start..])? + 1;
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
        let start = cursor + memchr::memchr(b'<', xml.as_bytes().get(cursor..)?)?;
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
        let name_length = xml.as_bytes()[name_start..end]
            .iter()
            .position(|byte| byte.is_ascii_whitespace() || matches!(byte, b'/' | b'>'))?;
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

fn uncached_child_ranges(xml: &str) -> Vec<TaggedRange<'_>> {
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
                        tag_name: token.name,
                    });
                } else {
                    pending = Some((token.name, token.start));
                }
            }
            if !token.empty {
                depth += 1;
            }
        }
    }
    output
}

fn cache_ranges<'a>(cache: &'a mut ChildCache, xml: &str) -> Option<&'a [CachedRange]> {
    if xml.len().saturating_add(128) > MAX_CACHE_BYTES {
        return None;
    }
    for recent_index in 0..cache.recent.len() {
        let Some(index) = cache.recent[recent_index] else { break; };
        if cache.entries.get(index).is_some_and(|(source, _)| source == xml)
        {
            cache.recent[..=recent_index].rotate_right(1);
            return Some(&cache.entries[index].1);
        }
    }
    let hash = cache.hasher.hash_one(xml);
    let index = cache.ranges.get(&hash)
        .and_then(|indices| indices.iter().copied().find(|&index| {
            cache.entries.get(index).is_some_and(|(source, _)| source == xml)
        }));
    if let Some(index) = index {
        cache.remember(index);
        return Some(&cache.entries[index].1);
    }
    let mut bytes = 0;
    let value = retained_ranges(xml, &mut bytes)?;
    if cache.bytes.saturating_add(bytes) > MAX_CACHE_BYTES {
        cache.ranges.clear();
        cache.entries.clear();
        cache.bytes = 0;
        cache.recent = [None; 4];
    }
    cache.bytes += bytes;
    let index = cache.entries.len();
    cache.entries.push(value);
    cache.ranges.entry(hash).or_default().push(index);
    cache.remember(index);
    Some(&cache.entries[index].1)
}

fn retained_ranges(xml: &str, retained_bytes: &mut usize) -> Option<(String, Vec<CachedRange>)> {
    if retained_bytes.saturating_add(xml.len()) > MAX_CACHE_BYTES {
        return None;
    }
    let ranges = uncached_child_ranges(xml);
    let bytes = xml.len().saturating_add(ranges.len() * std::mem::size_of::<CachedRange>()).saturating_add(128);
    if retained_bytes.saturating_add(bytes) > MAX_CACHE_BYTES {
        return None;
    }
    let ranges = ranges.into_iter().map(|tag| {
        let name_start = tag.tag_name.as_ptr() as usize - xml.as_ptr() as usize;
        CachedRange { start: tag.start, end: tag.end, name_start, name_end: name_start + tag.tag_name.len() }
    }).collect();
    *retained_bytes += bytes;
    Some((xml.to_owned(), ranges))
}

pub(crate) fn extract_direct_child_tag_ranges(xml: &str) -> Vec<TaggedRange<'_>> {
    let cached = CHILD_CACHE.with(|slot| {
        let mut cache = slot.borrow_mut();
        let cache = cache.as_mut()?;
        Some(cache_ranges(cache, xml)?.iter().map(|tag| TaggedRange {
            start: tag.start, end: tag.end, tag_name: &xml[tag.name_start..tag.name_end],
        }).collect())
    });
    cached.unwrap_or_else(|| uncached_child_ranges(xml))
}

pub(crate) fn direct_child_tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    find_direct_child(xml, |tag| tag.eq_ignore_ascii_case(name))
}

pub(crate) fn direct_child_local<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    find_direct_child(xml, |tag| tag.rsplit(':').next() == Some(name))
}

fn find_direct_child<'a>(xml: &'a str, matches: impl Fn(&str) -> bool) -> Option<&'a str> {
    let cached = CHILD_CACHE.with(|slot| {
        let mut cache = slot.borrow_mut();
        let cache = cache.as_mut()?;
        Some(cache_ranges(cache, xml)?.iter().find(|tag| matches(&xml[tag.name_start..tag.name_end]))
            .map(|tag| &xml[tag.start..tag.end]))
    });
    if let Some(result) = cached { return result; }
    let mut tags = tokens(xml);
    let root = tags.next()?;
    if root.empty || root.close {
        return None;
    }
    let mut depth = 0usize;
    let mut selected = None;
    for token in tags {
        if token.close {
            if depth == 0 {
                return None;
            }
            depth -= 1;
            if depth == 0 {
                if let Some(start) = selected.take() {
                    return Some(&xml[start..token.end]);
                }
            }
        } else {
            if depth == 0 && matches(token.name) {
                if token.empty {
                    return Some(&xml[token.start..token.end]);
                }
                selected = Some(token.start);
            }
            if !token.empty {
                depth += 1;
            }
        }
    }
    None
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
    while let Some(offset) = memchr::memchr(b'=', &bytes[cursor..]) {
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
        let value_end = value_start + memchr::memchr(quote, &bytes[value_start..])?;
        if bytes[start..end].eq_ignore_ascii_case(attribute.as_bytes()) {
            return Some(&xml[value_start..value_end]);
        }
        cursor = value_end + 1;
    }
    None
}

pub(crate) fn escape_xml_text(value: &str) -> String {
    let bytes = value.as_bytes();
    if memchr::memchr3(b'&', b'<', b'>', bytes).is_none()
        && memchr::memchr2(b'"', b'\'', bytes).is_none()
    {
        return value.to_owned();
    }
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

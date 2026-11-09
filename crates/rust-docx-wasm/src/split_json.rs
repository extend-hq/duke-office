//! Parses JSON that is too long to be one JavaScript string.
//!
//! `JSON.parse` takes its text as a JavaScript string, and engines cap string length: V8 at
//! 2^29 - 24 characters, or 2^28 - 16 on 32-bit platforms. A document with hundreds of thousands
//! of runs serializes to more than that. Its JSON is parsed in pieces here, and the pieces are
//! joined into the value a single `JSON.parse` call would have returned.

use js_sys::{Array, Object, Reflect, JSON};
use wasm_bindgen::{JsCast, JsValue};

/// Longest text passed to one `JSON.parse` call, in bytes. A string has at most one UTF-16 code
/// unit per UTF-8 byte, so this stays under every engine's string limit.
const MAX_PARSE_BYTES: usize = 128 << 20;

/// Part of an array's or object's children, in source order.
#[derive(Debug, PartialEq)]
enum Segment<'a> {
    /// Consecutive children with the commas between them.
    Children(&'a str),
    /// One child too long to share a parse call. `key` is its key in an object, as JSON text.
    Child {
        key: Option<&'a str>,
        value: &'a str,
    },
}

/// Parses `json`, which must be compact JSON as `serde_json` writes it (no whitespace between
/// tokens), into the value `JSON.parse` returns for it.
pub(crate) fn parse(json: &str) -> Result<JsValue, JsValue> {
    parse_within(json, MAX_PARSE_BYTES)
}

fn parse_within(json: &str, limit: usize) -> Result<JsValue, JsValue> {
    let opening = json.as_bytes().first().copied();
    if json.len() <= limit || !matches!(opening, Some(b'[' | b'{')) {
        // A string or number cannot be divided: the engine parses it or reports its own limit.
        return JSON::parse(json);
    }
    if opening == Some(b'[') {
        let mut array = Array::new();
        for segment in segments(json, limit) {
            match segment {
                Segment::Children(children) => {
                    let parsed = JSON::parse(&format!("[{children}]"))?;
                    array = array.concat(parsed.unchecked_ref::<Array>());
                }
                Segment::Child { value, .. } => {
                    array.push(&parse_within(value, limit)?);
                }
            }
        }
        return Ok(array.into());
    }
    let object = Object::new();
    for segment in segments(json, limit) {
        // Properties are defined, not assigned, so a `__proto__` member stays an own property
        // as it does in `JSON.parse`.
        match segment {
            Segment::Children(children) => {
                let parsed = JSON::parse(&format!("{{{children}}}"))?;
                let descriptors =
                    Object::get_own_property_descriptors(parsed.unchecked_ref::<Object>());
                Object::define_properties(&object, descriptors.unchecked_ref());
            }
            Segment::Child { key, value } => {
                let descriptor = Object::new();
                Reflect::set(&descriptor, &"value".into(), &parse_within(value, limit)?)?;
                for attribute in ["writable", "enumerable", "configurable"] {
                    Reflect::set(&descriptor, &attribute.into(), &JsValue::TRUE)?;
                }
                let key = JSON::parse(key.expect("a member of an object has a key"))?;
                Reflect::define_property(&object, &key, &descriptor)?;
            }
        }
    }
    Ok(object.into())
}

/// Divides the children of `container`, a compact JSON array or object, so that each run of
/// children wrapped in the container's brackets is at most `limit` bytes long.
fn segments(container: &str, limit: usize) -> Vec<Segment<'_>> {
    let bytes = container.as_bytes();
    let is_object = bytes[0] == b'{';
    let end = bytes.len() - 1;
    let room = limit.saturating_sub(2);
    let mut segments = Vec::new();
    // The current run of short children starts at `run_start`; `child` is the next one to place.
    let mut run_start = 1;
    let mut child = 1;
    while child < end {
        let value_start = if is_object {
            string_end(bytes, child) + 1
        } else {
            child
        };
        let child_end = value_end(bytes, value_start);
        if child_end - child > room {
            if child > run_start {
                segments.push(Segment::Children(&container[run_start..child - 1]));
            }
            segments.push(Segment::Child {
                key: is_object.then(|| &container[child..value_start - 1]),
                value: &container[value_start..child_end],
            });
            run_start = child_end + 1;
        } else if child_end - run_start > room {
            segments.push(Segment::Children(&container[run_start..child - 1]));
            run_start = child;
        }
        // Step over the comma or closing bracket that follows the child.
        child = child_end + 1;
    }
    if run_start < end {
        segments.push(Segment::Children(&container[run_start..end]));
    }
    segments
}

/// Index just past the JSON value that starts at `start`.
fn value_end(bytes: &[u8], start: usize) -> usize {
    match bytes[start] {
        b'"' => string_end(bytes, start),
        b'[' | b'{' => {
            let mut depth = 0usize;
            let mut index = start;
            loop {
                match bytes[index] {
                    b'"' => {
                        index = string_end(bytes, index);
                        continue;
                    }
                    b'[' | b'{' => depth += 1,
                    b']' | b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            return index + 1;
                        }
                    }
                    _ => {}
                }
                index += 1;
            }
        }
        _ => bytes[start..]
            .iter()
            .position(|byte| matches!(byte, b',' | b']' | b'}'))
            .map_or(bytes.len(), |length| start + length),
    }
}

/// Index just past the closing quote of the JSON string that starts at `start`.
fn string_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 1;
    while bytes[index] != b'"' {
        index += if bytes[index] == b'\\' { 2 } else { 1 };
    }
    index + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Map, Value};

    // Brackets, commas and quotes inside strings, escapes before a closing quote, empty and
    // nested containers, every scalar kind and a `__proto__` key.
    const SAMPLE: &str = r#"{"a":[],"b":{},"text":"q\"uo\\te ]}[{, é é😀","nodes":[{"k":[1,-2.5e3,true,false,null],"s":"\\"},[[["deep"]]],"x",0,{"children":[{"t":"a"},{"t":"b"},{"t":"c"}]}],"__proto__":{"n":1},"last":"\\\""}"#;

    /// Rebuilds `json` the way `parse_within` does, with `serde_json` values in place of
    /// JavaScript ones, checking that every split container's segments cover it exactly.
    fn rebuild(json: &str, limit: usize) -> Value {
        if json.len() <= limit || !json.starts_with(['[', '{']) {
            return serde_json::from_str(json).unwrap();
        }
        let mut items = Vec::new();
        let mut members = Map::new();
        let mut covered = Vec::new();
        for segment in segments(json, limit) {
            match segment {
                Segment::Children(children) => {
                    assert!(!children.is_empty());
                    assert!(children.len() + 2 <= limit, "{children} at limit {limit}");
                    if json.starts_with('[') {
                        let parsed: Vec<Value> =
                            serde_json::from_str(&format!("[{children}]")).unwrap();
                        items.extend(parsed);
                    } else {
                        let parsed: Map<String, Value> =
                            serde_json::from_str(&format!("{{{children}}}")).unwrap();
                        members.extend(parsed);
                    }
                    covered.push(children.to_string());
                }
                Segment::Child {
                    key: Some(key),
                    value,
                } => {
                    members.insert(serde_json::from_str(key).unwrap(), rebuild(value, limit));
                    covered.push(format!("{key}:{value}"));
                }
                Segment::Child { key: None, value } => {
                    items.push(rebuild(value, limit));
                    covered.push(value.to_string());
                }
            }
        }
        assert_eq!(covered.join(","), json[1..json.len() - 1]);
        if json.starts_with('[') {
            assert!(members.is_empty());
            Value::Array(items)
        } else {
            assert!(items.is_empty());
            Value::Object(members)
        }
    }

    #[test]
    fn every_limit_rebuilds_the_same_value() {
        let whole: Value = serde_json::from_str(SAMPLE).unwrap();
        for limit in 0..=SAMPLE.len() + 1 {
            assert_eq!(rebuild(SAMPLE, limit), whole, "limit {limit}");
        }
    }

    #[test]
    fn long_children_stand_alone_between_runs() {
        assert_eq!(
            segments("[1,22,[3,4],5]", 6),
            [
                Segment::Children("1,22"),
                Segment::Child {
                    key: None,
                    value: "[3,4]"
                },
                Segment::Children("5"),
            ]
        );
        assert_eq!(
            segments(r#"{"a":1,"b":"]},","c":[3,4,5]}"#, 12),
            [
                Segment::Children(r#""a":1"#),
                Segment::Children(r#""b":"]},""#),
                Segment::Child {
                    key: Some(r#""c""#),
                    value: "[3,4,5]"
                },
            ]
        );
        assert_eq!(segments("[]", 0), []);
        assert_eq!(segments("{}", 0), []);
    }
}

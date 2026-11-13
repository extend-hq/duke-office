//! Out-of-band payloads for large image fields during model deserialization.
//!
//! `DocNode` and `ParagraphChildNode` are untagged, so serde buffers each node
//! into a generic tree before matching a variant. A JSON number array then costs
//! one buffered value per image byte, repeated for every untagged level. Callers
//! that already hold image bytes and data URIs can instead place them in
//! [`TransportPayloads`] and write a short token in the JSON; the image field
//! deserializers resolve the token, so only the token is buffered.
//!
//! Image `data` also accepts a base64 string or the legacy number array, and
//! `src` accepts any ordinary string.

use std::cell::RefCell;
use std::fmt;

use base64::Engine;
use serde::de::{self, Deserializer, SeqAccess, Visitor};

/// Prefix of a token that refers to [`TransportPayloads::bytes`].
pub const BYTES_TOKEN_PREFIX: &str = "\u{0}duke-pages-bytes:";
/// Prefix of a token that refers to [`TransportPayloads::text`].
pub const TEXT_TOKEN_PREFIX: &str = "\u{0}duke-pages-text:";

/// Large values referenced by tokens in model JSON.
#[derive(Debug, Default)]
pub struct TransportPayloads {
    pub bytes: Vec<Vec<u8>>,
    pub text: Vec<String>,
}

thread_local! {
    static PAYLOADS: RefCell<Option<TransportPayloads>> = const { RefCell::new(None) };
}

struct Installed(Option<TransportPayloads>);

impl Drop for Installed {
    fn drop(&mut self) {
        let previous = self.0.take();
        PAYLOADS.with(|slot| *slot.borrow_mut() = previous);
    }
}

/// Runs `parse` with `payloads` available to token-aware image fields.
///
/// Tokens are only resolved inside `parse`; the payloads are released when it
/// returns, including on panic.
pub fn with_transport_payloads<T>(payloads: TransportPayloads, parse: impl FnOnce() -> T) -> T {
    let previous = PAYLOADS.with(|slot| slot.borrow_mut().replace(payloads));
    let _installed = Installed(previous);
    parse()
}

fn token_index(value: &str, prefix: &str) -> Option<Result<usize, String>> {
    let index = value.strip_prefix(prefix)?;
    Some(index.parse().map_err(|_| format!("invalid transport token {value:?}")))
}

fn resolve<T: Clone>(index: usize, select: impl Fn(&TransportPayloads) -> &Vec<T>) -> Result<T, String> {
    PAYLOADS.with(|slot| {
        let payloads = slot.borrow();
        let payloads = payloads
            .as_ref()
            .ok_or_else(|| "transport token used outside a transport scope".to_string())?;
        // Untagged variant attempts may resolve the same field more than once,
        // so the payload is cloned rather than moved out.
        select(payloads)
            .get(index)
            .cloned()
            .ok_or_else(|| format!("transport payload {index} is missing"))
    })
}

struct ImageDataVisitor;

impl<'de> Visitor<'de> for ImageDataVisitor {
    type Value = Option<Vec<u8>>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("image bytes as a byte array, base64 string or transport token")
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(self)
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        if let Some(index) = token_index(value, BYTES_TOKEN_PREFIX) {
            return index.and_then(|index| resolve(index, |p| &p.bytes)).map(Some).map_err(E::custom);
        }
        base64::engine::general_purpose::STANDARD
            .decode(value)
            .map(Some)
            .map_err(|error| E::custom(format!("invalid base64 image data: {error}")))
    }

    fn visit_bytes<E: de::Error>(self, value: &[u8]) -> Result<Self::Value, E> {
        Ok(Some(value.to_vec()))
    }

    fn visit_byte_buf<E: de::Error>(self, value: Vec<u8>) -> Result<Self::Value, E> {
        Ok(Some(value))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut bytes = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(1 << 20));
        while let Some(byte) = seq.next_element::<u8>()? {
            bytes.push(byte);
        }
        Ok(Some(bytes))
    }
}

/// Deserializes optional image bytes from a byte array, base64 string or bytes token.
pub fn deserialize_image_data<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error> {
    deserializer.deserialize_option(ImageDataVisitor)
}

struct TextVisitor;

impl<'de> Visitor<'de> for TextVisitor {
    type Value = Option<String>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string or transport token")
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_str(self)
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        match token_index(value, TEXT_TOKEN_PREFIX) {
            Some(index) => index.and_then(|index| resolve(index, |p| &p.text)).map(Some).map_err(E::custom),
            None => Ok(Some(value.to_owned())),
        }
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
        if value.starts_with(TEXT_TOKEN_PREFIX) {
            return self.visit_str(&value);
        }
        Ok(Some(value))
    }
}

/// Deserializes an optional string that may be a text token.
pub fn deserialize_optional_text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    deserializer.deserialize_option(TextVisitor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ImageRunNode;

    fn image(json: &str) -> Result<ImageRunNode, serde_json::Error> {
        serde_json::from_str(json)
    }

    #[test]
    fn image_data_accepts_number_arrays_base64_and_null() {
        assert_eq!(image(r#"{"data":[1,2,255]}"#).unwrap().data, Some(vec![1, 2, 255]));
        assert_eq!(image(r#"{"data":"AQL/"}"#).unwrap().data, Some(vec![1, 2, 255]));
        assert_eq!(image(r#"{"data":null}"#).unwrap().data, None);
        assert_eq!(image(r#"{}"#).unwrap().data, None);
        assert!(image(r#"{"data":"not base64!"}"#).is_err());
    }

    #[test]
    fn tokens_resolve_inside_untagged_nodes() {
        let payloads = TransportPayloads {
            bytes: vec![vec![9, 8, 7]],
            text: vec!["data:image/png;base64,CQgH".into()],
        };
        let json = format!(
            r#"{{"type":"paragraph","children":[{{"type":"image","src":"{}0","data":"{}0","partName":"word/media/a.png"}}]}}"#,
            TEXT_TOKEN_PREFIX.replace('\0', "\\u0000"),
            BYTES_TOKEN_PREFIX.replace('\0', "\\u0000"),
        );
        let node: crate::DocNode = with_transport_payloads(payloads, || serde_json::from_str(&json)).unwrap();
        let crate::DocNode::Paragraph(paragraph) = node else { panic!("expected paragraph") };
        let crate::ParagraphChildNode::Image(image) = &paragraph.children[0] else { panic!("expected image") };
        assert_eq!(image.data.as_deref(), Some(&[9, 8, 7][..]));
        assert_eq!(image.src.as_deref(), Some("data:image/png;base64,CQgH"));
    }

    #[test]
    fn tokens_fail_outside_a_scope_or_when_missing() {
        let token = format!(r#"{{"data":"{}0"}}"#, BYTES_TOKEN_PREFIX.replace('\0', "\\u0000"));
        assert!(image(&token).is_err());
        let missing = with_transport_payloads(TransportPayloads::default(), || image(&token));
        assert!(missing.is_err());
        assert!(PAYLOADS.with(|slot| slot.borrow().is_none()));
    }
}

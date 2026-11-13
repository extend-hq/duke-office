//! OpenDocument containers: the ZIP package and the single-file flat XML form.

use std::cell::Cell;
use std::collections::BTreeMap;

use crate::{Limits, OdfError};

const MIME_PREFIX: &str = "application/vnd.oasis.opendocument.";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OdfFamily {
    Text,
    Spreadsheet,
    Presentation,
    Graphics,
    Chart,
    Formula,
    Image,
    Database,
}

/// What an OpenDocument file declares itself to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OdfKind {
    pub family: OdfFamily,
    /// A template (`.ott`, `.otp`, ...). Master documents are plain text documents here.
    pub template: bool,
    /// The single-file XML form (`.fodt`, `.fodp`, ...).
    pub flat: bool,
}

impl OdfKind {
    /// Maps an `application/vnd.oasis.opendocument.*` media type.
    pub fn from_mimetype(mimetype: &str) -> Option<Self> {
        let name = mimetype.trim().strip_prefix(MIME_PREFIX)?;
        let (name, flat) = match name.strip_suffix("-flat-xml") {
            Some(name) => (name, true),
            None => (name, false),
        };
        let (family, template) = match name {
            "text" | "text-master" => (OdfFamily::Text, false),
            "text-template" | "text-master-template" | "text-web" => (OdfFamily::Text, true),
            "spreadsheet" => (OdfFamily::Spreadsheet, false),
            "spreadsheet-template" => (OdfFamily::Spreadsheet, true),
            "presentation" => (OdfFamily::Presentation, false),
            "presentation-template" => (OdfFamily::Presentation, true),
            "graphics" => (OdfFamily::Graphics, false),
            "graphics-template" => (OdfFamily::Graphics, true),
            "chart" => (OdfFamily::Chart, false),
            "chart-template" => (OdfFamily::Chart, true),
            "formula" => (OdfFamily::Formula, false),
            "formula-template" => (OdfFamily::Formula, true),
            "image" => (OdfFamily::Image, false),
            "image-template" => (OdfFamily::Image, true),
            "base" | "database" => (OdfFamily::Database, false),
            _ => return None,
        };
        Some(Self {
            family,
            template,
            flat,
        })
    }
}

/// Identifies an OpenDocument file from its bytes without unpacking it.
///
/// Other ZIP-based formats, including Office Open XML and EPUB, return `None`.
pub fn sniff(bytes: &[u8]) -> Option<OdfKind> {
    if bytes.starts_with(b"PK\x03\x04") {
        if let Some(mimetype) = leading_mimetype(bytes) {
            return OdfKind::from_mimetype(mimetype);
        }
        if local_name(bytes, 0) == Some(b"[Content_Types].xml") {
            return None;
        }
        let zip = Zip::open(bytes, &Limits::default()).ok()?;
        if let Some(entry) = zip.entries.get("mimetype") {
            let data = zip.read_entry(entry, 256).ok()?;
            return OdfKind::from_mimetype(std::str::from_utf8(&data).ok()?);
        }
        if !zip.entries.contains_key("content.xml") {
            return None;
        }
        let entry = zip.entries.get("META-INF/manifest.xml")?;
        let manifest = zip.read_entry(entry, 4 * 1024 * 1024).ok()?;
        return manifest_mimetype(std::str::from_utf8(&manifest).ok()?);
    }
    let head = &bytes[..bytes.len().min(8192)];
    let head = String::from_utf8_lossy(head);
    let trimmed = head.trim_start_matches('\u{feff}').trim_start();
    if !trimmed.starts_with('<') || !head.contains(":document") {
        return None;
    }
    attribute_value(&head, "office:mimetype").and_then(|mimetype| {
        OdfKind::from_mimetype(mimetype).map(|kind| OdfKind { flat: true, ..kind })
    })
}

fn local_name(bytes: &[u8], offset: usize) -> Option<&[u8]> {
    let header = bytes.get(offset..offset.checked_add(30)?)?;
    let name_len = u16::from_le_bytes([header[26], header[27]]) as usize;
    bytes.get(offset + 30..offset + 30 + name_len)
}

/// The `mimetype` entry when it is first and stored, as the standard requires.
fn leading_mimetype(bytes: &[u8]) -> Option<&str> {
    if local_name(bytes, 0)? != b"mimetype" {
        return None;
    }
    let header = bytes.get(..30)?;
    let method = u16::from_le_bytes([header[8], header[9]]);
    let size = u32::from_le_bytes([header[18], header[19], header[20], header[21]]) as usize;
    let extra = u16::from_le_bytes([header[28], header[29]]) as usize;
    if method != 0 || size == 0 || size > 256 {
        return None;
    }
    let start = 38 + extra;
    std::str::from_utf8(bytes.get(start..start + size)?).ok()
}

fn attribute_value<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = xml;
    while let Some(index) = rest.find(name) {
        let after = rest[index + name.len()..].trim_start();
        if let Some(value) = after.strip_prefix('=') {
            let value = value.trim_start();
            let quote = value.chars().next().filter(|ch| *ch == '"' || *ch == '\'')?;
            let value = &value[1..];
            return value.find(quote).map(|end| &value[..end]);
        }
        rest = &rest[index + name.len()..];
    }
    None
}

fn manifest_mimetype(manifest: &str) -> Option<OdfKind> {
    let document = crate::dom::Document::parse(manifest, usize::MAX).ok()?;
    let attribute = |node: crate::dom::Node<'_, '_>, name: &str| {
        node.attributes()
            .iter()
            .find(|attribute| attribute.name == name)
            .map(|attribute| attribute.value.to_string())
    };
    document
        .root_element()
        .children()
        .filter(|node| node.tag_name().name() == "file-entry")
        .find(|node| attribute(*node, "full-path").as_deref() == Some("/"))
        .and_then(|node| attribute(node, "media-type"))
        .and_then(|media_type| OdfKind::from_mimetype(&media_type))
}

struct ZipEntry {
    method: u16,
    flags: u16,
    compressed: usize,
    uncompressed: usize,
    offset: usize,
}

struct Zip<'a> {
    bytes: &'a [u8],
    entries: BTreeMap<String, ZipEntry>,
}

fn u16_at(bytes: &[u8], offset: usize) -> Option<usize> {
    let slice = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]) as usize)
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<usize> {
    let slice = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]) as usize)
}

impl<'a> Zip<'a> {
    fn open(bytes: &'a [u8], limits: &Limits) -> Result<Self, OdfError> {
        let corrupt = || OdfError::Corrupt("ZIP central directory".into());
        let floor = bytes.len().saturating_sub(22 + 65_535);
        let eocd = (floor..=bytes.len().saturating_sub(22))
            .rev()
            .find(|offset| bytes[*offset..].starts_with(b"PK\x05\x06"))
            .ok_or_else(corrupt)?;
        let count = u16_at(bytes, eocd + 10).ok_or_else(corrupt)?;
        let size = u32_at(bytes, eocd + 12).ok_or_else(corrupt)?;
        let start = u32_at(bytes, eocd + 16).ok_or_else(corrupt)?;
        if count == 0xffff || size == 0xffff_ffff || start == 0xffff_ffff {
            return Err(OdfError::Unsupported("ZIP64 packages".into()));
        }
        if count > limits.max_entries {
            return Err(OdfError::Limit("ZIP entry count".into()));
        }
        let mut entries = BTreeMap::new();
        let mut cursor = start;
        for _ in 0..count {
            if bytes.get(cursor..cursor + 4) != Some(b"PK\x01\x02") {
                return Err(corrupt());
            }
            let field16 = |at| u16_at(bytes, cursor + at).ok_or_else(corrupt);
            let field32 = |at| u32_at(bytes, cursor + at).ok_or_else(corrupt);
            let name_len = field16(28)?;
            let extra_len = field16(30)?;
            let comment_len = field16(32)?;
            let name = bytes
                .get(cursor + 46..cursor + 46 + name_len)
                .ok_or_else(corrupt)?;
            let entry = ZipEntry {
                flags: field16(8)? as u16,
                method: field16(10)? as u16,
                compressed: field32(20)?,
                uncompressed: field32(24)?,
                offset: field32(42)?,
            };
            if entry.uncompressed > limits.max_entry_bytes {
                return Err(OdfError::Limit("ZIP entry size".into()));
            }
            entries
                .entry(String::from_utf8_lossy(name).into_owned())
                .or_insert(entry);
            cursor += 46 + name_len + extra_len + comment_len;
        }
        Ok(Self { bytes, entries })
    }

    fn read_entry(&self, entry: &ZipEntry, limit: usize) -> Result<Vec<u8>, OdfError> {
        let corrupt = || OdfError::Corrupt("ZIP entry".into());
        if entry.flags & 1 != 0 {
            return Err(OdfError::Encrypted);
        }
        if entry.uncompressed > limit {
            return Err(OdfError::Limit("ZIP entry size".into()));
        }
        if self.bytes.get(entry.offset..entry.offset + 4) != Some(b"PK\x03\x04") {
            return Err(corrupt());
        }
        let name_len = u16_at(self.bytes, entry.offset + 26).ok_or_else(corrupt)?;
        let extra_len = u16_at(self.bytes, entry.offset + 28).ok_or_else(corrupt)?;
        let start = entry.offset + 30 + name_len + extra_len;
        let data = self
            .bytes
            .get(start..start.checked_add(entry.compressed).ok_or_else(corrupt)?)
            .ok_or_else(corrupt)?;
        match entry.method {
            0 if data.len() == entry.uncompressed => Ok(data.to_vec()),
            8 => {
                let expected = entry.uncompressed;
                // The fast decoder writes into a buffer of the declared size, so
                // it is used only when that size is plausible for the input.
                let plausible = expected <= 64 * 1024 * 1024
                    && expected <= data.len().saturating_mul(512).max(1024 * 1024);
                let fast = plausible
                    .then(|| {
                        let mut output = vec![0; expected];
                        let length = {
                            let config = zlib_rs::InflateConfig { window_bits: -15 };
                            let (decoded, status) = zlib_rs::decompress_slice(&mut output, data, config);
                            (status == zlib_rs::ReturnCode::Ok).then_some(decoded.len())
                        }?;
                        output.truncate(length);
                        Some(output)
                    })
                    .flatten();
                let inflated = match fast {
                    Some(inflated) => inflated,
                    None => miniz_oxide::inflate::decompress_to_vec_with_limit(data, expected)
                        .map_err(|_| corrupt())?,
                };
                if inflated.len() == entry.uncompressed {
                    Ok(inflated)
                } else {
                    Err(corrupt())
                }
            }
            0 => Err(corrupt()),
            method => Err(OdfError::Unsupported(format!(
                "ZIP compression method {method}"
            ))),
        }
    }
}

enum Source<'a> {
    Zip(Zip<'a>),
    Flat(String),
}

/// An opened OpenDocument file. Parts are inflated on request and counted
/// against [`Limits::max_total_uncompressed_bytes`].
pub struct OdfPackage<'a> {
    pub kind: OdfKind,
    source: Source<'a>,
    limits: Limits,
    inflated: Cell<usize>,
}

impl<'a> OdfPackage<'a> {
    pub fn open(bytes: &'a [u8], limits: &Limits) -> Result<Self, OdfError> {
        if bytes.len() > limits.max_input_bytes {
            return Err(OdfError::Limit("input size".into()));
        }
        let kind = sniff(bytes).ok_or(OdfError::NotOdf)?;
        let source = if kind.flat {
            Source::Flat(decode_xml(bytes.to_vec())?)
        } else {
            Source::Zip(Zip::open(bytes, limits)?)
        };
        let package = Self {
            kind,
            source,
            limits: *limits,
            inflated: Cell::new(0),
        };
        if let Some(manifest) = package.xml("META-INF/manifest.xml")? {
            if manifest.contains("encryption-data") {
                return Err(OdfError::Encrypted);
            }
        }
        Ok(package)
    }

    pub fn is_flat(&self) -> bool {
        matches!(self.source, Source::Flat(_))
    }

    /// The whole document of a flat XML file.
    pub fn flat_xml(&self) -> Option<&str> {
        match &self.source {
            Source::Flat(xml) => Some(xml),
            Source::Zip(_) => None,
        }
    }

    pub fn contains(&self, path: &str) -> bool {
        match &self.source {
            Source::Zip(zip) => zip.entries.contains_key(path),
            Source::Flat(_) => false,
        }
    }

    /// Reads a package entry. Flat XML files have none.
    pub fn file(&self, path: &str) -> Result<Option<Vec<u8>>, OdfError> {
        let Source::Zip(zip) = &self.source else {
            return Ok(None);
        };
        let Some(entry) = zip.entries.get(path) else {
            return Ok(None);
        };
        let total = self.inflated.get().saturating_add(entry.uncompressed);
        if total > self.limits.max_total_uncompressed_bytes {
            return Err(OdfError::Limit("total uncompressed size".into()));
        }
        let data = zip.read_entry(entry, self.limits.max_entry_bytes)?;
        self.inflated.set(total);
        Ok(Some(data))
    }

    /// Reads a package entry as UTF-8 XML text.
    pub fn xml(&self, path: &str) -> Result<Option<String>, OdfError> {
        self.file(path)?.map(decode_xml).transpose()
    }
}

fn decode_xml(bytes: Vec<u8>) -> Result<String, OdfError> {
    let mut text = String::from_utf8(bytes)
        .map_err(|_| OdfError::Unsupported("XML that is not UTF-8".into()))?;
    if text.starts_with('\u{feff}') {
        text.drain(..'\u{feff}'.len_utf8());
    }
    blank_external_doctype(&mut text);
    Ok(text)
}

/// Blanks a document type declaration that has no internal subset, as older
/// formula documents carry. It only names an external DTD, which is never
/// fetched, and defines no entities. A declaration with an internal subset is
/// left in place for the XML parser to reject.
fn blank_external_doctype(text: &mut String) {
    // A declaration can only come before the root element, so only the start
    // of the part is searched, not megabytes of content.
    let mut head = text.len().min(8192);
    while !text.is_char_boundary(head) {
        head -= 1;
    }
    let Some(start) = text[..head].find("<!DOCTYPE") else {
        return;
    };
    // It must be in the prolog: only declarations, comments and processing
    // instructions may come before it.
    let bytes = text.as_bytes();
    if text[..start]
        .match_indices('<')
        .any(|(index, _)| !matches!(bytes.get(index + 1), Some(b'?' | b'!')))
    {
        return;
    }
    let Some(length) = text[start..].find('>').map(|end| end + 1) else {
        return;
    };
    if text[start..start + length].contains('[') {
        return;
    }
    text.replace_range(start..start + length, &" ".repeat(length));
}

/// Resolves a package-relative reference such as `./Object 1` or
/// `Pictures/a.png`. External and absolute references return `None`.
pub fn internal_path(href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty() || href.starts_with('#') || href.starts_with('/') || href.contains("://") {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in href.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Builds a stored (uncompressed) ZIP for tests.
    pub(crate) fn stored_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data) in files {
            let offset = out.len() as u32;
            let mut header = Vec::new();
            header.extend_from_slice(&[0x14, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            header.extend_from_slice(&0u32.to_le_bytes());
            header.extend_from_slice(&(data.len() as u32).to_le_bytes());
            header.extend_from_slice(&(data.len() as u32).to_le_bytes());
            header.extend_from_slice(&(name.len() as u16).to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(b"PK\x03\x04");
            out.extend_from_slice(&header);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(data);
            central.extend_from_slice(b"PK\x01\x02");
            central.extend_from_slice(&[0x14, 0]);
            central.extend_from_slice(&header);
            central.extend_from_slice(&[0; 10]);
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let start = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&start.to_le_bytes());
        out.extend_from_slice(&[0; 2]);
        out
    }

    #[test]
    fn maps_every_registered_media_type() {
        let kind = |name: &str| OdfKind::from_mimetype(&format!("{MIME_PREFIX}{name}"));
        assert_eq!(kind("text").map(|kind| kind.family), Some(OdfFamily::Text));
        assert!(kind("text-master-template").unwrap().template);
        assert!(kind("presentation-flat-xml").unwrap().flat);
        assert_eq!(
            kind("graphics-template").map(|kind| kind.family),
            Some(OdfFamily::Graphics)
        );
        assert_eq!(kind("base").map(|kind| kind.family), Some(OdfFamily::Database));
        assert_eq!(kind("unknown"), None);
        assert_eq!(OdfKind::from_mimetype("application/epub+zip"), None);
    }

    #[test]
    fn sniffs_packages_without_mistaking_other_zips() {
        let odt = stored_zip(&[
            ("mimetype", b"application/vnd.oasis.opendocument.text"),
            ("content.xml", b"<x/>"),
        ]);
        assert_eq!(sniff(&odt).map(|kind| kind.family), Some(OdfFamily::Text));
        let late = stored_zip(&[
            ("content.xml", b"<x/>"),
            ("mimetype", b"application/vnd.oasis.opendocument.presentation"),
        ]);
        assert_eq!(
            sniff(&late).map(|kind| kind.family),
            Some(OdfFamily::Presentation)
        );
        let docx = stored_zip(&[("[Content_Types].xml", b"<Types/>")]);
        assert_eq!(sniff(&docx), None);
        let epub = stored_zip(&[("mimetype", b"application/epub+zip")]);
        assert_eq!(sniff(&epub), None);
        assert_eq!(sniff(b"\xd0\xcf\x11\xe0"), None);
    }

    #[test]
    fn sniffs_flat_xml_and_reads_entries() {
        let flat = br#"<?xml version="1.0"?><office:document xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" office:mimetype="application/vnd.oasis.opendocument.text"/>"#;
        let kind = sniff(flat).unwrap();
        assert!(kind.flat && kind.family == OdfFamily::Text);
        assert_eq!(sniff(b"<html><body>office:document</body></html>"), None);

        let odt = stored_zip(&[
            ("mimetype", b"application/vnd.oasis.opendocument.text"),
            ("content.xml", b"\xef\xbb\xbf<x/>"),
        ]);
        let package = OdfPackage::open(&odt, &Limits::default()).unwrap();
        assert_eq!(package.xml("content.xml").unwrap().as_deref(), Some("<x/>"));
        assert_eq!(package.file("missing").unwrap(), None);
    }

    #[test]
    fn drops_only_document_types_without_an_internal_subset() {
        let external = r#"<?xml version="1.0"?><!DOCTYPE math PUBLIC "-//x//EN" "math.dtd"><math/>"#;
        let decoded = decode_xml(external.as_bytes().to_vec()).unwrap();
        assert_eq!(decoded.len(), external.len());
        assert!(!decoded.contains("DOCTYPE") && decoded.ends_with("<math/>"));
        assert!(crate::dom::Document::parse(&decoded, usize::MAX).is_ok());

        let internal = r#"<!DOCTYPE a [<!ENTITY e "x">]><a>&e;</a>"#;
        assert_eq!(decode_xml(internal.as_bytes().to_vec()).unwrap(), internal);
        let in_content = "<a>&lt;!DOCTYPE b&gt;<!DOCTYPE b></a>";
        assert_eq!(decode_xml(in_content.as_bytes().to_vec()).unwrap(), in_content);
    }

    #[test]
    fn inflates_deflated_entries() {
        let content = "<office:document-content>".repeat(200);
        let deflated = miniz_oxide::deflate::compress_to_vec(content.as_bytes(), 6);
        let mut zip = stored_zip(&[
            ("mimetype", b"application/vnd.oasis.opendocument.text"),
            ("content.xml", &deflated),
        ]);
        // Mark the second entry as deflated and record its real size in both headers.
        let size = (content.len() as u32).to_le_bytes();
        let local = zip.windows(11).position(|window| window == b"content.xml").unwrap() - 30;
        let central = zip.windows(11).rposition(|window| window == b"content.xml").unwrap() - 46;
        zip[local + 8] = 8;
        zip[local + 22..local + 26].copy_from_slice(&size);
        zip[central + 10] = 8;
        zip[central + 24..central + 28].copy_from_slice(&size);
        let package = OdfPackage::open(&zip, &Limits::default()).unwrap();
        assert_eq!(package.xml("content.xml").unwrap().as_deref(), Some(content.as_str()));

        // A declared size that does not match the data is an error, not a short read.
        zip[central + 24..central + 28].copy_from_slice(&(content.len() as u32 + 1).to_le_bytes());
        let package = OdfPackage::open(&zip, &Limits::default()).unwrap();
        assert!(matches!(package.xml("content.xml"), Err(OdfError::Corrupt(_))));
    }

    #[test]
    fn rejects_encrypted_and_oversized_packages() {
        let encrypted = stored_zip(&[
            ("mimetype", b"application/vnd.oasis.opendocument.text"),
            (
                "META-INF/manifest.xml",
                b"<manifest:manifest><manifest:encryption-data/></manifest:manifest>",
            ),
        ]);
        assert_eq!(
            OdfPackage::open(&encrypted, &Limits::default()).err(),
            Some(OdfError::Encrypted)
        );
        let odt = stored_zip(&[
            ("mimetype", b"application/vnd.oasis.opendocument.text"),
            ("content.xml", b"<x/>"),
        ]);
        let limits = Limits {
            max_total_uncompressed_bytes: 2,
            ..Limits::default()
        };
        let package = OdfPackage::open(&odt, &limits).unwrap();
        assert!(matches!(package.xml("content.xml"), Err(OdfError::Limit(_))));
    }

    #[test]
    fn resolves_only_internal_references() {
        assert_eq!(internal_path("./Object 1").as_deref(), Some("Object 1"));
        assert_eq!(
            internal_path("Pictures/../Pictures/a.png").as_deref(),
            Some("Pictures/a.png")
        );
        assert_eq!(internal_path("https://example.com/a.png"), None);
        assert_eq!(internal_path("../outside.png"), None);
        assert_eq!(internal_path("#anchor"), None);
    }
}

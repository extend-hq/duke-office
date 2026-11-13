//! Identifying embedded images.

/// A file extension for image bytes, from their signature or, failing that,
/// from the name they were stored under.
pub fn image_extension(bytes: &[u8], name: &str) -> Option<&'static str> {
    let head = &bytes[..bytes.len().min(512)];
    if head.starts_with(b"\x89PNG") {
        Some("png")
    } else if head.starts_with(b"\xff\xd8\xff") {
        Some("jpeg")
    } else if head.starts_with(b"GIF8") {
        Some("gif")
    } else if head.starts_with(b"BM") {
        Some("bmp")
    } else if head.starts_with(b"II*\0") || head.starts_with(b"MM\0*") {
        Some("tiff")
    } else if head.get(40..44) == Some(b" EMF") {
        Some("emf")
    } else if head.starts_with(b"\xd7\xcd\xc6\x9a") || head.starts_with(b"\x01\x00\x09\x00") {
        Some("wmf")
    } else if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP") {
        Some("webp")
    } else if String::from_utf8_lossy(head).contains("<svg") {
        Some("svg")
    } else {
        match name.rsplit('.').next().map(str::to_ascii_lowercase).as_deref() {
            Some("png") => Some("png"),
            Some("jpg" | "jpeg") => Some("jpeg"),
            Some("gif") => Some("gif"),
            Some("bmp") => Some("bmp"),
            Some("tif" | "tiff") => Some("tiff"),
            Some("emf") => Some("emf"),
            Some("wmf") => Some("wmf"),
            Some("svg") => Some("svg"),
            Some("webp") => Some("webp"),
            _ => None,
        }
    }
}

/// The media type for an extension returned by [`image_extension`].
pub fn image_content_type(extension: &str) -> &'static str {
    match extension {
        "png" => "image/png",
        "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tiff" => "image/tiff",
        "emf" => "image/x-emf",
        "wmf" => "image/x-wmf",
        "webp" => "image/webp",
        _ => "image/svg+xml",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_the_signature_over_the_name() {
        assert_eq!(image_extension(b"\x89PNG\r\n\x1a\n", "picture.jpg"), Some("png"));
        assert_eq!(image_extension(b"\xff\xd8\xff\xe0", "picture"), Some("jpeg"));
        assert_eq!(image_extension(b"<?xml version=\"1.0\"?><svg/>", "a"), Some("svg"));
        assert_eq!(image_extension(b"unknown", "Pictures/a.TIF"), Some("tiff"));
        assert_eq!(image_extension(b"unknown", "ObjectReplacements/Object 1"), None);
        assert_eq!(image_content_type("emf"), "image/x-emf");
    }
}

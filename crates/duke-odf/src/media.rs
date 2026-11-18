//! Identifying embedded images.

/// Identifies HEIF-family stills and sequences from the major and compatible brands.
pub fn heif_image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.get(4..8)? != b"ftyp" {
        return None;
    }
    let (header, size) = match u32::from_be_bytes(bytes[..4].try_into().ok()?) {
        0 => (8, bytes.len()),
        1 => (
            16,
            usize::try_from(u64::from_be_bytes(bytes.get(8..16)?.try_into().ok()?)).ok()?,
        ),
        size => (8, size as usize),
    };
    let payload = bytes.get(header..size)?;
    let compatible = payload.get(8..)?;
    if !compatible.len().is_multiple_of(4) {
        return None;
    }
    let mut avif = false;
    let mut hevc = false;
    let mut heif = false;
    let mut sequence = false;
    for brand in std::iter::once(&payload[..4]).chain(compatible.chunks_exact(4)) {
        match brand {
            b"avif" => avif = true,
            b"avis" => {
                avif = true;
                sequence = true;
            }
            b"heic" | b"heix" | b"heim" | b"heis" => hevc = true,
            b"hevc" | b"hevx" | b"hevm" | b"hevs" => {
                hevc = true;
                sequence = true;
            }
            b"mif1" | b"mif2" => heif = true,
            b"msf1" => {
                heif = true;
                sequence = true;
            }
            _ => {}
        }
    }
    if avif {
        Some(if sequence { "avifs" } else { "avif" })
    } else if hevc {
        Some(if sequence { "heics" } else { "heic" })
    } else if heif {
        Some(if sequence { "heifs" } else { "heif" })
    } else {
        None
    }
}

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
    } else if let Some(extension) = heif_image_extension(bytes) {
        Some(extension)
    } else if head.get(40..44) == Some(b" EMF") {
        Some("emf")
    } else if head.starts_with(b"\xd7\xcd\xc6\x9a") || head.starts_with(b"\x01\x00\x09\x00") {
        Some("wmf")
    } else if head.starts_with(b"VCLMTF") {
        Some("svm")
    } else if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP") {
        Some("webp")
    } else if String::from_utf8_lossy(head).contains("<svg") {
        Some("svg")
    } else {
        match name
            .rsplit('.')
            .next()
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("png") => Some("png"),
            Some("jpg" | "jpeg") => Some("jpeg"),
            Some("gif") => Some("gif"),
            Some("bmp") => Some("bmp"),
            Some("tif" | "tiff") => Some("tiff"),
            Some("emf") => Some("emf"),
            Some("wmf") => Some("wmf"),
            Some("svg") => Some("svg"),
            Some("svm") => Some("svm"),
            Some("webp") => Some("webp"),
            Some("heic") => Some("heic"),
            Some("heics") => Some("heics"),
            Some("heif" | "hif") => Some("heif"),
            Some("heifs") => Some("heifs"),
            Some("avif") => Some("avif"),
            Some("avifs") => Some("avifs"),
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
        "svm" => "application/x-openoffice-gdimetafile",
        "webp" => "image/webp",
        "heic" => "image/heic",
        "heics" => "image/heic-sequence",
        "heif" => "image/heif",
        "heifs" => "image/heif-sequence",
        "avif" | "avifs" => "image/avif",
        _ => "image/svg+xml",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_the_signature_over_the_name() {
        assert_eq!(
            image_extension(b"\x89PNG\r\n\x1a\n", "picture.jpg"),
            Some("png")
        );
        assert_eq!(
            image_extension(b"\xff\xd8\xff\xe0", "picture"),
            Some("jpeg")
        );
        assert_eq!(
            image_extension(b"<?xml version=\"1.0\"?><svg/>", "a"),
            Some("svg")
        );
        assert_eq!(image_extension(b"unknown", "Pictures/a.TIF"), Some("tiff"));
        assert_eq!(
            image_extension(b"unknown", "ObjectReplacements/Object 1"),
            None
        );
        assert_eq!(image_content_type("emf"), "image/x-emf");
    }

    #[test]
    fn resolves_image_brands_within_the_file_type_box() {
        assert_eq!(
            image_extension(b"\0\0\0\x14ftypmif1\0\0\0\0avif", "a.heic"),
            Some("avif")
        );
        assert_eq!(
            heif_image_extension(b"\0\0\0\x18ftypheic\0\0\0\0mif1msf1"),
            Some("heics")
        );
        assert_eq!(heif_image_extension(b"\0\0\0\x10ftypisomavifheic"), None);
        assert_eq!(heif_image_extension(b"\0\0\0\x18ftypavif\0\0\0\0"), None);
        assert_eq!(
            heif_image_extension(b"\0\0\0\x01ftyp\0\0\0\0\0\0\0\x18msf1\0\0\0\0"),
            Some("heifs")
        );
        assert_eq!(image_extension(b"", "Pictures/a.HIF"), Some("heif"));
        assert_eq!(image_content_type("heics"), "image/heic-sequence");
        assert_eq!(image_content_type("avifs"), "image/avif");
    }
}

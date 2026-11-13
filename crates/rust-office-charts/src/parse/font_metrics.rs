const CARLITO: &[u8] = include_bytes!("font_metrics/carlito.bin");

fn u16_at(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap())
}

fn codepoint(data: &[u8], index: usize) -> u32 {
    let offset = 4 + index * 4;
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

pub(super) fn width(text: &str, size: f64, family: &str, bold: bool, italic: bool) -> Option<f64> {
    let primary = family.split(',').next()?.trim().trim_matches(['\'', '"']);
    let data: &[u8] = match primary.to_ascii_lowercase().as_str() {
        "calibri" | "carlito" => CARLITO,
        "arial" | "arimo" => include_bytes!("font_metrics/arimo.bin"),
        "times new roman" | "tinos" => include_bytes!("font_metrics/tinos.bin"),
        "courier new" | "cousine" => include_bytes!("font_metrics/cousine.bin"),
        _ => return None,
    };
    let count = usize::from(u16_at(data, 2));
    let style = usize::from(bold) + 2 * usize::from(italic);
    let start = 4 + count * 4 + style * count * 2;
    let mut advance = 0_u64;
    for character in text.chars() {
        let cp = character as u32;
        if (32..=126).contains(&cp) {
            let index = (cp - 30) as usize;
            if index < count && codepoint(data, index) == cp {
                advance += u64::from(u16_at(data, start + index * 2));
                continue;
            }
        }
        let (mut low, mut high) = (0, count);
        while low < high {
            let mid = (low + high) / 2;
            if codepoint(data, mid) < cp {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        if low == count || codepoint(data, low) != cp {
            return None;
        }
        advance += u64::from(u16_at(data, start + low * 2));
    }
    Some(advance as f64 * size / f64::from(u16_at(data, 0)))
}

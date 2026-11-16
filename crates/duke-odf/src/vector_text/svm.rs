//! Text-bearing records in the versioned StarView drawing stream.

use super::{Collector, Matrix, VectorText, MAX_RECORDS, MAX_TEXT};

struct Bytes<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Bytes<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Option<&'a [u8]> {
        let end = self.offset.checked_add(length)?;
        let bytes = self.data.get(self.offset..end)?;
        self.offset = end;
        Some(bytes)
    }

    fn word(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    fn dword(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn point(&mut self) -> Option<(f64, f64)> {
        Some((self.dword()? as i32 as f64, self.dword()? as i32 as f64))
    }

    fn envelope(&mut self) -> Option<(u16, Bytes<'a>)> {
        let version = self.word()?;
        let size = self.dword()? as usize;
        (version != 0).then_some(())?;
        Some((version, Bytes::new(self.take(size)?)))
    }

    fn counted_bytes(&mut self) -> Option<&'a [u8]> {
        let length = self.word()? as usize;
        self.take(length)
    }

    fn counted_unicode(&mut self) -> Option<&'a [u8]> {
        let length = self.word()? as usize;
        self.take(length.checked_mul(2)?)
    }

    fn text(&mut self, encoding: u16) -> Option<(&'a [u8], bool)> {
        if encoding == 65535 {
            let length = self.dword()? as usize;
            if length > MAX_TEXT {
                return None;
            }
            Some((self.take(length.checked_mul(2)?)?, true))
        } else {
            Some((self.counted_bytes()?, false))
        }
    }
}

fn decode(bytes: &[u8], encoding: u16) -> Option<String> {
    use encoding_rs::*;
    if matches!(encoding, 0 | 11) {
        return bytes
            .is_ascii()
            .then(|| String::from_utf8_lossy(bytes).into_owned());
    }
    if encoding == 12 {
        return Some(bytes.iter().map(|b| char::from(*b)).collect());
    }
    let encoding = match encoding {
        1 => WINDOWS_1252,
        2 => MACINTOSH,
        13 => ISO_8859_2,
        14 => ISO_8859_3,
        15 => ISO_8859_4,
        16 => ISO_8859_5,
        17 => ISO_8859_6,
        18 => ISO_8859_7,
        19 => ISO_8859_8,
        20 | 36 => WINDOWS_1254,
        21 => ISO_8859_14,
        22 => ISO_8859_15,
        33 => WINDOWS_1250,
        34 => WINDOWS_1251,
        35 => WINDOWS_1253,
        37 => WINDOWS_1255,
        38 => WINDOWS_1256,
        39 => WINDOWS_1257,
        40 => WINDOWS_1258,
        60 | 64 => SHIFT_JIS,
        61 | 65 | 67 | 70 => GBK,
        62 | 79 => EUC_KR,
        63 | 68 => BIG5,
        69 => EUC_JP,
        72 => ISO_2022_JP,
        74 => KOI8_R,
        76 => UTF_8,
        77 => ISO_8859_10,
        78 => ISO_8859_13,
        85 => GB18030,
        87 => WINDOWS_874,
        88 => KOI8_U,
        _ => return None,
    };
    let (text, errors) = encoding.decode_without_bom_handling(bytes);
    (!errors).then(|| text.into_owned())
}

fn substring(
    bytes: &[u8],
    unicode: bool,
    encoding: u16,
    range: Option<(usize, usize)>,
) -> Option<String> {
    let select = |units: &[u16]| {
        let (start, length) = range.unwrap_or((0, units.len()));
        String::from_utf16(units.get(start..start.checked_add(length)?)?).ok()
    };
    if unicode {
        let (start, length) = range.unwrap_or((0, bytes.len() / 2));
        let slice = bytes.get(start.checked_mul(2)?..start.checked_add(length)?.checked_mul(2)?)?;
        super::utf16(slice)
    } else {
        select(&decode(bytes, encoding)?.encode_utf16().collect::<Vec<_>>())
    }
}

struct Mapping {
    matrix: Matrix,
    relative: bool,
}

struct Continuation<'a> {
    bytes: &'a [u8],
    unicode: bool,
    encoding: u16,
    end: usize,
    span: usize,
}

fn mapping(input: &mut Bytes<'_>) -> Option<Mapping> {
    let (_, mut data) = input.envelope()?;
    let unit = data.word()?;
    let (x, y) = data.point()?;
    let mut fractions = [0.; 2];
    for value in &mut fractions {
        let numerator = data.dword()? as i32;
        let denominator = data.dword()? as i32;
        if denominator == 0 {
            return None;
        }
        *value = numerator as f64 / denominator as f64;
    }
    let simple = data.take(1)?[0] != 0;
    let units_per_inch = match unit {
        0 => 2540.,
        1 => 254.,
        2 => 25.4,
        3 => 2.54,
        4 => 1000.,
        5 => 100.,
        6 => 10.,
        7 | 13 => 1.,
        8 => 72.,
        9 => 1440.,
        10 => 96.,
        _ => return None,
    };
    let (sx, sy, x, y) = if simple {
        (1., 1., 0., 0.)
    } else {
        (fractions[0], fractions[1], x, y)
    };
    let (sx, sy) = (sx / units_per_inch, sy / units_per_inch);
    Some(Mapping {
        matrix: Matrix([sx, 0., 0., sy, x * sx, y * sy]),
        relative: unit == 13,
    })
}

fn text_record<'a>(
    kind: u16,
    version: u16,
    data: &mut Bytes<'a>,
    encoding: u16,
    position: Matrix,
    out: &mut Collector,
    continuation: &mut Option<Continuation<'a>>,
) -> Option<()> {
    let (x, y) = data.point()?;
    if kind == 115 {
        data.point()?;
    }
    let (mut bytes, mut unicode) = data.text(encoding)?;
    if kind == 114 {
        data.dword()?;
    }
    let range = if kind == 115 {
        data.word()?;
        None
    } else {
        Some((data.word()? as usize, data.word()? as usize))
    };
    if kind == 113 {
        let advances = data.dword()? as usize;
        data.take(advances.checked_mul(4)?)?;
    }
    if version >= 2 {
        bytes = data.counted_unicode()?;
        unicode = true;
    }
    if kind == 113 && version >= 3 {
        let kashidas = data.dword()? as usize;
        data.take(kashidas)?;
    }
    if kind == 113 && version >= 4 && data.take(1)?[0] != 0 {
        data.take(4)?;
    }
    if let Some(text) = substring(bytes, unicode, encoding, range) {
        if let (Some(previous), Some((start, length))) = (continuation.as_mut(), range) {
            if length != 0
                && start == previous.end
                && unicode == previous.unicode
                && (unicode || encoding == previous.encoding)
                && bytes == previous.bytes
            {
                out.bytes = out.bytes.checked_add(text.len())?;
                if out.bytes > MAX_TEXT {
                    return None;
                }
                out.spans[previous.span].2.push_str(&text);
                out.spans.truncate(previous.span + 1);
                previous.end = start.checked_add(length)?;
                return Some(());
            }
        }
        let wrap_hyphen =
            continuation.is_some() && matches!(text.as_str(), "-" | "\u{00ad}" | "\u{2010}");
        let (x, y) = position.point(x, y);
        let span = out.spans.len();
        out.push(x, y, text)?;
        if !wrap_hyphen {
            *continuation = None;
            if let Some((0, length)) = range.filter(|(_, length)| *length != 0) {
                let full_length = if unicode {
                    Some(bytes.len() / 2)
                } else {
                    decode(bytes, encoding).map(|s| s.encode_utf16().count())
                };
                if out.spans.len() > span && full_length.is_some_and(|n| length < n) {
                    *continuation = Some(Continuation {
                        bytes,
                        unicode,
                        encoding,
                        end: length,
                        span,
                    });
                }
            }
        }
    } else {
        out.partial = true;
        *continuation = None;
    }
    Some(())
}

fn drawing(
    input: &mut Bytes<'_>,
    out: &mut Collector,
    remaining: &mut usize,
    depth: usize,
) -> Option<[f64; 4]> {
    if depth > 32 || input.take(6)? != b"VCLMTF" {
        return None;
    }
    let (_, mut header) = input.envelope()?;
    header.dword()?;
    let preferred = mapping(&mut header)?;
    let (width, height) = header.point()?;
    let count = header.dword()? as usize;
    *remaining = remaining.checked_sub(count)?;
    let mut position = preferred.matrix;
    let mut stack = Vec::new();
    // Stream encoding follows font records, independently of drawing-state restoration.
    let mut encoding = 11;
    let mut continuation = None;
    for _ in 0..count {
        let kind = input.word()?;
        if kind == 0 {
            continue;
        }
        let (version, mut data) = input.envelope()?;
        match kind {
            112..=115 => text_record(
                kind,
                version,
                &mut data,
                encoding,
                position,
                out,
                &mut continuation,
            )?,
            137 => {
                let map = mapping(&mut data)?;
                position = if map.relative {
                    position.then(map.matrix)
                } else {
                    map.matrix
                };
                if !position.0.iter().all(|v| v.is_finite()) {
                    return None;
                }
            }
            138 => {
                let (_, mut font) = data.envelope()?;
                font.counted_bytes()?;
                font.counted_bytes()?;
                font.point()?;
                encoding = font.word()?;
            }
            139 => {
                if stack.len() >= 256 {
                    return None;
                }
                stack.push((data.word()?, position));
            }
            140 => {
                let (flags, saved) = stack.pop()?;
                if flags & 0x10 != 0 {
                    position = saved;
                }
            }
            147 => {
                continuation = None;
                let start = out.spans.len();
                let bounds = drawing(&mut data, out, remaining, depth + 1)?;
                let (x, y) = data.point()?;
                let (w, h) = data.point()?;
                let sw = bounds[2] - bounds[0];
                let sh = bounds[3] - bounds[1];
                if sw == 0. || sh == 0. {
                    return None;
                }
                let placement = position.then(Matrix([
                    w / sw,
                    0.,
                    0.,
                    h / sh,
                    x - bounds[0] * w / sw,
                    y - bounds[1] * h / sh,
                ]));
                for (y, x, _) in &mut out.spans[start..] {
                    (*x, *y) = placement.point(*x, *y);
                    if !x.is_finite() || !y.is_finite() {
                        return None;
                    }
                }
                out.partial = true;
            }
            512 => {
                let name = data.counted_bytes()?;
                data.dword()?;
                let size = data.dword()? as usize;
                data.take(size)?;
                if matches!(
                    name,
                    b"XTEXT_PAINTSHAPE_BEGIN" | b"XTEXT_PAINTSHAPE_END" | b"XTEXT_EOP"
                ) {
                    continuation = None;
                }
            }
            128..=131 => out.partial = true,
            143 => {
                continuation = None;
                out.partial = true;
            }
            _ => {}
        }
    }
    let (left, top) = preferred.matrix.point(0., 0.);
    let (right, bottom) = preferred.matrix.point(width, height);
    Some([
        left.min(right),
        top.min(bottom),
        left.max(right),
        top.max(bottom),
    ])
}

pub(super) fn extract(bytes: &[u8], clip: Option<[f64; 4]>) -> Option<VectorText> {
    let mut out = Collector::default();
    let mut remaining = MAX_RECORDS;
    let bounds = drawing(&mut Bytes::new(bytes), &mut out, &mut remaining, 0)?;
    Some(out.finish(Some(bounds), clip))
}

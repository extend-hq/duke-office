//! Bounded text recovery from vector drawing records; no raster recognition.

use std::collections::{BTreeMap, BTreeSet};

mod svm;

const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_TEXT: usize = 1024 * 1024;
const MAX_RECORDS: usize = 250_000;

#[derive(Clone, Debug, Default)]
pub struct VectorText {
    pub text: String,
    /// Some drawing operations could not be interpreted as Unicode text.
    pub partial: bool,
}

#[derive(Default)]
struct Collector {
    spans: Vec<(f64, f64, String)>,
    bytes: usize,
    partial: bool,
}

impl Collector {
    fn push(&mut self, x: f64, y: f64, text: String) -> Option<()> {
        if !x.is_finite() || !y.is_finite() {
            self.partial = true;
            return Some(());
        }
        let text = text.trim_matches('\0').to_owned();
        if text.trim().is_empty() {
            return Some(());
        }
        self.bytes = self.bytes.checked_add(text.len() + 1)?;
        if self.bytes > MAX_TEXT || self.spans.len() >= MAX_RECORDS {
            return None;
        }
        self.spans.push((y, x, text));
        Some(())
    }

    fn finish(mut self, bounds: Option<[f64; 4]>, clip: Option<[f64; 4]>) -> VectorText {
        if let (Some([left, top, right, bottom]), Some([x0, y0, x1, y1])) = (bounds, clip) {
            let width = right - left;
            let height = bottom - top;
            if width > 0. && height > 0. {
                self.spans.retain(|(y, x, _)| {
                    *x >= left + x0 * width
                        && *x <= left + x1 * width
                        && *y >= top + y0 * height
                        && *y <= top + y1 * height
                });
            }
        }
        self.spans
            .sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        VectorText {
            text: self
                .spans
                .into_iter()
                .map(|(_, _, text)| text)
                .collect::<Vec<_>>()
                .join("\n"),
            partial: self.partial,
        }
    }
}

fn u16_at(b: &[u8], n: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        b.get(n..n.checked_add(2)?)?.try_into().ok()?,
    ))
}
fn i16_at(b: &[u8], n: usize) -> Option<i16> {
    u16_at(b, n).map(|n| n as i16)
}
fn u32_at(b: &[u8], n: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        b.get(n..n.checked_add(4)?)?.try_into().ok()?,
    ))
}
fn i32_at(b: &[u8], n: usize) -> Option<i32> {
    u32_at(b, n).map(|n| n as i32)
}
fn float_at(b: &[u8], n: usize) -> Option<f64> {
    let n = f32::from_bits(u32_at(b, n)?) as f64;
    n.is_finite().then_some(n)
}

fn ansi(bytes: &[u8], charset: u8) -> Option<String> {
    use encoding_rs::*;
    let encoding = match charset {
        0 | 1 => WINDOWS_1252,
        128 => SHIFT_JIS,
        129 => EUC_KR,
        134 => GBK,
        136 => BIG5,
        161 => WINDOWS_1253,
        162 => WINDOWS_1254,
        163 => WINDOWS_1258,
        177 => WINDOWS_1255,
        178 => WINDOWS_1256,
        186 => WINDOWS_1257,
        204 => WINDOWS_1251,
        222 => WINDOWS_874,
        238 => WINDOWS_1250,
        _ => return None,
    };
    let (text, errors) = encoding.decode_without_bom_handling(bytes);
    (!errors).then(|| text.into_owned())
}

fn utf16(bytes: &[u8]) -> Option<String> {
    if bytes.len() % 2 != 0 || bytes.len() > MAX_TEXT * 2 {
        return None;
    }
    String::from_utf16(
        &bytes
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect::<Vec<_>>(),
    )
    .ok()
}

#[derive(Clone, Copy)]
struct Matrix([f64; 6]);
impl Default for Matrix {
    fn default() -> Self {
        Self([1., 0., 0., 1., 0., 0.])
    }
}
impl Matrix {
    fn point(self, x: f64, y: f64) -> (f64, f64) {
        let [a, b, c, d, e, f] = self.0;
        (a * x + c * y + e, b * x + d * y + f)
    }
    fn then(self, rhs: Self) -> Self {
        let [a, b, c, d, e, f] = self.0;
        let [g, h, i, j, k, l] = rhs.0;
        Self([
            a * g + c * h,
            b * g + d * h,
            a * i + c * j,
            b * i + d * j,
            a * k + c * l + e,
            b * k + d * l + f,
        ])
    }
}

#[derive(Clone, Copy, Default)]
struct State {
    charset: u8,
    world: Matrix,
    window: (f64, f64),
    viewport: (f64, f64),
    extent: Option<(f64, f64)>,
    viewport_extent: Option<(f64, f64)>,
}
impl State {
    fn point(self, x: f64, y: f64) -> (f64, f64) {
        let (x, y) = self.world.point(x, y);
        let (sx, sy) = match (self.extent, self.viewport_extent) {
            (Some((wx, wy)), Some((vx, vy))) if wx != 0. && wy != 0. => (vx / wx, vy / wy),
            (Some((wx, wy)), None) => (
                if wx < 0. { -1. } else { 1. },
                if wy < 0. { -1. } else { 1. },
            ),
            _ => (1., 1.),
        };
        (
            (x - self.window.0) * sx + self.viewport.0,
            (y - self.window.1) * sy + self.viewport.1,
        )
    }
}

fn restore(stack: &mut Vec<State>, state: &mut State, level: i32) -> Option<()> {
    let index = if level < 0 {
        stack.len().checked_sub(level.unsigned_abs() as usize)?
    } else {
        (level as usize).checked_sub(1)?
    };
    *state = *stack.get(index)?;
    stack.truncate(index);
    Some(())
}

/// Recover stored text from WMF, EMF, SVM or SVG. Unsupported or malformed inputs
/// return `None`; glyph indices are never treated as character codes.
pub fn extract(bytes: &[u8], name: &str) -> Option<VectorText> {
    extract_clipped(bytes, name, None)
}

/// Limit recovered text origins to a normalized visible region of the image.
pub fn extract_clipped(bytes: &[u8], name: &str, clip: Option<[f64; 4]>) -> Option<VectorText> {
    if bytes.len() > MAX_BYTES || clip.is_some_and(|c| !c.iter().all(|n| n.is_finite())) {
        return None;
    }
    match crate::media::image_extension(bytes, name)? {
        "wmf" => wmf(bytes, clip),
        "emf" => emf(bytes, clip),
        "svg" => svg_clipped(std::str::from_utf8(bytes).ok()?, clip),
        "svm" => svm::extract(bytes, clip),
        _ => None,
    }
}

fn wmf(bytes: &[u8], clip: Option<[f64; 4]>) -> Option<VectorText> {
    let header = if bytes.starts_with(b"\xd7\xcd\xc6\x9a") {
        22
    } else {
        0
    };
    if !matches!(u16_at(bytes, header)?, 1 | 2) || u16_at(bytes, header + 2)? != 9 {
        return None;
    }
    let end = header.checked_add((u32_at(bytes, header + 6)? as usize).checked_mul(2)?)?;
    let bytes = bytes.get(..end)?;
    let mut pos = header + 18;
    let mut objects: Vec<Option<Option<u8>>> = Vec::new();
    let mut free_objects = BTreeSet::new();
    let mut state = State::default();
    let mut stack = Vec::new();
    let mut out = Collector::default();
    let mut embedded = Vec::new();
    let mut embedded_total = None;
    for _ in 0..MAX_RECORDS {
        let size = (u32_at(bytes, pos)? as usize).checked_mul(2)?;
        if size < 6 {
            return None;
        }
        let record = bytes.get(pos..pos.checked_add(size)?)?;
        let kind = u16_at(record, 4)?;
        let p = &record[6..];
        pos += size;
        match kind {
            0 => {
                if embedded_total == Some(embedded.len()) {
                    if let Some(text) =
                        emf(&embedded, clip).filter(|t| !t.text.is_empty() && !t.partial)
                    {
                        return Some(text);
                    }
                }
                let bounds = state.extent.map(|(w, h)| [0., 0., w.abs(), h.abs()]);
                return Some(out.finish(bounds, clip));
            }
            0x02fb | 0x02fa | 0x02fc | 0x00f7 | 0x01f9 | 0x0142 | 0x06ff => {
                let font = if kind == 0x02fb {
                    Some(*p.get(13)?)
                } else {
                    None
                };
                if let Some(index) = free_objects.pop_first() {
                    objects[index] = Some(font);
                } else if objects.len() < 65536 {
                    objects.push(Some(font));
                } else {
                    return None;
                }
            }
            0x012d => {
                if let Some(Some(Some(charset))) = objects.get(u16_at(p, 0)? as usize) {
                    state.charset = *charset;
                }
            }
            0x01f0 => {
                let index = u16_at(p, 0)? as usize;
                if let Some(slot) = objects.get_mut(index) {
                    *slot = None;
                    free_objects.insert(index);
                }
            }
            0x001e => {
                if stack.len() >= 256 {
                    return None;
                }
                stack.push(state);
            }
            0x0127 => {
                restore(&mut stack, &mut state, i16_at(p, 0)? as i32)?;
            }
            0x020b => state.window = (i16_at(p, 2)? as f64, i16_at(p, 0)? as f64),
            0x020c => state.extent = Some((i16_at(p, 2)? as f64, i16_at(p, 0)? as f64)),
            0x020d => state.viewport = (i16_at(p, 2)? as f64, i16_at(p, 0)? as f64),
            0x020e => state.viewport_extent = Some((i16_at(p, 2)? as f64, i16_at(p, 0)? as f64)),
            0x0521 | 0x0a32 => {
                let (start, n, x, y) = if kind == 0x0521 {
                    let n = u16_at(p, 0)? as usize;
                    let end = 2 + n + (n % 2);
                    (2, n, i16_at(p, end + 2)?, i16_at(p, end)?)
                } else {
                    let n = u16_at(p, 4)? as usize;
                    let options = u16_at(p, 6)?;
                    if options & 0x10 != 0 {
                        out.partial = true;
                        continue;
                    }
                    (
                        if options & 6 != 0 { 16 } else { 8 },
                        n,
                        i16_at(p, 2)?,
                        i16_at(p, 0)?,
                    )
                };
                let data = p.get(start..start + n)?;
                if let Some(text) = ansi(data, state.charset) {
                    let (x, y) = state.point(x as f64, y as f64);
                    out.push(x, y, text)?;
                } else {
                    out.partial = true;
                }
            }
            0x0626 if u16_at(p, 0)? == 15 && p.get(4..8) == Some(b"WMFC") => {
                let count = u32_at(p, 26)? as usize;
                let remaining = u32_at(p, 30)? as usize;
                let total = u32_at(p, 34)? as usize;
                if total > MAX_BYTES
                    || count > 8192
                    || embedded_total.is_some_and(|t| t != total)
                    || embedded.len().checked_add(count)?.checked_add(remaining)? != total
                {
                    return None;
                }
                embedded_total = Some(total);
                embedded.extend_from_slice(p.get(38..38 + count)?);
            }
            _ => {}
        }
    }
    None
}

fn emf(bytes: &[u8], clip: Option<[f64; 4]>) -> Option<VectorText> {
    if u32_at(bytes, 0)? != 1 || bytes.get(40..44) != Some(b" EMF") {
        return None;
    }
    let bytes = bytes.get(..u32_at(bytes, 48)? as usize)?;
    let mut pos = 0usize;
    let mut state = State::default();
    let mut stack = Vec::new();
    let mut fonts = BTreeMap::new();
    let mut out = Collector::default();
    for _ in 0..MAX_RECORDS {
        let kind = u32_at(bytes, pos)?;
        let size = u32_at(bytes, pos + 4)? as usize;
        if size < 8 || size % 4 != 0 {
            return None;
        }
        let r = bytes.get(pos..pos.checked_add(size)?)?;
        pos += size;
        match kind {
            14 => {
                return Some(out.finish(
                    Some([
                        i32_at(bytes, 8)? as f64,
                        i32_at(bytes, 12)? as f64,
                        i32_at(bytes, 16)? as f64,
                        i32_at(bytes, 20)? as f64,
                    ]),
                    clip,
                ))
            }
            82 => {
                if fonts.len() >= 65536 {
                    return None;
                }
                fonts.insert(u32_at(r, 8)?, *r.get(35)?);
            }
            37 => {
                if let Some(cs) = fonts.get(&u32_at(r, 8)?) {
                    state.charset = *cs;
                }
            }
            40 => {
                fonts.remove(&u32_at(r, 8)?);
            }
            33 => {
                if stack.len() >= 256 {
                    return None;
                }
                stack.push(state);
            }
            34 => {
                restore(&mut stack, &mut state, i32_at(r, 8)?)?;
            }
            9 => state.extent = Some((i32_at(r, 8)? as f64, i32_at(r, 12)? as f64)),
            10 => state.window = (i32_at(r, 8)? as f64, i32_at(r, 12)? as f64),
            11 => state.viewport_extent = Some((i32_at(r, 8)? as f64, i32_at(r, 12)? as f64)),
            12 => state.viewport = (i32_at(r, 8)? as f64, i32_at(r, 12)? as f64),
            35 | 36 => {
                let m = Matrix([
                    float_at(r, 8)?,
                    float_at(r, 12)?,
                    float_at(r, 16)?,
                    float_at(r, 20)?,
                    float_at(r, 24)?,
                    float_at(r, 28)?,
                ]);
                state.world = if kind == 35 {
                    m
                } else {
                    match u32_at(r, 32)? {
                        1 => Matrix::default(),
                        2 => m.then(state.world),
                        3 => state.world.then(m),
                        4 => m,
                        _ => return None,
                    }
                };
            }
            83 | 84 => {
                emf_text(r, 36, 76, kind == 84, state, &mut out)?;
            }
            96 | 97 => {
                let count = u32_at(r, 36)? as usize;
                if count > r.len().saturating_sub(40) / 40 {
                    return None;
                }
                for i in 0..count {
                    emf_text(r, 40 + i * 40, 40 + count * 40, kind == 97, state, &mut out)?;
                }
            }
            108 => {
                let n = u32_at(r, 16)? as usize;
                let flags = u32_at(r, 20)?;
                let start: usize = if flags & 0x100 != 0 { 36 } else { 52 };
                let wide = flags & 0x200 == 0;
                if flags & 0x10 != 0 {
                    out.partial = true;
                    continue;
                }
                let data =
                    r.get(start..start.checked_add(n.checked_mul(if wide { 2 } else { 1 })?)?)?;
                let text = if wide {
                    utf16(data)
                } else {
                    Some(data.iter().map(|b| char::from(*b)).collect())
                };
                if let Some(text) = text {
                    let (x, y) = state.point(i32_at(r, 8)? as f64, i32_at(r, 12)? as f64);
                    out.push(x, y, text)?;
                } else {
                    out.partial = true;
                }
            }
            _ => {}
        }
    }
    None
}

fn emf_text(
    r: &[u8],
    base: usize,
    string_min: usize,
    wide: bool,
    state: State,
    out: &mut Collector,
) -> Option<()> {
    let count = u32_at(r, base + 8)? as usize;
    let start = u32_at(r, base + 12)? as usize;
    let flags = u32_at(r, base + 16)?;
    if flags & 0x10 != 0 {
        out.partial = true;
        return Some(());
    }
    if count != 0 && (start < string_min || (wide && start % 2 != 0)) {
        return None;
    }
    let data = r.get(start..start.checked_add(count.checked_mul(if wide { 2 } else { 1 })?)?)?;
    let text = if wide {
        utf16(data)
    } else {
        ansi(data, state.charset)
    };
    if let Some(text) = text {
        let (x, y) = state.point(i32_at(r, base)? as f64, i32_at(r, base + 4)? as f64);
        out.push(x, y, text)?;
    } else {
        out.partial = true;
    }
    Some(())
}

fn svg_attr<'a>(node: crate::dom::Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .iter()
        .find(|a| a.namespace.is_none() && a.name == name)
        .map(|a| a.value.as_ref())
}

fn svg_matrix(value: &str) -> Option<Matrix> {
    let mut rest = value.trim();
    let mut matrix = Matrix::default();
    while !rest.is_empty() {
        let (name, args) = rest.split_once('(')?;
        let (args, tail) = args.split_once(')')?;
        let values: Vec<f64> = args
            .split(|c: char| c.is_ascii_whitespace() || c == ',')
            .filter(|v| !v.is_empty())
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?;
        if !values.iter().all(|v| v.is_finite()) {
            return None;
        }
        let m = match (name.trim(), values.as_slice()) {
            ("matrix", [a, b, c, d, e, f]) => Matrix([*a, *b, *c, *d, *e, *f]),
            ("translate", [x]) => Matrix([1., 0., 0., 1., *x, 0.]),
            ("translate", [x, y]) => Matrix([1., 0., 0., 1., *x, *y]),
            ("scale", [s]) => Matrix([*s, 0., 0., *s, 0., 0.]),
            ("scale", [x, y]) => Matrix([*x, 0., 0., *y, 0., 0.]),
            ("rotate", [a]) => {
                let (s, c) = a.to_radians().sin_cos();
                Matrix([c, s, -s, c, 0., 0.])
            }
            ("rotate", [a, x, y]) => {
                let (s, c) = a.to_radians().sin_cos();
                Matrix([c, s, -s, c, x - c * x + s * y, y - s * x - c * y])
            }
            _ => return None,
        };
        matrix = matrix.then(m);
        rest = tail.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
    }
    Some(matrix)
}

fn hidden(node: crate::dom::Node<'_, '_>) -> bool {
    let direct = svg_attr(node, "display") == Some("none")
        || matches!(svg_attr(node, "visibility"), Some("hidden" | "collapse"))
        || svg_attr(node, "opacity").and_then(|s| s.parse::<f64>().ok()) == Some(0.);
    direct
        || svg_attr(node, "style").is_some_and(|style| {
            style.split(';').any(|s| {
                s.split_once(':').is_some_and(|(k, v)| {
                    matches!(
                        (k.trim(), v.trim()),
                        ("display", "none")
                            | ("visibility", "hidden" | "collapse")
                            | ("opacity", "0")
                    )
                })
            })
        })
}

fn visible_text(node: crate::dom::Node<'_, '_>, out: &mut String, depth: usize) -> Option<()> {
    if depth > 128 || out.len() > MAX_TEXT {
        return None;
    }
    if hidden(node) {
        return Some(());
    }
    if let Some(text) = node.text() {
        out.push_str(text);
    }
    for child in node.children() {
        if !matches!(
            child.tag_name().name(),
            "title" | "desc" | "script" | "style" | "metadata"
        ) {
            visible_text(child, out, depth + 1)?;
        }
    }
    Some(())
}

/// Recover SVG text and positioned labels from outlined chart text. Definitions,
/// descriptions, hidden elements and scripts do not contribute text.
pub fn svg(source: &str) -> Option<VectorText> {
    svg_clipped(source, None)
}

fn svg_clipped(source: &str, clip: Option<[f64; 4]>) -> Option<VectorText> {
    if source.len() > MAX_BYTES {
        return None;
    }
    let doc = crate::dom::Document::parse(source, MAX_RECORDS).ok()?;
    if doc.root_element().tag_name().name() != "svg" {
        return None;
    }
    let mut out = Collector::default();
    let mut pending = vec![(doc.root_element(), Matrix::default(), 0)];
    while let Some((node, parent, depth)) = pending.pop() {
        if depth > 128 {
            return None;
        }
        if !node.is_element() || hidden(node) {
            continue;
        }
        let name = node.tag_name().name();
        if matches!(
            name,
            "defs" | "clipPath" | "mask" | "script" | "style" | "metadata" | "desc" | "title"
        ) {
            continue;
        }
        let matrix = match svg_attr(node, "transform") {
            Some(value) => match svg_matrix(value) {
                Some(m) => parent.then(m),
                None => {
                    out.partial = true;
                    parent
                }
            },
            None => parent,
        };
        let outlined = name == "g"
            && svg_attr(node, "x").is_some()
            && svg_attr(node, "y").is_some()
            && svg_attr(node, "font-size").is_some()
            && node.children().any(|c| c.tag_name().name() == "title");
        if name == "text" || outlined {
            let coordinate = |key| {
                svg_attr(node, key)
                    .and_then(|s| {
                        s.split(|c: char| c.is_ascii_whitespace() || c == ',')
                            .next()
                    })
                    .and_then(|s| s.parse::<f64>().ok())
                    .filter(|n| n.is_finite())
                    .unwrap_or(0.)
            };
            let (x, y) = matrix.point(coordinate("x"), coordinate("y"));
            let mut text = String::new();
            if outlined {
                visible_text(
                    node.children().find(|c| c.tag_name().name() == "title")?,
                    &mut text,
                    0,
                )?;
            } else {
                visible_text(node, &mut text, 0)?;
            }
            out.push(x, y, text)?;
        } else {
            let children: Vec<_> = node.children().collect();
            pending.extend(children.into_iter().rev().map(|c| (c, matrix, depth + 1)));
        }
    }
    let bounds = svg_attr(doc.root_element(), "viewBox").and_then(|s| {
        let v: Vec<f64> = s
            .split(|c: char| c.is_ascii_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?;
        if let [x, y, w, h] = v.as_slice() {
            Some([*x, *y, x + w, y + h])
        } else {
            None
        }
    });
    Some(out.finish(bounds, clip))
}

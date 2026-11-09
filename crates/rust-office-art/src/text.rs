//! Portable font shaping, outlines, and DrawingML text-path deformation.

use kurbo::{Affine, BezPath, PathEl, Point, Shape, Vec2};
use rustybuzz::{ttf_parser, Face, UnicodeBuffer};

use crate::geometry::text_warp_paths;
use crate::xml::get_attribute_ref;

const MAX_TEXT_BYTES: usize = 64 * 1024;
const MAX_GLYPHS: usize = 8192;
const MAX_ELEMENTS: usize = 1_000_000;

fn svg_path(path: &BezPath) -> String {
    let mut result = String::new();
    let mut emit = |command: char, points: &[Point]| {
        if !result.is_empty() {
            result.push(' ');
        }
        result.push(command);
        for (index, point) in points.iter().enumerate() {
            if index > 0 {
                result.push(' ');
            }
            result.push_str(&point.x.to_string());
            result.push(',');
            result.push_str(&point.y.to_string());
        }
    };
    for element in path.elements() {
        match *element {
            PathEl::MoveTo(p) => emit('M', &[p]),
            PathEl::LineTo(p) => emit('L', &[p]),
            PathEl::QuadTo(a, b) => emit('Q', &[a, b]),
            PathEl::CurveTo(a, b, c) => emit('C', &[a, b, c]),
            PathEl::ClosePath => emit('Z', &[]),
        }
    }
    result
}

#[derive(Clone, Copy, Debug)]
pub struct TextFont<'a> {
    pub family: &'a str,
    pub size_px: f64,
    pub bold: bool,
    pub italic: bool,
}

#[derive(Clone, Debug)]
pub struct OutlinedText {
    /// Glyph contours in pixel coordinates, with an alphabetic baseline at zero.
    pub path: String,
    pub advance_width: f64,
    pub ascent: f64,
    pub descent: f64,
    pub line_gap: f64,
    pub ink_bounds: [f64; 4],
    /// The portable font used to shape this text.
    pub font_family: &'static str,
    glyphs: Vec<(BezPath, f64, f64)>,
}

macro_rules! faces {
    ($family:literal, $prefix:literal) => {
        [
            include_bytes!(concat!("fonts/", $family, "/", $prefix, "-Regular.ttf")).as_slice(),
            include_bytes!(concat!("fonts/", $family, "/", $prefix, "-Bold.ttf")).as_slice(),
            include_bytes!(concat!("fonts/", $family, "/", $prefix, "-Italic.ttf")).as_slice(),
            include_bytes!(concat!("fonts/", $family, "/", $prefix, "-BoldItalic.ttf")).as_slice(),
        ]
    };
}

fn font_data(font: TextFont<'_>) -> (&'static [u8], &'static str) {
    let family = font.family.to_ascii_lowercase();
    let style = usize::from(font.bold) + 2 * usize::from(font.italic);
    let (fonts, name) = if family.contains("times") || family.contains("tinos") || family == "serif"
    {
        (faces!("tinos", "Tinos"), "Tinos")
    } else if family.contains("courier") || family.contains("cousine") || family == "monospace" {
        (faces!("cousine", "Cousine"), "Cousine")
    } else if family.contains("calibri")
        || family.contains("carlito")
        || family.contains("aptos")
        || family.is_empty()
    {
        (faces!("carlito", "OfficeArtSans"), "OfficeArtSans")
    } else {
        (faces!("arimo", "Arimo"), "Arimo")
    };
    (fonts[style], name)
}

struct OutlineBuilder {
    path: BezPath,
    x: f64,
    y: f64,
    scale: f64,
}

impl OutlineBuilder {
    fn point(&self, x: f32, y: f32) -> Point {
        Point::new(
            self.x + f64::from(x) * self.scale,
            self.y - f64::from(y) * self.scale,
        )
    }
}

impl ttf_parser::OutlineBuilder for OutlineBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to(self.point(x, y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to(self.point(x, y));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.path.quad_to(self.point(x1, y1), self.point(x, y));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.path
            .curve_to(self.point(x1, y1), self.point(x2, y2), self.point(x, y));
    }
    fn close(&mut self) {
        self.path.close_path();
    }
}

/// Shape a complete run, preserving kerning, ligatures, and script-specific positioning.
/// Missing glyphs return no outline so the caller can retain its text fallback.
pub fn outline_text(text: &str, font: TextFont<'_>) -> Option<OutlinedText> {
    if text.len() > MAX_TEXT_BYTES
        || !font.size_px.is_finite()
        || !(0.0..=2400.0).contains(&font.size_px)
    {
        return None;
    }
    let (bytes, font_family) = font_data(font);
    outline_with_font(text, font.size_px, bytes, font_family)
}

/// Shape outlines with caller-supplied OpenType font bytes for exact font matching.
/// Font data is bounded and is not embedded in the resulting SVG path.
pub fn outline_text_with_font(text: &str, size_px: f64, bytes: &[u8]) -> Option<OutlinedText> {
    outline_with_font(text, size_px, bytes, "Custom")
}

fn outline_with_font(
    text: &str,
    size_px: f64,
    bytes: &[u8],
    font_family: &'static str,
) -> Option<OutlinedText> {
    if text.len() > MAX_TEXT_BYTES
        || bytes.len() > 16 * 1024 * 1024
        || !size_px.is_finite()
        || !(0.0..=2400.0).contains(&size_px)
    {
        return None;
    }
    let face = Face::from_slice(bytes, 0)?;
    let scale = size_px / f64::from(face.units_per_em());
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let buffer = rustybuzz::shape(&face, &[], buffer);
    if buffer.len() > MAX_GLYPHS {
        return None;
    }
    let mut glyphs = Vec::with_capacity(buffer.len());
    let mut path = BezPath::new();
    let (mut x, mut y) = (0.0, 0.0);
    for (info, position) in buffer.glyph_infos().iter().zip(buffer.glyph_positions()) {
        if info.glyph_id == 0 {
            return None;
        }
        let advance = f64::from(position.x_advance) * scale;
        let mut outline = OutlineBuilder {
            path: BezPath::new(),
            x: x + f64::from(position.x_offset) * scale,
            y: y - f64::from(position.y_offset) * scale,
            scale,
        };
        face.outline_glyph(ttf_parser::GlyphId(info.glyph_id as u16), &mut outline);
        path.extend(outline.path.elements().iter().copied());
        if path.elements().len() > MAX_ELEMENTS {
            return None;
        }
        glyphs.push((outline.path, x, advance));
        x += advance;
        y -= f64::from(position.y_advance) * scale;
    }
    let bounds = path.bounding_box();
    let (ascent, descent, line_gap) = face
        .tables()
        .os2
        .filter(|table| !table.use_typographic_metrics() && table.windows_ascender() > 0)
        .map(|table| (table.windows_ascender(), table.windows_descender(), 0))
        .unwrap_or_else(|| (face.ascender(), face.descender(), face.line_gap()));
    Some(OutlinedText {
        path: svg_path(&path),
        advance_width: x.abs(),
        ascent: f64::from(ascent) * scale,
        descent: -f64::from(descent) * scale,
        line_gap: f64::from(line_gap) * scale,
        ink_bounds: [bounds.x0, bounds.y0, bounds.x1, bounds.y1],
        font_family,
        glyphs,
    })
}

/// Return a shaped advance width with the same portable fonts used for outlines.
pub fn measure_text(text: &str, font: TextFont<'_>) -> Option<f64> {
    if text.len() > MAX_TEXT_BYTES
        || !font.size_px.is_finite()
        || !(0.0..=2400.0).contains(&font.size_px)
    {
        return None;
    }
    let (bytes, _) = font_data(font);
    let face = Face::from_slice(bytes, 0)?;
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let buffer = rustybuzz::shape(&face, &[], buffer);
    if buffer.len() > MAX_GLYPHS || buffer.glyph_infos().iter().any(|info| info.glyph_id == 0) {
        return None;
    }
    Some(
        buffer
            .glyph_positions()
            .iter()
            .map(|p| f64::from(p.x_advance))
            .sum::<f64>()
            .abs()
            * font.size_px
            / f64::from(face.units_per_em()),
    )
}

/// Return shaped advances keyed by source-byte cluster for line breaking.
pub fn cluster_advances(text: &str, font: TextFont<'_>) -> Option<Vec<(usize, f64)>> {
    if text.len() > MAX_TEXT_BYTES
        || !font.size_px.is_finite()
        || !(0.0..=2400.0).contains(&font.size_px)
    {
        return None;
    }
    let (bytes, _) = font_data(font);
    let face = Face::from_slice(bytes, 0)?;
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let buffer = rustybuzz::shape(&face, &[], buffer);
    if buffer.len() > MAX_GLYPHS || buffer.glyph_infos().iter().any(|info| info.glyph_id == 0) {
        return None;
    }
    let scale = font.size_px / f64::from(face.units_per_em());
    let mut advances = std::collections::BTreeMap::<usize, f64>::new();
    for (info, position) in buffer.glyph_infos().iter().zip(buffer.glyph_positions()) {
        *advances.entry(info.cluster as usize).or_default() +=
            f64::from(position.x_advance) * scale;
    }
    Some(
        advances
            .into_iter()
            .map(|(offset, advance)| (offset, advance.abs()))
            .collect(),
    )
}

fn valid_point(point: Point) -> bool {
    point.x.is_finite() && point.y.is_finite() && point.x.abs().max(point.y.abs()) <= 1.0e8
}

fn emit_bounded(element: PathEl, budget: &mut usize, emit: &mut impl FnMut(PathEl)) -> Option<()> {
    *budget = budget.checked_sub(1)?;
    emit(element);
    Some(())
}

fn flatten_curve(
    points: [Point; 4],
    tolerance: f64,
    depth: usize,
    budget: &mut usize,
    emit: &mut impl FnMut(PathEl),
) -> Option<()> {
    let [a, b, c, d] = points;
    if points.iter().any(|point| !valid_point(*point)) {
        return None;
    }
    let chord = a.distance(d);
    let perimeter = a.distance(b) + b.distance(c) + c.distance(d);
    let distance = if chord > 1e-12 {
        ((b - a).cross(d - a).abs().max((c - a).cross(d - a).abs())) / chord
    } else {
        a.distance(b).max(a.distance(c))
    };
    if distance <= tolerance && perimeter - chord <= tolerance {
        return emit_bounded(PathEl::LineTo(d), budget, emit);
    }
    if depth >= 20 || *budget == 0 {
        return None;
    }
    let ab = a.lerp(b, 0.5);
    let bc = b.lerp(c, 0.5);
    let cd = c.lerp(d, 0.5);
    let abc = ab.lerp(bc, 0.5);
    let bcd = bc.lerp(cd, 0.5);
    let mid = abc.lerp(bcd, 0.5);
    flatten_curve([a, ab, abc, mid], tolerance, depth + 1, budget, emit)?;
    flatten_curve([mid, bcd, cd, d], tolerance, depth + 1, budget, emit)
}

fn flatten_bounded(
    path: &BezPath,
    tolerance: f64,
    limit: usize,
    mut emit: impl FnMut(PathEl),
) -> Option<()> {
    let mut budget = limit;
    let mut previous = None;
    let mut start = None;
    for element in path.elements() {
        match *element {
            PathEl::MoveTo(point) if valid_point(point) => {
                emit_bounded(*element, &mut budget, &mut emit)?;
                previous = Some(point);
                start = Some(point);
            }
            PathEl::LineTo(point) if valid_point(point) => {
                previous?;
                emit_bounded(*element, &mut budget, &mut emit)?;
                previous = Some(point);
            }
            PathEl::QuadTo(control, end) => {
                let previous_point = previous?;
                flatten_curve(
                    [
                        previous_point,
                        previous_point.lerp(control, 2.0 / 3.0),
                        end.lerp(control, 2.0 / 3.0),
                        end,
                    ],
                    tolerance,
                    0,
                    &mut budget,
                    &mut emit,
                )?;
                previous = Some(end);
            }
            PathEl::CurveTo(a, b, end) => {
                flatten_curve([previous?, a, b, end], tolerance, 0, &mut budget, &mut emit)?;
                previous = Some(end);
            }
            PathEl::ClosePath => {
                emit_bounded(*element, &mut budget, &mut emit)?;
                previous = start;
            }
            _ => return None,
        }
    }
    Some(())
}

struct GuidePath {
    points: Vec<(Point, f64)>,
    length: f64,
}

impl GuidePath {
    fn new(d: &str) -> Option<Self> {
        let path = BezPath::from_svg(d).ok()?;
        let mut points: Vec<(Point, f64)> = Vec::new();
        let mut length = 0.0;
        flatten_bounded(&path, 0.025, 16_384, |el| {
            if let PathEl::MoveTo(p) | PathEl::LineTo(p) = el {
                if let Some((last, _)) = points.last() {
                    let distance = last.distance(p);
                    if distance <= 1e-8 {
                        return;
                    }
                    length += distance;
                }
                points.push((p, length));
            }
        })?;
        (points.len() >= 2 && length > 0.0).then_some(Self { points, length })
    }

    fn at(&self, fraction: f64) -> (Point, Vec2) {
        let distance = fraction.clamp(0.0, 1.0) * self.length;
        let index = self
            .points
            .partition_point(|(_, n)| *n < distance)
            .clamp(1, self.points.len() - 1);
        let (p0, d0) = self.points[index - 1];
        let (p1, d1) = self.points[index];
        let tangent = (p1 - p0).normalize();
        (
            p0.lerp(
                p1,
                if d1 > d0 {
                    (distance - d0) / (d1 - d0)
                } else {
                    0.0
                },
            ),
            tangent,
        )
    }
}

/// Deform glyph outlines through the standard preset paths and adjustment formulas.
/// Envelope presets map the full ink height; single paths keep each glyph rigid.
pub fn warp_text(
    text: &OutlinedText,
    warp_xml: &str,
    width: f64,
    height: f64,
    from_word_art: bool,
) -> Option<String> {
    warp_text_aligned(text, warp_xml, width, height, from_word_art, 0.5)
}

/// Apply a preset with horizontal alignment from zero (start) to one (end).
pub fn warp_text_aligned(
    text: &OutlinedText,
    warp_xml: &str,
    width: f64,
    height: f64,
    _from_word_art: bool,
    alignment: f64,
) -> Option<String> {
    if !alignment.is_finite() || !(0.0..=1.0).contains(&alignment) {
        return None;
    }
    if [width, height]
        .iter()
        .any(|n| !n.is_finite() || *n <= 0.0 || *n > 1_000_000.0)
        || text.path.len() > 16 * 1024 * 1024
    {
        return None;
    }
    let preset = get_attribute_ref(warp_xml, "prst")?;
    if matches!(preset, "textNoShape") {
        return Some(text.path.clone());
    }
    let paths = text_warp_paths(warp_xml, width, height)?;
    let guides: Vec<GuidePath> = paths
        .iter()
        .map(|path| GuidePath::new(&path.d))
        .collect::<Option<_>>()?;
    let mut output = BezPath::new();
    if guides.len() == 2 {
        let top = &guides[0];
        let bottom = &guides[1];
        let [left, upper, right, lower] = text.ink_bounds;
        if right <= left || lower <= upper {
            return Some(String::new());
        }
        let source = BezPath::from_svg(&text.path).ok()?;
        let scale = (height / (lower - upper))
            .max(width / (right - left))
            .max(1.0);
        let map = |p: Point| {
            let fraction = (p.x - left) / (right - left);
            top.at(fraction)
                .0
                .lerp(bottom.at(fraction).0, (p.y - upper) / (lower - upper))
        };
        flatten_bounded(
            &source,
            (0.06 / scale).max(0.001),
            MAX_ELEMENTS,
            |el| match el {
                PathEl::MoveTo(p) => output.move_to(map(p)),
                PathEl::LineTo(p) => output.line_to(map(p)),
                PathEl::ClosePath => output.close_path(),
                _ => {}
            },
        )?;
    } else if guides.len() == 1 {
        let guide = &guides[0];
        let scale = if text.advance_width > 0.0 {
            (guide.length / text.advance_width).min(1.0)
        } else {
            1.0
        };
        let run_width = text.advance_width * scale;
        let start = (guide.length - run_width).max(0.0) * alignment;
        for (path, _, _) in &text.glyphs {
            if path.elements().is_empty() {
                continue;
            }
            let bounds = path.bounding_box();
            let center = (bounds.x0 + bounds.x1) / 2.0;
            let anchor_y = bounds.y0 + 0.65 * (bounds.y1 - bounds.y0);
            let (point, tangent) = guide.at((start + center * scale) / guide.length);
            let normal = Vec2::new(-tangent.y, tangent.x);
            let transform = Affine::new([
                tangent.x * scale,
                tangent.y * scale,
                normal.x * scale,
                normal.y * scale,
                point.x - (tangent.x * center + normal.x * anchor_y) * scale,
                point.y - (tangent.y * center + normal.y * anchor_y) * scale,
            ]);
            output.extend((transform * path).elements().iter().copied());
        }
    } else {
        return None;
    }
    if output.elements().len() > MAX_ELEMENTS {
        return None;
    }
    Some(svg_path(&output))
}

/// Translate or scale an outline without changing its winding or curve segments.
pub fn transform_path(path: &str, scale_x: f64, scale_y: f64, x: f64, y: f64) -> Option<String> {
    if path.len() > 16 * 1024 * 1024
        || [scale_x, scale_y, x, y]
            .iter()
            .any(|n| !n.is_finite() || n.abs() > 1e8)
    {
        return None;
    }
    let path = BezPath::from_svg(path).ok()?;
    if path.elements().len() > MAX_ELEMENTS {
        return None;
    }
    let transformed = Affine::new([scale_x, 0.0, 0.0, scale_y, x, y]) * path;
    if transformed.elements().iter().any(|element| match *element {
        PathEl::MoveTo(a) | PathEl::LineTo(a) => !valid_point(a),
        PathEl::QuadTo(a, b) => !valid_point(a) || !valid_point(b),
        PathEl::CurveTo(a, b, c) => !valid_point(a) || !valid_point(b) || !valid_point(c),
        PathEl::ClosePath => false,
    }) {
        return None;
    }
    Some(svg_path(&transformed))
}

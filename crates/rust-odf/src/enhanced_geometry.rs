//! Bounded evaluation of enhanced geometry into independently painted SVG paths.

use std::collections::HashMap;
use std::f64::consts::{PI, TAU};
use std::fmt::Write;

use crate::draw::{normalize_path, CustomGeometry};

#[derive(Clone, Debug, PartialEq)]
pub struct GeometryPath {
    /// Absolute SVG commands in the geometry's view box.
    pub data: String,
    pub fill: bool,
    pub stroke: bool,
}

impl Default for GeometryPath {
    fn default() -> Self {
        Self { data: String::new(), fill: true, stroke: true }
    }
}

struct Evaluator<'a> {
    geometry: &'a CustomGeometry,
    equations: HashMap<&'a str, &'a str>,
    values: HashMap<String, Option<f64>>,
    remaining: usize,
    size: [f64; 2],
    paint: [bool; 2],
}

impl Evaluator<'_> {
    fn value(&mut self, name: &str, depth: usize) -> Option<f64> {
        if depth > 256 || self.remaining == 0 { return None; }
        self.remaining -= 1;
        if let Some(index) = name.strip_prefix('$') {
            return self.geometry.modifiers.get(index.parse::<usize>().ok()?).copied();
        }
        if let Some(name) = name.strip_prefix('?') {
            if let Some(value) = self.values.get(name) { return *value; }
            let formula = *self.equations.get(name)?;
            self.values.insert(name.to_owned(), None);
            let mut expression = Expression { text: formula.as_bytes(), index: 0, eval: self, depth: depth + 1, active: true };
            let value = expression.expression(0)?;
            expression.space();
            if expression.index != expression.text.len() { return None; }
            self.values.insert(name.to_owned(), Some(value));
            return Some(value);
        }
        let [left, top, width, height] = self.geometry.coordinate_box(self.size);
        Some(match name {
            "pi" => PI,
            "left" => left,
            "top" => top,
            "right" => left + width,
            "bottom" => top + height,
            "width" => width,
            "height" => height,
            "logwidth" => self.size[0],
            "logheight" => self.size[1],
            "hasfill" => u8::from(self.paint[0]) as f64,
            "hasstroke" => u8::from(self.paint[1]) as f64,
            _ => return None,
        })
    }
}

struct Expression<'a, 'b, 'c> {
    text: &'a [u8],
    index: usize,
    eval: &'b mut Evaluator<'c>,
    depth: usize,
    active: bool,
}

impl Expression<'_, '_, '_> {
    fn space(&mut self) {
        while self.text.get(self.index).is_some_and(u8::is_ascii_whitespace) { self.index += 1; }
    }

    fn take(&mut self, byte: u8) -> bool {
        self.space();
        if self.text.get(self.index) == Some(&byte) { self.index += 1; true } else { false }
    }

    fn expression(&mut self, minimum: u8) -> Option<f64> {
        if self.depth > 256 || self.eval.remaining == 0 { return None; }
        self.eval.remaining -= 1;
        self.depth += 1;
        let mut left = self.atom()?;
        loop {
            self.space();
            let op = self.text.get(self.index).copied().unwrap_or(0);
            let precedence = match op { b'+' | b'-' => 1, b'*' | b'/' => 2, _ => 0 };
            if precedence == 0 || precedence < minimum { break; }
            self.index += 1;
            let right = self.expression(precedence + 1)?;
            left = match op { b'+' => left + right, b'-' => left - right, b'*' => left * right, _ => left / right };
        }
        self.depth -= 1;
        if self.active { left.is_finite().then_some(left) } else { Some(0.0) }
    }

    fn atom(&mut self) -> Option<f64> {
        self.space();
        if self.take(b'-') { return self.expression(3).map(|v| -v); }
        if self.take(b'+') { return self.expression(3); }
        if self.take(b'(') {
            let value = self.expression(0)?;
            return self.take(b')').then_some(value);
        }
        let start = self.index;
        if self.text.get(start).is_some_and(|b| b.is_ascii_alphabetic() || matches!(b, b'?' | b'$')) {
            self.index += 1;
            while self.text.get(self.index).is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_') { self.index += 1; }
            let name = std::str::from_utf8(&self.text[start..self.index]).ok()?;
            if !self.take(b'(') { return if self.active { self.eval.value(name, self.depth) } else { Some(0.0) }; }
            if name == "if" {
                let condition = self.expression(0)?;
                if !self.take(b',') { return None; }
                let active = self.active;
                self.active = active && condition > 0.0;
                let yes = self.expression(0)?;
                if !self.take(b',') { return None; }
                self.active = active && condition <= 0.0;
                let no = self.expression(0)?;
                self.active = active;
                return self.take(b')').then_some(if condition > 0.0 { yes } else { no });
            }
            let a = self.expression(0)?;
            let b = if self.take(b',') { Some(self.expression(0)?) } else { None };
            let c = if self.take(b',') { Some(self.expression(0)?) } else { None };
            if !self.take(b')') { return None; }
            let value = match (name, b, c) {
                ("abs", None, None) => a.abs(), ("sqrt", None, None) => a.sqrt(),
                ("sin", None, None) => a.sin(), ("cos", None, None) => a.cos(),
                ("tan", None, None) => a.tan(), ("atan", None, None) => a.atan(),
                ("min", Some(b), None) => a.min(b), ("max", Some(b), None) => a.max(b),
                ("atan2", Some(b), None) => a.atan2(b),
                ("if", Some(b), Some(c)) => if a > 0.0 { b } else { c },
                _ => return None,
            };
            return if self.active { value.is_finite().then_some(value) } else { Some(0.0) };
        }
        while self.text.get(self.index).is_some_and(|b| b.is_ascii_digit() || *b == b'.') { self.index += 1; }
        if matches!(self.text.get(self.index), Some(b'e' | b'E')) {
            self.index += 1;
            if matches!(self.text.get(self.index), Some(b'+' | b'-')) { self.index += 1; }
            while self.text.get(self.index).is_some_and(u8::is_ascii_digit) { self.index += 1; }
        }
        std::str::from_utf8(&self.text[start..self.index]).ok()?.parse::<f64>().ok().filter(|v| v.is_finite())
    }
}

fn point(path: &mut String, command: char, x: f64, y: f64) {
    let _ = write!(path, " {command} {x} {y}");
}

fn arc(path: &mut String, centre: [f64; 2], radius: [f64; 2], start: f64, sweep: f64) -> [f64; 2] {
    let steps = (sweep.abs() / (PI / 2.0)).ceil().max(1.0).min(4.0) as usize;
    let step = sweep / steps as f64;
    for index in 0..steps {
        let a = start + index as f64 * step;
        let b = a + step;
        let k = 4.0 / 3.0 * (step / 4.0).tan();
        let _ = write!(path, " C {} {} {} {} {} {}",
            centre[0] + radius[0] * (a.cos() - k * a.sin()), centre[1] + radius[1] * (a.sin() + k * a.cos()),
            centre[0] + radius[0] * (b.cos() + k * b.sin()), centre[1] + radius[1] * (b.sin() - k * b.cos()),
            centre[0] + radius[0] * b.cos(), centre[1] + radius[1] * b.sin());
    }
    [centre[0] + radius[0] * (start + sweep).cos(), centre[1] + radius[1] * (start + sweep).sin()]
}

impl CustomGeometry {
    /// Zero extents select the logical drawing dimensions.
    pub fn coordinate_box(&self, logical_size: [f64; 2]) -> [f64; 4] {
        let mut bounds = self.view_box.unwrap_or([0.0, 0.0, 21600.0, 21600.0]);
        if bounds[2] == 0.0 { bounds[2] = logical_size[0]; }
        if bounds[3] == 0.0 { bounds[3] = logical_size[1]; }
        bounds
    }

    /// Evaluates stored geometry, retaining each path's paint flags. Logical
    /// dimensions use hundredths of a millimetre. Invalid or unsupported paths fail closed.
    pub fn evaluated_paths(&self, logical_size: [f64; 2], fill: bool, stroke: bool) -> Option<Vec<GeometryPath>> {
        let source = self.path.as_deref()?;
        if source.len() > 4 * 1024 * 1024 || self.equations.len() > 65536 { return None; }
        let mut eval = Evaluator {
            geometry: self, equations: self.equations.iter().map(|(n, f)| (n.as_str(), f.as_str())).collect(),
            values: HashMap::new(), remaining: 1_000_000, size: logical_size, paint: [fill, stroke],
        };
        let mut parser = Expression { text: source.as_bytes(), index: 0, eval: &mut eval, depth: 0, active: true };
        let mut result = Vec::new();
        let mut path = GeometryPath::default();
        let mut position = [0.0, 0.0];
        let mut origin = position;
        let mut command = None;
        let mut segments = 0usize;
        loop {
            parser.space();
            if parser.take(b',') { continue; }
            let Some(&byte) = parser.text.get(parser.index) else { break; };
            if byte.is_ascii_uppercase() {
                parser.index += 1;
                command = Some(byte);
                match byte {
                    b'Z' => { path.data.push_str(" Z"); position = origin; command = None; continue; }
                    b'N' => {
                        if !path.data.trim().is_empty() { result.push(std::mem::take(&mut path)); }
                        command = None; continue;
                    }
                    b'F' => { path.fill = false; command = None; continue; }
                    b'S' => { path.stroke = false; command = None; continue; }
                    _ => {}
                }
            }
            let op = command?;
            let count = match op {
                b'M' | b'L' | b'X' | b'Y' => 2, b'Q' | b'G' => 4,
                b'C' | b'T' | b'U' => 6, b'A' | b'B' | b'V' | b'W' => 8, _ => return None,
            };
            let mut v = [0.0; 8];
            for value in &mut v[..count] {
                parser.take(b',');
                *value = parser.atom()?;
                if !value.is_finite() || value.abs() > 1e15 { return None; }
            }
            segments += 1;
            if segments > 100_000 { return None; }
            match op {
                b'M' | b'L' => {
                    position = [v[0], v[1]]; point(&mut path.data, op as char, v[0], v[1]);
                    if op == b'M' { origin = position; command = Some(b'L'); }
                }
                b'C' | b'Q' => {
                    let _ = write!(path.data, " {}", op as char);
                    for value in &v[..count] { let _ = write!(path.data, " {value}"); }
                    position = [v[count - 2], v[count - 1]];
                }
                b'X' | b'Y' => {
                    let end = [v[0], v[1]];
                    let centre = if op == b'X' { [position[0], end[1]] } else { [end[0], position[1]] };
                    let radius = [(end[0] - position[0]).abs(), (end[1] - position[1]).abs()];
                    if radius[0] == 0.0 || radius[1] == 0.0 { point(&mut path.data, 'L', end[0], end[1]); }
                    else {
                        let a = ((position[1] - centre[1]) / radius[1]).atan2((position[0] - centre[0]) / radius[0]);
                        let b = ((end[1] - centre[1]) / radius[1]).atan2((end[0] - centre[0]) / radius[0]);
                        let sweep = (b - a + PI).rem_euclid(TAU) - PI;
                        arc(&mut path.data, centre, radius, a, sweep);
                    }
                    position = end; command = Some(if op == b'X' { b'Y' } else { b'X' });
                }
                b'T' | b'U' | b'G' => {
                    let radius = [v[2], v[3]];
                    let (centre, radius, a, sweep) = if op == b'G' {
                        let radius = [v[0].abs(), v[1].abs()];
                        let a = (radius[0] * v[2].to_radians().sin()).atan2(radius[1] * v[2].to_radians().cos());
                        let b = (radius[0] * (v[2] + v[3]).to_radians().sin()).atan2(radius[1] * (v[2] + v[3]).to_radians().cos());
                        let sweep = if v[3].abs() >= 360.0 { TAU.copysign(v[3]) } else if v[3] < 0.0 { -(a - b).rem_euclid(TAU) } else { (b - a).rem_euclid(TAU) };
                        ([position[0] - radius[0] * a.cos(), position[1] - radius[1] * a.sin()], radius, a, sweep)
                    } else {
                        if radius[0] <= 0.0 || radius[1] <= 0.0 { return None; }
                        let a = (radius[0] * v[4].to_radians().sin()).atan2(radius[1] * v[4].to_radians().cos());
                        let b = (radius[0] * v[5].to_radians().sin()).atan2(radius[1] * v[5].to_radians().cos());
                        let sweep = if ((v[5] - v[4]).abs() - 360.0).abs() < 1e-8 { TAU } else { (b - a).rem_euclid(TAU) };
                        ([v[0], v[1]], radius, a, sweep)
                    };
                    if op != b'G' {
                        let start = [centre[0] + radius[0] * a.cos(), centre[1] + radius[1] * a.sin()];
                        let move_to = op == b'U' || path.data.is_empty();
                        point(&mut path.data, if move_to { 'M' } else { 'L' }, start[0], start[1]);
                        if move_to { origin = start; }
                    }
                    position = arc(&mut path.data, centre, radius, a, sweep);
                }
                b'A' | b'B' | b'V' | b'W' => {
                    let centre = [(v[0] + v[2]) / 2.0, (v[1] + v[3]) / 2.0];
                    let radius = [(v[2] - v[0]).abs() / 2.0, (v[3] - v[1]).abs() / 2.0];
                    if radius[0] == 0.0 || radius[1] == 0.0 { return None; }
                    let a = ((v[5] - centre[1]) / radius[1]).atan2((v[4] - centre[0]) / radius[0]);
                    let b = ((v[7] - centre[1]) / radius[1]).atan2((v[6] - centre[0]) / radius[0]);
                    let clockwise = matches!(op, b'V' | b'W');
                    let mut sweep = if clockwise { (b - a).rem_euclid(TAU) } else { -(a - b).rem_euclid(TAU) };
                    if sweep.abs() < 1e-12 { sweep = if clockwise { TAU } else { -TAU }; }
                    let start = [centre[0] + radius[0] * a.cos(), centre[1] + radius[1] * a.sin()];
                    let move_to = matches!(op, b'B' | b'V') || path.data.is_empty();
                    point(&mut path.data, if move_to { 'M' } else { 'L' }, start[0], start[1]);
                    if move_to { origin = start; }
                    position = arc(&mut path.data, centre, radius, a, sweep);
                }
                _ => return None,
            }
        }
        if !path.data.trim().is_empty() { result.push(path); }
        (!result.is_empty()).then_some(result)
    }

    /// A normalized outline for hosts that cannot represent separate path paints.
    pub fn evaluated_outline(&self, logical_size: [f64; 2]) -> Option<String> {
        let paths = self.evaluated_paths(logical_size, true, true)?;
        let combined = paths.iter().map(|path| path.data.as_str()).collect::<Vec<_>>().join(" ");
        normalize_path(&combined, self.coordinate_box(logical_size))
    }
}

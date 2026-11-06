use std::fmt::Write;

use base64::Engine;

use roxmltree::{Document, Node};

use crate::{PaintBounds, RenderedPicture};

const INKML: &str = "http://www.w3.org/2003/InkML";
const MAX_POINTS: usize = 65_536;
const MAX_OUTPUT: usize = 16 * 1024 * 1024;

type Element<'a, 'input> = Node<'a, 'input>;

fn named(node: Element<'_, '_>, name: &str) -> bool {
    node.is_element()
        && node.tag_name().name() == name
        && node.tag_name().namespace() == Some(INKML)
}

fn child<'a, 'input>(node: Element<'a, 'input>, name: &str) -> Option<Element<'a, 'input>> {
    node.children().find(|node| named(*node, name))
}

fn reference<'a, 'input>(
    document: &'a Document<'input>,
    value: &str,
    name: &str,
) -> Option<Element<'a, 'input>> {
    let id = value.strip_prefix('#')?;
    document.descendants().find(|node| {
        named(*node, name)
            && node.attribute(("http://www.w3.org/XML/1998/namespace", "id")) == Some(id)
    })
}

fn number(value: &str) -> Option<f64> {
    value.parse::<f64>().ok().filter(|value| value.is_finite())
}

fn pixels_per_unit(unit: &str) -> Option<f64> {
    Some(match unit {
        "cm" => 96.0 / 2.54,
        "mm" => 96.0 / 25.4,
        "m" => 96.0 / 0.0254,
        "in" => 96.0,
        "pt" => 96.0 / 72.0,
        "px" | "dip" | "dev" | "" => 1.0,
        _ => return None,
    })
}

fn inherited<'a, 'input>(node: Element<'a, 'input>, attribute: &str) -> Option<&'a str> {
    node.ancestors()
        .filter(|node| node.is_element())
        .find_map(|node| node.attribute(attribute))
}

fn context_chain<'a, 'input>(
    document: &'a Document<'input>,
    trace: Element<'a, 'input>,
) -> Option<Vec<Element<'a, 'input>>> {
    let mut context = inherited(trace, "contextRef")
        .map(|value| reference(document, value, "context"))
        .transpose_option()?;
    if context.is_none() {
        context = trace.prev_siblings().find(|node| named(*node, "context"));
    }
    let mut chain = Vec::new();
    while let Some(current) = context {
        if chain.len() >= 16 || chain.contains(&current) {
            return None;
        }
        chain.push(current);
        context = current
            .attribute("contextRef")
            .map(|value| reference(document, value, "context"))
            .transpose_option()?;
    }
    chain.reverse();
    Some(chain)
}

trait TransposeOption<T> {
    fn transpose_option(self) -> Option<Option<T>>;
}

impl<T> TransposeOption<T> for Option<Option<T>> {
    fn transpose_option(self) -> Option<Option<T>> {
        match self {
            Some(value) => value.map(Some),
            None => Some(None),
        }
    }
}

#[derive(Clone)]
struct Channel {
    name: String,
    scale: f64,
    minimum: f64,
    maximum: f64,
    intermittent: bool,
    default: f64,
}

fn channels(document: &Document<'_>, contexts: &[Element<'_, '_>]) -> Option<Vec<Channel>> {
    let mut source = None;
    let mut format = child(document.root_element(), "traceFormat");
    let mut properties = None;
    for context in contexts {
        if let Some(value) = child(*context, "inkSource") {
            source = Some(value);
        } else if let Some(value) = context.attribute("inkSourceRef") {
            source = Some(reference(document, value, "inkSource")?);
        }
        if let Some(source) = source {
            format = child(source, "traceFormat").or(format);
            properties = child(source, "channelProperties").or(properties);
        }
        format = child(*context, "traceFormat").or(format);
    }
    let Some(format) = format else {
        return Some(
            ["X", "Y"]
                .into_iter()
                .map(|name| Channel {
                    name: name.into(),
                    scale: 1.0,
                    minimum: 0.0,
                    maximum: 1.0,
                    intermittent: false,
                    default: 0.0,
                })
                .collect(),
        );
    };
    let mut result = Vec::new();
    for channel in format.descendants().filter(|node| named(*node, "channel")) {
        if result.len() >= 32 {
            return None;
        }
        let name = channel.attribute("name")?;
        if matches!(
            name,
            "W" | "BW" | "BH" | "C" | "CR" | "CG" | "CB" | "CC" | "CM" | "CY" | "CK" | "A" | "OR"
        ) {
            return None;
        }
        let unit = channel.attribute("units").unwrap_or("dev");
        let mut scale = if matches!(name, "X" | "Y") {
            pixels_per_unit(unit)?
        } else {
            pixels_per_unit(unit).unwrap_or(1.0)
        };
        if let Some(properties) = properties {
            for property in properties.children().filter(|node| {
                named(*node, "channelProperty")
                    && node.attribute("channel") == Some(name)
                    && node.attribute("name") == Some("resolution")
            }) {
                let resolution = number(property.attribute("value")?)?;
                if resolution > 0.0 {
                    let unit = property
                        .attribute("units")
                        .and_then(|units| units.strip_prefix("1/"))
                        .unwrap_or(unit);
                    scale = pixels_per_unit(unit)? / resolution;
                }
            }
        }
        result.push(Channel {
            name: name.into(),
            scale,
            minimum: channel.attribute("min").and_then(number).unwrap_or(0.0),
            maximum: channel.attribute("max").and_then(number).unwrap_or(0.0),
            intermittent: channel
                .ancestors()
                .any(|node| named(node, "intermittentChannels")),
            default: channel
                .attribute("default")
                .and_then(|value| match value {
                    "T" => Some(1.0),
                    "F" => Some(0.0),
                    _ => number(value),
                })
                .unwrap_or(0.0),
        });
    }
    (!result.is_empty()).then_some(result)
}

#[derive(Clone, Copy, Default)]
struct ChannelState {
    value: f64,
    difference: f64,
    encoded: f64,
    mode: u8,
    initialized: bool,
}

fn token(input: &[u8], cursor: &mut usize) -> Option<(Option<u8>, Option<f64>)> {
    while input.get(*cursor).is_some_and(u8::is_ascii_whitespace) {
        *cursor += 1;
    }
    let mode = match input.get(*cursor) {
        Some(b'!') => Some(0),
        Some(b'\'') => Some(1),
        Some(b'"') => Some(2),
        _ => None,
    };
    if mode.is_some() {
        *cursor += 1;
    }
    while input.get(*cursor).is_some_and(u8::is_ascii_whitespace) {
        *cursor += 1;
    }
    match input.get(*cursor)? {
        b'*' => {
            *cursor += 1;
            return Some((mode, None));
        }
        b'T' => {
            *cursor += 1;
            return Some((mode, Some(1.0)));
        }
        b'F' => {
            *cursor += 1;
            return Some((mode, Some(0.0)));
        }
        _ => {}
    }
    let start = *cursor;
    if matches!(input.get(*cursor), Some(b'+' | b'-')) {
        *cursor += 1;
    }
    while input.get(*cursor).is_some_and(u8::is_ascii_digit) {
        *cursor += 1;
    }
    if input.get(*cursor) == Some(&b'.') {
        *cursor += 1;
        while input.get(*cursor).is_some_and(u8::is_ascii_digit) {
            *cursor += 1;
        }
    }
    if matches!(input.get(*cursor), Some(b'e' | b'E')) {
        *cursor += 1;
        if matches!(input.get(*cursor), Some(b'+' | b'-')) {
            *cursor += 1;
        }
        while input.get(*cursor).is_some_and(u8::is_ascii_digit) {
            *cursor += 1;
        }
    }
    Some((
        mode,
        Some(number(std::str::from_utf8(&input[start..*cursor]).ok()?)?),
    ))
}

#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
    pressure: f64,
    contact: bool,
}

fn points(text: &str, channels: &[Channel], remaining: &mut usize) -> Option<Vec<Point>> {
    let x = channels.iter().position(|channel| channel.name == "X")?;
    let y = channels.iter().position(|channel| channel.name == "Y")?;
    let pressure = channels.iter().position(|channel| channel.name == "F");
    let contact = channels.iter().position(|channel| channel.name == "S");
    let mut states = channels
        .iter()
        .map(|channel| ChannelState {
            value: channel.default,
            encoded: channel.default,
            ..Default::default()
        })
        .collect::<Vec<_>>();
    let mut result = Vec::new();
    for sample in text.split(',') {
        if sample.trim().is_empty() {
            continue;
        }
        if *remaining == 0 {
            return None;
        }
        *remaining -= 1;
        let mut cursor = 0;
        for (channel, state) in channels.iter().zip(&mut states) {
            if sample[cursor..].trim().is_empty() && channel.intermittent {
                continue;
            }
            while sample
                .as_bytes()
                .get(cursor)
                .is_some_and(u8::is_ascii_whitespace)
            {
                cursor += 1;
            }
            if sample.as_bytes().get(cursor) == Some(&b'?') && channel.intermittent {
                cursor += 1;
                continue;
            }
            let (mode, value) = token(sample.as_bytes(), &mut cursor)?;
            if let Some(mode) = mode {
                if (mode > 0 && !state.initialized) || (mode == 2 && state.mode == 0) {
                    return None;
                }
                state.mode = mode;
            }
            let encoded = value.unwrap_or(state.encoded);
            match state.mode {
                0 => {
                    state.difference = encoded - state.value;
                    state.value = encoded;
                }
                1 => {
                    state.difference = encoded;
                    state.value += encoded;
                }
                2 => {
                    state.difference += encoded;
                    state.value += state.difference;
                }
                _ => return None,
            }
            if !state.value.is_finite() || state.value.abs() > 1e12 {
                return None;
            }
            state.initialized = true;
            state.encoded = encoded;
        }
        if !sample[cursor..].trim().is_empty() {
            return None;
        }
        let pressure = pressure.map_or(0.5, |index| {
            let channel = &channels[index];
            if channel.maximum <= channel.minimum {
                0.5
            } else {
                ((states[index].value - channel.minimum) / (channel.maximum - channel.minimum))
                    .clamp(0.0, 1.0)
            }
        });
        let point = Point {
            x: states[x].value * channels[x].scale,
            y: states[y].value * channels[y].scale,
            pressure,
            contact: contact.is_none_or(|index| states[index].value != 0.0),
        };
        if !point.x.is_finite()
            || !point.y.is_finite()
            || point.x.abs() > 1e6
            || point.y.abs() > 1e6
        {
            return None;
        }
        result.push(point);
    }
    (!result.is_empty()).then_some(result)
}

struct Brush {
    width: f64,
    height: f64,
    color: String,
    opacity: f64,
    rectangle: bool,
    pencil: bool,
    ignore_pressure: bool,
}

fn brush(
    document: &Document<'_>,
    trace: Element<'_, '_>,
    contexts: &[Element<'_, '_>],
) -> Option<Brush> {
    let mut selected = None;
    for context in contexts {
        selected = child(*context, "brush").or(selected);
        if let Some(value) = context.attribute("brushRef") {
            selected = Some(reference(document, value, "brush")?);
        }
    }
    if let Some(value) = inherited(trace, "brushRef") {
        selected = Some(reference(document, value, "brush")?);
    }
    let mut chain = Vec::new();
    while let Some(current) = selected {
        if chain.len() >= 16 || chain.contains(&current) {
            return None;
        }
        chain.push(current);
        selected = current
            .attribute("brushRef")
            .map(|value| reference(document, value, "brush"))
            .transpose_option()?;
    }
    let mut result = Brush {
        width: 1.0,
        height: 1.0,
        color: "#000000".into(),
        opacity: 1.0,
        rectangle: false,
        pencil: false,
        ignore_pressure: false,
    };
    let mut highlighter = false;
    let mut explicit_height = false;
    for brush in chain.into_iter().rev() {
        for property in brush
            .children()
            .filter(|node| named(*node, "brushProperty"))
        {
            let name = property.attribute("name")?;
            let value = property.attribute("value")?;
            match name {
                "width" | "height" => {
                    let dimension = number(value)?
                        * pixels_per_unit(property.attribute("units").unwrap_or("dev"))?;
                    if !(0.0..=10_000.0).contains(&dimension) {
                        return None;
                    }
                    if name == "width" {
                        result.width = dimension;
                    } else {
                        result.height = dimension;
                        explicit_height = true;
                    }
                }
                "color" => {
                    if value.len() == 7
                        && value.starts_with('#')
                        && value[1..].bytes().all(|value| value.is_ascii_hexdigit())
                    {
                        result.color = value.to_owned();
                    } else {
                        return None;
                    }
                }
                "transparency" => {
                    result.opacity = 1.0 - number(value)?.clamp(0.0, 255.0) / 255.0;
                }
                "tip" => match value {
                    "ellipse" => result.rectangle = false,
                    "rectangle" => result.rectangle = true,
                    _ => return None,
                },
                "rasterOp" => match value {
                    "copyPen" => highlighter = false,
                    "maskPen" => highlighter = true,
                    _ => return None,
                },
                "inkEffects" => match value {
                    "pencil" => result.pencil = true,
                    "none" => result.pencil = false,
                    _ => return None,
                },
                "ignorePressure" => result.ignore_pressure = matches!(value, "true" | "1"),
                "fitToCurve" if matches!(value, "true" | "1") => return None,
                _ => {}
            }
        }
    }
    if !explicit_height {
        result.height = result.width;
    }
    if highlighter {
        result.opacity *= 0.5;
        result.ignore_pressure = true;
    }
    if result.pencil {
        result.height = result.width;
    }
    Some(result)
}

fn radii(point: Point, brush: &Brush) -> (f64, f64) {
    if brush.pencil {
        let radius = brush.width * 0.25 + pencil_speck_radius(brush.width);
        return (radius, radius);
    }
    let factor = if brush.ignore_pressure {
        1.0
    } else {
        point.pressure * 1.5 + 0.25
    };
    (brush.width * factor * 0.5, brush.height * factor * 0.5)
}

fn pencil_speck_radius(width: f64) -> f64 {
    let himetric = 96.0 / 2540.0;
    let scale = if width < 49.0 * himetric {
        1.0 / 3.0
    } else if width < 99.0 * himetric {
        2.0 / 3.0
    } else {
        1.0
    };
    75.0 * himetric * scale * 0.25
}

struct SpeckRandom {
    values: [u32; 624],
    index: usize,
}

impl SpeckRandom {
    fn new() -> Self {
        let mut values = [0u32; 624];
        values[0] = 5489;
        for index in 1..values.len() {
            values[index] = 1_812_433_253u32
                .wrapping_mul(values[index - 1] ^ (values[index - 1] >> 30))
                .wrapping_add(index as u32);
        }
        Self { values, index: 624 }
    }

    fn unit(&mut self) -> f64 {
        if self.index == self.values.len() {
            for index in 0..624 {
                let value = (self.values[index] & 0x8000_0000)
                    | (self.values[(index + 1) % 624] & 0x7fff_ffff);
                self.values[index] = self.values[(index + 397) % 624]
                    ^ (value >> 1)
                    ^ if value & 1 == 0 { 0 } else { 0x9908_b0df };
            }
            self.index = 0;
        }
        let mut value = self.values[self.index];
        self.index += 1;
        value ^= value >> 11;
        value ^= (value << 7) & 0x9d2c_5680;
        value ^= (value << 15) & 0xefc6_0000;
        value ^= value >> 18;
        f64::from(value) / 4_294_967_296.0
    }
}

struct PencilCanvas {
    alpha: Vec<f32>,
    width: usize,
    height: usize,
    x: f64,
    y: f64,
}

impl PencilCanvas {
    // Composite subpixel grain before display so its weight is stable across SVG rasterizers.
    const SCALE: f64 = 300.0 / 96.0;

    fn new(points: &[Point], margin: f64) -> Option<Self> {
        let first = points.first()?;
        let (mut left, mut top, mut right, mut bottom) = (first.x, first.y, first.x, first.y);
        for point in points {
            left = left.min(point.x);
            top = top.min(point.y);
            right = right.max(point.x);
            bottom = bottom.max(point.y);
        }
        let x = ((left - margin) * Self::SCALE).floor() / Self::SCALE;
        let y = ((top - margin) * Self::SCALE).floor() / Self::SCALE;
        let width = ((right + margin - x) * Self::SCALE).ceil() as usize;
        let height = ((bottom + margin - y) * Self::SCALE).ceil() as usize;
        if width == 0
            || height == 0
            || width > 8192
            || height > 8192
            || width.checked_mul(height)? > 4_194_304
        {
            return None;
        }
        Some(Self {
            alpha: vec![0.0; width * height],
            width,
            height,
            x,
            y,
        })
    }

    fn speck(&mut self, x: f64, y: f64, radius: f64, opacity: f64) {
        let x = (x - self.x) * Self::SCALE;
        let y = (y - self.y) * Self::SCALE;
        let radius = radius * Self::SCALE;
        let left = (x - radius).floor().max(0.0) as usize;
        let top = (y - radius).floor().max(0.0) as usize;
        let right = (x + radius).ceil().min(self.width as f64) as usize;
        let bottom = (y + radius).ceil().min(self.height as f64) as usize;
        for py in top..bottom {
            for px in left..right {
                let mut covered = 0;
                for sy in 0..4 {
                    for sx in 0..4 {
                        let dx = px as f64 + (sx as f64 + 0.5) * 0.25 - x;
                        let dy = py as f64 + (sy as f64 + 0.5) * 0.25 - y;
                        covered += usize::from(dx * dx + dy * dy <= radius * radius);
                    }
                }
                let value = &mut self.alpha[py * self.width + px];
                *value += (1.0 - *value) * (opacity * covered as f64 / 16.0) as f32;
            }
        }
    }

    fn append(&self, body: &mut String, color: &str) -> Option<(f64, f64, f64, f64)> {
        let rgb = [
            u8::from_str_radix(&color[1..3], 16).ok()?,
            u8::from_str_radix(&color[3..5], 16).ok()?,
            u8::from_str_radix(&color[5..7], 16).ok()?,
        ];
        let mut pixels = Vec::with_capacity(self.alpha.len() * 4);
        for alpha in &self.alpha {
            pixels.extend_from_slice(&[rgb[0], rgb[1], rgb[2], (alpha * 255.0).round() as u8]);
        }
        let image = image::RgbaImage::from_raw(self.width as u32, self.height as u32, pixels)?;
        let mut png = std::io::Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).ok()?;
        let data = base64::engine::general_purpose::STANDARD.encode(png.into_inner());
        let width = self.width as f64 / Self::SCALE;
        let height = self.height as f64 / Self::SCALE;
        let _ = write!(body, "<image x=\"{}\" y=\"{}\" width=\"{width}\" height=\"{height}\" href=\"data:image/png;base64,{data}\"/>", self.x, self.y);
        Some((self.x, self.y, self.x + width, self.y + height))
    }
}

fn pencil(
    body: &mut String,
    points: &[Point],
    brush: &Brush,
    remaining_specks: &mut usize,
    remaining_raster_pixels: &mut usize,
) -> Option<(f64, f64, f64, f64)> {
    let himetric = 96.0 / 2540.0;
    let width = brush.width / himetric;
    let spacing = if width <= 37.501 {
        16.0
    } else if width < 350.0 {
        35.0
    } else {
        width / 10.0
    } * himetric;
    let multiplier = if width < 49.0 {
        3.0
    } else if width < 99.0 {
        2.0
    } else {
        1.0
    };
    let count = ((width / 250.0).max(0.5) * 8.0).ceil() * multiplier;
    let radius = pencil_speck_radius(brush.width);
    let scatter = brush.width * 0.25;
    let mut canvas = PencilCanvas::new(points, scatter + radius)?;
    *remaining_raster_pixels = remaining_raster_pixels.checked_sub(canvas.alpha.len())?;
    // InkML does not preserve the speck atlas or seed; procedural grain stays reproducible.
    let mut random = SpeckRandom::new();
    let mut distance = 0.0;
    let mut stamp = |point: Point| -> Option<()> {
        let pressure = if brush.ignore_pressure {
            0.5
        } else {
            point.pressure
        };
        let opacity = ((pressure - 0.1).max(0.0) * 0.5).min(0.5) * brush.opacity;
        for _ in 0..count as usize {
            if *remaining_specks == 0 {
                return None;
            }
            *remaining_specks -= 1;
            let x = point.x + (random.unit() * 2.0 - 1.0) * scatter;
            let y = point.y + (random.unit() * 2.0 - 1.0) * scatter;
            canvas.speck(x, y, radius, opacity);
        }
        Some(())
    };
    if let Some(first) = points.first().filter(|point| point.contact) {
        stamp(*first)?;
    }
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if !a.contact || !b.contact {
            distance = 0.0;
            continue;
        }
        let length = (b.x - a.x).hypot(b.y - a.y);
        let mut at = spacing - distance;
        while at <= length {
            let t = at / length;
            stamp(Point {
                x: a.x + (b.x - a.x) * t,
                y: a.y + (b.y - a.y) * t,
                pressure: a.pressure + (b.pressure - a.pressure) * t,
                contact: true,
            })?;
            at += spacing;
        }
        distance = (distance + length) % spacing;
    }
    canvas.append(body, &brush.color)
}

fn footprint(path: &mut String, point: Point, rx: f64, ry: f64, rectangle: bool) {
    if rectangle {
        let _ = write!(
            path,
            "M{:.4} {:.4}h{:.4}v{:.4}h{:.4}Z",
            point.x - rx,
            point.y - ry,
            rx * 2.0,
            ry * 2.0,
            -rx * 2.0
        );
    } else if rx > 0.0 && ry > 0.0 {
        let _ = write!(
            path,
            "M{:.4} {:.4}a{rx:.4} {ry:.4} 0 1 0 {:.4} 0a{rx:.4} {ry:.4} 0 1 0 {:.4} 0Z",
            point.x - rx,
            point.y,
            rx * 2.0,
            -rx * 2.0
        );
    }
}

fn polygon(path: &mut String, points: &[(f64, f64)]) {
    if let Some((x, y)) = points.first() {
        let _ = write!(path, "M{x:.4} {y:.4}");
        for (x, y) in &points[1..] {
            let _ = write!(path, "L{x:.4} {y:.4}");
        }
        path.push('Z');
    }
}

fn segment(path: &mut String, a: Point, b: Point, brush: &Brush) {
    let (arx, ary) = radii(a, brush);
    let (brx, bry) = radii(b, brush);
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length = dx.hypot(dy);
    if length < 1e-10 {
        return;
    }
    if brush.rectangle {
        let mut corners = [(0.0, 0.0); 8];
        for (index, (point, rx, ry)) in [(a, arx, ary), (b, brx, bry)].into_iter().enumerate() {
            for (corner, (sx, sy)) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .into_iter()
                .enumerate()
            {
                corners[index * 4 + corner] = (point.x + sx * rx, point.y + sy * ry);
            }
        }
        corners.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let cross = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| {
            (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
        };
        // Construct each half separately so collinear brush edges stay bounded.
        let mut lower = Vec::new();
        let mut upper = Vec::new();
        for point in corners {
            while lower.len() >= 2
                && cross(lower[lower.len() - 2], lower[lower.len() - 1], point) <= 0.0
            {
                lower.pop();
            }
            lower.push(point);
        }
        for point in corners.into_iter().rev() {
            while upper.len() >= 2
                && cross(upper[upper.len() - 2], upper[upper.len() - 1], point) <= 0.0
            {
                upper.pop();
            }
            upper.push(point);
        }
        lower.pop();
        upper.pop();
        lower.extend(upper);
        polygon(path, &lower);
    } else {
        let (nx, ny) = (-dy / length, dx / length);
        let support = |rx: f64, ry: f64| {
            let length = (rx * nx).hypot(ry * ny);
            if length == 0.0 {
                (0.0, 0.0)
            } else {
                (rx * rx * nx / length, ry * ry * ny / length)
            }
        };
        let (ax, ay) = support(arx, ary);
        let (bx, by) = support(brx, bry);
        polygon(
            path,
            &[
                (a.x + ax, a.y + ay),
                (b.x + bx, b.y + by),
                (b.x - bx, b.y - by),
                (a.x - ax, a.y - ay),
            ],
        );
    }
}

/// Render bounded InkML traces in their declared physical coordinate frame.
pub fn render_ink(xml: &str, width: f64, height: f64) -> Option<RenderedPicture> {
    if xml.len() > 1024 * 1024
        || !width.is_finite()
        || !height.is_finite()
        || !(0.0..=1e6).contains(&width)
        || !(0.0..=1e6).contains(&height)
        || width == 0.0
        || height == 0.0
    {
        return None;
    }
    let document = Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 16_384,
        },
    )
    .ok()?;
    if !named(document.root_element(), "ink")
        || document
            .descendants()
            .any(|node| named(node, "canvasTransform") || named(node, "channelTransform"))
    {
        return None;
    }
    let mut body = String::new();
    let mut remaining = MAX_POINTS;
    let mut remaining_specks = MAX_POINTS;
    let mut remaining_raster_pixels = 16 * 1024 * 1024;
    let (mut left, mut top, mut right, mut bottom) = (0.0_f64, 0.0_f64, width, height);
    let mut count = 0;
    for trace in document.descendants().filter(|node| {
        named(*node, "trace")
            && !node
                .ancestors()
                .any(|ancestor| named(ancestor, "definitions"))
    }) {
        count += 1;
        if count > 1024 || trace.attribute("continuation").is_some() {
            return None;
        }
        let contexts = context_chain(&document, trace)?;
        let channels = channels(&document, &contexts)?;
        if trace.attribute("type") == Some("penUp")
            && !channels.iter().any(|channel| channel.name == "S")
        {
            continue;
        }
        let brush = brush(&document, trace, &contexts)?;
        if brush.pencil
            && channels
                .iter()
                .any(|channel| matches!(channel.name.as_str(), "OTx" | "OTy" | "OA" | "OE"))
        {
            return None;
        }
        let text = trace
            .children()
            .filter_map(|node| node.text())
            .collect::<String>();
        let points = points(&text, &channels, &mut remaining)?;
        let mut path = String::new();
        for (index, point) in points.iter().copied().enumerate() {
            if !point.contact {
                continue;
            }
            let (rx, ry) = radii(point, &brush);
            left = left.min(point.x - rx);
            top = top.min(point.y - ry);
            right = right.max(point.x + rx);
            bottom = bottom.max(point.y + ry);
            if !brush.pencil && index > 0 && points[index - 1].contact {
                segment(&mut path, points[index - 1], point, &brush);
            }
            if !brush.pencil {
                footprint(&mut path, point, rx, ry, brush.rectangle);
            }
            if path.len() + body.len() > MAX_OUTPUT {
                return None;
            }
        }
        if brush.pencil {
            let (x1, y1, x2, y2) = pencil(
                &mut body,
                &points,
                &brush,
                &mut remaining_specks,
                &mut remaining_raster_pixels,
            )?;
            left = left.min(x1);
            top = top.min(y1);
            right = right.max(x2);
            bottom = bottom.max(y2);
        } else {
            let _ = write!(
                body,
                "<path fill=\"{}\" opacity=\"{:.6}\" d=\"{path}\"/>",
                brush.color, brush.opacity
            );
        }
        if body.len() > MAX_OUTPUT {
            return None;
        }
    }
    if count == 0 {
        return None;
    }
    let paint_width = right - left;
    let paint_height = bottom - top;
    let svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{paint_width}\" height=\"{paint_height}\" viewBox=\"{left} {top} {paint_width} {paint_height}\">{body}</svg>");
    Some(RenderedPicture {
        svg,
        paint_bounds: Some(PaintBounds {
            x_px: left,
            y_px: top,
            width_px: paint_width,
            height_px: paint_height,
            layout_width_px: width,
            layout_height_px: height,
        }),
    })
}

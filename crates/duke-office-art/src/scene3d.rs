use std::fmt::Write;

use kurbo::{BezPath, PathEl, Point, Shape};

use super::{child, fill_node, paint, stroke, Paint, PictureContext};
use crate::geometry::{GeometryPath, PathFill};
use crate::xml::{escape_xml_text, get_attribute_ref};
use crate::{number, percentage};

#[derive(Clone, Copy, Debug)]
struct V3(f64, f64, f64);

impl V3 {
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0, self.1 + other.1, self.2 + other.2)
    }
    fn mul(self, value: f64) -> Self {
        Self(self.0 * value, self.1 * value, self.2 * value)
    }
    fn dot(self, other: Self) -> f64 {
        self.0 * other.0 + self.1 * other.1 + self.2 * other.2
    }
    fn normalized(self) -> Self {
        self.mul(1.0 / self.dot(self).sqrt().max(1e-12))
    }
}

#[derive(Clone, Copy)]
struct Rotation([f64; 3]);

impl Rotation {
    fn apply(self, V3(x, y, z): V3) -> V3 {
        let [lat, lon, rev] = self.0.map(f64::to_radians);
        let (sl, cl) = libm::sincos(lon);
        let (x, z) = (cl * x - sl * z, sl * x + cl * z);
        let (sl, cl) = libm::sincos(lat);
        let (y, z) = (cl * y + sl * z, -sl * y + cl * z);
        let (sl, cl) = libm::sincos(rev);
        V3(cl * x + sl * y, -sl * x + cl * y, z)
    }

    fn inverse(self, value: V3) -> V3 {
        V3(
            value.dot(self.apply(V3(1.0, 0.0, 0.0))),
            value.dot(self.apply(V3(0.0, 1.0, 0.0))),
            value.dot(self.apply(V3(0.0, 0.0, 1.0))),
        )
    }

    fn from_xml(xml: &str, default: [f64; 3]) -> Option<Self> {
        let Some(xml) = child(xml, "rot") else {
            return Some(Self(default));
        };
        Some(Self([
            number(xml, "lat", 0.0)? / 60000.0,
            number(xml, "lon", 0.0)? / 60000.0,
            number(xml, "rev", 0.0)? / 60000.0,
        ]))
    }
}

#[derive(Clone, Copy)]
struct Light {
    color: [f64; 3],
    direction: V3,
    scale: f64,
    offset: f64,
    specular: bool,
    diffuse: bool,
}

include!("scene3d_data.rs");

struct Camera {
    rotation: Rotation,
    parallel: bool,
    legacy: bool,
    center: Point,
    eye: V3,
    skew: Point,
    zoom: f64,
}

impl Camera {
    fn parse(scene: &str, width: f64, height: f64) -> Option<Self> {
        let xml = child(scene, "camera").unwrap_or("");
        let name = get_attribute_ref(xml, "prst").unwrap_or("orthographicFront");
        let (_, parallel, angles, values) = CAMERAS.iter().find(|(key, ..)| *key == name)?;
        let [ox, oy, skew, skew_angle, ex, ey, ez] = *values;
        let zoom = percentage(xml, "zoom", 1.0)?;
        if !(0.001..=1000.0).contains(&zoom) {
            return None;
        }
        let mut distance = ez * 96.0 / 2540.0;
        if let Some(fov) = get_attribute_ref(xml, "fov") {
            let angle = fov.parse::<f64>().ok()? / 60000.0;
            if !(0.01..179.0).contains(&angle) {
                return None;
            }
            distance = width.max(height) / (2.0 * libm::tan(angle.to_radians() / 2.0));
        }
        Some(Self {
            rotation: Rotation::from_xml(xml, *angles)?,
            parallel: *parallel,
            legacy: name.starts_with("legacy"),
            center: Point::new(width * (0.5 + ox), height * (0.5 + oy)),
            eye: V3(ex * 96.0 / 2540.0, ey * 96.0 / 2540.0, distance),
            skew: Point::new(
                -skew / 100.0 * libm::cos(skew_angle.to_radians()),
                -skew / 100.0 * libm::sin(skew_angle.to_radians()),
            ),
            zoom,
        })
    }

    fn point(&self, point: Point, z: f64) -> Option<V3> {
        let p = self
            .rotation
            .apply(V3(point.x - self.center.x, point.y - self.center.y, z));
        let (x, y) = if self.parallel {
            (p.0 + p.2 * self.skew.x, p.1 + p.2 * self.skew.y)
        } else {
            let denominator = self.eye.2 - p.2;
            if denominator <= 0.01 {
                return None;
            }
            let scale = self.eye.2 / denominator;
            (
                self.eye.0 + (p.0 - self.eye.0) * scale,
                self.eye.1 + (p.1 - self.eye.1) * scale,
            )
        };
        let result = V3(
            self.center.x + x * self.zoom,
            self.center.y + y * self.zoom,
            p.2,
        );
        [result.0, result.1, result.2]
            .into_iter()
            .all(|v| v.is_finite() && v.abs() < 1e8)
            .then_some(result)
    }

    fn view(&self, point: Point, z: f64) -> V3 {
        if self.parallel {
            return self.rotation.inverse(V3(0.0, 0.0, 1.0));
        }
        self.rotation
            .inverse(self.eye)
            .add(V3(self.center.x - point.x, self.center.y - point.y, -z))
            .normalized()
    }
}

struct Material {
    diffuse: f64,
    specular: f64,
    power: f64,
    specular_tint: [f64; 2],
    diffuse_tint: [f64; 2],
    wireframe: bool,
    blinn: bool,
    emissive: f64,
}

impl Material {
    fn parse(xml: &str) -> Option<Self> {
        let name = get_attribute_ref(xml, "prstMaterial").unwrap_or("warmMatte");
        let (diffuse, specular, power, metal, wireframe) = match name {
            "clear" => (1.0, 0.6, 20.0, false, false),
            "dkEdge" => (0.7, 1.0, 35.0, false, false),
            "flat" => (0.0, 0.6, 50.0, false, false),
            "legacyMatte" | "matte" => (1.0, 0.0, 1.0, false, false),
            "legacyMetal" => (2.0 / 3.0, 1.0, 32.0, true, false),
            "legacyPlastic" => (1.0, 1.0, 32.0, false, false),
            "metal" => (1.0, 1.0, 12.0, true, false),
            "plastic" => (1.0, 0.6, 12.0, true, false),
            "powder" => (1.0, 0.3, 10.0, false, false),
            "softEdge" => (1.0, 1.0, 35.0, false, false),
            "softmetal" => (1.0, 1.0, 8.0, true, false),
            "translucentPowder" => (1.0, 0.3, 10.0, false, false),
            "warmMatte" => (1.0, 0.3, 8.0, false, false),
            "legacyWireframe" => (1.0, 0.0, 1.0, false, true),
            _ => return None,
        };
        Some(Self {
            diffuse,
            specular,
            power,
            specular_tint: match name {
                "metal" => [-0.32, 1.32],
                "softmetal" => [0.5, 0.5],
                "plastic" => [0.0, 1.0],
                _ if metal => [1.0, 0.0],
                _ => [0.0, 1.0],
            },
            diffuse_tint: if name == "powder" {
                [0.5, 0.5]
            } else {
                [1.0, 0.0]
            },
            wireframe,
            blinn: matches!(
                name,
                "legacyMatte" | "legacyPlastic" | "legacyMetal" | "powder" | "translucentPowder"
            ),
            emissive: if name == "flat" { 1.0 } else { 0.0 },
        })
    }
}

struct Lighting {
    ambient: f64,
    lights: Vec<Light>,
    material: Material,
}

impl Lighting {
    fn parse(scene: &str, properties: &str, camera: &Camera) -> Option<Self> {
        let xml = child(scene, "lightRig").unwrap_or("");
        let name = get_attribute_ref(xml, "rig").unwrap_or("threePt");
        let (_, ambient, preset) = LIGHTS.iter().find(|(key, ..)| *key == name)?;
        let angle = match get_attribute_ref(xml, "dir").unwrap_or("t") {
            "t" => 0.0,
            "tr" => 45.0,
            "r" => 90.0,
            "br" => 135.0,
            "b" => 180.0,
            "bl" => 225.0,
            "l" => 270.0,
            "tl" => 315.0,
            _ => return None,
        };
        let rotation = Rotation::from_xml(xml, [0.0, 0.0, -angle])?;
        let lights = preset
            .iter()
            .map(|light| {
                // The declared directions describe incoming light in the rig's coordinate system.
                let V3(x, y, z) = light.direction;
                let mut direction = rotation.apply(V3(y, x, -z).normalized());
                if camera.legacy {
                    direction = camera.rotation.inverse(direction);
                }
                Light {
                    direction,
                    ..*light
                }
            })
            .collect();
        Some(Self {
            ambient: *ambient,
            lights,
            material: Material::parse(properties)?,
        })
    }

    fn channels(&self, normal: V3, view: V3) -> ([f64; 3], [f64; 3]) {
        let mut diffuse = [self.ambient * self.material.diffuse; 3];
        let mut specular = [0.0; 3];
        for light in &self.lights {
            let dot = normal.dot(light.direction);
            let strength = (dot * light.scale + light.offset).max(0.0);
            let reflection = normal.mul(2.0 * dot).add(light.direction.mul(-1.0));
            let alignment = if self.material.blinn {
                normal.dot(light.direction.add(view).normalized())
            } else {
                reflection.dot(view)
            };
            let highlight = if dot > 0.0 {
                libm::pow(alignment.max(0.0), self.material.power) * self.material.specular
            } else {
                0.0
            };
            for i in 0..3 {
                if light.diffuse {
                    diffuse[i] += light.color[i] * strength * self.material.diffuse;
                }
                if light.specular {
                    specular[i] += light.color[i] * highlight;
                }
            }
        }
        (diffuse, specular)
    }

    fn color(&self, base: &Paint, normal: V3, view: V3, defs: &mut String, id: usize) -> Paint {
        let (diffuse, specular) = self.channels(normal, view);
        let color = base
            .value
            .strip_prefix('#')
            .filter(|v| v.len() == 6)
            .and_then(|hex| u32::from_str_radix(hex, 16).ok());
        if let Some(rgb) = color {
            let mut values = [0u8; 3];
            for i in 0..3 {
                let channel = ((rgb >> (16 - i * 8)) & 255) as f64;
                let [scale, bias] = self.material.specular_tint;
                let highlight = channel * scale + 255.0 * bias;
                let [scale, bias] = self.material.diffuse_tint;
                let base = channel * scale + 255.0 * bias;
                values[i] = (base * diffuse[i]
                    + highlight * specular[i]
                    + channel * self.material.emissive)
                    .clamp(0.0, 255.0) as u8;
            }
            return Paint {
                value: format!("#{:02x}{:02x}{:02x}", values[0], values[1], values[2]),
                opacity: base.opacity,
            };
        }
        let _ = write!(
            defs,
            r##"<filter id="shape-3d-light-{id}" color-interpolation-filters="sRGB"><feComponentTransfer>"##
        );
        for (i, channel) in ["R", "G", "B"].into_iter().enumerate() {
            let slope = diffuse[i] * self.material.diffuse_tint[0]
                + specular[i] * self.material.specular_tint[0]
                + self.material.emissive;
            let intercept = diffuse[i] * self.material.diffuse_tint[1]
                + specular[i] * self.material.specular_tint[1];
            let _ = write!(
                defs,
                r##"<feFunc{channel} type="linear" slope="{slope}" intercept="{intercept}"/>"##
            );
        }
        defs.push_str("</feComponentTransfer></filter>");
        base.clone()
    }
}

struct Bevel {
    width: f64,
    height: f64,
    profile: &'static [(f64, f64)],
}

const ROUND: &[(f64, f64)] = &[
    (0.0, 0.0),
    (0.034074, 0.258819),
    (0.133975, 0.5),
    (0.292893, 0.707107),
    (0.5, 0.866025),
    (0.741181, 0.965926),
    (1.0, 1.0),
];
const ANGLE: &[(f64, f64)] = &[(0.0, 0.0), (1.0, 1.0)];
const INSET: &[(f64, f64)] = &[
    (0.0, 0.0),
    (0.258819, 0.034074),
    (0.5, 0.133975),
    (0.707107, 0.292893),
    (0.866025, 0.5),
    (0.965926, 0.741181),
    (1.0, 1.0),
];
const STEP: &[(f64, f64)] = &[(0.0, 0.0), (0.5, 0.0), (0.5, 0.5), (1.0, 0.5), (1.0, 1.0)];

impl Bevel {
    fn parse(xml: Option<&str>, width: f64, height: f64) -> Option<Self> {
        let Some(xml) = xml else {
            return Some(Self {
                width: 0.0,
                height: 0.0,
                profile: ANGLE,
            });
        };
        let w = number(xml, "w", 76200.0)? / 9525.0;
        let h = number(xml, "h", 76200.0)? / 9525.0;
        if w < 0.0 || h < 0.0 || w > 100000.0 || h > 100000.0 {
            return None;
        }
        let profile = match get_attribute_ref(xml, "prst").unwrap_or("circle") {
            "circle" | "softRound" | "convex" => ROUND,
            "angle" | "divot" => ANGLE,
            "relaxedInset" | "coolSlant" => INSET,
            "artDeco" | "riblet" | "hardEdge" | "slope" => STEP,
            _ => return None,
        };
        Some(Self {
            width: w.min(width.min(height) / 2.0),
            height: h,
            profile,
        })
    }
}

fn bevel_height(bevel: &Bevel, offset: f64) -> f64 {
    if bevel.width <= 0.0 {
        return 0.0;
    }
    let fraction = (offset / bevel.width).clamp(0.0, 1.0);
    for pair in bevel.profile.windows(2) {
        if fraction <= pair[1].0 && pair[1].0 > pair[0].0 {
            let t = (fraction - pair[0].0) / (pair[1].0 - pair[0].0);
            return bevel.height * (pair[0].1 * (1.0 - t) + pair[1].1 * t);
        }
    }
    bevel.height
}

struct Contour {
    points: Vec<Point>,
    closed: bool,
}

fn distance(a: Point, b: Point) -> f64 {
    libm::hypot(a.x - b.x, a.y - b.y)
}

fn flatten_cubic(
    points: &mut Vec<Point>,
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    depth: u8,
    count: &mut usize,
) -> Option<()> {
    if *count >= 16384 {
        return None;
    }
    let chord = p3 - p0;
    let length = libm::hypot(chord.x, chord.y);
    let distance = |p: Point| {
        if length < 1e-10 {
            distance(p, p0)
        } else {
            ((p - p0).cross(chord) / length).abs()
        }
    };
    if depth == 16 || distance(p1).max(distance(p2)) <= 0.15 {
        points.push(p3);
        *count += 1;
        return Some(());
    }
    let a = p0.midpoint(p1);
    let b = p1.midpoint(p2);
    let c = p2.midpoint(p3);
    let d = a.midpoint(b);
    let e = b.midpoint(c);
    let middle = d.midpoint(e);
    flatten_cubic(points, p0, a, d, middle, depth + 1, count)?;
    flatten_cubic(points, middle, e, c, p3, depth + 1, count)
}

fn contours(path: &str) -> Option<Vec<Contour>> {
    let parsed = BezPath::from_svg(path).ok()?;
    if parsed.elements().len() > 8192 {
        return None;
    }
    let valid = |p: Point| p.x.is_finite() && p.y.is_finite() && p.x.abs().max(p.y.abs()) <= 1e8;
    let mut result = Vec::<Contour>::new();
    let mut count = 0;
    for element in parsed.elements() {
        if count >= 16384 {
            return None;
        }
        match *element {
            PathEl::MoveTo(p) if valid(p) => {
                result.push(Contour {
                    points: vec![p],
                    closed: false,
                });
                count += 1;
            }
            PathEl::LineTo(p) if valid(p) => {
                let points = &mut result.last_mut()?.points;
                if points.last().is_none_or(|last| distance(*last, p) > 1e-7) {
                    points.push(p);
                    count += 1;
                }
            }
            PathEl::QuadTo(control, end) if valid(control) && valid(end) => {
                let points = &mut result.last_mut()?.points;
                let start = *points.last()?;
                flatten_cubic(
                    points,
                    start,
                    start.lerp(control, 2.0 / 3.0),
                    end.lerp(control, 2.0 / 3.0),
                    end,
                    0,
                    &mut count,
                )?;
            }
            PathEl::CurveTo(a, b, end) if valid(a) && valid(b) && valid(end) => {
                let points = &mut result.last_mut()?.points;
                flatten_cubic(points, *points.last()?, a, b, end, 0, &mut count)?;
            }
            PathEl::ClosePath => {
                result.last_mut()?.closed = true;
            }
            _ => return None,
        }
    }
    for contour in &mut result {
        if contour.closed
            && contour.points.len() > 1
            && distance(contour.points[0], *contour.points.last()?) < 1e-7
        {
            contour.points.pop();
        }
    }
    Some(result)
}

fn signed_area(points: &[Point]) -> f64 {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum::<f64>()
        / 2.0
}

fn outward(points: &[Point], index: usize, orientation: f64) -> Point {
    let a = points[index];
    let b = points[(index + 1) % points.len()];
    let length = distance(a, b).max(1e-10);
    Point::new(
        orientation * (b.y - a.y) / length,
        orientation * (a.x - b.x) / length,
    )
}

fn inset(points: &[Point], amount: f64, orientation: f64) -> Vec<Point> {
    points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let a = outward(points, (i + points.len() - 1) % points.len(), orientation);
            let b = outward(points, i, orientation);
            let denominator = (1.0 + a.x * b.x + a.y * b.y).max(0.2);
            Point::new(
                p.x - amount * (a.x + b.x) / denominator,
                p.y - amount * (a.y + b.y) / denominator,
            )
        })
        .collect()
}

fn safe_inset(path: &str, contours: &[Contour], requested: f64, orientation: f64) -> Option<f64> {
    if requested == 0.0 {
        return Some(0.0);
    }
    let original = BezPath::from_svg(path).ok()?;
    if contours
        .iter()
        .map(|c| c.points.len())
        .sum::<usize>()
        .saturating_mul(original.elements().len())
        > 2_000_000
    {
        return None;
    }

    let valid = |distance: f64| {
        contours
            .iter()
            .filter(|contour| contour.closed && contour.points.len() > 2)
            .all(|contour| {
                let candidate = inset(&contour.points, distance, orientation);
                candidate.iter().enumerate().all(|(i, p)| {
                    let next = (i + 1) % candidate.len();
                    original.winding(p.lerp(candidate[next], 0.5)) != 0
                })
            })
    };
    if valid(requested) {
        return Some(requested);
    }
    let (mut low, mut high) = (0.0, requested);
    for _ in 0..12 {
        let middle = (low + high) / 2.0;
        if valid(middle) {
            low = middle;
        } else {
            high = middle;
        }
    }
    Some(low * 0.95)
}

struct Face {
    depth: f64,
    body: String,
    path: String,
}
pub(super) struct Scene {
    pub body: String,
    pub silhouette: String,
    pub outside: f64,
    pub bounds: [f64; 4],
}

struct Mesh<'a> {
    camera: &'a Camera,
    lighting: &'a Lighting,
    defs: &'a mut String,
    faces: Vec<Face>,
    bounds: [f64; 4],
    id: usize,
}

impl Mesh<'_> {
    fn project(&mut self, points: &[Point], z: f64) -> Option<Vec<V3>> {
        points
            .iter()
            .map(|p| {
                let v = self.camera.point(*p, z)?;
                self.bounds[0] = self.bounds[0].min(v.0);
                self.bounds[1] = self.bounds[1].min(v.1);
                self.bounds[2] = self.bounds[2].max(v.0);
                self.bounds[3] = self.bounds[3].max(v.1);
                Some(v)
            })
            .collect()
    }

    fn face(
        &mut self,
        vertices: &[V3],
        normal: V3,
        center: Point,
        z: f64,
        base: &Paint,
    ) -> Option<()> {
        if self.faces.len() > 131072 {
            return None;
        }
        if base.value == "none" || normal.dot(self.camera.view(center, z)) <= 0.0 {
            return Some(());
        }
        let mut path = String::new();
        for (i, v) in vertices.iter().enumerate() {
            let _ = write!(
                path,
                "{}{:.4} {:.4}",
                if i == 0 { "M" } else { "L" },
                v.0,
                v.1
            );
        }
        path.push('Z');
        let body = self.painted(&path, normal, center, z, base, true);
        self.faces.push(Face {
            depth: vertices.iter().map(|v| v.2).sum::<f64>() / vertices.len() as f64,
            body,
            path,
        });
        Some(())
    }

    fn painted(
        &mut self,
        path: &str,
        normal: V3,
        center: Point,
        z: f64,
        base: &Paint,
        seam: bool,
    ) -> String {
        let id = self.id;
        self.id += 1;
        let paint = self
            .lighting
            .color(base, normal, self.camera.view(center, z), self.defs, id);
        let filter = if paint.value.starts_with("url(") {
            format!(r##" filter="url(#shape-3d-light-{id})""##)
        } else {
            String::new()
        };
        if self.lighting.material.wireframe {
            return format!(
                r##"<path d="{path}" fill="none" stroke="{}" stroke-width="0.75" opacity="{}"{filter}/>"##,
                escape_xml_text(&paint.value),
                paint.opacity
            );
        }
        let stroke = if seam && paint.opacity >= 0.999 {
            format!(
                r##" stroke="{}" stroke-width="0.25" stroke-linejoin="round""##,
                escape_xml_text(&paint.value)
            )
        } else {
            String::new()
        };
        format!(
            r##"<path d="{path}" fill="{}" fill-opacity="{}"{stroke}{filter}/>"##,
            escape_xml_text(&paint.value),
            paint.opacity
        )
    }

    fn connect(
        &mut self,
        a: &[Point],
        za: f64,
        b: &[Point],
        zb: f64,
        orientation: f64,
        base: &Paint,
    ) -> Option<()> {
        let pa = self.project(a, za)?;
        let pb = self.project(b, zb)?;
        for i in 0..a.len() {
            let next = (i + 1) % a.len();
            let edge = outward(a, i, orientation);
            let dx = ((b[i].x - a[i].x) + (b[next].x - a[next].x)) / 2.0;
            let dy = ((b[i].y - a[i].y) + (b[next].y - a[next].y)) / 2.0;
            let distance = dx * edge.x + dy * edge.y;
            let dz = zb - za;
            let normal = V3(
                edge.x * dz.abs(),
                edge.y * dz.abs(),
                -distance * dz.signum(),
            )
            .normalized();
            let center = Point::new(
                (a[i].x + a[next].x + b[i].x + b[next].x) / 4.0,
                (a[i].y + a[next].y + b[i].y + b[next].y) / 4.0,
            );
            self.face(
                &[pa[i], pa[next], pb[next], pb[i]],
                normal,
                center,
                (za + zb) / 2.0,
                base,
            )?;
        }
        Some(())
    }
}

pub(super) fn render(
    paths: &[GeometryPath],
    properties: &str,
    fill: Option<&str>,
    line: Option<&str>,
    width: f64,
    height: f64,
    context: &impl PictureContext,
    defs: &mut String,
) -> Option<Scene> {
    let scene = child(properties, "scene3d").unwrap_or("");
    let properties3d = child(properties, "sp3d")
        .or_else(|| child(properties, "props3d"))
        .unwrap_or("");
    let camera = Camera::parse(scene, width, height)?;
    let lighting = Lighting::parse(scene, properties3d, &camera)?;
    let depth = number(properties3d, "extrusionH", 0.0)? / 9525.0;
    let elevation = number(properties3d, "z", 0.0)? / 9525.0;
    let contour_width = number(properties3d, "contourW", 0.0)? / 9525.0;
    if depth < 0.0
        || depth > 100000.0
        || elevation.abs() > 100000.0
        || contour_width < 0.0
        || contour_width > 100000.0
    {
        return None;
    }
    let top = Bevel::parse(child(properties3d, "bevelT"), width, height)?;
    let bottom = Bevel::parse(child(properties3d, "bevelB"), width, height)?;
    let outline = stroke(line, false, width, height, context, defs)?;
    let mut mesh = Mesh {
        camera: &camera,
        lighting: &lighting,
        defs,
        faces: Vec::new(),
        bounds: [0.0, 0.0, width, height],
        id: 0,
    };
    for (index, path) in paths.iter().enumerate() {
        let surface = paint(
            fill,
            false,
            &format!("shape-3d-fill-{index}"),
            width,
            height,
            path.fill,
            context,
            mesh.defs,
        )?;
        let color = |xml: Option<&str>, fallback: &Paint| -> Option<Paint> {
            if let Some(xml) = xml {
                let color = context.resolve_color(xml)?;
                Some(Paint {
                    value: color.color,
                    opacity: color.opacity.unwrap_or(1.0),
                })
            } else {
                Some(fallback.clone())
            }
        };
        let extrusion = color(child(properties3d, "extrusionClr"), &surface)?;
        let outline_paint = paint(
            line.and_then(fill_node),
            false,
            &format!("shape-3d-line-{index}"),
            width,
            height,
            PathFill::Norm,
            context,
            mesh.defs,
        )?;
        let edge = color(
            child(properties3d, "contourClr"),
            if outline_paint.value != "none" {
                &outline_paint
            } else {
                &surface
            },
        )?;
        let subpaths = contours(&path.d)?;
        let has_open_path = subpaths.iter().any(|contour| !contour.closed);
        let exterior_sign = subpaths
            .iter()
            .filter(|c| c.closed)
            .max_by(|a, b| {
                signed_area(&a.points)
                    .abs()
                    .total_cmp(&signed_area(&b.points).abs())
            })
            .map(|c| signed_area(&c.points).signum())
            .unwrap_or(1.0);
        let top = Bevel {
            width: safe_inset(&path.d, &subpaths, top.width, exterior_sign)?,
            height: top.height,
            profile: top.profile,
        };
        let bottom = Bevel {
            width: safe_inset(&path.d, &subpaths, bottom.width, exterior_sign)?,
            height: bottom.height,
            profile: bottom.profile,
        };
        let mut front_path = String::new();
        let mut back_path = String::new();
        let mut front_depth = 0.0;
        let mut back_depth = 0.0;
        let mut point_count = 0;
        for contour in subpaths {
            let points = &contour.points;
            if points.len() < 2 {
                continue;
            }
            let closed = contour.closed && points.len() > 2;
            let orientation = exterior_sign;
            let front = if closed {
                inset(points, top.width, orientation)
            } else {
                points.clone()
            };
            let back = if closed {
                inset(points, bottom.width, orientation)
            } else {
                points.clone()
            };
            let front_z = elevation + top.height;
            let back_z = elevation - depth - bottom.height;
            let projected_front = mesh.project(&front, front_z)?;
            let projected_back = mesh.project(&back, back_z)?;
            for (i, (a, b)) in projected_front.iter().zip(&projected_back).enumerate() {
                let command = if i == 0 { "M" } else { "L" };
                let _ = write!(front_path, "{command}{:.4} {:.4}", a.0, a.1);
                let _ = write!(back_path, "{command}{:.4} {:.4}", b.0, b.1);
                front_depth += a.2;
                back_depth += b.2;
                point_count += 1;
            }
            if closed {
                front_path.push('Z');
                back_path.push('Z');
                mesh.connect(
                    points,
                    elevation - depth,
                    points,
                    elevation,
                    orientation,
                    &extrusion,
                )?;
                if top.width > 0.0 && top.height > 0.0 {
                    for profile in top.profile.windows(2) {
                        let a = inset(points, top.width * profile[0].0, orientation);
                        let b = inset(points, top.width * profile[1].0, orientation);
                        mesh.connect(
                            &a,
                            elevation + top.height * profile[0].1,
                            &b,
                            elevation + top.height * profile[1].1,
                            orientation,
                            &surface,
                        )?;
                    }
                }
                if bottom.width > 0.0 && bottom.height > 0.0 {
                    for profile in bottom.profile.windows(2) {
                        let a = inset(points, bottom.width * profile[0].0, orientation);
                        let b = inset(points, bottom.width * profile[1].0, orientation);
                        mesh.connect(
                            &a,
                            elevation - depth - bottom.height * profile[0].1,
                            &b,
                            elevation - depth - bottom.height * profile[1].1,
                            orientation,
                            &extrusion,
                        )?;
                    }
                }
                if path.stroke && outline.width > 0.0 {
                    let a = inset(points, -outline.width / 2.0, orientation);
                    let b = inset(points, outline.width / 2.0, orientation);
                    mesh.connect(
                        &a,
                        elevation,
                        &b,
                        elevation + bevel_height(&top, outline.width / 2.0) + 0.02,
                        orientation,
                        &outline_paint,
                    )?;
                }
            }
        }
        if point_count == 0 {
            continue;
        }
        let center = Point::new(width / 2.0, height / 2.0);
        for (path, depth_order, normal, z) in [
            (
                back_path,
                back_depth,
                V3(0.0, 0.0, -1.0),
                elevation - depth - bottom.height,
            ),
            (
                front_path,
                front_depth,
                V3(0.0, 0.0, 1.0),
                elevation + top.height,
            ),
        ] {
            if normal.dot(camera.view(center, z)) <= 0.0 {
                continue;
            }
            let mut body = if surface.value == "none" {
                String::new()
            } else {
                mesh.painted(&path, normal, center, z, &surface, false)
            };
            if has_open_path && outline.width > 0.0 {
                let _ = write!(
                    body,
                    r##"<path d="{path}" fill="none" {}/>"##,
                    outline.attributes
                );
            }
            mesh.faces.push(Face {
                depth: depth_order / point_count as f64,
                body,
                path,
            });
        }
        if contour_width > 0.0 && edge.value != "none" {
            for contour in contours(&path.d)? {
                if !contour.closed {
                    continue;
                }
                let projected = mesh.project(&contour.points, elevation - depth)?;
                let mut path = String::new();
                for (i, v) in projected.iter().enumerate() {
                    let _ = write!(
                        path,
                        "{}{:.4} {:.4}",
                        if i == 0 { "M" } else { "L" },
                        v.0,
                        v.1
                    );
                }
                path.push('Z');
                let body = format!(
                    r##"<path d="{path}" fill="none" stroke="{}" stroke-opacity="{}" stroke-width="{contour_width}" stroke-linejoin="round"/>"##,
                    escape_xml_text(&edge.value),
                    edge.opacity
                );
                mesh.faces.push(Face {
                    depth: projected.iter().map(|p| p.2).sum::<f64>() / projected.len() as f64,
                    body,
                    path,
                });
            }
        }
    }
    mesh.faces.sort_by(|a, b| a.depth.total_cmp(&b.depth));
    let mut body = String::new();
    let mut silhouette = String::new();
    for face in mesh.faces {
        body.push_str(&face.body);
        silhouette.push_str(&face.path);
        silhouette.push(' ');
    }
    let outside = (-mesh.bounds[0])
        .max(-mesh.bounds[1])
        .max(mesh.bounds[2] - width)
        .max(mesh.bounds[3] - height)
        + outline.margin.max(contour_width / 2.0);
    let margin = contour_width / 2.0 + 0.25;
    let bounds = [
        mesh.bounds[0] - margin,
        mesh.bounds[1] - margin,
        mesh.bounds[2] + margin,
        mesh.bounds[3] + margin,
    ];
    Some(Scene {
        body,
        silhouette,
        outside,
        bounds,
    })
}

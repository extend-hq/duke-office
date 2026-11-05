use super::*;
use crate::chart::child;
use std::collections::BTreeMap;

#[path = "../cartesian_labels.rs"]
mod data_labels;
use data_labels::DataLabels;

fn val<'a>(node: Node<'a, 'a>, name: &str) -> Option<&'a str> {
    setting(node, name)
}

fn setting<'a>(node: Node<'a, 'a>, name: &str) -> Option<&'a str> {
    child(node, name)?.attribute("val")
}

fn gap(node: Node, key: &str) -> f64 {
    setting(node, key)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(150)
        .min(500) as f64
        / 100.
}

fn point_value(point: &crate::chart::ChartPoint) -> Option<f64> {
    super::number(point.value.as_deref())
}

fn plot_values(
    points: &[crate::chart::ChartPoint],
    count: u64,
    zero_blanks: bool,
) -> Vec<(u32, Option<f64>)> {
    let mut values = Vec::with_capacity(points.len());
    let mut next = 0u64;
    for point in points {
        let index = u64::from(point.index);
        if zero_blanks && next < index {
            values.push((next as u32, Some(0.)));
            if index - next > 1 {
                values.push(((index - 1) as u32, Some(0.)));
            }
        }
        values.push((point.index, point_value(point)));
        next = index + 1;
    }
    if zero_blanks && next < count {
        values.push((next as u32, Some(0.)));
        if count - next > 1 {
            values.push(((count - 1) as u32, Some(0.)));
        }
    }
    values
}

struct Strip {
    index: u32,
    x: f64,
    bottom: f64,
    top: f64,
}

#[derive(Clone, Copy)]
struct Extrusion<'a> {
    scene: &'a Scene,
    near: f64,
    far: f64,
    fill: &'a str,
    stroke: &'a str,
    material: Material,
    gradient: &'a [(f64, String)],
    paint_id: &'a str,
    value_range: [f64; 2],
    bevel: Option<[f64; 2]>,
    top_cap: bool,
    bottom_cap: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Material {
    Matte,
    Flat,
    TranslucentPowder,
}

fn gradient_at(stops: &[(f64, String)], position: f64) -> String {
    let Some(first) = stops.first() else {
        return String::new();
    };
    let mut previous = first;
    for stop in stops.iter().skip(1) {
        if position <= stop.0 {
            let t = ((position - previous.0) / (stop.0 - previous.0).max(f64::MIN_POSITIVE))
                .clamp(0., 1.);
            let mut color = String::from("#");
            for channel in [1, 3, 5] {
                let a = previous
                    .1
                    .get(channel..channel + 2)
                    .and_then(|v| u8::from_str_radix(v, 16).ok())
                    .unwrap_or(0);
                let b = stop
                    .1
                    .get(channel..channel + 2)
                    .and_then(|v| u8::from_str_radix(v, 16).ok())
                    .unwrap_or(0);
                write!(
                    color,
                    "{:02x}",
                    (f64::from(a) * (1. - t) + f64::from(b) * t).round() as u8
                )
                .unwrap();
            }
            return color;
        }
        previous = stop;
    }
    previous.1.clone()
}

fn material(properties: Option<Node>, fallback: Option<&str>) -> Material {
    match properties
        .and_then(|sp| sp.children().find(|n| n.tag_name().name() == "sp3d"))
        .and_then(|sp| sp.attribute("prstMaterial"))
        .or(fallback)
    {
        Some("flat") => Material::Flat,
        Some("translucentPowder") => Material::TranslucentPowder,
        _ => Material::Matte,
    }
}

fn floor_shadow(
    out: &mut String,
    scene: &Scene,
    colors: &ChartColors,
    shadow: Node,
    category: [f64; 2],
    depth: [f64; 2],
    caster_height: f64,
    shape: &str,
    source: usize,
    index: u32,
) {
    let Some(color) = colors.solid_color(shadow) else {
        return;
    };
    let attribute = |name, fallback| number(shadow.attribute(name)).unwrap_or(fallback);
    let blur = (attribute("blurRad", 0.) / 9525.).clamp(0., 256.) / 2.;
    let distance = attribute("dist", 0.) / 9525.;
    let angle = (attribute("dir", 0.) / 60000.).to_radians();
    let floor = [
        scene.project(0., 0., 0.),
        scene.project(1., 0., 0.),
        scene.project(1., 0., 1.),
        scene.project(0., 0., 1.),
    ];
    let x = floor.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let y = floor.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let width = floor.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max) - x;
    let height = floor.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max) - y;
    if width <= 0. || height <= 0. {
        return;
    }
    let id = format!("c3df{source}_{index}");
    write!(out, r#"<defs><clipPath id="{id}c">"#).unwrap();
    polygon(out, &floor, "white", "none");
    write!(out, r#"</clipPath><filter id="{id}" filterUnits="userSpaceOnUse" x="{x:.2}" y="{y:.2}" width="{width:.2}" height="{height:.2}" color-interpolation-filters="sRGB"><feGaussianBlur stdDeviation="{blur:.3}"/></filter></defs><g data-chart-effect="true" pointer-events="none" clip-path="url(#{id}c)"><g filter="url(#{id})" transform="translate({:.3} {:.3})">"#, angle.cos()*distance, angle.sin()*distance).unwrap();
    let footprint = if matches!(shape, "cylinder" | "cone" | "coneToMax") {
        (0..64)
            .map(|i| {
                let angle = i as f64 * std::f64::consts::TAU / 64.;
                [
                    (category[0] + category[1]) / 2.
                        + (category[1] - category[0]) / 2. * angle.cos(),
                    (depth[0] + depth[1]) / 2. + (depth[1] - depth[0]) / 2. * angle.sin(),
                ]
            })
            .collect::<Vec<_>>()
    } else {
        vec![
            [category[0], depth[0]],
            [category[1], depth[0]],
            [category[1], depth[1]],
            [category[0], depth[1]],
        ]
    };
    let cast = caster_height * scene.model_height / 0.6;
    let mut points = footprint
        .iter()
        .copied()
        .chain(footprint.iter().map(|p| {
            [
                p[0] + cast * angle.cos(),
                p[1] + cast * angle.sin() / scene.model_depth,
            ]
        }))
        .collect::<Vec<_>>();
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    let cross = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let half = |points: &[[f64; 2]]| {
        let mut hull = Vec::new();
        for point in points {
            while hull.len() >= 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], *point) <= 0.
            {
                hull.pop();
            }
            hull.push(*point);
        }
        hull.pop();
        hull
    };
    let mut hull = half(&points);
    points.reverse();
    hull.extend(half(&points));
    for (axis, edge, direction) in [(0, 0., 1.), (0, 1., -1.), (1, 0., 1.), (1, 1., -1.)] {
        let mut clipped = Vec::new();
        for (a, b) in hull
            .iter()
            .zip(hull.iter().cycle().skip(1))
            .take(hull.len())
        {
            let inside_a = (a[axis] - edge) * direction >= 0.;
            let inside_b = (b[axis] - edge) * direction >= 0.;
            if inside_a {
                clipped.push(*a);
            }
            if inside_a != inside_b {
                let t = (edge - a[axis]) / (b[axis] - a[axis]);
                clipped.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
            }
        }
        hull = clipped;
    }
    let points = hull
        .iter()
        .map(|p| scene.project(p[0], 0., p[1]))
        .collect::<Vec<_>>();
    polygon(out, &points, &color, "none");
    out.push_str("</g></g>");
}

impl Extrusion<'_> {
    fn face(&self, out: &mut String, points: &[[f64; 3]; 4], light: f64) {
        let projected = points.map(|[x, y, z]| self.scene.project(x, y, z));
        let cross = projected
            .iter()
            .zip(projected.iter().cycle().skip(1))
            .map(|(a, b)| a.x * b.y - b.x * a.y)
            .sum::<f64>();
        if cross < -1e-9 {
            let light = if self.material == Material::Flat {
                1.
            } else {
                light
            };
            let fill = if self.gradient.is_empty() {
                shade(self.fill, light)
            } else if points.iter().all(|p| (p[1] - points[0][1]).abs() < 1e-9) {
                let t = ((self.value_range[1] - points[0][1])
                    / (self.value_range[1] - self.value_range[0]))
                    .clamp(0., 1.);
                shade(&gradient_at(self.gradient, t), light)
            } else {
                let id = format!("{}_{}", self.paint_id, out.len());
                let x = points.iter().map(|p| p[0]).sum::<f64>() / 4.;
                let z = points.iter().map(|p| p[2]).sum::<f64>() / 4.;
                let top = self.scene.project(x, self.value_range[1], z);
                let bottom = self.scene.project(x, self.value_range[0], z);
                write!(out, r#"<defs><linearGradient id="{id}" gradientUnits="userSpaceOnUse" x1="{:.3}" y1="{:.3}" x2="{:.3}" y2="{:.3}">"#, top.x,top.y,bottom.x,bottom.y).unwrap();
                for (offset, color) in self.gradient {
                    write!(
                        out,
                        r#"<stop offset="{offset:.5}" stop-color="{}"/>"#,
                        shade(color, light)
                    )
                    .unwrap();
                }
                out.push_str("</linearGradient></defs>");
                format!("url(#{id})")
            };
            if self.material == Material::TranslucentPowder {
                out.push_str(r#"<g fill-opacity="0.7">"#);
            }
            polygon(out, &projected, &fill, self.stroke);
            if self.material == Material::TranslucentPowder {
                out.push_str("</g>");
            }
        }
    }

    fn prism(&self, out: &mut String, a: &Strip, b: &Strip) {
        let (x0, x1) = (a.x, b.x);
        let (y0, y1) = (a.bottom.min(a.top), b.bottom.min(b.top));
        let (h0, h1) = (a.bottom.max(a.top), b.bottom.max(b.top));
        let (z0, z1) = (self.near, self.far);

        if let Some([width, height]) = self
            .bevel
            .filter(|_| self.top_cap && (h0 - h1).abs() < 1e-9)
        {
            let dx = (width / self.scene.scale).min((x1 - x0).abs() * 0.45);
            let dz =
                (width / self.scene.scale / self.scene.model_depth).min((z1 - z0).abs() * 0.45);
            let dy =
                (height / self.scene.scale / self.scene.model_height).min((h0 - y0).abs() * 0.45);
            if dx > 0. && dz > 0. && dy > 0. {
                let body = Extrusion {
                    bevel: None,
                    top_cap: false,
                    ..*self
                };
                let a = Strip {
                    index: a.index,
                    x: x0,
                    bottom: y0,
                    top: h0 - dy,
                };
                let b = Strip {
                    index: b.index,
                    x: x1,
                    bottom: y1,
                    top: h1 - dy,
                };
                body.prism(out, &a, &b);
                let ring = |i: usize| {
                    let angle = i as f64 * std::f64::consts::FRAC_PI_2 / 4.;
                    let inset = 1. - angle.cos();
                    let y = h0 - dy + dy * angle.sin();
                    [
                        [x0 + dx * inset, y, z0 + dz * inset],
                        [x1 - dx * inset, y, z0 + dz * inset],
                        [x1 - dx * inset, y, z1 - dz * inset],
                        [x0 + dx * inset, y, z1 - dz * inset],
                    ]
                };
                for i in 0..4 {
                    let lower = ring(i);
                    let upper = ring(i + 1);
                    for edge in 0..4 {
                        let next = (edge + 1) % 4;
                        let face = [lower[edge], lower[next], upper[next], upper[edge]];
                        let a = [
                            face[1][0] - face[0][0],
                            (face[1][1] - face[0][1]) * self.scene.model_height,
                            (face[1][2] - face[0][2]) * self.scene.model_depth,
                        ];
                        let b = [
                            face[2][0] - face[0][0],
                            (face[2][1] - face[0][1]) * self.scene.model_height,
                            (face[2][2] - face[0][2]) * self.scene.model_depth,
                        ];
                        let n = [
                            a[2] * b[1] - a[1] * b[2],
                            a[0] * b[2] - a[2] * b[0],
                            a[1] * b[0] - a[0] * b[1],
                        ];
                        let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                        let light = 0.4
                            + 0.75
                                * ((-0.4 * n[0] + 0.6 * n[1] - n[2]) / (1.52_f64.sqrt() * length))
                                    .max(0.);
                        body.face(out, &face, light);
                    }
                }
                body.face(out, &ring(4), 0.4 + 0.75 * 0.6 / 1.52_f64.sqrt());
                return;
            }
        }

        let slope = (h1 - h0).atan2(x1 - x0);
        let faces = [
            ([[x0, y0, z0], [x1, y1, z0], [x1, h1, z0], [x0, h0, z0]], 1.),
            (
                [[x0, y0, z1], [x0, h0, z1], [x1, h1, z1], [x1, y1, z1]],
                0.70,
            ),
            (
                [[x0, y0, z0], [x0, h0, z0], [x0, h0, z1], [x0, y0, z1]],
                0.64,
            ),
            (
                [[x1, y1, z0], [x1, y1, z1], [x1, h1, z1], [x1, h1, z0]],
                0.64,
            ),
            (
                [[x0, h0, z0], [x1, h1, z0], [x1, h1, z1], [x0, h0, z1]],
                0.75 - 0.10 * slope.sin(),
            ),
            (
                [[x0, y0, z0], [x0, y0, z1], [x1, y1, z1], [x1, y1, z0]],
                0.40,
            ),
        ];
        for (index, (points, light)) in faces.iter().enumerate() {
            if (index == 4 && !self.top_cap) || (index == 5 && !self.bottom_cap) {
                continue;
            }
            self.face(out, points, *light);
        }
    }

    fn shaped_bar(
        &self,
        out: &mut String,
        category: [f64; 2],
        value: [f64; 2],
        horizontal: bool,
        shape: &str,
        start_radius: f64,
        end_radius: f64,
    ) {
        let pyramidal = matches!(shape, "pyramid" | "pyramidToMax");
        let count = if pyramidal {
            4
        } else if self.scene.spreadsheet {
            64
        } else {
            24
        };
        let shape_point = |i: usize, height: f64, radius: f64| {
            let angle = (i as f64 + if pyramidal { 0.5 } else { 0. }) * std::f64::consts::TAU
                / count as f64;
            let radius = radius
                * if pyramidal {
                    std::f64::consts::SQRT_2
                } else {
                    1.
                };
            let category = (category[0] + category[1]) / 2.
                + (category[1] - category[0]).abs() / 2. * angle.cos() * radius;
            let z =
                (self.near + self.far) / 2. + (self.far - self.near) / 2. * angle.sin() * radius;
            if horizontal {
                [height, category, z]
            } else {
                [category, height, z]
            }
        };
        let invert = horizontal ^ (value[1] < value[0]);
        for i in 0..count {
            let next = (i + 1) % count;
            let a = shape_point(i, value[0], start_radius);
            let b = shape_point(next, value[0], start_radius);
            let c = shape_point(next, value[1], end_radius);
            let d = shape_point(i, value[1], end_radius);
            let angle = (i as f64 + if pyramidal { 1. } else { 0.5 }) * std::f64::consts::TAU
                / count as f64;
            let light = (0.65 - 0.33 * angle.sin() - 0.10 * angle.cos()).clamp(0.4, 1.);
            let mut side = [a, b, c, d];
            let mut base = [shape_point(i, value[0], 0.), b, a, a];
            let mut top = [shape_point(i, value[1], 0.), d, c, c];
            if invert {
                side.reverse();
                base.reverse();
                top.reverse();
            }
            self.face(out, &side, light);
            let (base_cap, end_cap) = if value[1] < value[0] {
                (self.top_cap, self.bottom_cap)
            } else {
                (self.bottom_cap, self.top_cap)
            };
            if base_cap {
                self.face(out, &base, 0.40);
            }
            if end_cap {
                self.face(
                    out,
                    &top,
                    if self.scene.spreadsheet {
                        0.4 + 0.75 / 3_f64.sqrt()
                    } else {
                        0.75
                    },
                );
            }
        }
    }

    fn ribbon(&self, out: &mut String, a: &Strip, b: &Strip) {
        let slope = (b.top - a.top).atan2(b.x - a.x);
        let (z0, z1) = (self.near, self.far);
        let points = [
            [a.x, a.top, z0],
            [b.x, b.top, z0],
            [b.x, b.top, z1],
            [a.x, a.top, z1],
        ];
        self.face(out, &points, 0.75 - 0.10 * slope.sin());
        self.face(out, &[points[3], points[2], points[1], points[0]], 0.40);
    }
}

pub(super) fn draw(
    scene: &Scene,
    data: &ChartData,
    group: Node,
    colors: &ChartColors,
    font: &str,
    canvas: (f64, f64),
    out: &mut String,
) {
    let group_type = group.tag_name().name();
    let bars = group_type == "bar3DChart";
    let lines = group_type == "line3DChart";
    if !bars && !lines && group_type != "area3DChart" {
        return;
    }

    let xml_series: Vec<_> = group
        .children()
        .filter(|n| crate::chart::chart_node(*n, "ser"))
        .collect();
    let offset = group
        .prev_siblings()
        .skip(1)
        .filter(|g| g.is_element() && g.tag_name().name().ends_with("Chart"))
        .flat_map(|g| g.children())
        .filter(|n| crate::chart::chart_node(*n, "ser"))
        .count();
    let series: Vec<_> = data
        .series
        .iter()
        .enumerate()
        .skip(offset)
        .take(xml_series.len())
        .collect();
    if series.is_empty() {
        return;
    }
    let count = series
        .iter()
        .flat_map(|(_, s)| s.points.last())
        .map(|p| u64::from(p.index) + 1)
        .max()
        .unwrap_or(0);
    let declared = group
        .descendants()
        .filter(|n| crate::chart::chart_node(*n, "ptCount"))
        .filter_map(|n| n.attribute("val")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    let count = count.max(u64::from(declared));
    if count == 0 {
        return;
    }
    let nf = count as f64;
    let nseries = series.len() as f64;
    let grouping = setting(group, "grouping").unwrap_or(if bars && !scene.spreadsheet {
        "clustered"
    } else {
        "standard"
    });
    let percent = grouping == "percentStacked";
    let stacked = grouping == "stacked" || percent;
    let clustered = bars && !stacked && grouping != "standard";
    let horizontal = bars && setting(group, "barDir") == Some("bar");
    let band = 1. / (if clustered { nseries } else { 1. } + gap(group, "gapWidth"));
    let layers = if stacked || clustered { 1. } else { nseries };
    let depth = 1. / (1. + gap(group, "gapDepth")) / layers;
    let plot = group.parent_element().unwrap_or(group);
    let blanks = plot
        .parent_element()
        .and_then(|c| setting(c, "dispBlanksAs"))
        .unwrap_or("gap");
    let category_axis = child(plot, "catAx").or_else(|| child(plot, "dateAx"));
    let reverse = category_axis
        .and_then(|n| child(n, "scaling"))
        .and_then(|s| setting(s, "orientation"))
        == Some("maxMin");
    let category = |x: f64| if reverse { 1. - x } else { x };
    let reverse_depth = child(plot, "serAx")
        .and_then(|n| child(n, "scaling"))
        .and_then(|n| setting(n, "orientation"))
        == Some("maxMin");
    let mut totals = BTreeMap::<u32, f64>::new();
    if percent {
        for (_, s) in &series {
            for p in &s.points {
                if let Some(value) = point_value(p) {
                    *totals.entry(p.index).or_default() += value.abs();
                }
            }
        }
    }
    let mut stack_extents = BTreeMap::<u32, (f64, f64)>::new();
    if scene.spreadsheet && stacked {
        for (_, s) in &series {
            for p in &s.points {
                if let Some(value) = point_value(p) {
                    let extent = stack_extents.entry(p.index).or_default();
                    if value < 0. {
                        extent.0 += value;
                    } else {
                        extent.1 += value;
                    }
                }
            }
        }
    }
    let mut sums = BTreeMap::<u32, (f64, f64)>::new();
    let mut drawn: Vec<(f64, String)> = Vec::new();
    let mut labels = String::new();
    let style_effects = colors
        .style_effects_xml("dataPoint3D")
        .and_then(|xml| roxmltree::Document::parse(xml).ok());
    let inherited_shadow = style_effects.as_ref().and_then(|doc| {
        doc.descendants()
            .find(|node| node.tag_name().name() == "outerShdw")
    });
    let inherited_bevel = style_effects
        .as_ref()
        .and_then(|doc| doc.descendants().find(|n| n.tag_name().name() == "bevelT"));
    let style_fill = colors
        .style_fill_xml("dataPoint3D")
        .and_then(|xml| roxmltree::Document::parse(xml).ok());
    let mut shadows = String::new();
    for (order, (source, s)) in series.iter().enumerate() {
        let center = if layers == 1. {
            0.5
        } else {
            (order as f64 + 0.5) / layers
        };
        let center = if reverse_depth { 1. - center } else { center };
        let near = center - depth * 0.5;
        let far = center + depth * 0.5;
        let fill = if lines && !scene.spreadsheet {
            colors.line(*source)
        } else {
            colors.fill(*source, None)
        };
        let stroke = colors.outline(*source, None).unwrap_or("none");
        let data_labels = (scene.spreadsheet && bars)
            .then(|| DataLabels::new(group, xml_series[order], font, canvas))
            .flatten();
        let series_material = if scene.spreadsheet {
            material(
                child(xml_series[order], "spPr"),
                colors.style_material("dataPoint3D"),
            )
        } else {
            Material::Matte
        };
        let mut shapes = format!(r#"<g data-docx-series="{source}""#);
        colors.hit_attrs(&mut shapes, "series", *source, None);
        shapes.push('>');
        let mut anchors = String::new();
        let mut previous: Option<Strip> = None;
        let shape = setting(xml_series[order], "shape")
            .or_else(|| setting(group, "shape"))
            .unwrap_or("box");
        let series_max = if shape.ends_with("ToMax") {
            s.points
                .iter()
                .filter_map(point_value)
                .map(f64::abs)
                .fold(0., f64::max)
        } else {
            0.
        };
        for (index, cached) in plot_values(&s.points, count, !bars && blanks == "zero") {
            let value = match cached {
                Some(value) => value,
                None if blanks == "zero" => 0.,
                None => {
                    if blanks != "span" {
                        previous = None;
                    }
                    continue;
                }
            };
            let value = if percent {
                let sum = totals.get(&index).copied().unwrap_or(0.);
                if sum > 0. {
                    value / sum
                } else {
                    0.
                }
            } else {
                value
            };
            let base = if stacked {
                let sum = sums.entry(index).or_default();
                let slot = if value < 0. { &mut sum.1 } else { &mut sum.0 };
                let prev = *slot;
                *slot += value;
                prev
            } else {
                0.
            };
            let h0 = scene.value(base).clamp(0., 1.);
            let h1 = scene.value(base + value).clamp(0., 1.);
            if bars {
                let shift = if clustered {
                    (order as f64 - (nseries - 1.) * 0.5) * band
                } else {
                    0.
                };
                let mid = index as f64 + 0.5 + shift;
                let lo = category((mid - band * 0.5) / nf);
                let hi = category((mid + band * 0.5) / nf);
                if let Some((labels_config, point)) = data_labels.as_ref().zip(
                    s.points
                        .binary_search_by_key(&index, |p| p.index)
                        .ok()
                        .map(|i| &s.points[i]),
                ) {
                    let category_center = (lo + hi) / 2.;
                    let (start, end) = if horizontal {
                        (
                            scene.project(h0, category_center, center),
                            scene.project(h1, category_center, center),
                        )
                    } else {
                        (
                            scene.project(category_center, h0, center),
                            scene.project(category_center, h1, center),
                        )
                    };
                    labels_config.draw(
                        &mut labels,
                        s,
                        point,
                        (end.x, end.y),
                        (start.x, start.y),
                        if stacked { "ctr" } else { "outEnd" },
                        horizontal,
                        0.,
                        *source,
                        colors,
                    );
                }
                if (h0 - h1).abs() < 1e-12 {
                    continue;
                }
                let (x0, x1, y0, y1) = if horizontal {
                    (h0.min(h1), h0.max(h1), lo.min(hi), lo.max(hi))
                } else {
                    (lo.min(hi), lo.max(hi), h0.min(h1), h0.max(h1))
                };
                let ink = colors.fill(*source, Some(index));
                let point = xml_series[order].children().find(|n| {
                    chart_node(*n, "dPt") && num(Some(*n), "idx") == Some(f64::from(index))
                });
                let invert = scene.spreadsheet
                    && horizontal
                    && value < 0.
                    && on(setting(xml_series[order], "invertIfNegative"));
                let gradient = if scene.spreadsheet
                    && !horizontal
                    && colors.explicit_fill(*source, Some(index)).is_none()
                {
                    style_fill
                        .as_ref()
                        .map(|doc| {
                            doc.descendants()
                                .filter(|n| n.tag_name().name() == "gs")
                                .filter_map(|n| {
                                    Some((
                                        number(n.attribute("pos"))? / 100000.,
                                        colors.style_solid_color(n, &ink)?,
                                    ))
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                let bevel = if scene.spreadsheet
                    && !horizontal
                    && series_material != Material::TranslucentPowder
                {
                    point
                        .and_then(|p| child(p, "spPr"))
                        .or_else(|| child(xml_series[order], "spPr"))
                        .and_then(|p| p.descendants().find(|n| n.tag_name().name() == "bevelT"))
                        .or(inherited_bevel)
                        .map(|n| {
                            [
                                number(n.attribute("w")).unwrap_or(76200.) / 9525.,
                                number(n.attribute("h")).unwrap_or(76200.) / 9525.,
                            ]
                        })
                } else {
                    None
                };
                let paint_id = format!("c3dp{source}_{index}");
                let (top_cap, bottom_cap) = if scene.spreadsheet && stacked && !horizontal {
                    let (low, high) = stack_extents.get(&index).copied().unwrap_or_default();
                    let divisor = if percent {
                        totals
                            .get(&index)
                            .copied()
                            .unwrap_or(1.)
                            .max(f64::MIN_POSITIVE)
                    } else {
                        1.
                    };
                    let (low, high) = (low / divisor, high / divisor);
                    let epsilon = (high - low).max(1.) * 1e-9;
                    let start = base.abs() < epsilon
                        && if value > 0. {
                            low.abs() < epsilon
                        } else {
                            high.abs() < epsilon
                        };
                    let end =
                        ((base + value) - if value > 0. { high } else { low }).abs() < epsilon;
                    if h1 > h0 {
                        (end, start)
                    } else {
                        (start, end)
                    }
                } else {
                    (true, true)
                };
                let paint = Extrusion {
                    scene,
                    near,
                    far,
                    fill: if invert { "#ffffff" } else { &ink },
                    stroke: if scene.spreadsheet && shape != "box" {
                        "none"
                    } else {
                        colors.outline(*source, Some(index)).unwrap_or(if invert {
                            &ink
                        } else {
                            "none"
                        })
                    },
                    material: series_material,
                    gradient: &gradient,
                    paint_id: &paint_id,
                    value_range: [h0.min(h1), h0.max(h1)],
                    bevel,
                    top_cap,
                    bottom_cap,
                };
                if colors.interactive {
                    let middle = (lo + hi) / 2.;
                    let top = if horizontal {
                        scene.project(h1, middle, center)
                    } else {
                        scene.project(middle, h1, center)
                    };
                    begin_point_hit(&mut shapes, colors, "bar3D", *source, index, top);
                }
                let mut geometry = String::new();
                if matches!(
                    shape,
                    "cylinder" | "cone" | "coneToMax" | "pyramid" | "pyramidToMax"
                ) {
                    let mut start_radius = 1.;
                    let mut end_radius = if shape == "cylinder" {
                        1.
                    } else if series_max > 0. {
                        (1. - value.abs() / series_max).clamp(0., 1.)
                    } else {
                        0.
                    };
                    if scene.spreadsheet && stacked && shape != "cylinder" {
                        let total =
                            stack_extents
                                .get(&index)
                                .map_or(0., |&(lo, hi)| if value < 0. { -lo } else { hi });
                        let total = if percent {
                            total
                                / totals
                                    .get(&index)
                                    .copied()
                                    .unwrap_or(1.)
                                    .max(f64::MIN_POSITIVE)
                        } else {
                            total
                        };
                        if total > 0. {
                            start_radius = (1. - base.abs() / total).clamp(0., 1.);
                            end_radius = (1. - (base + value).abs() / total).clamp(0., 1.);
                        }
                    }
                    paint.shaped_bar(
                        &mut geometry,
                        [lo, hi],
                        [h0, h1],
                        horizontal,
                        shape,
                        start_radius,
                        end_radius,
                    );
                } else {
                    let start = Strip {
                        index,
                        x: x0,
                        bottom: y0,
                        top: y1,
                    };
                    let end = Strip { x: x1, ..start };
                    paint.prism(&mut geometry, &start, &end);
                }
                let mut bounds = [
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ];
                for x in [x0, x1] {
                    for y in [y0, y1] {
                        for z in [near, far] {
                            let p = scene.project(x, y, z);
                            bounds[0] = bounds[0].min(p.x);
                            bounds[1] = bounds[1].min(p.y);
                            bounds[2] = bounds[2].max(p.x);
                            bounds[3] = bounds[3].max(p.y);
                        }
                    }
                }
                bounds[2] -= bounds[0];
                bounds[3] -= bounds[1];
                if scene.spreadsheet {
                    let properties = point
                        .and_then(ChartColors::effect_properties)
                        .or_else(|| ChartColors::effect_properties(xml_series[order]));
                    if properties.is_none() && !horizontal && inherited_shadow.is_some() {
                        if base == 0. && value > 0. {
                            floor_shadow(
                                &mut shadows,
                                scene,
                                colors,
                                inherited_shadow.unwrap(),
                                [lo, hi],
                                [near, far],
                                h1,
                                shape,
                                *source,
                                index,
                            );
                        }
                        shapes.push_str(&geometry);
                    } else {
                        colors.paint_styled_shape(
                            &mut shapes,
                            properties,
                            "dataPoint3D",
                            &ink,
                            bounds,
                            &geometry,
                        );
                    }
                } else {
                    shapes.push_str(&geometry);
                }
                colors.end_hit(&mut shapes);
            } else {
                let x = category(
                    if (lines
                        && setting(child(plot, "valAx").unwrap_or(plot), "crossBetween")
                            != Some("midCat"))
                        || (scene.spreadsheet
                            && setting(child(plot, "valAx").unwrap_or(plot), "crossBetween")
                                == Some("between"))
                    {
                        (f64::from(index) + 0.5) / nf
                    } else if count == 1 {
                        0.5
                    } else {
                        f64::from(index) / (nf - 1.)
                    },
                );
                let curr = Strip {
                    index,
                    x,
                    bottom: h0,
                    top: h1,
                };
                if colors.interactive
                    && s.points
                        .binary_search_by_key(&index, |p| p.index)
                        .ok()
                        .is_some_and(|i| point_value(&s.points[i]).is_some())
                {
                    point_anchor(
                        &mut anchors,
                        colors,
                        *source,
                        index,
                        scene.project(x, h1, center),
                    );
                }
                if let Some(prev) = previous.as_ref().filter(|prev| {
                    matches!(blanks, "span" | "zero") || index.checked_sub(prev.index) == Some(1)
                }) {
                    let paint = Extrusion {
                        scene,
                        near,
                        far,
                        fill: &fill,
                        stroke,
                        material: series_material,
                        gradient: &[],
                        paint_id: "",
                        value_range: [0., 1.],
                        bevel: None,
                        top_cap: true,
                        bottom_cap: true,
                    };
                    let (a, b) = if prev.x < curr.x {
                        (prev, &curr)
                    } else {
                        (&curr, prev)
                    };
                    if lines {
                        paint.ribbon(&mut shapes, a, b)
                    } else {
                        let d0 = a.top - a.bottom;
                        let d1 = b.top - b.bottom;
                        if d0 * d1 < 0. {
                            let t = d0 / (d0 - d1);
                            let baseline = a.bottom + (b.bottom - a.bottom) * t;
                            let cross = Strip {
                                index: a.index,
                                x: a.x + (b.x - a.x) * t,
                                bottom: baseline,
                                top: baseline,
                            };
                            paint.prism(&mut shapes, a, &cross);
                            paint.prism(&mut shapes, &cross, b);
                        } else {
                            paint.prism(&mut shapes, a, b);
                        }
                        // The fill seam follows the roof of each area strip;
                        // outlining every face would expose internal divisions.
                        if stroke == "none"
                            && colors.legacy_defaults()
                            && colors.outline(*source, None).is_none()
                        {
                            for (v0, v1, z) in [
                                (a.top, b.top, near),
                                (a.top, b.top, far),
                                (a.bottom, b.bottom, near),
                            ] {
                                super::stroke_line(
                                    &mut shapes,
                                    scene.project(a.x, v0, z),
                                    scene.project(b.x, v1, z),
                                    "#ffffff",
                                );
                            }
                        }
                    }
                }
                previous = Some(curr);
            }
        }
        let depth_order = center * scene.rot_y.to_radians().cos();
        shapes.push_str(&anchors);
        shapes.push_str("</g>");
        drawn.push((depth_order, shapes));
    }
    drawn.sort_by(|a, b| b.0.total_cmp(&a.0));
    out.push_str(&shadows);
    for (_, shapes) in drawn {
        out.push_str(&shapes);
    }
    out.push_str(&labels);
}

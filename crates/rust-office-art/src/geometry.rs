use std::collections::HashMap;
use std::fmt::Write;
use std::sync::OnceLock;

use crate::xml::{extract_direct_child_tag_ranges, get_attribute_ref};

const PRESETS: &str = include_str!("geometry_data/presetShapeDefinitions.xml");
const MAX_XML_BYTES: usize = 1024 * 1024;
const MAX_GUIDES: usize = 1024;
const MAX_PATHS: usize = 256;
const MAX_COMMANDS: usize = 8192;
const MAX_VALUE: f64 = 1.0e100;
const MAX_COORDINATE: f64 = 1.0e8;
const FULL_ANGLE: f64 = 21_600_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PathFill {
    None,
    Norm,
    Lighten,
    LightenLess,
    Darken,
    DarkenLess,
}

#[derive(Clone, Debug)]
pub(crate) struct GeometryPath {
    pub d: String,
    pub fill: PathFill,
    pub stroke: bool,
    pub closed: bool,
    pub bounds: [f64; 4],
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn child<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    extract_direct_child_tag_ranges(xml)
        .into_iter()
        .find(|tag| local(&tag.tag_name) == name)
        .map(|tag| &xml[tag.start..tag.end])
}

fn preset(name: &str) -> Option<&'static str> {
    static INDEX: OnceLock<HashMap<String, &'static str>> = OnceLock::new();
    INDEX
        .get_or_init(|| {
            extract_direct_child_tag_ranges(PRESETS)
                .into_iter()
                .map(|tag| (tag.tag_name, &PRESETS[tag.start..tag.end]))
                .collect()
        })
        .get(name)
        .copied()
}

fn finite(value: f64) -> Option<f64> {
    (value.is_finite() && value.abs() <= MAX_VALUE).then_some(value)
}

struct Guides {
    values: HashMap<String, f64>,
    count: usize,
}

impl Guides {
    fn new(width: f64, height: f64) -> Self {
        let short = width.min(height);
        let mut values = HashMap::from([
            ("w".to_string(), width),
            ("h".to_string(), height),
            ("l".to_string(), 0.0),
            ("t".to_string(), 0.0),
            ("r".to_string(), width),
            ("b".to_string(), height),
            ("hc".to_string(), width / 2.0),
            ("vc".to_string(), height / 2.0),
            ("ss".to_string(), short),
            ("ls".to_string(), width.max(height)),
            ("cd2".to_string(), FULL_ANGLE / 2.0),
            ("cd4".to_string(), FULL_ANGLE / 4.0),
            ("cd8".to_string(), FULL_ANGLE / 8.0),
            ("3cd4".to_string(), FULL_ANGLE * 3.0 / 4.0),
            ("3cd8".to_string(), FULL_ANGLE * 3.0 / 8.0),
            ("5cd8".to_string(), FULL_ANGLE * 5.0 / 8.0),
            ("7cd8".to_string(), FULL_ANGLE * 7.0 / 8.0),
        ]);
        for divisor in [2, 3, 4, 5, 6, 8, 10, 12, 16, 32] {
            for (prefix, value) in [("wd", width), ("hd", height), ("ssd", short)] {
                values.insert(format!("{prefix}{divisor}"), value / f64::from(divisor));
            }
        }
        Self { values, count: 0 }
    }

    fn value(&self, token: &str) -> Option<f64> {
        if token.len() > 128 {
            return None;
        }
        self.values
            .get(token)
            .copied()
            .or_else(|| token.parse().ok())
            .and_then(finite)
    }

    fn formula(&self, formula: &str) -> Option<f64> {
        if formula.len() > 1024 {
            return None;
        }
        let mut parts = formula.split_ascii_whitespace();
        let operation = parts.next()?;
        let args = parts
            .map(|part| self.value(part))
            .collect::<Option<Vec<_>>>()?;
        let angle = |value: f64| value / 60_000.0 * std::f64::consts::PI / 180.0;
        finite(match (operation, args.as_slice()) {
            ("val", [x]) => *x,
            ("+-", [x, y, z, ..]) => x + y - z,
            ("*/", [x, y, z]) => {
                if *z == 0.0 {
                    0.0
                } else {
                    x * y / z
                }
            }
            ("+/", [x, y, z]) => {
                if *z == 0.0 {
                    0.0
                } else {
                    (x + y) / z
                }
            }
            ("?:", [x, y, z]) => {
                if *x > 0.0 {
                    *y
                } else {
                    *z
                }
            }
            ("abs", [x]) => x.abs(),
            ("at2", [x, y]) => y.atan2(*x).to_degrees() * 60_000.0,
            ("cat2", [x, y, z]) => x * z.atan2(*y).cos(),
            ("sat2", [x, y, z]) => x * z.atan2(*y).sin(),
            ("cos", [x, y]) => x * angle(*y).cos(),
            ("sin", [x, y]) => x * angle(*y).sin(),
            ("tan", [x, y]) => x * angle(*y).tan(),
            ("min", [x, y]) => x.min(*y),
            ("max", [x, y]) => x.max(*y),
            ("mod", [x, y, z]) => x.hypot(*y).hypot(*z),
            ("pin", [x, y, z]) => x.max(y.min(*z)),
            ("sqrt", [x]) if *x >= 0.0 => x.sqrt(),
            _ => return None,
        })
    }

    fn apply(&mut self, xml: Option<&str>) -> Option<()> {
        let Some(xml) = xml else {
            return Some(());
        };
        for tag in extract_direct_child_tag_ranges(xml) {
            if local(&tag.tag_name) != "gd" {
                return None;
            }
            self.count += 1;
            if self.count > MAX_GUIDES {
                return None;
            }
            let xml = &xml[tag.start..tag.end];
            let name = get_attribute_ref(xml, "name")?;
            if name.is_empty() || name.len() > 128 {
                return None;
            }
            let value = self.formula(get_attribute_ref(xml, "fmla")?)?;
            self.values.insert(name.to_string(), value);
        }
        Some(())
    }
}

struct PathBuilder {
    d: String,
    current: Option<[f64; 2]>,
    start: Option<[f64; 2]>,
    bounds: [f64; 4],
    scale: [f64; 2],
    open: bool,
    all_closed: bool,
    commands: usize,
}

impl PathBuilder {
    fn point(&self, point: [f64; 2]) -> Option<[f64; 2]> {
        let point = [point[0] * self.scale[0], point[1] * self.scale[1]];
        point
            .iter()
            .all(|value| value.is_finite() && value.abs() <= MAX_COORDINATE)
            .then_some(point)
    }

    fn include(&mut self, point: [f64; 2]) {
        self.bounds[0] = self.bounds[0].min(point[0]);
        self.bounds[1] = self.bounds[1].min(point[1]);
        self.bounds[2] = self.bounds[2].max(point[0]);
        self.bounds[3] = self.bounds[3].max(point[1]);
    }

    fn emit_point(&mut self, operation: &str, raw: [f64; 2]) -> Option<()> {
        let point = self.point(raw)?;
        self.include(point);
        write!(self.d, "{operation}{} {} ", point[0], point[1]).ok()?;
        self.current = Some(raw);
        Some(())
    }

    fn arc(&mut self, rx: f64, ry: f64, start: f64, sweep: f64) -> Option<()> {
        if rx < 0.0 || ry < 0.0 || sweep.abs() > FULL_ANGLE * 16.0 {
            return None;
        }
        let current = self.current?;
        if sweep == 0.0 {
            return Some(());
        }
        let ellipse = |angle: f64| {
            let angle = (angle / 60_000.0).to_radians();
            let parameter = (rx * angle.sin()).atan2(ry * angle.cos());
            [rx * parameter.cos(), ry * parameter.sin()]
        };
        let offset = ellipse(start);
        let center = [current[0] - offset[0], current[1] - offset[1]];
        let parts = (sweep.abs() / (FULL_ANGLE / 2.0)).ceil() as usize;
        let radii = self.point([rx, ry])?;
        for index in 1..=parts {
            let offset = ellipse(start + sweep * index as f64 / parts as f64);
            let raw = [center[0] + offset[0], center[1] + offset[1]];
            let point = self.point(raw)?;
            self.include(point);
            write!(
                self.d,
                "A{} {} 0 0 {} {} {} ",
                radii[0],
                radii[1],
                u8::from(sweep > 0.0),
                point[0],
                point[1]
            )
            .ok()?;
            self.current = Some(raw);
        }
        for angle in [
            0.0,
            FULL_ANGLE / 4.0,
            FULL_ANGLE / 2.0,
            FULL_ANGLE * 3.0 / 4.0,
        ] {
            let distance = if sweep > 0.0 {
                (angle - start).rem_euclid(FULL_ANGLE)
            } else {
                (start - angle).rem_euclid(FULL_ANGLE)
            };
            if distance <= sweep.abs() {
                let offset = ellipse(angle);
                self.include(self.point([center[0] + offset[0], center[1] + offset[1]])?);
            }
        }
        self.open = true;
        Some(())
    }
}

fn path(
    xml: &str,
    guides: &Guides,
    width: f64,
    height: f64,
    units: [f64; 2],
    remaining: &mut usize,
) -> Option<GeometryPath> {
    let dimensions = [
        get_attribute_ref(xml, "w")
            .map(|v| guides.value(v))
            .unwrap_or(Some(units[0]))?,
        get_attribute_ref(xml, "h")
            .map(|v| guides.value(v))
            .unwrap_or(Some(units[1]))?,
    ];
    if dimensions.iter().any(|value| *value < 0.0)
        || (dimensions[0] == 0.0 && width != 0.0)
        || (dimensions[1] == 0.0 && height != 0.0)
    {
        return None;
    }
    let fill = match get_attribute_ref(xml, "fill").unwrap_or("norm") {
        "none" => PathFill::None,
        "norm" => PathFill::Norm,
        "lighten" => PathFill::Lighten,
        "lightenLess" => PathFill::LightenLess,
        "darken" => PathFill::Darken,
        "darkenLess" => PathFill::DarkenLess,
        _ => return None,
    };
    let stroke = match get_attribute_ref(xml, "stroke").unwrap_or("true") {
        "true" | "1" | "on" => true,
        "false" | "0" | "off" => false,
        _ => return None,
    };
    let mut builder = PathBuilder {
        d: String::new(),
        current: None,
        start: None,
        bounds: [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        scale: [
            if dimensions[0] == 0.0 {
                0.0
            } else {
                width / dimensions[0]
            },
            if dimensions[1] == 0.0 {
                0.0
            } else {
                height / dimensions[1]
            },
        ],
        open: false,
        all_closed: true,
        commands: 0,
    };
    for command in extract_direct_child_tag_ranges(xml) {
        *remaining = remaining.checked_sub(1)?;
        builder.commands += 1;
        let command_xml = &xml[command.start..command.end];
        let name = local(&command.tag_name);
        let points = extract_direct_child_tag_ranges(command_xml)
            .into_iter()
            .map(|point| {
                if local(&point.tag_name) != "pt" {
                    return None;
                }
                let xml = &command_xml[point.start..point.end];
                Some([
                    guides.value(get_attribute_ref(xml, "x")?)?,
                    guides.value(get_attribute_ref(xml, "y")?)?,
                ])
            })
            .collect::<Option<Vec<_>>>()?;
        match (name, points.as_slice()) {
            ("moveTo", [point]) => {
                if builder.open {
                    builder.all_closed = false;
                }
                builder.emit_point("M", *point)?;
                builder.start = Some(*point);
                builder.open = true;
            }
            ("lnTo", [point]) => {
                builder.current?;
                builder.emit_point("L", *point)?;
                builder.open = true;
            }
            ("quadBezTo", [control, end]) => {
                builder.current?;
                builder.emit_point("Q", *control)?;
                builder.emit_point("", *end)?;
                builder.open = true;
            }
            ("cubicBezTo", [first, second, end]) => {
                builder.current?;
                builder.emit_point("C", *first)?;
                builder.emit_point("", *second)?;
                builder.emit_point("", *end)?;
                builder.open = true;
            }
            ("arcTo", []) => builder.arc(
                guides.value(get_attribute_ref(command_xml, "wR")?)?,
                guides.value(get_attribute_ref(command_xml, "hR")?)?,
                guides.value(get_attribute_ref(command_xml, "stAng")?)?,
                guides.value(get_attribute_ref(command_xml, "swAng")?)?,
            )?,
            ("close", []) => {
                builder.current = Some(builder.start?);
                builder.d.push_str("Z ");
                builder.open = false;
            }
            _ => return None,
        }
    }
    if builder.commands == 0 || builder.bounds.iter().any(|value| !value.is_finite()) {
        return None;
    }
    Some(GeometryPath {
        d: builder.d,
        fill,
        stroke,
        closed: builder.all_closed && !builder.open,
        bounds: builder.bounds,
    })
}

pub(crate) fn geometry_paths(
    properties_xml: &str,
    width: f64,
    height: f64,
) -> Option<Vec<GeometryPath>> {
    if properties_xml.len() > MAX_XML_BYTES
        || [width, height]
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0 || *value > MAX_COORDINATE)
    {
        return None;
    }
    let custom = child(properties_xml, "custGeom");
    let preset_xml = child(properties_xml, "prstGeom");
    if custom.is_some() && preset_xml.is_some() {
        return None;
    }
    let geometry = if let Some(custom) = custom {
        custom
    } else if let Some(preset_xml) = preset_xml {
        preset(get_attribute_ref(preset_xml, "prst")?)?
    } else if get_attribute_ref(properties_xml, "prst").is_some() {
        preset(get_attribute_ref(properties_xml, "prst")?)?
    } else if child(properties_xml, "pathLst").is_some() {
        properties_xml
    } else {
        preset("rect")?
    };
    let units = [finite(width * 9525.0)?, finite(height * 9525.0)?];
    let mut guides = Guides::new(units[0], units[1]);
    guides.apply(child(geometry, "avLst"))?;
    if let Some(preset_xml) = preset_xml {
        guides.apply(child(preset_xml, "avLst"))?;
    } else if get_attribute_ref(properties_xml, "prst").is_some() {
        guides.apply(child(properties_xml, "avLst"))?;
    }
    guides.apply(child(geometry, "gdLst"))?;
    let paths = child(geometry, "pathLst")?;
    let tags = extract_direct_child_tag_ranges(paths);
    if tags.is_empty() || tags.len() > MAX_PATHS {
        return None;
    }
    let mut remaining = MAX_COMMANDS;
    tags.into_iter()
        .map(|tag| {
            if local(&tag.tag_name) != "path" {
                return None;
            }
            path(
                &paths[tag.start..tag.end],
                &guides,
                width,
                height,
                units,
                &mut remaining,
            )
        })
        .collect()
}

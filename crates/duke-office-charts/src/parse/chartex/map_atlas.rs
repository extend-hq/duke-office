use super::geo_cache::{GeoCache, GeoRegion};
use crate::chart::ChartPoint;
use roxmltree::Node;
use std::{collections::HashMap, sync::OnceLock};

struct Region {
    aliases: Vec<String>,
    geometry: GeoRegion,
}

struct Atlas {
    countries: Vec<Region>,
    states: Vec<Region>,
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn integer(&mut self) -> Option<usize> {
        let mut result = 0;
        for shift in (0..35).step_by(7) {
            let (&byte, rest) = self.0.split_first()?;
            self.0 = rest;
            result |= usize::from(byte & 127) << shift;
            if byte < 128 {
                return Some(result);
            }
        }
        None
    }

    fn string(&mut self) -> Option<String> {
        let length = self.integer()?;
        let value = std::str::from_utf8(self.0.get(..length)?).ok()?.to_owned();
        self.0 = self.0.get(length..)?;
        Some(value)
    }

    fn regions(&mut self) -> Option<Vec<Region>> {
        let count = self.integer()?.min(512);
        let mut regions = Vec::with_capacity(count);
        for _ in 0..count {
            let name = self.string()?;
            let aliases = (0..self.integer()?)
                .map(|_| self.string())
                .collect::<Option<Vec<_>>>()?;
            let mut rings = Vec::new();
            for _ in 0..self.integer()? {
                let mut ring = Vec::new();
                let (mut x, mut y) = (0_i64, 0_i64);
                for _ in 0..self.integer()? {
                    let signed = |value: usize| (value >> 1) as i64 ^ -((value & 1) as i64);
                    x += signed(self.integer()?);
                    y += signed(self.integer()?);
                    ring.push((x as f64 / 100_000., y as f64 / 100_000.));
                }
                rings.push(ring);
            }
            regions.push(Region {
                aliases,
                geometry: GeoRegion {
                    name,
                    entity_type: String::new(),
                    point_index: None,
                    in_view: true,
                    in_natural_view: true,
                    center: None,
                    bounds: None,
                    polygons: vec![rings],
                },
            });
        }
        Some(regions)
    }
}

fn atlas() -> Option<&'static Atlas> {
    static ATLAS: OnceLock<Option<Atlas>> = OnceLock::new();
    ATLAS
        .get_or_init(|| {
            let bytes = miniz_oxide::inflate::decompress_to_vec_with_limit(
                include_bytes!("map_atlas/boundaries.deflate"),
                1 << 20,
            )
            .ok()?;
            if !bytes.starts_with(b"OCMA1") {
                return None;
            }
            let mut reader = Reader(&bytes[5..]);
            let atlas = Atlas {
                countries: reader.regions()?,
                states: reader.regions()?,
            };
            reader.0.is_empty().then_some(atlas)
        })
        .as_ref()
}

fn key(label: &str) -> String {
    if label.len() > 1024 {
        return String::new();
    }
    let mut key = String::new();
    for c in label.chars().flat_map(char::to_lowercase) {
        match c {
            'a'..='z' | '0'..='9' => key.push(c),
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => key.push('a'),
            'è' | 'é' | 'ê' | 'ë' => key.push('e'),
            'ì' | 'í' | 'î' | 'ï' => key.push('i'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => key.push('o'),
            'ù' | 'ú' | 'û' | 'ü' => key.push('u'),
            'ç' => key.push('c'),
            'ñ' => key.push('n'),
            'ý' | 'ÿ' => key.push('y'),
            '&' => key.push_str("and"),
            _ => (),
        }
    }
    key
}

pub(super) fn read(geography: Node, points: &[ChartPoint]) -> Option<GeoCache> {
    if points.len() > 150_000 {
        return None;
    }
    let atlas = atlas()?;
    let labels: Vec<_> = points
        .iter()
        .filter_map(|point| {
            let leaf = point
                .categories
                .iter()
                .min_by_key(|category| category.level)?;
            Some((point.index, key(&leaf.label)))
        })
        .collect();
    let lookup = |regions: &[Region]| {
        let index: HashMap<_, _> = regions
            .iter()
            .enumerate()
            .flat_map(|(i, region)| region.aliases.iter().map(move |alias| (alias.as_str(), i)))
            .collect();
        labels
            .iter()
            .filter_map(|(point, label)| Some((*index.get(label.as_str())?, *point)))
            .collect::<HashMap<_, _>>()
    };
    let countries = lookup(&atlas.countries);
    let states = lookup(&atlas.states);
    let us = geography
        .attribute("cultureRegion")
        .is_some_and(|culture| culture.eq_ignore_ascii_case("US"))
        && !states.is_empty()
        && states.len() >= countries.len();
    let (source, matches) = if us {
        (&atlas.states, states)
    } else {
        (&atlas.countries, countries)
    };
    if matches.is_empty() {
        return None;
    }
    let data_only = geography.attribute("viewedRegionType") == Some("dataOnly")
        || (matches.len() == 1 && geography.attribute("viewedRegionType") != Some("world"));
    let regions = source
        .iter()
        .enumerate()
        .filter(|(i, _)| !data_only || matches.contains_key(i))
        .map(|(i, entry)| {
            let mut region = entry.geometry.clone();
            region.point_index = matches.get(&i).copied();
            if !us {
                if let Some(label) = region
                    .point_index
                    .and_then(|index| points.iter().find(|point| point.index == index))
                    .and_then(|point| {
                        point
                            .categories
                            .iter()
                            .min_by_key(|category| category.level)
                    })
                {
                    region.name = label.label.clone();
                }
            }
            if data_only {
                let points: Vec<_> = region
                    .polygons
                    .iter()
                    .flatten()
                    .flatten()
                    .copied()
                    .collect();
                let mut longitudes: Vec<_> = points.iter().map(|p| p.0).collect();
                longitudes.sort_by(f64::total_cmp);
                if let (Some(&first), Some(&last)) = (longitudes.first(), longitudes.last()) {
                    let (mut west, mut east, mut gap) = (first, last, first + 360. - last);
                    for pair in longitudes.windows(2) {
                        if pair[1] - pair[0] > gap {
                            gap = pair[1] - pair[0];
                            (west, east) = (pair[1], pair[0]);
                        }
                    }
                    let south = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
                    let north = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
                    region.bounds = Some([west, south, east, north]);
                }
            }
            region
        })
        .collect();
    Some(GeoCache {
        regions,
        credits: Vec::new(),
        us_only: us && !data_only,
        regional: Some(us || data_only),
        projection_bounds: None,
        antimeridian_cut: true,
    })
}

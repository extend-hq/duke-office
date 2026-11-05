use super::{line, text, text_width, Coords};
use std::collections::BTreeMap;

pub(super) type Hierarchy = BTreeMap<usize, BTreeMap<u32, String>>;

fn band_extent(labels: &BTreeMap<u32, String>, size: f64, horizontal: bool) -> f64 {
    if horizontal {
        size * 2.0
    } else {
        labels
            .values()
            .map(|s| text_width(s, size))
            .fold(0., f64::max)
            + size * 0.9
    }
}

pub(super) fn extent(hierarchy: &Hierarchy, size: f64, horizontal: bool) -> f64 {
    hierarchy
        .values()
        .map(|labels| band_extent(labels, size, horizontal))
        .sum()
}

pub(super) fn leaf_extent(
    labels: &BTreeMap<u32, String>,
    size: f64,
    rotation: f64,
    horizontal: bool,
) -> f64 {
    let width = labels
        .values()
        .map(|s| text_width(s, size))
        .fold(0., f64::max);
    if !horizontal {
        width + size * 0.9
    } else if rotation == 0. {
        size * 2.29
    } else {
        width * rotation.to_radians().abs().sin() + size * 1.4
    }
}

pub(super) fn draw(
    svg: &mut String,
    hierarchy: &Hierarchy,
    coords: &Coords,
    axis_position: f64,
    leaf_extent: f64,
    size: f64,
    face: &str,
    stroke: &str,
    side: &str,
) {
    let horizontal = !coords.horizontal;
    let direction = if side == "l" || side == "t" { -1. } else { 1. };
    let count = coords.count.min(u32::MAX as f64) as u32;
    let edge = |index: u32| {
        let (x, y) = coords.point(f64::from(index) - 0.5, 0.);
        if horizontal {
            x.clamp(coords.rect.left, coords.rect.right)
        } else {
            y.clamp(coords.rect.top, coords.rect.bottom)
        }
    };
    let mut offset = leaf_extent;
    for (&level, labels) in hierarchy {
        let band = band_extent(labels, size, horizontal);
        let inner = axis_position + direction * offset;
        let outer = inner + direction * band;
        for (&start, label) in labels.range(..count) {
            let end = hierarchy
                .range(level..)
                .filter_map(|(_, entries)| {
                    entries
                        .range((std::ops::Bound::Excluded(start), std::ops::Bound::Unbounded))
                        .next()
                        .map(|(&i, _)| i)
                })
                .min()
                .unwrap_or(count)
                .min(count);
            if end <= start {
                continue;
            }
            let a = edge(start);
            let b = edge(end);
            let center = (a + b) * 0.5;
            if horizontal {
                line(svg, a, axis_position, a, outer, stroke);
                line(svg, b, axis_position, b, outer, stroke);
                text(
                    svg,
                    center,
                    (inner + outer) * 0.5 + size * 0.31,
                    "middle",
                    size,
                    face,
                    label,
                    false,
                );
            } else {
                line(svg, axis_position, a, outer, a, stroke);
                line(svg, axis_position, b, outer, b, stroke);
                text(
                    svg,
                    (inner + outer) * 0.5,
                    center + size * 0.31,
                    "middle",
                    size,
                    face,
                    label,
                    false,
                );
            }
        }
        offset += band;
    }
}

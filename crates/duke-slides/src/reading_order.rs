//! Geometric extraction order without changing the drawing stack.

use crate::{PresentationBlock, SlideNode};

#[derive(Clone, Copy)]
struct Rect([f64; 4]);

fn bounds(node: &SlideNode, depth: usize) -> Result<Option<Rect>, ()> {
    if depth > 64 {
        return Err(());
    }
    if let SlideNode::Group {
        children,
        transform,
        ..
    } = node
    {
        let mut bounds: Option<Rect> = None;
        for child in children {
            let Some(Rect(r)) = self::bounds(child, depth + 1)? else {
                continue;
            };
            bounds = Some(match bounds {
                Some(Rect(b)) => Rect([
                    b[0].min(r[0]),
                    b[1].min(r[1]),
                    b[2].max(r[2]),
                    b[3].max(r[3]),
                ]),
                None => Rect(r),
            });
        }
        if bounds.is_some()
            && (transform.width <= 0
                || transform.height <= 0
                || transform.rotation.is_some_and(|r| r != 0.))
        {
            return Err(());
        }
        return Ok(bounds);
    }
    let has_text = node
        .text_content()
        .is_some_and(|t| !t.text.trim().is_empty())
        || match node {
            SlideNode::Shape { paragraphs, .. } => paragraphs
                .iter()
                .any(|p| p.runs.iter().any(|r| !r.text.trim().is_empty())),
            SlideNode::Table { rows, .. } => rows.iter().flatten().any(|c| {
                c.paragraphs
                    .iter()
                    .any(|p| p.runs.iter().any(|r| !r.text.trim().is_empty()))
            }),
            SlideNode::Chart { title, series, .. } => {
                title.as_deref().is_some_and(|t| !t.trim().is_empty())
                    || series.iter().any(|s| {
                        s.name.as_deref().is_some_and(|t| !t.trim().is_empty())
                            || s.categories.as_ref().is_some_and(|c| {
                                c.iter()
                                    .any(|v| v.as_str().is_some_and(|t| !t.trim().is_empty()))
                            })
                    })
            }
            _ => false,
        };
    if !has_text {
        return Ok(None);
    }
    let transform = match node {
        SlideNode::Shape { transform, .. }
        | SlideNode::Image { transform, .. }
        | SlideNode::Table { transform, .. }
        | SlideNode::Chart { transform, .. }
        | SlideNode::Media { transform, .. }
        | SlideNode::Unknown { transform, .. }
        | SlideNode::Group { transform, .. } => transform,
    };
    if transform.width <= 0 || transform.height <= 0 || transform.rotation.is_some_and(|r| r != 0.)
    {
        return Err(());
    }
    let (x, y) = (transform.x as f64, transform.y as f64);
    // Tolerate sub-point edge overlaps, capped at one percent of each dimension.
    let inset_x = (transform.width as f64 * 0.005).min(3_175.);
    let inset_y = (transform.height as f64 * 0.005).min(3_175.);
    Ok(Some(Rect([
        x + inset_x,
        y + inset_y,
        x + transform.width as f64 - inset_x,
        y + transform.height as f64 - inset_y,
    ])))
}

struct Axis {
    groups: Vec<Vec<usize>>,
    gap: f64,
    split: f64,
}

fn axis(items: &[usize], boxes: &[Option<Rect>], axis: usize) -> Axis {
    let mut sorted = items.to_vec();
    sorted.sort_by(|a, b| boxes[*a].unwrap().0[axis].total_cmp(&boxes[*b].unwrap().0[axis]));
    let mut result = Axis {
        groups: Vec::new(),
        gap: 0.,
        split: 0.,
    };
    let mut end = f64::NEG_INFINITY;
    for item in sorted {
        let Rect(b) = boxes[item].unwrap();
        if b[axis] > end + 1. {
            if end.is_finite() && b[axis] - end > result.gap {
                result.gap = b[axis] - end;
                result.split = b[axis];
            }
            result.groups.push(Vec::new());
        }
        result.groups.last_mut().unwrap().push(item);
        end = end.max(b[axis + 2]);
    }
    result
}

fn partition(items: Vec<usize>, boxes: &[Option<Rect>], depth: usize) -> Vec<usize> {
    if items.len() < 2 || depth >= 64 {
        return items;
    }
    let x = axis(&items, boxes, 0);
    let y = axis(&items, boxes, 1);
    let grid = x.groups.len() > 1
        && y.groups.len() > 1
        && x.groups.len().checked_mul(y.groups.len()) == Some(items.len())
        && {
            let mut columns = vec![0; boxes.len()];
            for (column, group) in x.groups.iter().enumerate() {
                for item in group {
                    columns[*item] = column;
                }
            }
            let mut occupied = vec![false; items.len()];
            y.groups.iter().enumerate().all(|(row, group)| {
                group.iter().all(|item| {
                    let slot = columns[*item] * y.groups.len() + row;
                    !std::mem::replace(&mut occupied[slot], true)
                })
            })
        };
    let (dimension, cut) = if x.gap > y.gap && !grid {
        (0, x)
    } else {
        (1, y)
    };
    if cut.gap == 0. {
        return items;
    }
    // Stable partitioning retains source order when overlapping regions cannot be separated.
    let (before, after): (Vec<_>, Vec<_>) = items
        .into_iter()
        .partition(|i| boxes[*i].unwrap().0[dimension] < cut.split);
    let mut result = partition(before, boxes, depth + 1);
    result.extend(partition(after, boxes, depth + 1));
    result
}

/// Indices for geometric text extraction. Clear whitespace separates regions;
/// complete grids read by row. Ambiguous overlaps retain their source order.
/// Nodes without text or usable geometry stay in their original slots. Groups
/// remain units; apply this function separately to their children when visiting.
/// Coordinates must be in the same page space, as in the OpenDocument model.
pub fn presentation_node_reading_order(nodes: &[SlideNode]) -> Vec<usize> {
    if nodes.len() > 4096 {
        return (0..nodes.len()).collect();
    }
    let boxes: Vec<_> = nodes.iter().map(|n| bounds(n, 0).ok().flatten()).collect();
    let items = boxes
        .iter()
        .enumerate()
        .filter_map(|(i, b)| b.is_some().then_some(i))
        .collect();
    let mut ordered = partition(items, &boxes, 0).into_iter();
    boxes
        .iter()
        .enumerate()
        .map(|(i, b)| {
            if b.is_some() {
                ordered.next().unwrap()
            } else {
                i
            }
        })
        .collect()
}

pub(crate) fn reorder_blocks(blocks: &mut [PresentationBlock], nodes: &[SlideNode]) {
    for (block, node) in blocks.iter_mut().zip(nodes) {
        if let SlideNode::Group { children, .. } = node {
            reorder_blocks(&mut block.children, children);
            block.content = block
                .children
                .iter()
                .map(|b| b.content.as_str())
                .collect::<Vec<_>>()
                .join("\n");
        }
    }
    let mut ranks = vec![0; nodes.len()];
    for (rank, index) in presentation_node_reading_order(nodes)
        .into_iter()
        .enumerate()
    {
        ranks[index] = rank;
    }
    blocks.sort_by_key(|b| {
        b.source_refs
            .first()
            .and_then(|s| s.node_path.last())
            .and_then(|i| ranks.get(*i))
            .copied()
            .unwrap_or(usize::MAX)
    });
}

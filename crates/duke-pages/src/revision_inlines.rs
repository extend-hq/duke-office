//! Ordered inline spans annotated with tracked insertions and deletions.
use crate::{
    BlockDetails, DocumentBlock, DocumentRevision, ParagraphInline, RevisionKind, RevisionScope,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InlineChange {
    #[default]
    Unchanged,
    Deletion,
    Insertion,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RevisionInline {
    pub inline: ParagraphInline,
    pub change: InlineChange,
}

#[derive(Default)]
struct Boundary {
    delta: i64,
    removed: Vec<String>,
}
struct Edit {
    start: usize,
    end: usize,
    removed: Option<String>,
}

fn paragraph_inlines(
    block: &DocumentBlock,
    edits: &[Edit],
    inherited: InlineChange,
) -> Vec<RevisionInline> {
    let mut length = 0;
    let mut offsets = HashSet::from([0]);
    for c in block.content.chars() {
        length += c.len_utf16();
        offsets.insert(length);
    }
    let mut boundaries: BTreeMap<usize, Boundary> = BTreeMap::new();
    for edit in edits {
        if edit.start > edit.end || !offsets.contains(&edit.start) || !offsets.contains(&edit.end) {
            continue;
        }
        if let Some(text) = &edit.removed {
            boundaries
                .entry(edit.start)
                .or_default()
                .removed
                .push(text.clone());
        } else {
            boundaries.entry(edit.start).or_default().delta += 1;
            boundaries.entry(edit.end).or_default().delta -= 1;
        }
    }
    let inlines = match &block.details {
        BlockDetails::Paragraph(d) => d.inlines.clone(),
        BlockDetails::Raw(d) => d
            .get("inlines")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_else(|| {
                vec![ParagraphInline::Text {
                    start: 0,
                    end: length,
                    text: block.content.clone(),
                    style: None,
                    link: None,
                    note_reference: None,
                }]
            }),
        _ => Vec::new(),
    };
    let mut events = boundaries.into_iter().peekable();
    let mut position = 0;
    let mut inserted = 0i64;
    let mut output = Vec::new();
    fn flush(
        events: &mut std::iter::Peekable<std::collections::btree_map::IntoIter<usize, Boundary>>,
        position: usize,
        inserted: &mut i64,
        output: &mut Vec<RevisionInline>,
    ) {
        if events.peek().is_some_and(|(offset, _)| *offset == position) {
            let (_, event) = events.next().unwrap();
            for text in event.removed {
                output.push(RevisionInline {
                    inline: ParagraphInline::Text {
                        start: position,
                        end: position,
                        text,
                        style: None,
                        link: None,
                        note_reference: None,
                    },
                    change: InlineChange::Deletion,
                });
            }
            *inserted += event.delta;
        }
    }
    for inline in inlines {
        flush(&mut events, position, &mut inserted, &mut output);
        let change = |n: i64| {
            if inherited != InlineChange::Unchanged {
                inherited
            } else if n > 0 {
                InlineChange::Insertion
            } else {
                InlineChange::Unchanged
            }
        };
        let text = match &inline {
            ParagraphInline::Block { block_id, .. } => {
                output.push(RevisionInline {
                    inline: ParagraphInline::Block {
                        block_id: block_id.clone(),
                        offset: position,
                    },
                    change: change(inserted),
                });
                continue;
            }
            ParagraphInline::Text { text, .. } | ParagraphInline::FormField { text, .. } => text,
        };
        let start = position;
        let utf16: Vec<_> = text.encode_utf16().collect();
        let end = start + utf16.len();
        if start == end && matches!(inline, ParagraphInline::FormField { .. }) {
            output.push(RevisionInline {
                inline: inline.clone(),
                change: change(inserted),
            });
        }
        while position < end {
            flush(&mut events, position, &mut inserted, &mut output);
            let next = end.min(events.peek().map(|(offset, _)| *offset).unwrap_or(end));
            let Ok(text) = String::from_utf16(&utf16[position - start..next - start]) else {
                // Ignore invalid boundaries rather than splitting a Unicode scalar.
                events.next();
                continue;
            };
            let piece = match &inline {
                ParagraphInline::Text {
                    style,
                    link,
                    note_reference,
                    ..
                } => ParagraphInline::Text {
                    start: position,
                    end: next,
                    text,
                    style: style.clone(),
                    link: link.clone(),
                    note_reference: note_reference.clone(),
                },
                ParagraphInline::FormField { block_id, .. } => ParagraphInline::FormField {
                    block_id: block_id.clone(),
                    start: position,
                    end: next,
                    text,
                },
                _ => unreachable!(),
            };
            output.push(RevisionInline {
                inline: piece,
                change: change(inserted),
            });
            position = next;
        }
    }
    flush(&mut events, position, &mut inserted, &mut output);
    output
}

/// Produce spans only for affected paragraphs; all offsets address current text in UTF-16.
/// Deleted text is anchored at an empty range and does not shift subsequent offsets.
pub fn revision_inlines(
    blocks: &[DocumentBlock],
    revisions: &[DocumentRevision],
) -> BTreeMap<String, Vec<RevisionInline>> {
    let mut edits: HashMap<&str, Vec<Edit>> = HashMap::new();
    let mut containers = HashMap::new();
    for revision in revisions {
        let deletion = matches!(
            revision.kind,
            RevisionKind::Deletion | RevisionKind::MoveFrom
        );
        if !deletion
            && !matches!(
                revision.kind,
                RevisionKind::Insertion | RevisionKind::MoveTo
            )
        {
            continue;
        }
        for source in &revision.source_refs {
            if matches!(
                revision.scope,
                RevisionScope::TableRow | RevisionScope::TableCell
            ) {
                containers.insert(
                    source.node_id.as_str(),
                    if deletion {
                        InlineChange::Deletion
                    } else {
                        InlineChange::Insertion
                    },
                );
            } else if revision.scope == RevisionScope::Text {
                if let (Some(start), Some(end)) = (source.start, source.end) {
                    if if deletion {
                        start != end || revision.text.as_ref().is_none_or(String::is_empty)
                    } else {
                        start >= end
                    } {
                        continue;
                    }
                    edits.entry(&source.node_id).or_default().push(Edit {
                        start,
                        end,
                        removed: if deletion {
                            revision.text.clone()
                        } else {
                            None
                        },
                    });
                }
            }
        }
    }
    fn visit(
        block: &DocumentBlock,
        inherited: InlineChange,
        edits: &HashMap<&str, Vec<Edit>>,
        containers: &HashMap<&str, InlineChange>,
        output: &mut BTreeMap<String, Vec<RevisionInline>>,
    ) {
        let state = containers
            .get(block.id.as_str())
            .copied()
            .unwrap_or(inherited);
        let paragraph = match &block.details {
            BlockDetails::Paragraph(_) => true,
            BlockDetails::Raw(d) => d["kind"] == "paragraph",
            _ => false,
        };
        let mut mapped = Vec::new();
        let changes = if paragraph && block.source_refs.len() > 1 {
            let mut base = 0;
            for source in &block.source_refs {
                let (Some(start), Some(end)) = (source.start, source.end) else {
                    continue;
                };
                if end < start {
                    continue;
                }
                for edit in edits.get(source.node_id.as_str()).into_iter().flatten() {
                    if edit.start >= start && edit.end <= end {
                        mapped.push(Edit {
                            start: base + edit.start - start,
                            end: base + edit.end - start,
                            removed: edit.removed.clone(),
                        });
                    }
                }
                base += end - start;
            }
            mapped.as_slice()
        } else {
            edits.get(block.id.as_str()).map(Vec::as_slice).unwrap_or_default()
        };
        if paragraph && (!changes.is_empty() || state != InlineChange::Unchanged) {
            output.insert(
                block.id.clone(),
                paragraph_inlines(block, changes, state),
            );
        }
        for child in &block.children {
            visit(child, state, edits, containers, output);
        }
    }
    let mut output = BTreeMap::new();
    if !edits.is_empty() || !containers.is_empty() {
        for block in blocks {
            visit(
                block,
                InlineChange::Unchanged,
                &edits,
                &containers,
                &mut output,
            );
        }
    }
    output
}

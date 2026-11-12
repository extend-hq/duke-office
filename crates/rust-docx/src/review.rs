use crate::xml::{
    direct_child_tag, extract_direct_child_tag_ranges, get_attribute_ref, tag_token_end,
};
use crate::{
    model::*, revisions::RevisionCollector, BlockType, DocumentBlock, DocumentRevision, SourceRef,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentTarget {
    #[serde(flatten)]
    pub source: SourceRef,
    pub child_start: usize,
    pub child_end: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentComment {
    pub id: i64,
    pub author: Option<String>,
    pub initials: Option<String>,
    pub date: Option<String>,
    pub text: String,
    pub parent_id: Option<i64>,
    pub resolved: Option<bool>,
    pub block_ids: Vec<String>,
    pub targets: Vec<CommentTarget>,
    pub reference: Option<SourceRef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentReview {
    pub schema_version: u32,
    pub revisions: Vec<DocumentRevision>,
    pub comments: Vec<DocumentComment>,
}

pub fn extract_document_review(model: &DocModel) -> DocumentReview {
    collect(model, true)
}

pub fn extract_document_comments(model: &DocModel) -> Vec<DocumentComment> {
    collect(model, false).comments
}

fn source(model: &DocModel, part: &str, story: &str, id: String) -> SourceRef {
    SourceRef {
        document_id: model
            .metadata
            .document_id
            .clone()
            .unwrap_or_else(|| "unsaved".into()),
        node_id: id,
        part_name: part.into(),
        story: story.into(),
        start: None,
        end: None,
        offset_encoding: "utf-16".into(),
    }
}

fn node_id(node: &DocNode) -> Option<&str> {
    match node {
        DocNode::Paragraph(p) => p.block_id.as_deref(),
        DocNode::Table(t) => t.block_id.as_deref(),
    }
}

fn block_ids(nodes: &[DocNode], ids: &mut Vec<String>) {
    for node in nodes {
        if let Some(id) = node_id(node) {
            ids.push(id.to_owned());
        }
        match node {
            DocNode::Paragraph(p) => {
                for child in &p.children {
                    if let ParagraphChildNode::Image(image) = child {
                        if let Some(id) = &image.block_id {
                            ids.push(id.clone());
                        }
                        if let Some(nodes) = &image.content_nodes {
                            block_ids(nodes, ids);
                        }
                    }
                }
            }
            DocNode::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        for n in &cell.nodes {
                            match n {
                                TableCellContentNode::Paragraph(p) => {
                                    block_ids(&[DocNode::Paragraph(p.clone())], ids)
                                }
                                TableCellContentNode::Table(t) => {
                                    block_ids(&[DocNode::Table(*t.clone())], ids)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

struct Collector<'a> {
    model: &'a DocModel,
    revisions: Option<RevisionCollector>,
    comments: BTreeMap<i64, DocumentComment>,
}

impl Collector<'_> {
    fn story(&mut self, nodes: &[DocNode], part: &str, kind: &str, root: &str) {
        let moves = self
            .revisions
            .as_mut()
            .map(|r| std::mem::take(&mut r.moves));
        self.nodes(nodes, part, kind, root, &mut BTreeSet::new());
        if let (Some(r), Some(moves)) = (&mut self.revisions, moves) {
            r.moves = moves;
        }
    }

    fn nodes(
        &mut self,
        nodes: &[DocNode],
        part: &str,
        kind: &str,
        root: &str,
        active: &mut BTreeSet<i64>,
    ) {
        for (index, node) in nodes.iter().enumerate() {
            let id = node_id(node)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{root}/{index}"));
            let source = source(self.model, part, kind, id);
            match node {
                DocNode::Paragraph(p) => self.paragraph(p, &source, active),
                DocNode::Table(t) => self.table(t, &source, active),
            }
        }
    }

    fn table(&mut self, table: &TableNode, source: &SourceRef, active: &mut BTreeSet<i64>) {
        if let (Some(r), Some(xml)) = (&mut self.revisions, &table.source_xml) {
            r.table(xml, source, table);
        }
        for (ri, row) in table.rows.iter().enumerate() {
            for (ci, cell) in row.cells.iter().enumerate() {
                for (ni, node) in cell.nodes.iter().enumerate() {
                    let fallback = format!("{}/row/{ri}/cell/{ci}/{ni}", source.node_id);
                    match node {
                        TableCellContentNode::Paragraph(p) => self.paragraph(
                            p,
                            &SourceRef {
                                node_id: p.block_id.clone().unwrap_or(fallback),
                                ..source.clone()
                            },
                            active,
                        ),
                        TableCellContentNode::Table(t) => self.table(
                            t,
                            &SourceRef {
                                node_id: t.block_id.clone().unwrap_or(fallback),
                                ..source.clone()
                            },
                            active,
                        ),
                    }
                }
            }
        }
    }

    fn paragraph(&mut self, p: &ParagraphNode, source: &SourceRef, active: &mut BTreeSet<i64>) {
        if let Some(xml) = &p.source_xml {
            if let Some(r) = &mut self.revisions {
                let content = p
                    .children
                    .iter()
                    .map(|c| match c {
                        ParagraphChildNode::Text(t) => t.text.as_str(),
                        ParagraphChildNode::FormField(f) => f.value.as_deref().unwrap_or(""),
                        ParagraphChildNode::Image(_) => "",
                    })
                    .collect();
                let block = DocumentBlock {
                    object: "block".into(),
                    id: source.node_id.clone(),
                    r#type: BlockType::Text,
                    content,
                    details: serde_json::Value::Null.into(),
                    source_refs: vec![source.clone()],
                    parent_block_id: None,
                    children: vec![],
                    words: None,
                };
                r.paragraph(xml, &block, p);
            }
            if !self.comments.is_empty() {
                CommentWalk {
                    collector: self,
                    paragraph: p,
                    source,
                    active,
                    offset: 0,
                    child: 0,
                }
                .visit(xml);
            }
        }
        if let (None, Some(ranges)) = (&p.source_xml, &p.comment_ranges) {
            self.comment_ranges(p, ranges, source);
        } else if p.source_xml.is_none() && !active.is_empty() {
            let mut offset = 0;
            for (index, child) in p.children.iter().enumerate() {
                let length = match child {
                    ParagraphChildNode::Text(t) => t.text.encode_utf16().count(),
                    ParagraphChildNode::FormField(f) => {
                        f.value.as_deref().unwrap_or("").encode_utf16().count()
                    }
                    _ => 0,
                };
                let target = match child {
                    ParagraphChildNode::Image(image) => SourceRef {
                        node_id: image
                            .block_id
                            .clone()
                            .unwrap_or_else(|| format!("{}/child/{index}", source.node_id)),
                        ..source.clone()
                    },
                    _ => SourceRef {
                        start: Some(offset),
                        end: Some(offset + length),
                        ..source.clone()
                    },
                };
                for id in active.iter() {
                    self.comments
                        .get_mut(id)
                        .unwrap()
                        .targets
                        .push(CommentTarget {
                            source: target.clone(),
                            child_start: index,
                            child_end: index + 1,
                        });
                }
                offset += length;
            }
        }
        for (index, child) in p.children.iter().enumerate() {
            if let ParagraphChildNode::Image(image) = child {
                if let Some(nodes) = &image.content_nodes {
                    let root = image
                        .block_id
                        .clone()
                        .unwrap_or_else(|| format!("{}/child/{index}", source.node_id));
                    self.story(
                        nodes,
                        &source.part_name,
                        "textbox",
                        &format!("{root}/content"),
                    );
                }
            }
        }
    }
}

impl Collector<'_> {
    /// Targets of the comments a paragraph without Word markup states as child ranges.
    fn comment_ranges(&mut self, p: &ParagraphNode, ranges: &[CommentRange], source: &SourceRef) {
        // UTF-16 offset of every child, and of the paragraph's end.
        let mut offsets = Vec::with_capacity(p.children.len() + 1);
        let mut offset = 0;
        for child in &p.children {
            offsets.push(offset);
            offset += match child {
                ParagraphChildNode::Text(t) => t.text.encode_utf16().count(),
                ParagraphChildNode::FormField(f) => {
                    f.value.as_deref().unwrap_or("").encode_utf16().count()
                }
                ParagraphChildNode::Image(_) => 0,
            };
        }
        offsets.push(offset);
        for range in ranges {
            let Some(comment) = self.comments.get_mut(&range.comment_id) else {
                continue;
            };
            let end = range.child_end.min(p.children.len());
            let mut text_start = None;
            for index in range.child_start..=end {
                let image = match p.children.get(index).filter(|_| index < end) {
                    Some(ParagraphChildNode::Image(image)) => Some(image),
                    Some(_) => {
                        text_start.get_or_insert(index);
                        continue;
                    }
                    None => None,
                };
                // An image, or the end of the range, closes the text before it.
                if let Some(start) = text_start.take() {
                    comment.targets.push(CommentTarget {
                        source: SourceRef {
                            start: Some(offsets[start]),
                            end: Some(offsets[index]),
                            ..source.clone()
                        },
                        child_start: start,
                        child_end: index,
                    });
                }
                if let Some(image) = image {
                    comment.targets.push(CommentTarget {
                        source: SourceRef {
                            node_id: image
                                .block_id
                                .clone()
                                .unwrap_or_else(|| format!("{}/child/{index}", source.node_id)),
                            ..source.clone()
                        },
                        child_start: index,
                        child_end: index + 1,
                    });
                }
            }
            if range.ends_here {
                comment.reference = Some(SourceRef {
                    start: Some(offsets[end]),
                    end: Some(offsets[end]),
                    ..source.clone()
                });
            }
        }
    }
}

struct CommentWalk<'a, 'b> {
    collector: &'a mut Collector<'b>,
    paragraph: &'a ParagraphNode,
    source: &'a SourceRef,
    active: &'a mut BTreeSet<i64>,
    offset: usize,
    child: usize,
}

impl CommentWalk<'_, '_> {
    fn visit(&mut self, xml: &str) {
        for range in extract_direct_child_tag_ranges(xml) {
            let child = &xml[range.start..range.end];
            match range.tag_name.as_str() {
                "w:commentRangeStart" | "w:commentRangeEnd" | "w:commentReference" => {
                    let id = tag_token_end(child, 0)
                        .and_then(|end| get_attribute_ref(&child[..end], "w:id"))
                        .and_then(|s| s.parse::<i64>().ok());
                    if let Some(id) = id.filter(|id| self.collector.comments.contains_key(id)) {
                        match range.tag_name.as_str() {
                            "w:commentRangeStart" => {
                                self.active.insert(id);
                            }
                            "w:commentRangeEnd" => {
                                self.active.remove(&id);
                            }
                            _ => {
                                self.collector.comments.get_mut(&id).unwrap().reference =
                                    Some(SourceRef {
                                        start: Some(self.offset),
                                        end: Some(self.offset),
                                        ..self.source.clone()
                                    });
                            }
                        }
                    }
                }
                "w:r" => {
                    self.visit(child);
                    let active = crate::parse::run_text_xml(child);
                    let tokens = crate::parse::parse_run_text_tokens(&active);
                    let count = tokens
                        .iter()
                        .filter(|t| !t.text.is_empty() || t.note_reference.is_some())
                        .count()
                        + crate::xml::extract_balanced_tag_slices(&active, "w:drawing").len()
                        + crate::xml::extract_balanced_tag_slices(&active, "w:pict").len();
                    for _ in 0..count {
                        let Some(node) = self.paragraph.children.get(self.child) else {
                            break;
                        };
                        let length = match node {
                            ParagraphChildNode::Text(t) => t.text.encode_utf16().count(),
                            ParagraphChildNode::FormField(f) => {
                                f.value.as_deref().unwrap_or("").encode_utf16().count()
                            }
                            _ => 0,
                        };
                        let target_source = if let ParagraphChildNode::Image(image) = node {
                            SourceRef {
                                node_id: image.block_id.clone().unwrap_or_else(|| {
                                    format!("{}/child/{}", self.source.node_id, self.child)
                                }),
                                ..self.source.clone()
                            }
                        } else {
                            SourceRef {
                                start: Some(self.offset),
                                end: Some(self.offset + length),
                                ..self.source.clone()
                            }
                        };
                        for id in self.active.iter() {
                            let comment = self.collector.comments.get_mut(id).unwrap();
                            if let Some(last) = comment.targets.last_mut().filter(|last| {
                                target_source.start.is_some()
                                    && last.source.node_id == target_source.node_id
                                    && last.source.end == target_source.start
                                    && last.child_end == self.child
                            }) {
                                last.source.end = target_source.end;
                                last.child_end = self.child + 1;
                            } else {
                                comment.targets.push(CommentTarget {
                                    source: target_source.clone(),
                                    child_start: self.child,
                                    child_end: self.child + 1,
                                });
                            }
                        }
                        self.offset += length;
                        self.child += 1;
                    }
                }
                "w:ins" | "w:moveTo" | "w:hyperlink" | "w:sdt" | "w:sdtContent" | "w:smartTag"
                | "w:customXml" | "w:fldSimple" | "w:dir" | "w:bdo" => self.visit(child),
                "mc:AlternateContent" => {
                    if let Some(xml) = direct_child_tag(child, "mc:Choice")
                        .or_else(|| direct_child_tag(child, "mc:Fallback"))
                    {
                        self.visit(xml);
                    }
                }
                _ => {}
            }
        }
    }
}

fn collect(model: &DocModel, revisions: bool) -> DocumentReview {
    let comments = model
        .metadata
        .comments
        .iter()
        .flatten()
        .map(|c| {
            let mut ids = vec![];
            if let Some(nodes) = &c.nodes {
                block_ids(nodes, &mut ids);
            }
            (
                c.id,
                DocumentComment {
                    id: c.id,
                    author: c.author.clone(),
                    initials: c.initials.clone(),
                    date: c.date.clone(),
                    text: c.text.clone(),
                    parent_id: c.parent_id,
                    resolved: c.resolved,
                    block_ids: ids,
                    targets: vec![],
                    reference: None,
                },
            )
        })
        .collect();
    let mut collector = Collector {
        model,
        revisions: revisions.then(RevisionCollector::default),
        comments,
    };
    let part = |name: &str| {
        model
            .metadata
            .source_part_names
            .as_ref()
            .and_then(|parts| parts.get(name))
            .cloned()
            .unwrap_or_else(|| format!("word/{name}.xml"))
    };
    let root = format!(
        "docx:{}",
        model.metadata.document_id.as_deref().unwrap_or("unsaved")
    );
    let main = part("document");
    collector.story(&model.nodes, &main, "body", &format!("{root}/{main}/body"));
    if let (Some(r), Some(section), Some(last)) = (
        &mut collector.revisions,
        &model.metadata.section_properties_xml,
        model.nodes.last(),
    ) {
        let in_paragraph = matches!(last, DocNode::Paragraph(p) if p.source_xml.as_deref().is_some_and(|xml| xml.contains(section)));
        if !in_paragraph {
            let id = node_id(last)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{root}/{main}/body/{}", model.nodes.len() - 1));
            r.final_section(section, &source(model, &main, "body", id));
        }
    }
    let mut seen = HashSet::new();
    for (kind, sections) in [("header", &model.metadata.header_sections)] {
        for section in sections.iter().chain(
            model
                .metadata
                .sections
                .iter()
                .flatten()
                .flat_map(|s| s.header_sections.iter()),
        ) {
            if seen.insert(section.part_name.clone()) {
                collector.story(
                    &section.nodes,
                    &section.part_name,
                    kind,
                    &format!("{root}/{}", section.part_name),
                );
            }
        }
    }
    for section in model.metadata.footer_sections.iter().chain(
        model
            .metadata
            .sections
            .iter()
            .flatten()
            .flat_map(|s| s.footer_sections.iter()),
    ) {
        if seen.insert(section.part_name.clone()) {
            collector.story(
                &section.nodes,
                &section.part_name,
                "footer",
                &format!("{root}/{}", section.part_name),
            );
        }
    }
    for (kind, notes) in [
        ("footnote", &model.metadata.footnotes),
        ("endnote", &model.metadata.endnotes),
    ] {
        let part = part(&format!("{kind}s"));
        for note in notes.iter().flatten() {
            if let Some(nodes) = &note.nodes {
                collector.story(
                    nodes,
                    &part,
                    kind,
                    &format!("{root}/{part}/{kind}/{}", note.id),
                );
            }
        }
    }
    let part = part("comments");
    for comment in model.metadata.comments.iter().flatten() {
        if let Some(nodes) = &comment.nodes {
            collector.story(
                nodes,
                &part,
                "comment",
                &format!("{root}/{part}/comment/{}", comment.id),
            );
        }
    }
    for c in collector.comments.values_mut() {
        if c.targets.is_empty() {
            if let Some(source) = &c.reference {
                c.targets.push(CommentTarget {
                    source: source.clone(),
                    child_start: 0,
                    child_end: 0,
                });
            }
        }
    }
    let inherited: Vec<_> = collector
        .comments
        .values()
        .filter_map(|c| {
            if !c.targets.is_empty() {
                return None;
            }
            let parent = collector.comments.get(&c.parent_id?)?;
            Some((c.id, parent.targets.clone(), parent.reference.clone()))
        })
        .collect();
    for (id, targets, reference) in inherited {
        let c = collector.comments.get_mut(&id).unwrap();
        c.targets = targets;
        c.reference = reference;
    }
    DocumentReview {
        schema_version: 1,
        revisions: collector.revisions.map(|r| r.records).unwrap_or_default(),
        comments: collector.comments.into_values().collect(),
    }
}

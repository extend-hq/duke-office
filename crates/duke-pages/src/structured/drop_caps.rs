use super::{BlockDetails, DocumentBlock, ParagraphInline};
use crate::model::{ParagraphChildNode, ParagraphNode};

pub(super) fn can_join(cap: &ParagraphNode, body: &ParagraphNode) -> bool {
    cap.style.as_ref().is_some_and(|style| style.drop_cap.is_some())
        && body.style.as_ref().is_none_or(|style| {
            style.drop_cap.is_none() && style.page_break_before != Some(true)
        })
        && cap.children.iter().all(|child| {
            matches!(child, ParagraphChildNode::Text(run) if !run.text.contains(['\r', '\n']))
        })
        && cap.children.iter().any(|child| {
            matches!(child, ParagraphChildNode::Text(run) if !run.text.trim().is_empty())
        })
        && body.children.iter().any(|child| match child {
            ParagraphChildNode::Text(run) => !run.text.trim().is_empty(),
            ParagraphChildNode::FormField(field) => field.value.as_ref().is_some_and(|value| !value.trim().is_empty()),
            _ => false,
        })
        && !cap.source_xml.as_deref().is_some_and(|xml| {
            crate::xml::direct_child_tag(xml, "w:pPr")
                .and_then(|props| crate::xml::direct_child_tag(props, "w:sectPr"))
                .is_some()
        })
}

pub(super) fn join(mut cap: DocumentBlock, mut body: DocumentBlock) -> DocumentBlock {
    let cap_length = cap.content.encode_utf16().count();
    let body_length = body.content.encode_utf16().count();
    if let (BlockDetails::Paragraph(cap_details), BlockDetails::Paragraph(body_details)) =
        (&mut cap.details, &mut body.details)
    {
        body_details.drop_cap = cap_details.drop_cap.take();
        let drop_cap = cap_details
            .style
            .as_ref()
            .and_then(|style| style.drop_cap.clone());
        body_details
            .style
            .get_or_insert_with(Default::default)
            .drop_cap = drop_cap;
        for inline in &mut body_details.inlines {
            match inline {
                ParagraphInline::Text { start, end, .. }
                | ParagraphInline::FormField { start, end, .. } => {
                    *start += cap_length;
                    *end += cap_length;
                }
                ParagraphInline::Block { offset, .. } => *offset += cap_length,
            }
        }
        cap_details.inlines.append(&mut body_details.inlines);
        body_details.inlines = std::mem::take(&mut cap_details.inlines);
        cap_details
            .field_instructions
            .append(&mut body_details.field_instructions);
        body_details.field_instructions = std::mem::take(&mut cap_details.field_instructions);
    }
    for (block, length) in [(&mut cap, cap_length), (&mut body, body_length)] {
        for source in &mut block.source_refs {
            source.start = Some(0);
            source.end = Some(length);
        }
    }
    for child in &mut body.children {
        child.parent_block_id = Some(cap.id.clone());
    }
    cap.content.push_str(&body.content);
    cap.source_refs.append(&mut body.source_refs);
    cap.children.append(&mut body.children);
    cap.r#type = body.r#type;
    cap.details = body.details;
    cap
}

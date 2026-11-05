use crate::model::{
    DocModel, DocNode, ImageRunNode, ParagraphChildNode, ParagraphNode, TableCellContentNode,
    TableNode,
};
use crate::{OoxmlPackage, ParseOptions};
use rust_office_art::model3d::{parse_model3d, Model3DAsset};

struct Models<'a> {
    package: &'a OoxmlPackage,
    options: &'a ParseOptions,
    content_types: super::ContentTypeLookup,
    missing_assets: bool,
}

impl Models<'_> {
    fn image(&mut self, image: &mut ImageRunNode) {
        if let Some(xml) = image.source_xml.as_deref() {
            image.model_3d = parse_model3d(xml, self.options, |id| {
                let target = image
                    .presentation
                    .as_ref()?
                    .assets
                    .iter()
                    .find(|asset| asset.relationship_id.as_deref() == Some(id))?
                    .target
                    .as_deref()?;
                let data = self.package.binary_assets.get(target)?;
                Some(Model3DAsset {
                    part_name: target.to_owned(),
                    content_type: super::content_type_for_part(target, &self.content_types)
                        .unwrap_or_else(|| "application/octet-stream".into()),
                    data: Some(data.clone().into()),
                })
            })
            .map(Box::new);
            if let Some(model) = &image.model_3d {
                image.semantic_kind = Some("model_3d".into());
                self.missing_assets |=
                    model.model_relationship_id.is_some() && model.asset.is_none();
            }
        }
        if let Some(nodes) = &mut image.content_nodes {
            self.nodes(nodes);
        }
    }

    fn children(&mut self, children: &mut [ParagraphChildNode]) {
        for child in children {
            match child {
                ParagraphChildNode::Image(image) => self.image(image),
                ParagraphChildNode::FormField(field) => {
                    if let Some(preview) = &mut field.source_preview {
                        self.image(&mut preview.image);
                    }
                }
                ParagraphChildNode::Text(_) => {}
            }
        }
    }

    fn paragraph(&mut self, paragraph: &mut ParagraphNode) {
        self.children(&mut paragraph.children);
        for fragment in paragraph.review_content.iter_mut().flatten() {
            self.children(&mut fragment.children);
        }
    }

    fn table(&mut self, table: &mut TableNode) {
        for row in table.rows.iter_mut().chain(
            table
                .review_rows
                .iter_mut()
                .flatten()
                .map(|fragment| &mut fragment.row),
        ) {
            for cell in &mut row.cells {
                for node in &mut cell.nodes {
                    match node {
                        TableCellContentNode::Paragraph(paragraph) => self.paragraph(paragraph),
                        TableCellContentNode::Table(table) => self.table(table),
                    }
                }
            }
        }
    }

    fn nodes(&mut self, nodes: &mut [DocNode]) {
        for node in nodes {
            match node {
                DocNode::Paragraph(paragraph) => self.paragraph(paragraph),
                DocNode::Table(table) => self.table(table),
            }
        }
    }
}

pub(super) fn attach_models(model: &mut DocModel, package: &OoxmlPackage, options: &ParseOptions) {
    let mut models = Models {
        package,
        options,
        content_types: super::parse_content_types(package),
        missing_assets: false,
    };
    models.nodes(&mut model.nodes);
    let metadata = &mut model.metadata;
    for header in &mut metadata.header_sections {
        models.nodes(&mut header.nodes);
    }
    for footer in &mut metadata.footer_sections {
        models.nodes(&mut footer.nodes);
    }
    for section in metadata.sections.iter_mut().flatten() {
        for header in &mut section.header_sections {
            models.nodes(&mut header.nodes);
        }
        for footer in &mut section.footer_sections {
            models.nodes(&mut footer.nodes);
        }
    }
    for notes in [&mut metadata.footnotes, &mut metadata.endnotes] {
        for note in notes.iter_mut().flatten() {
            if let Some(nodes) = &mut note.nodes {
                models.nodes(nodes);
            }
        }
    }
    for comment in metadata.comments.iter_mut().flatten() {
        if let Some(nodes) = &mut comment.nodes {
            models.nodes(nodes);
        }
    }
    if models.missing_assets {
        metadata
            .warnings
            .push("An embedded 3D model asset could not be resolved".into());
    }
}

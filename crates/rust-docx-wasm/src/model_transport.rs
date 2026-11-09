use std::collections::HashMap;

use js_sys::{Reflect, Uint8Array};
use rust_docx::{
    DocModel, DocNode, ImageRunNode, NumberingLevelDefinition, OoxmlPackage, ParagraphChildNode,
    ParagraphNode, TableCellContentNode, TableNode,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

struct Payload {
    path: Vec<JsValue>,
    src: Option<JsValue>,
    data: Option<JsValue>,
}

struct Transport<'a> {
    package: &'a OoxmlPackage,
    assets: HashMap<String, JsValue>,
    sources: HashMap<String, JsValue>,
    detached: HashMap<Vec<u8>, JsValue>,
    path: Vec<JsValue>,
    payloads: Vec<Payload>,
}

impl Transport<'_> {
    fn at(&mut self, key: impl Into<JsValue>, visit: impl FnOnce(&mut Self)) {
        self.path.push(key.into());
        visit(self);
        self.path.pop();
    }

    fn list<T>(&mut self, key: &str, items: &mut [T], mut visit: impl FnMut(&mut Self, &mut T)) {
        self.at(key, |this| {
            for (index, item) in items.iter_mut().enumerate() {
                this.at(index as u32, |this| visit(this, item));
            }
        });
    }

    fn payload(&mut self, src: &mut Option<String>, data: Option<Vec<u8>>, part: Option<&str>) {
        let src = src.take().map(|src| {
            self.sources
                .entry(src)
                .or_insert_with_key(|src| JsValue::from_str(src))
                .clone()
        });
        let data = data.map(|data| {
            if let Some(part) = part {
                if self.package.binary_assets.get(part) == Some(&data) {
                    if let Some(asset) = self.assets.get(part) {
                        return asset.clone();
                    }
                }
            }
            self.detached
                .entry(data)
                .or_insert_with_key(|data| Uint8Array::from(data.as_slice()).into())
                .clone()
        });
        if src.is_some() || data.is_some() {
            self.payloads.push(Payload {
                path: self.path.clone(),
                src,
                data,
            });
        }
    }

    fn image(&mut self, image: &mut ImageRunNode) {
        if let Some(asset) = image.model_3d.as_mut().and_then(|model| model.asset.as_mut()) {
            self.at("model3d", |this| {
                this.at("asset", |this| {
                    this.payload(
                        &mut None,
                        asset.data.take().map(|data| data.into_vec()),
                        Some(&asset.part_name),
                    );
                });
            });
        }
        self.payload(
            &mut image.src,
            image.data.take(),
            image.part_name.as_deref(),
        );
        if let Some(nodes) = &mut image.content_nodes {
            self.nodes_at("contentNodes", nodes);
        }
    }

    fn paragraph(&mut self, paragraph: &mut ParagraphNode) {
        self.children(&mut paragraph.children);
        if let Some(content) = &mut paragraph.review_content {
            self.list("reviewContent", content, |this, fragment| this.children(&mut fragment.children));
        }
    }

    fn children(&mut self, children: &mut [ParagraphChildNode]) {
        self.list(
            "children",
            children,
            |this, child| match child {
                ParagraphChildNode::Image(image) => this.image(image),
                ParagraphChildNode::FormField(field) => {
                    if let Some(preview) = &mut field.source_preview {
                        this.at("sourcePreview", |this| {
                            this.at("image", |this| this.image(&mut preview.image));
                        });
                    }
                }
                ParagraphChildNode::Text(_) => {}
            },
        );
    }

    fn table(&mut self, table: &mut TableNode) {
        self.list("rows", &mut table.rows, Self::row);
        if let Some(rows) = &mut table.review_rows {
            self.list("reviewRows", rows, |this, fragment| this.at("row", |this| this.row(&mut fragment.row)));
        }
    }

    fn row(&mut self, row: &mut rust_docx::TableRowNode) {
        self.list("cells", &mut row.cells, |this, cell| {
            this.list("nodes", &mut cell.nodes, |this, node| match node {
                TableCellContentNode::Paragraph(paragraph) => this.paragraph(paragraph),
                TableCellContentNode::Table(table) => this.table(table),
            });
        });
    }

    fn nodes(&mut self, nodes: &mut [DocNode]) {
        self.nodes_at("nodes", nodes);
    }

    fn nodes_at(&mut self, key: &str, nodes: &mut [DocNode]) {
        self.list(key, nodes, |this, node| match node {
            DocNode::Paragraph(paragraph) => this.paragraph(paragraph),
            DocNode::Table(table) => this.table(table),
        });
    }

    fn level(&mut self, level: &mut NumberingLevelDefinition) {
        if let Some(bullet) = &mut level.picture_bullet {
            self.at("pictureBullet", |this| {
                this.payload(&mut bullet.src, None, None)
            });
        }
    }

    fn model(&mut self, model: &mut DocModel) {
        self.nodes(&mut model.nodes);
        self.at("metadata", |this| {
            let metadata = &mut model.metadata;
            this.list(
                "headerSections",
                &mut metadata.header_sections,
                |this, header| this.nodes(&mut header.nodes),
            );
            this.list(
                "footerSections",
                &mut metadata.footer_sections,
                |this, footer| this.nodes(&mut footer.nodes),
            );
            if let Some(sections) = &mut metadata.sections {
                this.list("sections", sections, |this, section| {
                    this.list(
                        "headerSections",
                        &mut section.header_sections,
                        |this, header| this.nodes(&mut header.nodes),
                    );
                    this.list(
                        "footerSections",
                        &mut section.footer_sections,
                        |this, footer| this.nodes(&mut footer.nodes),
                    );
                });
            }
            for (key, notes) in [
                ("footnotes", &mut metadata.footnotes),
                ("endnotes", &mut metadata.endnotes),
            ] {
                if let Some(notes) = notes {
                    this.list(key, notes, |this, note| {
                        if let Some(nodes) = &mut note.nodes {
                            this.nodes(nodes);
                        }
                    });
                }
            }
            if let Some(comments) = &mut metadata.comments {
                this.list("comments", comments, |this, comment| {
                    if let Some(nodes) = &mut comment.nodes {
                        this.nodes(nodes);
                    }
                });
            }
            if let Some(numbering) = &mut metadata.numbering_definitions {
                this.at("numberingDefinitions", |this| {
                    this.list("abstracts", &mut numbering.abstracts, |this, definition| {
                        this.list("levels", &mut definition.levels, Self::level);
                    });
                    this.list("instances", &mut numbering.instances, |this, instance| {
                        if let Some(levels) = &mut instance.level_overrides {
                            this.list("levelOverrides", levels, Self::level);
                        }
                    });
                });
            }
        });
    }
}

pub(crate) fn model_to_js(
    mut model: DocModel,
    package: &OoxmlPackage,
    assets: &JsValue,
) -> Result<JsValue, JsValue> {
    let mut transport = Transport {
        package,
        assets: package
            .binary_assets
            .keys()
            .map(|key| {
                Reflect::get(assets, &JsValue::from_str(key)).map(|value| {
                    let value = if value.is_instance_of::<Uint8Array>() {
                        value
                    } else {
                        Uint8Array::from(package.binary_assets[key].as_slice()).into()
                    };
                    (key.clone(), value)
                })
            })
            .collect::<Result<_, _>>()?,
        sources: HashMap::new(),
        detached: HashMap::new(),
        path: Vec::new(),
        payloads: Vec::new(),
    };
    // Keep repeated image payloads out of JSON and share immutable asset buffers.
    transport.model(&mut model);
    let json = serde_json::to_string(&model)
        .map_err(|error| JsValue::from_str(&format!("Model serialization failed: {error}")))?;
    drop(model);
    let result = crate::split_json::parse(&json)?;
    for payload in transport.payloads {
        let mut target = result.clone();
        for key in payload.path {
            target = Reflect::get(&target, &key)?;
        }
        if let Some(src) = payload.src {
            Reflect::set(&target, &JsValue::from_str("src"), &src)?;
        }
        if let Some(data) = payload.data {
            Reflect::set(&target, &JsValue::from_str("data"), &data)?;
        }
    }
    Ok(result)
}

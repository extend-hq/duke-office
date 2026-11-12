use rust_docx::{
    extract_document_blocks, extract_document_review, is_odf_format, package_to_bytes,
    parse_document_blocks, parse_document_bundle, parse_document_bytes, parse_document_model,
    parse_odt, BlockType, DocModel, DocNode, DocumentBlock, HeadingLevel, ImageRunNode,
    NoteReferenceKind, ParagraphAlignment, ParagraphChildNode, ParagraphNode, TableCellContentNode,
    TableNode, TextRunNode,
};

const NAMESPACES: &str = concat!(
    r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" "#,
    r#"xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" "#,
    r#"xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" "#,
    r#"xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" "#,
    r#"xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" "#,
    r#"xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" "#,
    r#"xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" "#,
    r#"xmlns:xlink="http://www.w3.org/1999/xlink" "#,
    r#"xmlns:dc="http://purl.org/dc/elements/1.1/""#,
);

const STYLES: &str = concat!(
    r#"<style:default-style style:family="paragraph"><style:text-properties fo:font-size="11pt"/></style:default-style>"#,
    r#"<style:style style:name="Standard" style:family="paragraph"/>"#,
    r#"<style:style style:name="Heading_20_1" style:display-name="Heading 1" style:family="paragraph" style:parent-style-name="Standard" style:default-outline-level="1"><style:text-properties fo:font-size="18pt" fo:font-weight="bold"/></style:style>"#,
    r##"<style:style style:name="Internet_20_link" style:display-name="Internet link" style:family="text"><style:text-properties fo:color="#000080"/></style:style>"##,
    r#"<text:list-style style:name="Bullets"><text:list-level-style-bullet text:level="1" text:bullet-char="•"/><text:list-level-style-bullet text:level="2" text:bullet-char="◦"/></text:list-style>"#,
    r#"<text:list-style style:name="Numbers"><text:list-level-style-number text:level="1" style:num-format="1" style:num-suffix="."/></text:list-style>"#,
);

const AUTOMATIC: &str = concat!(
    r#"<style:style style:name="P1" style:family="paragraph" style:parent-style-name="Standard"><style:paragraph-properties fo:text-align="center"/></style:style>"#,
    r#"<style:style style:name="P2" style:family="paragraph" style:parent-style-name="Standard"><style:paragraph-properties fo:break-after="page"/></style:style>"#,
    r#"<style:style style:name="P3" style:family="paragraph" style:parent-style-name="Standard" style:master-page-name="Wide"/>"#,
    r#"<style:style style:name="T2" style:family="text"><style:text-properties text:display="none"/></style:style>"#,
    r#"<style:style style:name="T3" style:family="text"><style:text-properties fo:font-size="50%"/></style:style>"#,
    r##"<style:style style:name="T1" style:family="text"><style:text-properties fo:font-weight="bold" fo:color="#ff0000"/></style:style>"##,
    r#"<style:page-layout style:name="pm2"><style:page-layout-properties fo:page-width="11in" fo:page-height="8.5in"/></style:page-layout>"#,
    r#"<style:page-layout style:name="pm1"><style:page-layout-properties fo:page-width="8.5in" fo:page-height="11in" fo:margin-top="1in" fo:margin-bottom="1in" fo:margin-left="1.25in" fo:margin-right="1.25in"/></style:page-layout>"#,
);

const MASTER: &str = r#"<style:master-page style:name="Standard" style:page-layout-name="pm1"><style:header><text:p>Running header</text:p></style:header><style:footer><text:p>Page <text:page-number text:select-page="current">7</text:page-number></text:p></style:footer></style:master-page><style:master-page style:name="Wide" style:page-layout-name="pm2"/>"#;

/// A 1x1 PNG.
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==";

fn flat_document(mimetype: &str, body: &str) -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document {NAMESPACES} office:version="1.3" office:mimetype="application/vnd.oasis.opendocument.{mimetype}"><office:meta><dc:title>Fixture</dc:title></office:meta><office:styles>{STYLES}</office:styles><office:automatic-styles>{AUTOMATIC}</office:automatic-styles><office:master-styles>{MASTER}</office:master-styles><office:body>{body}</office:body></office:document>"#
    )
    .into_bytes()
}

fn text_document(content: &str) -> Vec<u8> {
    flat_document("text", &format!("<office:text>{content}</office:text>"))
}

fn flatten<'a>(blocks: &'a [DocumentBlock], out: &mut Vec<&'a DocumentBlock>) {
    for block in blocks {
        out.push(block);
        flatten(&block.children, out);
    }
}

fn label(block: &DocumentBlock) -> Option<String> {
    block
        .details
        .to_json_value()
        .get("listLabel")
        .and_then(|label| label.as_str().map(str::to_owned))
}

fn paragraphs(model: &DocModel) -> Vec<&ParagraphNode> {
    model
        .nodes
        .iter()
        .filter_map(|node| match node {
            DocNode::Paragraph(paragraph) => Some(paragraph),
            DocNode::Table(_) => None,
        })
        .collect()
}

fn runs(paragraph: &ParagraphNode) -> Vec<&TextRunNode> {
    paragraph
        .children
        .iter()
        .filter_map(|child| match child {
            ParagraphChildNode::Text(run) => Some(run),
            _ => None,
        })
        .collect()
}

fn text(paragraph: &ParagraphNode) -> String {
    runs(paragraph).iter().map(|run| run.text.as_str()).collect()
}

fn images(paragraph: &ParagraphNode) -> Vec<&ImageRunNode> {
    paragraph
        .children
        .iter()
        .filter_map(|child| match child {
            ParagraphChildNode::Image(image) => Some(&**image),
            _ => None,
        })
        .collect()
}

fn first_table(model: &DocModel) -> &TableNode {
    model
        .nodes
        .iter()
        .find_map(|node| match node {
            DocNode::Table(table) => Some(table),
            DocNode::Paragraph(_) => None,
        })
        .expect("table node")
}

/// Every node of the model, so a test can assert something holds throughout.
fn every_node(model: &DocModel) -> Vec<&DocNode> {
    fn collect<'a>(nodes: &'a [DocNode], out: &mut Vec<&'a DocNode>) {
        for node in nodes {
            out.push(node);
            if let DocNode::Paragraph(paragraph) = node {
                for image in images(paragraph) {
                    collect(image.content_nodes.as_deref().unwrap_or_default(), out);
                }
            }
        }
    }
    let metadata = &model.metadata;
    let mut out = Vec::new();
    collect(&model.nodes, &mut out);
    for header in &metadata.header_sections {
        collect(&header.nodes, &mut out);
    }
    for footer in &metadata.footer_sections {
        collect(&footer.nodes, &mut out);
    }
    for note in metadata.footnotes.iter().chain(&metadata.endnotes).flatten() {
        collect(note.nodes.as_deref().unwrap_or_default(), &mut out);
    }
    for comment in metadata.comments.iter().flatten() {
        collect(comment.nodes.as_deref().unwrap_or_default(), &mut out);
    }
    out
}

#[test]
fn builds_the_model_directly_from_text_structure() {
    let bytes = text_document(concat!(
        r#"<text:h text:style-name="Heading_20_1" text:outline-level="1">Report</text:h>"#,
        r#"<text:p text:style-name="P1">Plain <text:span text:style-name="T1">loud</text:span><text:s text:c="2"/>and a <text:a xlink:href="https://example.com/a?b=1&amp;c=2">link</text:a>.</text:p>"#,
        r#"<text:list text:style-name="Bullets"><text:list-item><text:p>First</text:p><text:list><text:list-item><text:p>Nested</text:p></text:list-item></text:list></text:list-item></text:list>"#,
        r#"<text:list text:style-name="Numbers"><text:list-item><text:p>One</text:p></text:list-item><text:list-item><text:p>Two</text:p></text:list-item></text:list>"#,
        r#"<text:p>Noted<text:note text:note-class="footnote"><text:note-citation>1</text:note-citation><text:note-body><text:p>Footnote text</text:p></text:note-body></text:note></text:p>"#,
    ));
    assert!(is_odf_format(&bytes));

    let model = parse_odt(&bytes).expect("parse flat OpenDocument text");
    let body = paragraphs(&model);
    assert_eq!(body.len(), 7);

    let heading = body[0].style.as_ref().unwrap();
    assert_eq!(heading.heading_level, Some(HeadingLevel::One));
    assert_eq!(heading.style_id.as_deref(), Some("Heading1"));
    assert_eq!(heading.style_name.as_deref(), Some("Heading 1"));
    // Run formatting is resolved through the style chain and the defaults.
    let title = runs(body[0])[0].style.as_ref().unwrap();
    assert_eq!((title.bold, title.font_size_pt), (Some(true), Some(18.0)));

    assert_eq!(body[1].style.as_ref().unwrap().align, Some(ParagraphAlignment::Center));
    let loud = runs(body[1]).into_iter().find(|run| run.text == "loud").unwrap();
    let loud_style = loud.style.as_ref().unwrap();
    assert_eq!(loud_style.color.as_deref(), Some("#FF0000"));
    assert_eq!((loud_style.bold, loud_style.font_size_pt), (Some(true), Some(11.0)));
    let link = runs(body[1]).into_iter().find(|run| run.text == "link").unwrap();
    assert_eq!(link.link.as_deref(), Some("https://example.com/a?b=1&c=2"));
    assert_eq!(link.style.as_ref().unwrap().color.as_deref(), Some("#000080"));

    // Lists are numbering definitions the paragraphs point at.
    let numbering = model.metadata.numbering_definitions.as_ref().unwrap();
    let nested = body[3].style.as_ref().unwrap().numbering.as_ref().unwrap();
    assert_eq!(nested.ilvl, 1);
    let instance = numbering.instances.iter().find(|instance| instance.num_id == nested.num_id).unwrap();
    let bullets = numbering
        .abstracts
        .iter()
        .find(|definition| definition.abstract_num_id == instance.abstract_num_id)
        .unwrap();
    assert_eq!(bullets.levels[1].text.as_deref(), Some("◦"));
    assert_eq!(bullets.levels[1].format.as_deref(), Some("bullet"));

    // A note is a reference on a run and a story in the metadata.
    let reference = runs(body[6])
        .into_iter()
        .find_map(|run| run.note_reference.as_ref())
        .expect("note reference");
    assert_eq!(reference.kind, NoteReferenceKind::Footnote);
    let footnotes = model.metadata.footnotes.as_ref().unwrap();
    assert_eq!((footnotes[0].id, footnotes[0].text.as_str()), (reference.id, "Footnote text"));

    assert!(model
        .metadata
        .paragraph_styles
        .iter()
        .any(|style| style.id == "Heading1" && style.heading_level == Some(HeadingLevel::One)));
    assert_eq!(model.metadata.default_paragraph_style_id.as_deref(), Some("Normal"));

    let structured = extract_document_blocks(&model);
    // Top-level blocks in reading order; the header story is its own block.
    let body: Vec<(&BlockType, &str, Option<String>)> = structured
        .blocks
        .iter()
        .filter(|block| !matches!(block.r#type, BlockType::Header | BlockType::Footer))
        .map(|block| (&block.r#type, block.content.as_str(), label(block)))
        .collect();
    assert_eq!(
        body,
        vec![
            (&BlockType::Heading, "Report", None),
            (&BlockType::Text, "Plain loud  and a link.", None),
            (&BlockType::Text, "First", Some("•".into())),
            (&BlockType::Text, "Nested", Some("◦".into())),
            (&BlockType::Text, "One", Some("1.".into())),
            (&BlockType::Text, "Two", Some("2.".into())),
            (&BlockType::Text, "Noted", None),
            (&BlockType::Text, "Footnote text", None),
        ]
    );
    assert!(structured
        .blocks
        .iter()
        .any(|block| block.r#type == BlockType::Header && block.content == "Running header"));
    // Every entry point that takes bytes reaches the same model.
    assert_eq!(parse_document_model(&bytes).unwrap(), model);
    assert_eq!(parse_document_blocks(&bytes).unwrap(), structured);
    let bundle = parse_document_bundle(&bytes, Default::default()).unwrap();
    assert_eq!(bundle.document, structured);
}

#[test]
fn carries_no_word_markup() {
    let bytes = text_document(&format!(
        concat!(
            r#"<text:h text:style-name="Heading_20_1" text:outline-level="1">Title</text:h>"#,
            r#"<text:p>Body<text:note text:note-class="endnote"><text:note-citation>i</text:note-citation><text:note-body><text:p>End</text:p></text:note-body></text:note></text:p>"#,
            r#"<table:table><table:table-column/><table:table-row><table:table-cell><text:p>Cell</text:p></table:table-cell></table:table-row></table:table>"#,
            r#"<text:p><draw:frame text:anchor-type="paragraph" svg:width="2in" svg:height="1in"><draw:text-box><text:p>Boxed</text:p></draw:text-box></draw:frame></text:p>"#,
            r#"<text:p><draw:frame text:anchor-type="as-char" svg:width="1in" svg:height="1in"><draw:image><office:binary-data>{}</office:binary-data></draw:image></draw:frame></text:p>"#,
        ),
        PNG
    ));
    let model = parse_odt(&bytes).unwrap();
    let nodes = every_node(&model);
    assert!(nodes.len() >= 9, "{}", nodes.len());
    for node in nodes {
        match node {
            DocNode::Paragraph(paragraph) => {
                assert!(paragraph.source_xml.is_none());
                assert!(paragraph.source_paragraph_mark_properties_xml.is_none());
                assert!(images(paragraph).iter().all(|image| image.source_xml.is_none()));
            }
            DocNode::Table(table) => assert!(table.source_xml.is_none()),
        }
    }
    assert!(model.metadata.document_open_tag.is_none());
    // An OpenDocument file is never turned into a Word package.
    let error = parse_document_bytes(&bytes).expect_err("no Word package for OpenDocument text");
    assert!(error.contains("parse_document_model"), "{error}");
    // Sources are named after the OpenDocument parts they come from.
    let structured = extract_document_blocks(&model);
    let mut blocks = Vec::new();
    flatten(&structured.blocks, &mut blocks);
    assert!(blocks.iter().all(|block| block
        .source_refs
        .iter()
        .all(|source| source.part_name == "content.xml" || source.part_name.starts_with("styles.xml/"))));
    assert!(blocks.iter().any(|block| block.content == "End" && block.source_refs[0].story == "endnote"));
    assert!(blocks
        .iter()
        .any(|block| block.r#type == BlockType::Figure && block.content == "Boxed"));
}

#[test]
fn imports_tables_with_merged_cells() {
    let bytes = text_document(concat!(
        r#"<table:table table:name="T"><table:table-column table:number-columns-repeated="3"/>"#,
        r#"<table:table-header-rows><table:table-row><table:table-cell><text:p>A</text:p></table:table-cell><table:table-cell><text:p>B</text:p></table:table-cell><table:table-cell><text:p>C</text:p></table:table-cell></table:table-row></table:table-header-rows>"#,
        r#"<table:table-row><table:table-cell table:number-columns-spanned="2"><text:p>Wide</text:p></table:table-cell><table:covered-table-cell/><table:table-cell table:number-rows-spanned="2"><text:p>Tall</text:p></table:table-cell></table:table-row>"#,
        r#"<table:table-row><table:table-cell><text:p>Left</text:p></table:table-cell><table:table-cell><text:p>Middle</text:p></table:table-cell><table:covered-table-cell/></table:table-row>"#,
        r#"</table:table><text:p>After</text:p>"#,
    ));
    let model = parse_odt(&bytes).unwrap();
    let table = first_table(&model);
    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.rows[0].style.as_ref().unwrap().is_header, Some(true));
    let wide = table.rows[1].cells[0].style.as_ref().unwrap();
    assert_eq!(wide.grid_span, Some(2));
    // The merged cell counts the rows it covers; the cell below continues it.
    let tall = table.rows[1].cells[1].style.as_ref().unwrap();
    assert_eq!(tall.row_span, Some(2));
    let continued = table.rows[2].cells[2].style.as_ref().unwrap();
    assert_eq!(continued.v_merge_continuation, Some(true));
    assert!(matches!(
        &table.rows[2].cells[2].nodes[..],
        [TableCellContentNode::Paragraph(paragraph)] if text(paragraph).is_empty()
    ));

    let structured = extract_document_blocks(&model);
    let table = structured
        .blocks
        .iter()
        .find(|block| block.r#type == BlockType::Table)
        .expect("table block");
    assert_eq!(table.content, "A\tB\tC\nWide\tTall\nLeft\tMiddle\t");
    let mut descendants = Vec::new();
    flatten(&table.children, &mut descendants);
    let cells: Vec<&str> = descendants
        .iter()
        .filter(|block| block.r#type == BlockType::TableCell)
        .map(|cell| cell.content.as_str())
        .collect();
    // The empty cell that continues the merge is not a block.
    assert_eq!(cells, ["A", "B", "C", "Wide", "Tall", "Left", "Middle"]);
    // The repeated header row is recognised as the table head.
    assert!(descendants
        .iter()
        .any(|block| block.r#type == BlockType::TableHead && block.content == "A\tB\tC"));
}

#[test]
fn shows_a_repeated_text_table_row_once() {
    // LibreOffice Writer ignores the spreadsheet repeat count on a text table's row.
    let bytes = text_document(concat!(
        r#"<table:table><table:table-column table:number-columns-repeated="2"/>"#,
        r#"<table:table-row table:number-rows-repeated="500"><table:table-cell><text:p>Only</text:p></table:table-cell><table:table-cell><text:p>once</text:p></table:table-cell></table:table-row>"#,
        r#"</table:table>"#,
    ));
    let model = parse_odt(&bytes).unwrap();
    assert_eq!(first_table(&model).rows.len(), 1);
    let structured = extract_document_blocks(&model);
    let table = structured.blocks.iter().find(|block| block.r#type == BlockType::Table).unwrap();
    assert_eq!(table.content, "Only\tonce");
}

#[test]
fn imports_inline_images_page_setup_and_headers() {
    let bytes = text_document(&format!(
        r#"<text:p><draw:frame draw:name="Dot" text:anchor-type="as-char" svg:width="1in" svg:height="0.5in"><draw:image><office:binary-data>{PNG}</office:binary-data></draw:image><svg:desc>A dot</svg:desc></draw:frame></text:p>"#
    ));
    let model = parse_odt(&bytes).unwrap();
    let body = paragraphs(&model);
    let image = images(body[0])[0];
    assert_eq!((image.width_px, image.height_px), (Some(96.0), Some(48.0)));
    assert_eq!(image.content_type.as_deref(), Some("image/png"));
    assert!(image.data.as_ref().is_some_and(|data| data.starts_with(b"\x89PNG")));
    assert!(image.floating.is_none());

    // Page setup is the model's section properties; headers are stories beside it.
    let sections = model.metadata.sections.as_ref().unwrap();
    assert_eq!(sections.len(), 1);
    let setup = model.metadata.section_properties_xml.as_deref().unwrap();
    assert!(setup.contains(r#"<w:pgSz w:w="12240" w:h="15840"/>"#), "{setup}");
    assert!(setup.contains(r#"w:left="1800""#), "{setup}");
    let header = &model.metadata.header_sections[0];
    assert_eq!(header.reference_type.as_deref(), Some("default"));
    assert_eq!(header.part_name, "styles.xml/header1");
    // The page number stays a field, so it is classified as one.
    let DocNode::Paragraph(footer) = &model.metadata.footer_sections[0].nodes[0] else {
        panic!("footer paragraph");
    };
    let field = runs(footer).into_iter().find(|run| run.field_instruction.is_some()).unwrap();
    assert_eq!((field.text.as_str(), field.field_instruction.as_deref()), ("7", Some("PAGE")));

    let structured = extract_document_blocks(&model);
    let mut blocks = Vec::new();
    flatten(&structured.blocks, &mut blocks);
    let figure = blocks
        .iter()
        .find(|block| block.r#type == BlockType::Figure)
        .expect("figure block");
    assert_eq!(figure.content, "A dot");
    assert!(figure.details.to_json_value()["imageUrl"]
        .as_str()
        .is_some_and(|url| url.starts_with("data:image/png;base64,iVBOR")));
    assert!(blocks
        .iter()
        .any(|block| block.r#type == BlockType::Header && block.content == "Running header"));
    assert!(blocks
        .iter()
        .any(|block| block.r#type == BlockType::PageNumber && block.content == "Page 7"));
}

#[test]
fn an_image_used_twice_keeps_its_bytes_in_both_places() {
    let frame = format!(
        r#"<draw:frame text:anchor-type="as-char" svg:width="1in" svg:height="1in"><draw:image><office:binary-data>{PNG}</office:binary-data></draw:image></draw:frame>"#
    );
    // Identical inline data is one image in the file.
    let model = parse_odt(&text_document(&format!("<text:p>{frame}</text:p><text:p>{frame}</text:p>"))).unwrap();
    let body = paragraphs(&model);
    let (first, second) = (images(body[0])[0], images(body[1])[0]);
    assert!(first.data.as_ref().is_some_and(|data| !data.is_empty()));
    assert_eq!(first.data, second.data);
}

#[test]
fn page_style_switches_split_the_body_into_sections() {
    let bytes = text_document(concat!(
        r#"<text:p>Portrait</text:p>"#,
        r#"<text:p text:style-name="P2">Last on its page</text:p>"#,
        r#"<text:p>Next page</text:p>"#,
        r#"<text:p text:style-name="P3">Landscape</text:p>"#,
    ));
    let model = parse_odt(&bytes).unwrap();
    let body = paragraphs(&model);
    assert_eq!(body.len(), 4);
    // A break after one paragraph is a break before the next.
    assert_eq!(body[1].style.as_ref().unwrap().page_break_before, None);
    assert_eq!(body[2].style.as_ref().unwrap().page_break_before, Some(true));
    assert_eq!(body[3].style.as_ref().unwrap().page_break_before, None);

    let sections = model.metadata.sections.as_ref().unwrap();
    let starts: Vec<i64> = sections.iter().map(|section| section.start_node_index).collect();
    assert_eq!(starts, [0, 3]);
    assert_eq!(sections[0].header_sections.len(), 1);
    assert!(sections[1].header_sections.is_empty());
    let wide = sections[1].section_properties_xml.as_deref().unwrap();
    assert!(wide.contains(r#"w:w="15840" w:h="12240" w:orient="landscape""#), "{wide}");
    assert_eq!(model.metadata.section_properties_xml.as_deref(), Some(wide));
    // The header of the first section is still extracted once.
    let structured = extract_document_blocks(&model);
    assert_eq!(
        structured.blocks.iter().filter(|block| block.r#type == BlockType::Header).count(),
        1
    );
}

#[test]
fn hidden_text_is_left_out_and_relative_sizes_resolve() {
    let bytes = text_document(
        r#"<text:p>Shown<text:span text:style-name="T2"> hidden</text:span> <text:span text:style-name="T3">small</text:span></text:p>"#,
    );
    let model = parse_odt(&bytes).unwrap();
    let body = paragraphs(&model);
    assert_eq!(text(body[0]), "Shown small");
    let small = runs(body[0]).into_iter().find(|run| run.text == "small").unwrap();
    assert_eq!(small.style.as_ref().unwrap().font_size_pt, Some(5.5));
}

#[test]
fn comments_keep_the_text_they_are_anchored_to() {
    let bytes = text_document(concat!(
        r#"<text:p>Before <office:annotation office:name="c1"><dc:creator>Ada</dc:creator><dc:date>2026-01-02T03:04:05</dc:date><text:p>Check this</text:p></office:annotation>marked text<office:annotation-end office:name="c1"/> after</text:p>"#,
        r#"<text:p>Point<office:annotation><dc:creator>Bo</dc:creator><text:p>Here</text:p></office:annotation> comment</text:p>"#,
    ));
    let model = parse_odt(&bytes).unwrap();
    let comments = model.metadata.comments.as_ref().unwrap();
    assert_eq!(comments.len(), 2);
    assert_eq!((comments[0].author.as_deref(), comments[0].text.as_str()), (Some("Ada"), "Check this"));

    let review = extract_document_review(&model);
    let ranged = &review.comments[0];
    assert_eq!(ranged.targets.len(), 1);
    let target = &ranged.targets[0];
    let body = paragraphs(&model);
    assert_eq!(Some(target.source.node_id.as_str()), body[0].block_id.as_deref());
    // "Before " is seven UTF-16 units and "marked text" eleven.
    assert_eq!((target.source.start, target.source.end), (Some(7), Some(18)));
    assert_eq!(ranged.reference.as_ref().unwrap().start, Some(18));
    // A comment without a range marks a point.
    let point = &review.comments[1];
    assert_eq!(point.targets.len(), 1);
    assert_eq!((point.targets[0].source.start, point.targets[0].source.end), (Some(5), Some(5)));
    let reference = point.reference.as_ref().unwrap();
    assert_eq!(Some(reference.node_id.as_str()), body[1].block_id.as_deref());
    assert_eq!(reference.start, Some(5));
    // The comment bodies are stories of their own.
    let structured = extract_document_blocks(&model);
    assert!(structured
        .blocks
        .iter()
        .any(|block| block.content == "Check this" && block.source_refs[0].story == "comment"));
}

#[test]
fn formulas_keep_their_mathml() {
    let bytes = text_document(concat!(
        r#"<text:p>Area <draw:frame text:anchor-type="as-char" svg:width="1in" svg:height="0.25in"><draw:object>"#,
        r#"<math:math xmlns:math="http://www.w3.org/1998/Math/MathML"><math:mi>r</math:mi></math:math>"#,
        r#"</draw:object></draw:frame></text:p>"#,
    ));
    let model = parse_odt(&bytes).unwrap();
    let body = paragraphs(&model);
    let formula = images(body[0])[0];
    assert_eq!(formula.semantic_kind.as_deref(), Some("formula"));
    assert!(formula.math_ml.as_deref().is_some_and(|math| math.contains("<math:mi>r</math:mi>")));
    assert_eq!((formula.width_px, formula.height_px), (Some(96.0), Some(24.0)));
    let structured = extract_document_blocks(&model);
    let mut blocks = Vec::new();
    flatten(&structured.blocks, &mut blocks);
    assert!(blocks.iter().any(|block| block.r#type == BlockType::Formula));
}

#[test]
fn the_same_bytes_give_the_same_identities() {
    let bytes = text_document(r#"<text:p>Stable</text:p><text:p>Ids</text:p>"#);
    let (first, second) = (parse_odt(&bytes).unwrap(), parse_odt(&bytes).unwrap());
    assert_eq!(first, second);
    let id = first.metadata.document_id.as_deref().unwrap();
    assert_eq!(id.len(), 64);
    let body = paragraphs(&first);
    assert_eq!(body[1].block_id.as_deref(), Some(format!("docx:{id}/content.xml/body/1").as_str()));
    let other = parse_odt(&text_document(r#"<text:p>Changed</text:p>"#)).unwrap();
    assert_ne!(other.metadata.document_id, first.metadata.document_id);
}

#[test]
fn rejects_other_opendocument_types_and_leaves_docx_alone() {
    let presentation = flat_document("presentation", "<office:presentation/>");
    assert!(is_odf_format(&presentation));
    for error in [
        parse_document_bytes(&presentation).expect_err("presentations are not Word documents"),
        parse_odt(&presentation).expect_err("presentations are not text documents"),
        parse_document_blocks(&presentation).map(|_| ()).expect_err("no blocks for a presentation"),
    ] {
        assert!(error.contains("not text documents"), "{error}");
    }

    let docx = package_to_bytes(&rust_docx::create_minimal_docx_package(None)).unwrap();
    assert!(!is_odf_format(&docx));
    assert!(parse_document_bytes(&docx).is_ok());
    assert!(parse_document_model(&docx).is_ok());
    assert!(parse_odt(&docx).is_err());
}

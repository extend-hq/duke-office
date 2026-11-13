use duke_slides::{
    detect_format, extract_presentation_blocks, parse_presentation, ExtractPresentationBlocksOptions,
    FillStyle, ParseError, ParseLimits, PresentationFormat, SlideNode,
};

const NAMESPACES: &str = concat!(
    r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" "#,
    r#"xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" "#,
    r#"xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" "#,
    r#"xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" "#,
    r#"xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" "#,
    r#"xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0" "#,
    r#"xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" "#,
    r#"xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" "#,
    r#"xmlns:xlink="http://www.w3.org/1999/xlink" "#,
    r#"xmlns:dc="http://purl.org/dc/elements/1.1/""#,
);

const STYLES: &str = concat!(
    r#"<style:default-style style:family="graphic"><style:text-properties fo:font-size="18pt" style:font-name="Body"/></style:default-style>"#,
    r##"<style:style style:name="Default-title" style:family="presentation"><style:graphic-properties draw:fill="none" draw:stroke="none" draw:textarea-vertical-align="middle"/><style:paragraph-properties fo:text-align="center"/><style:text-properties fo:font-size="44pt" fo:color="#112233"/></style:style>"##,
    r##"<style:style style:name="Default-outline1" style:family="presentation"><style:graphic-properties draw:fill="none" draw:stroke="none"><text:list-style style:name="Default-outline1"><text:list-level-style-bullet text:level="1" text:bullet-char="●"/><text:list-level-style-bullet text:level="2" text:bullet-char="–"/></text:list-style></style:graphic-properties><style:text-properties fo:font-size="32pt"/></style:style>"##,
    r#"<style:style style:name="Default-outline2" style:family="presentation" style:parent-style-name="Default-outline1"><style:text-properties fo:font-size="28pt"/></style:style>"#,
);

const AUTOMATIC: &str = concat!(
    r#"<style:page-layout style:name="PM1"><style:page-layout-properties fo:page-width="10in" fo:page-height="5.625in"/></style:page-layout>"#,
    r##"<style:style style:name="dp1" style:family="drawing-page"><style:drawing-page-properties draw:fill="solid" draw:fill-color="#fafafa"/></style:style>"##,
    r#"<style:style style:name="dp2" style:family="drawing-page"><style:drawing-page-properties presentation:visibility="hidden"/></style:style>"#,
    r##"<style:style style:name="gr1" style:family="graphic"><style:graphic-properties draw:fill="solid" draw:fill-color="#336699" draw:stroke="solid" svg:stroke-color="#000000" svg:stroke-width="0.0208in"/></style:style>"##,
    r#"<style:style style:name="pr1" style:family="presentation" style:parent-style-name="Default-title"/>"#,
    r#"<style:style style:name="pr2" style:family="presentation" style:parent-style-name="Default-outline1"/>"#,
    r#"<style:style style:name="T1" style:family="text"><style:text-properties fo:font-weight="bold"/></style:style>"#,
    r##"<style:style style:name="ce1" style:family="table-cell"><style:table-cell-properties fo:background-color="#eeeeee"/></style:style>"##,
);

const MASTER: &str = concat!(
    r#"<style:master-page style:name="Default" style:page-layout-name="PM1" draw:style-name="dp1">"#,
    r#"<draw:rect draw:style-name="gr1" svg:x="0in" svg:y="5in" svg:width="10in" svg:height="0.625in"/>"#,
    r#"<draw:frame presentation:class="title" presentation:style-name="pr1" svg:x="1in" svg:y="1in" svg:width="8in" svg:height="1in"><draw:text-box><text:p>Click to edit the title</text:p></draw:text-box></draw:frame>"#,
    r#"</style:master-page>"#,
);

/// A 1x1 PNG.
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==";

fn flat_document(mimetype: &str, body: &str) -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document {NAMESPACES} office:version="1.3" office:mimetype="application/vnd.oasis.opendocument.{mimetype}"><office:meta><dc:title>Deck</dc:title></office:meta><office:styles>{STYLES}</office:styles><office:automatic-styles>{AUTOMATIC}</office:automatic-styles><office:master-styles>{MASTER}</office:master-styles><office:body>{body}</office:body></office:document>"#
    )
    .into_bytes()
}

fn deck() -> Vec<u8> {
    let slide_one = format!(
        concat!(
            r#"<draw:page draw:name="Intro" draw:master-page-name="Default">"#,
            r#"<draw:frame draw:id="title1" presentation:class="title" presentation:style-name="pr1" svg:x="1in" svg:y="0.5in" svg:width="8in" svg:height="1in"><draw:text-box><text:p>Quarterly <text:span text:style-name="T1">review</text:span></text:p></draw:text-box></draw:frame>"#,
            r#"<draw:frame presentation:class="outline" presentation:style-name="pr2" svg:x="1in" svg:y="2in" svg:width="8in" svg:height="2in"><draw:text-box><text:list><text:list-item><text:p>Revenue</text:p><text:list><text:list-item><text:p>Up 12%</text:p></text:list-item></text:list></text:list-item></text:list></draw:text-box></draw:frame>"#,
            r#"<draw:custom-shape draw:style-name="gr1" svg:x="1in" svg:y="4in" svg:width="2in" svg:height="0.5in"><text:p>Callout</text:p><draw:enhanced-geometry draw:type="ooxml-roundRect" svg:viewBox="0 0 21600 21600"/></draw:custom-shape>"#,
            r#"<draw:frame draw:name="Dot" svg:x="4in" svg:y="4in" svg:width="1in" svg:height="0.5in"><draw:image><office:binary-data>{png}</office:binary-data></draw:image><svg:desc>A dot</svg:desc></draw:frame>"#,
            r#"<draw:frame presentation:class="subtitle" presentation:placeholder="true" svg:x="0in" svg:y="0in" svg:width="1in" svg:height="1in"><draw:text-box/></draw:frame>"#,
            r#"<presentation:notes><draw:frame presentation:class="notes" svg:x="0in" svg:y="0in" svg:width="1in" svg:height="1in"><draw:text-box><text:p>Speak slowly</text:p></draw:text-box></draw:frame></presentation:notes>"#,
            r#"</draw:page>"#,
        ),
        png = PNG
    );
    let slide_two = concat!(
        r#"<draw:page draw:name="Data" draw:style-name="dp2" draw:master-page-name="Default">"#,
        r#"<draw:frame svg:x="1in" svg:y="1in" svg:width="4in" svg:height="1in"><table:table><table:table-column table:number-columns-repeated="2"/>"#,
        r#"<table:table-row><table:table-cell table:style-name="ce1" table:number-columns-spanned="2"><text:p>Header</text:p></table:table-cell><table:covered-table-cell/></table:table-row>"#,
        r#"<table:table-row><table:table-cell><text:p>Left</text:p></table:table-cell><table:table-cell><text:p>Right</text:p></table:table-cell></table:table-row>"#,
        r#"</table:table></draw:frame>"#,
        r#"<draw:line svg:x1="3in" svg:y1="4in" svg:x2="1in" svg:y2="4.5in"/>"#,
        r#"</draw:page>"#,
    );
    flat_document(
        "presentation",
        &format!("<office:presentation>{slide_one}{slide_two}</office:presentation>"),
    )
}

#[test]
fn detects_presentations_and_drawings_but_not_other_opendocument_types() {
    assert_eq!(detect_format(&deck()).unwrap(), PresentationFormat::Odp);
    let drawing = flat_document("graphics", "<office:drawing/>");
    assert_eq!(detect_format(&drawing).unwrap(), PresentationFormat::Odg);
    let text = flat_document("text", "<office:text/>");
    assert!(matches!(detect_format(&text), Err(ParseError::UnsupportedFormat)));
    assert_eq!(
        detect_format(b"PK\x03\x04rest").unwrap(),
        PresentationFormat::Pptx
    );
}

#[test]
fn builds_slides_with_inherited_master_shapes() {
    let document = parse_presentation(&deck(), &ParseLimits::default()).expect("parse flat presentation");
    assert_eq!(document.format, PresentationFormat::Odp);
    assert_eq!((document.size.width_emu, document.size.height_emu), (9_144_000, 5_143_500));
    assert_eq!(document.metadata.as_ref().and_then(|metadata| metadata.title.as_deref()), Some("Deck"));
    assert_eq!(document.slides.len(), 2);
    assert_eq!(document.masters.len(), 1);
    // The master's placeholder prompt is not a slide object; its decoration is.
    assert_eq!(document.masters[0].nodes.len(), 1);

    let slide = &document.slides[0];
    assert_eq!(slide.name.as_deref(), Some("Intro"));
    assert_eq!(slide.hidden, None);
    assert!(matches!(&slide.background, Some(FillStyle::Solid { color }) if color.value == "#FAFAFA"));
    assert_eq!(slide.notes.len(), 1);
    assert_eq!(slide.notes[0].text, "Speak slowly");
    assert_eq!(document.slides[1].hidden, Some(true));

    // Master decoration, title, outline, callout and image; the empty placeholder is skipped.
    assert_eq!(slide.nodes.len(), 5);
    let SlideNode::Shape { id, fill, line, transform, .. } = &slide.nodes[0] else {
        panic!("inherited master shape");
    };
    assert!(id.starts_with("master:"), "{id}");
    assert!(matches!(fill, Some(FillStyle::Solid { color }) if color.value == "#336699"));
    assert!(line.as_ref().is_some_and(|line| line.width.is_some_and(|width| (width - 2.0).abs() < 0.01)));
    assert_eq!((transform.y, transform.height), (4_572_000, 571_500));
}

#[test]
fn resolves_text_formatting_through_the_style_chain() {
    let document = parse_presentation(&deck(), &ParseLimits::default()).unwrap();
    let slide = &document.slides[0];

    let SlideNode::Shape { id, paragraphs, fill, vertical_alignment, .. } = &slide.nodes[1] else {
        panic!("title shape");
    };
    assert_eq!(id, "title1");
    assert!(matches!(fill, Some(FillStyle::None)));
    assert!(vertical_alignment.is_some());
    assert_eq!(paragraphs[0].alignment.as_deref(), Some("center"));
    let runs = &paragraphs[0].runs;
    assert_eq!(runs[0].text, "Quarterly ");
    assert_eq!((runs[0].font_size_pt, runs[0].bold), (Some(44.0), None));
    assert_eq!(runs[0].color.as_ref().map(|color| color.value.as_str()), Some("#112233"));
    assert_eq!(runs[0].font_family.as_deref(), Some("Body"));
    assert_eq!((runs[1].text.as_str(), runs[1].bold), ("review", Some(true)));

    let SlideNode::Shape { paragraphs, .. } = &slide.nodes[2] else {
        panic!("outline shape");
    };
    let summary: Vec<_> = paragraphs
        .iter()
        .map(|paragraph| {
            (
                paragraph.runs[0].text.as_str(),
                paragraph.level,
                paragraph.bullet.as_ref().and_then(|bullet| bullet.value.as_deref()),
                paragraph.runs[0].font_size_pt,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("Revenue", Some(0), Some("●"), Some(32.0)),
            ("Up 12%", Some(1), Some("–"), Some(28.0)),
        ]
    );

    let SlideNode::Shape { geometry, paragraphs, .. } = &slide.nodes[3] else {
        panic!("custom shape");
    };
    assert_eq!(geometry.preset.as_deref(), Some("roundRect"));
    assert_eq!(paragraphs[0].runs[0].text, "Callout");
    assert_eq!(paragraphs[0].runs[0].font_size_pt, Some(18.0));
}

#[test]
fn imports_images_tables_and_lines() {
    let document = parse_presentation(&deck(), &ParseLimits::default()).unwrap();
    let SlideNode::Image { asset_id, metadata, transform, .. } = &document.slides[0].nodes[4] else {
        panic!("image node");
    };
    let asset = &document.assets[asset_id];
    assert_eq!(asset.content_type, "image/png");
    assert_eq!(asset.data.as_ref().map(|data| data.len()), Some(asset.byte_length));
    assert_eq!(metadata.as_ref().and_then(|metadata| metadata.alt_text.as_deref()), Some("A dot"));
    assert_eq!((transform.x, transform.width), (3_657_600, 914_400));

    let nodes = &document.slides[1].nodes;
    let table = nodes
        .iter()
        .find_map(|node| match node {
            SlideNode::Table { rows, column_widths, row_heights, .. } => Some((rows, column_widths, row_heights)),
            _ => None,
        })
        .expect("table node");
    let (rows, column_widths, row_heights) = table;
    assert_eq!(column_widths, &[1_828_800, 1_828_800]);
    assert_eq!(row_heights, &[457_200, 457_200]);
    assert_eq!(rows[0][0].col_span, Some(2));
    assert!(matches!(&rows[0][0].fill, Some(FillStyle::Solid { color }) if color.value == "#EEEEEE"));
    let merge = rows[0][1].merge.as_ref().expect("covered cell");
    assert!(merge.horizontal && !merge.vertical);
    assert_eq!(merge.origin.as_ref().map(|origin| (origin.row, origin.column)), Some((0, 0)));
    assert_eq!(rows[1][1].paragraphs[0].runs[0].text, "Right");

    let line = nodes
        .iter()
        .find_map(|node| match node {
            SlideNode::Shape { geometry, transform, .. } if geometry.preset.as_deref() == Some("line") => Some(transform),
            _ => None,
        })
        .expect("line shape");
    assert_eq!((line.x, line.y, line.width, line.height), (914_400, 3_657_600, 1_828_800, 457_200));
    assert_eq!((line.flip_horizontal, line.flip_vertical), (Some(true), None));
}

#[test]
fn extracts_the_same_blocks_as_other_formats() {
    let document = parse_presentation(&deck(), &ParseLimits::default()).unwrap();
    let structured =
        extract_presentation_blocks(&document, &ExtractPresentationBlocksOptions::default()).unwrap();
    fn texts(blocks: &[duke_slides::PresentationBlock], out: &mut Vec<String>) {
        for block in blocks {
            if block.r#type == "text" {
                out.push(block.content.clone());
            }
            texts(&block.children, out);
        }
    }
    let mut found = Vec::new();
    texts(&structured.blocks, &mut found);
    for expected in ["Quarterly review", "Revenue", "Up 12%", "Callout", "Header", "Left", "Right"] {
        assert!(found.iter().any(|text| text == expected), "{expected} missing from {found:?}");
    }
    assert!(!found.iter().any(|text| text.contains("Click to edit")));
}

#[test]
fn fills_master_footer_and_slide_number_fields_on_slides_that_show_them() {
    let styles = concat!(
        r#"<style:style style:name="shown" style:family="drawing-page"><style:drawing-page-properties presentation:display-footer="true" presentation:display-page-number="true" presentation:display-date-time="true"/></style:style>"#,
        r#"<style:style style:name="plain" style:family="drawing-page"><style:drawing-page-properties presentation:display-footer="false" presentation:display-page-number="false"/></style:style>"#,
    );
    let master = concat!(
        r#"<style:master-page style:name="M">"#,
        r#"<draw:frame presentation:class="footer" svg:x="1in" svg:y="5in" svg:width="4in" svg:height="0.3in"><draw:text-box><text:p><presentation:footer/></text:p></draw:text-box></draw:frame>"#,
        r#"<draw:frame presentation:class="page-number" svg:x="8in" svg:y="5in" svg:width="1in" svg:height="0.3in"><draw:text-box><text:p><text:page-number>&lt;number&gt;</text:page-number></text:p></draw:text-box></draw:frame>"#,
        r#"<draw:frame presentation:class="date-time" svg:x="6in" svg:y="5in" svg:width="1in" svg:height="0.3in"><draw:text-box><text:p><presentation:date-time/></text:p></draw:text-box></draw:frame>"#,
        r#"</style:master-page>"#,
    );
    let body = concat!(
        r#"<office:presentation><presentation:footer-decl presentation:name="ftr1">Confidential</presentation:footer-decl>"#,
        r#"<presentation:date-time-decl presentation:name="dtd1" presentation:source="current-date"/>"#,
        r#"<draw:page draw:style-name="plain" draw:master-page-name="M" presentation:use-footer-name="ftr1"/>"#,
        r#"<draw:page draw:style-name="shown" draw:master-page-name="M" presentation:use-footer-name="ftr1" presentation:use-date-time-name="dtd1"/>"#,
        r#"</office:presentation>"#,
    );
    let bytes = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document {NAMESPACES} office:mimetype="application/vnd.oasis.opendocument.presentation"><office:automatic-styles>{styles}</office:automatic-styles><office:master-styles>{master}</office:master-styles><office:body>{body}</office:body></office:document>"#
    )
    .into_bytes();
    let document = parse_presentation(&bytes, &ParseLimits::default()).unwrap();
    let texts = |index: usize| -> Vec<(String, String)> {
        document.slides[index]
            .nodes
            .iter()
            .filter_map(|node| match node {
                SlideNode::Shape { id, paragraphs, .. } => Some((
                    id.clone(),
                    paragraphs.iter().flat_map(|paragraph| &paragraph.runs).map(|run| run.text.as_str()).collect(),
                )),
                _ => None,
            })
            .collect()
    };
    // The first slide turns the fields off; the master's placeholders are not content.
    assert!(texts(0).is_empty());
    // The second shows the footer and its own number. Its date follows the
    // current day, which the file does not store, so that field stays out.
    let shown = texts(1);
    assert_eq!(shown.iter().map(|(_, text)| text.as_str()).collect::<Vec<_>>(), ["Confidential", "2"]);
    assert!(shown.iter().all(|(id, _)| id.starts_with("master:")));
}

#[test]
fn keeps_text_typed_onto_a_picture() {
    let body = format!(
        r#"<office:presentation><draw:page draw:master-page-name="Default"><draw:frame draw:id="pic" svg:x="1in" svg:y="1in" svg:width="2in" svg:height="1in"><draw:image><office:binary-data>{PNG}</office:binary-data><text:p>A caption</text:p></draw:image></draw:frame></draw:page></office:presentation>"#
    );
    let document = parse_presentation(&flat_document("presentation", &body), &ParseLimits::default()).unwrap();
    let group = document.slides[0]
        .nodes
        .iter()
        .find_map(|node| match node {
            SlideNode::Group { id, children, .. } if id == "pic" => Some(children),
            _ => None,
        })
        .expect("picture with its text");
    assert!(matches!(&group[0], SlideNode::Image { id, .. } if id == "pic:picture"));
    let SlideNode::Shape { paragraphs, transform, .. } = &group[1] else {
        panic!("caption shape");
    };
    assert_eq!(paragraphs[0].runs[0].text, "A caption");
    assert_eq!((transform.x, transform.width), (914_400, 1_828_800));
}

fn package(files: &[(&str, &str)]) -> Vec<u8> {
    use std::io::Write;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer.start_file("mimetype", options).unwrap();
    writer.write_all(b"application/vnd.oasis.opendocument.presentation").unwrap();
    for (name, content) in files {
        writer.start_file(*name, options).unwrap();
        writer.write_all(content.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn reads_embedded_charts_and_documents_that_have_no_drawable_preview() {
    let chart_namespace = r#"xmlns:chart="urn:oasis:names:tc:opendocument:xmlns:chart:1.0""#;
    let content = format!(
        concat!(
            r#"<office:document-content {ns}><office:body><office:presentation><draw:page draw:name="p1">"#,
            r#"<draw:frame draw:id="c1" svg:x="1in" svg:y="1in" svg:width="4in" svg:height="3in"><draw:object xlink:href="./Object 1"/></draw:frame>"#,
            r#"<draw:frame draw:id="t1" svg:x="5in" svg:y="1in" svg:width="4in" svg:height="3in"><draw:object xlink:href="./Object 2"/></draw:frame>"#,
            r#"<draw:frame draw:id="s1" svg:x="1in" svg:y="4in" svg:width="4in" svg:height="1in"><draw:object xlink:href="./Object 3"/></draw:frame>"#,
            r#"</draw:page></office:presentation></office:body></office:document-content>"#,
        ),
        ns = NAMESPACES
    );
    let chart = format!(
        concat!(
            r#"<office:document-content {ns} {chart}><office:body><office:chart><chart:chart chart:class="chart:bar"><chart:title><text:p>Sales</text:p></chart:title>"#,
            r#"<chart:plot-area><chart:series chart:values-cell-range-address="local-table.$B$2:.$B$3" chart:label-cell-address="local-table.$B$1"/></chart:plot-area>"#,
            r#"<table:table table:name="local-table"><table:table-header-rows><table:table-row><table:table-cell><text:p/></table:table-cell><table:table-cell><text:p>Units</text:p></table:table-cell></table:table-row></table:table-header-rows>"#,
            r#"<table:table-rows><table:table-row><table:table-cell><text:p>Q1</text:p></table:table-cell><table:table-cell office:value="3"><text:p>3</text:p></table:table-cell></table:table-row>"#,
            r#"<table:table-row><table:table-cell><text:p>Q2</text:p></table:table-cell><table:table-cell office:value="5"><text:p>5</text:p></table:table-cell></table:table-row></table:table-rows></table:table>"#,
            r#"</chart:chart></office:chart></office:body></office:document-content>"#,
        ),
        ns = NAMESPACES,
        chart = chart_namespace
    );
    let text = format!(
        r#"<office:document-content {NAMESPACES}><office:body><office:text><text:p>Embedded words</text:p></office:text></office:body></office:document-content>"#
    );
    let sheet = format!(
        r#"<office:document-content {NAMESPACES}><office:body><office:spreadsheet><table:table><table:table-column table:number-columns-repeated="1024"/><table:table-row><table:table-cell><text:p>Cell</text:p></table:table-cell><table:table-cell table:number-columns-repeated="1023"/></table:table-row><table:table-row table:number-rows-repeated="65535"><table:table-cell table:number-columns-repeated="1024"/></table:table-row></table:table></office:spreadsheet></office:body></office:document-content>"#
    );
    let bytes = package(&[
        ("content.xml", &content),
        ("Object 1/content.xml", &chart),
        ("Object 2/content.xml", &text),
        ("Object 3/content.xml", &sheet),
    ]);
    let document = parse_presentation(&bytes, &ParseLimits::default()).unwrap();
    let nodes = &document.slides[0].nodes;
    assert_eq!(nodes.len(), 3);
    let SlideNode::Chart { id, chart_type, title, series, chart_part_name, .. } = &nodes[0] else {
        panic!("chart node");
    };
    assert_eq!((id.as_str(), chart_type.as_str(), title.as_deref()), ("c1", "bar", Some("Sales")));
    assert_eq!(chart_part_name.as_deref(), Some("Object 1/content.xml"));
    assert_eq!(series[0].name.as_deref(), Some("Units"));
    assert_eq!(series[0].values, [Some(3.0), Some(5.0)]);
    assert_eq!(series[0].categories.as_ref().map(Vec::len), Some(2));

    let SlideNode::Shape { id, paragraphs, .. } = &nodes[1] else {
        panic!("embedded text");
    };
    assert_eq!((id.as_str(), paragraphs[0].runs[0].text.as_str()), ("t1", "Embedded words"));

    // The spreadsheet's unused million cells are not part of the table.
    let SlideNode::Table { id, rows, column_widths, .. } = &nodes[2] else {
        panic!("embedded sheet");
    };
    assert_eq!((id.as_str(), rows.len(), column_widths.len()), ("s1", 1, 1));
    assert_eq!(rows[0][0].paragraphs[0].runs[0].text, "Cell");
}

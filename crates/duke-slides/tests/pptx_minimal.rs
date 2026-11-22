use std::io::{Cursor, Write};

use duke_slides::{
    parse_presentation, parse_presentation_bundle, parse_presentation_content,
    ExtractPresentationBundleOptions, ExtractionLimits, ParseLimits, PresentationFormat, SlideNode,
};
use zip::{write::SimpleFileOptions, ZipWriter};

fn minimal_pptx() -> Vec<u8> {
    minimal_pptx_with_font(false)
}

fn minimal_pptx_with_font(include_font: bool) -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut output);
    let options = SimpleFileOptions::default();
    zip.start_file("ppt/presentation.xml", options).unwrap();
    zip.write_all(if include_font {
        br#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst><p:embeddedFontLst><p:embeddedFont><p:font typeface="Fixture Sans"/><p:regular r:id="rIdFont"/></p:embeddedFont></p:embeddedFontLst><p:sldSz cx="12192000" cy="6858000"/></p:presentation>"#.as_slice()
    } else {
        br#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst><p:sldSz cx="12192000" cy="6858000"/></p:presentation>"#.as_slice()
    }).unwrap();
    zip.start_file("ppt/_rels/presentation.xml.rels", options)
        .unwrap();
    zip.write_all(if include_font {
        br#"<Relationships><Relationship Id="rId2" Target="slides/slide1.xml"/><Relationship Id="rIdFont" Target="fonts/font1.fntdata"/></Relationships>"#.as_slice()
    } else {
        br#"<Relationships><Relationship Id="rId2" Target="slides/slide1.xml"/></Relationships>"#.as_slice()
    }).unwrap();
    zip.start_file("ppt/slides/slide1.xml", options).unwrap();
    zip.write_all(br#"<p:sld xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id="7" name="Title"/></p:nvSpPr><p:spPr><a:xfrm><a:off x="10" y="20"/><a:ext cx="300" cy="400"/></a:xfrm><a:prstGeom prst="rect"/></p:spPr><p:txBody><a:p><a:r><a:t>Hello</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#).unwrap();
    if include_font {
        let mut font = vec![0u8; 48];
        font[..4].copy_from_slice(b"OTTO");
        zip.start_file("ppt/fonts/font1.fntdata", options).unwrap();
        zip.write_all(&font).unwrap();
    }
    zip.finish().unwrap();
    output.into_inner()
}

#[test]
fn extracts_embedded_powerpoint_fonts() {
    let document =
        parse_presentation(&minimal_pptx_with_font(true), &ParseLimits::default()).unwrap();
    assert_eq!(document.embedded_fonts.len(), 1);
    let font = &document.embedded_fonts[0];
    assert_eq!(font.family, "Fixture Sans");
    assert_eq!(font.style, "normal");
    assert_eq!(font.weight, "400");
    let asset = document.assets.get(&font.asset_id).unwrap();
    assert_eq!(asset.content_type, "font/otf");
    assert_eq!(&asset.data.as_ref().unwrap()[..4], b"OTTO");
}

#[test]
fn parses_slide_order_size_transform_and_text() {
    use sha2::{Digest, Sha256};
    let bytes = minimal_pptx();
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let document = parse_presentation(&bytes, &ParseLimits::default()).unwrap();
    assert_eq!(document.source_sha256(), Some(digest.as_str()));
    assert_eq!(document.format, PresentationFormat::Pptx);
    assert_eq!(document.size.width_emu, 12_192_000);
    assert_eq!(document.slides.len(), 1);
    let SlideNode::Shape {
        id,
        transform,
        paragraphs,
        ..
    } = &document.slides[0].nodes[0]
    else {
        panic!("expected shape")
    };
    assert_eq!(id, "7");
    assert_eq!(
        (transform.x, transform.y, transform.width, transform.height),
        (10, 20, 300, 400)
    );
    assert_eq!(paragraphs[0].runs[0].text, "Hello");
}

#[test]
fn enforces_input_limit_before_decompression() {
    let bytes = minimal_pptx();
    let limits = ParseLimits {
        max_input_bytes: 4,
        ..ParseLimits::default()
    };
    assert!(parse_presentation(&bytes, &limits).is_err());
}

fn pptx_with_unreadable_unused_layout() -> Vec<u8> {
    const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    let mut output = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut output);
    let options = SimpleFileOptions::default();
    let parts = [
        (
            "ppt/presentation.xml",
            r#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldMasterIdLst><p:sldMasterId id="1" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst><p:sldSz cx="12192000" cy="6858000"/></p:presentation>"#.to_owned(),
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            format!(r#"<Relationships><Relationship Id="rId1" Type="{REL}/slideMaster" Target="slideMasters/slideMaster1.xml"/><Relationship Id="rId2" Type="{REL}/slide" Target="slides/slide1.xml"/></Relationships>"#),
        ),
        (
            "ppt/slides/slide1.xml",
            r#"<p:sld xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id="2" name="Body"/></p:nvSpPr><p:spPr/><p:txBody><a:p><a:r><a:t>Slide text</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#.to_owned(),
        ),
        (
            "ppt/slides/_rels/slide1.xml.rels",
            format!(r#"<Relationships><Relationship Id="rId1" Type="{REL}/slideLayout" Target="../slideLayouts/slideLayout1.xml"/></Relationships>"#),
        ),
        (
            "ppt/slideLayouts/slideLayout1.xml",
            r#"<p:sldLayout xmlns:p="p"><p:cSld><p:spTree/></p:cSld></p:sldLayout>"#.to_owned(),
        ),
        (
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
            format!(r#"<Relationships><Relationship Id="rId1" Type="{REL}/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#),
        ),
        (
            "ppt/slideLayouts/slideLayout2.xml",
            r#"<p:sldLayout xmlns:p="p"><p:cSld>"#.to_owned(),
        ),
        (
            "ppt/slideMasters/slideMaster1.xml",
            r#"<p:sldMaster xmlns:p="p"><p:cSld><p:spTree/></p:cSld></p:sldMaster>"#.to_owned(),
        ),
        (
            "ppt/slideMasters/_rels/slideMaster1.xml.rels",
            format!(r#"<Relationships><Relationship Id="rId1" Type="{REL}/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="{REL}/slideLayout" Target="../slideLayouts/slideLayout2.xml"/></Relationships>"#),
        ),
    ];
    for (path, xml) in parts {
        zip.start_file(path, options).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    output.into_inner()
}

#[test]
fn content_extraction_reads_only_the_layouts_slides_use() {
    let bytes = pptx_with_unreadable_unused_layout();
    assert!(parse_presentation(&bytes, &ParseLimits::default()).is_err());

    let options = ExtractPresentationBundleOptions::default();
    let content = parse_presentation_content(&bytes, options, true).unwrap();
    assert_eq!(content.body_text.as_deref().map(str::trim), Some("Slide text"));
    assert_eq!(content.bundle.slides.len(), 1);
    assert!(content.bundle.warnings.is_empty());
}

#[test]
fn in_memory_content_keeps_the_text_limit_without_measuring_serialized_size() {
    let bytes = minimal_pptx();
    let limits = |max_output_bytes| ExtractPresentationBundleOptions {
        limits: ExtractionLimits {
            max_output_bytes,
            ..Default::default()
        },
        ..Default::default()
    };
    // Larger than the slide text and smaller than the serialized bundle.
    assert!(parse_presentation_content(&bytes, limits(64), false).is_ok());
    assert!(parse_presentation_bundle(&bytes, limits(64)).is_err());
    assert!(parse_presentation_content(&bytes, limits(1), false).is_err());
}

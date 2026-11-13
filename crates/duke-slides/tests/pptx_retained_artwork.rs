use std::io::{Cursor, Seek, Write};

use duke_slides::{
    parse_presentation_retained, parse_presentation_with_options, render_node_artwork,
    ArtworkTarget, NodeArtwork, ParseLimits, ParseOptions, PresentationDocument, SlideNode,
};
use zip::{write::SimpleFileOptions, ZipWriter};

const FIXTURE: &[u8] = include_bytes!("fixtures/pretext-layout-cases.pptx");

fn node_id(node: &SlideNode) -> &str {
    match node {
        SlideNode::Shape { id, .. }
        | SlideNode::Image { id, .. }
        | SlideNode::Group { id, .. }
        | SlideNode::Table { id, .. }
        | SlideNode::Chart { id, .. }
        | SlideNode::Media { id, .. }
        | SlideNode::Unknown { id, .. } => id,
    }
}

fn eager_artwork(node: &SlideNode) -> Option<NodeArtwork> {
    match node {
        SlideNode::Shape {
            shape_svg: Some(svg),
            paint_bounds,
            ..
        } => Some(NodeArtwork {
            svg: svg.clone(),
            paint_bounds: paint_bounds.clone(),
            fallback_picture: None,
        }),
        SlideNode::Image {
            picture_svg: Some(svg),
            paint_bounds,
            fallback_picture,
            ..
        } => Some(NodeArtwork {
            svg: svg.clone(),
            paint_bounds: paint_bounds.clone(),
            fallback_picture: fallback_picture.clone(),
        }),
        SlideNode::Chart {
            chart_svg: Some(svg), ..
        } => Some(NodeArtwork {
            svg: svg.clone(),
            paint_bounds: None,
            fallback_picture: None,
        }),
        _ => None,
    }
}

/// Every node of a node list, in model order, with its eager artwork when the
/// id has not been seen before in that list (`find_node` returns the first).
fn collect(
    nodes: &[SlideNode],
    make: &dyn Fn(String) -> ArtworkTarget,
    seen: &mut std::collections::HashSet<String>,
    out: &mut Vec<(ArtworkTarget, Option<NodeArtwork>)>,
) {
    for node in nodes {
        let id = node_id(node).to_owned();
        if seen.insert(id.clone()) {
            out.push((make(id), eager_artwork(node)));
        }
        if let SlideNode::Group { children, .. } = node {
            collect(children, make, seen, out);
        }
    }
}

fn every_target(document: &PresentationDocument) -> Vec<(ArtworkTarget, Option<NodeArtwork>)> {
    let mut targets = Vec::new();
    for (index, slide) in document.slides.iter().enumerate() {
        let mut seen = Default::default();
        collect(
            &slide.nodes,
            &|node_id| ArtworkTarget {
                slide_index: Some(index),
                node_id,
                ..Default::default()
            },
            &mut seen,
            &mut targets,
        );
    }
    for layout in &document.layouts {
        let mut seen = Default::default();
        collect(
            &layout.nodes,
            &|node_id| ArtworkTarget {
                layout_id: Some(layout.id.clone()),
                node_id,
                ..Default::default()
            },
            &mut seen,
            &mut targets,
        );
    }
    for master in &document.masters {
        let mut seen = Default::default();
        collect(
            &master.nodes,
            &|node_id| ArtworkTarget {
                master_id: Some(master.id.clone()),
                node_id,
                ..Default::default()
            },
            &mut seen,
            &mut targets,
        );
    }
    targets
}

#[test]
fn retained_document_equals_the_deferred_parse() {
    let limits = ParseLimits::default();
    let deferred = parse_presentation_with_options(
        FIXTURE,
        &limits,
        &ParseOptions {
            defer_artwork: true,
            ..ParseOptions::default()
        },
    )
    .unwrap();
    // The option is implied: the handle always parses with artwork deferred.
    let retained = parse_presentation_retained(FIXTURE, &limits, &ParseOptions::default()).unwrap();
    assert_eq!(
        serde_json::to_value(retained.document()).unwrap(),
        serde_json::to_value(&deferred).unwrap()
    );
    assert_eq!(
        serde_json::to_value(retained.into_document()).unwrap(),
        serde_json::to_value(&deferred).unwrap()
    );
}

/// Every node of every node list renders the same through the handle, through
/// `render_node_artwork` and in the eager parse, on the layout fixture and on
/// the synthetic deck with decorated master and layout shapes.
#[test]
fn every_node_renders_like_the_eager_parse_and_the_reparsing_render() {
    let limits = ParseLimits::default();
    let options = ParseOptions::default();
    for (name, bytes) in [("fixture", FIXTURE.to_vec()), ("inherited", inherited_pptx())] {
        let eager = parse_presentation_with_options(&bytes, &limits, &options).unwrap();
        let retained = parse_presentation_retained(&bytes, &limits, &options).unwrap();
        let targets = every_target(&eager);
        let with_artwork = targets.iter().filter(|(_, artwork)| artwork.is_some()).count();
        assert!(with_artwork >= 3, "{name} renders too little artwork: {with_artwork}");
        if name == "inherited" {
            assert!(
                targets.iter().any(|(target, artwork)| target.layout_id.is_some() && artwork.is_some()),
                "{name} has no layout node with artwork"
            );
            assert!(
                targets.iter().any(|(target, artwork)| target.master_id.is_some() && artwork.is_some()),
                "{name} has no master node with artwork"
            );
        }
        for (target, expected) in &targets {
            let through_handle = retained.render_node_artwork(target).unwrap();
            assert_eq!(&through_handle, expected, "{name}: retained render differs for {target:?}");
            let reparsed = render_node_artwork(&bytes, &limits, &options, target).unwrap();
            assert_eq!(through_handle, reparsed, "{name}: retained and re-parsing renders differ for {target:?}");
        }
    }
}

fn decorated_shape(id: u32, name: &str, x: u32, fill: &str) -> String {
    format!(
        r##"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="200000"/><a:ext cx="914400" cy="457200"/></a:xfrm><a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="{fill}"/></a:solidFill><a:ln w="12700"><a:solidFill><a:srgbClr val="003300"/></a:solidFill></a:ln></p:spPr></p:sp>"##
    )
}

fn part(root: &str, attributes: &str, shapes: &str) -> Vec<u8> {
    format!(
        r##"<p:{root} xmlns:p="p" xmlns:a="a" xmlns:r="r"{attributes}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{shapes}</p:spTree></p:cSld></p:{root}>"##
    )
    .into_bytes()
}

/// A master with a decorated shape, a layout with a decorated shape and a title
/// placeholder, and two slides: the first shows the master shapes and holds a
/// decorated shape, a group with a decorated child and a title placeholder; the
/// second hides the master shapes.
fn inherited_pptx() -> Vec<u8> {
    const LAYOUT_REL: &[u8] = br##"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdLayout" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/></Relationships>"##;
    let mut output = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut output);
    add_file(
        &mut zip,
        "ppt/presentation.xml",
        br##"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rIdMaster"/></p:sldMasterIdLst><p:sldIdLst><p:sldId id="256" r:id="rId1"/><p:sldId id="257" r:id="rId2"/></p:sldIdLst><p:sldSz cx="12192000" cy="6858000"/></p:presentation>"##,
    );
    add_file(
        &mut zip,
        "ppt/_rels/presentation.xml.rels",
        br##"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide2.xml"/><Relationship Id="rIdMaster" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/></Relationships>"##,
    );
    add_file(&mut zip, "ppt/slides/_rels/slide1.xml.rels", LAYOUT_REL);
    add_file(&mut zip, "ppt/slides/_rels/slide2.xml.rels", LAYOUT_REL);
    let title = r##"<p:sp><p:nvSpPr><p:cNvPr id="23" name="Title"/><p:cNvSpPr/><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:p><a:r><a:t>Title</a:t></a:r></a:p></p:txBody></p:sp>"##;
    let group = format!(
        r##"<p:grpSp><p:nvGrpSpPr><p:cNvPr id="22" name="Group"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="3000000" y="3000000"/><a:ext cx="1828800" cy="914400"/><a:chOff x="0" y="0"/><a:chExt cx="914400" cy="457200"/></a:xfrm></p:grpSpPr>{}</p:grpSp>"##,
        decorated_shape(21, "Grouped", 0, "0000AA")
    );
    add_file(
        &mut zip,
        "ppt/slides/slide1.xml",
        &part("sld", "", &format!("{}{group}{title}", decorated_shape(20, "Own", 2_000_000, "AA0000"))),
    );
    add_file(
        &mut zip,
        "ppt/slides/slide2.xml",
        &part("sld", r#" showMasterSp="0""#, &decorated_shape(20, "Own", 2_000_000, "AA00AA")),
    );
    add_file(
        &mut zip,
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
        br##"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdMaster" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"##,
    );
    let layout_title = r##"<p:sp><p:nvSpPr><p:cNvPr id="31" name="Layout title"/><p:cNvSpPr/><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="500000" y="500000"/><a:ext cx="9000000" cy="1000000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="DDDDDD"/></a:solidFill></p:spPr></p:sp>"##;
    add_file(
        &mut zip,
        "ppt/slideLayouts/slideLayout1.xml",
        &part("sldLayout", "", &format!("{}{layout_title}", decorated_shape(30, "Layout decoration", 4_000_000, "00AA00"))),
    );
    add_file(
        &mut zip,
        "ppt/slideMasters/slideMaster1.xml",
        &part("sldMaster", "", &decorated_shape(40, "Master decoration", 6_000_000, "AAAA00")),
    );
    zip.finish().unwrap();
    output.into_inner()
}

#[test]
fn inherited_decorative_nodes_render_through_their_prefixed_ids() {
    let limits = ParseLimits::default();
    let options = ParseOptions::default();
    let bytes = inherited_pptx();
    let eager = parse_presentation_with_options(&bytes, &limits, &options).unwrap();
    let retained = parse_presentation_retained(&bytes, &limits, &options).unwrap();
    let ids = |index: usize| {
        eager.slides[index]
            .nodes
            .iter()
            .map(|node| node_id(node).to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(0), ["master:40", "layout:30", "20", "22", "23"]);
    assert_eq!(ids(1), ["layout:30", "20"]);
    let mut inherited = 0;
    for (index, slide) in eager.slides.iter().enumerate() {
        for node in &slide.nodes {
            let id = node_id(node);
            if !(id.starts_with("layout:") || id.starts_with("master:")) {
                continue;
            }
            let expected = eager_artwork(node).expect("decorations render");
            inherited += 1;
            let target = ArtworkTarget {
                slide_index: Some(index),
                node_id: id.to_owned(),
                ..Default::default()
            };
            assert_eq!(retained.render_node_artwork(&target).unwrap(), Some(expected));
        }
    }
    assert_eq!(inherited, 3);
    // The second slide hides the master shapes, so the master decoration is not
    // one of its nodes and cannot be rendered through it; its own list differs.
    let hidden = ArtworkTarget {
        slide_index: Some(1),
        node_id: "master:40".into(),
        ..Default::default()
    };
    assert_eq!(retained.render_node_artwork(&hidden).unwrap(), None);
    assert_eq!(render_node_artwork(&bytes, &limits, &options, &hidden).unwrap(), None);
    let grouped = ArtworkTarget {
        slide_index: Some(0),
        node_id: "21".into(),
        ..Default::default()
    };
    let SlideNode::Group { children, .. } = &eager.slides[0].nodes[3] else {
        panic!("expected the group")
    };
    let expected = eager_artwork(&children[0]).expect("the grouped shape renders");
    assert_eq!(retained.render_node_artwork(&grouped).unwrap(), Some(expected));
    // Layout and master node lists are addressed by their part names.
    let layout = ArtworkTarget {
        layout_id: Some("ppt/slideLayouts/slideLayout1.xml".into()),
        node_id: "30".into(),
        ..Default::default()
    };
    let master = ArtworkTarget {
        master_id: Some("ppt/slideMasters/slideMaster1.xml".into()),
        node_id: "40".into(),
        ..Default::default()
    };
    for target in [layout, master] {
        let expected = render_node_artwork(&bytes, &limits, &options, &target).unwrap();
        assert!(expected.is_some(), "{target:?} renders");
        assert_eq!(retained.render_node_artwork(&target).unwrap(), expected);
    }
}

#[test]
fn repeated_renders_are_identical_and_missing_targets_yield_none() {
    let limits = ParseLimits::default();
    let options = ParseOptions::default();
    let eager = parse_presentation_with_options(FIXTURE, &limits, &options).unwrap();
    let retained = parse_presentation_retained(FIXTURE, &limits, &options).unwrap();
    let (target, expected) = every_target(&eager)
        .into_iter()
        .find_map(|(target, artwork)| artwork.map(|artwork| (target, artwork)))
        .expect("fixture has a node with artwork");
    let first = retained.render_node_artwork(&target).unwrap();
    let second = retained.render_node_artwork(&target).unwrap();
    assert_eq!(first.as_ref(), Some(&expected));
    assert_eq!(first, second);
    let slide_index = target.slide_index.expect("the first artwork node is on a slide");
    for missing in [
        ArtworkTarget {
            slide_index: Some(slide_index),
            node_id: "no-such-node".into(),
            ..Default::default()
        },
        ArtworkTarget {
            slide_index: Some(eager.slides.len()),
            node_id: target.node_id.clone(),
            ..Default::default()
        },
        ArtworkTarget {
            layout_id: Some("ppt/slideLayouts/missing.xml".into()),
            node_id: target.node_id.clone(),
            ..Default::default()
        },
        ArtworkTarget {
            master_id: Some("ppt/slideMasters/missing.xml".into()),
            node_id: target.node_id.clone(),
            ..Default::default()
        },
        ArtworkTarget {
            node_id: target.node_id.clone(),
            ..Default::default()
        },
        // Two node lists selected at once never render, as with `render_node_artwork`.
        ArtworkTarget {
            slide_index: Some(slide_index),
            layout_id: eager.slides[slide_index].layout_id.clone(),
            node_id: target.node_id.clone(),
            ..Default::default()
        },
    ] {
        assert_eq!(retained.render_node_artwork(&missing).unwrap(), None, "{missing:?}");
        assert_eq!(render_node_artwork(FIXTURE, &limits, &options, &missing).unwrap(), None);
    }
}

const PNG: &[u8] = &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xff, 0xff, 0x3f, 0x00, 0x05, 0xfe, 0x02, 0xfe, 0x0d, 0xef, 0x46, 0xb8, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82];

const INK: &str = r##"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<inkml:ink xmlns:inkml="http://www.w3.org/2003/InkML"><inkml:definitions><inkml:context xml:id="ctx0"><inkml:inkSource xml:id="inkSrc0"><inkml:traceFormat><inkml:channel name="X" type="integer" min="-2.14748E9" max="2.14748E9" units="cm"/><inkml:channel name="Y" type="integer" min="-2.14748E9" max="2.14748E9" units="cm"/></inkml:traceFormat><inkml:channelProperties><inkml:channelProperty channel="X" name="resolution" value="1000" units="1/cm"/><inkml:channelProperty channel="Y" name="resolution" value="1000" units="1/cm"/></inkml:channelProperties></inkml:inkSource><inkml:timestamp xml:id="ts0" timeString="2020-01-17T15:16:03.382"/></inkml:context><inkml:brush xml:id="br0"><inkml:brushProperty name="width" value="0.05" units="cm"/><inkml:brushProperty name="height" value="0.05" units="cm"/><inkml:brushProperty name="ignorePressure" value="1"/></inkml:brush></inkml:definitions><inkml:trace contextRef="#ctx0" brushRef="#br0">0 1,'0'0</inkml:trace></inkml:ink>"##;

fn add_file<W: Write + Seek>(zip: &mut ZipWriter<W>, path: &str, contents: &[u8]) {
    zip.start_file(path, SimpleFileOptions::default()).unwrap();
    zip.write_all(contents).unwrap();
}

/// Two slides. The first holds an ink content part in a markup-compatibility
/// wrapper with a saved-preview fallback, as PowerPoint saves ink, and a
/// rotated picture of an image that only the second slide references through
/// its relationships. The eager parse renders the first slide before that image
/// is loaded, so the retained render must not see it either.
fn two_slide_pptx(ink_xml: &str) -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut output);
    add_file(
        &mut zip,
        "ppt/presentation.xml",
        br##"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="256" r:id="rId1"/><p:sldId id="257" r:id="rId2"/></p:sldIdLst><p:sldSz cx="12192000" cy="6858000"/></p:presentation>"##,
    );
    add_file(
        &mut zip,
        "ppt/_rels/presentation.xml.rels",
        br##"<Relationships><Relationship Id="rId1" Target="slides/slide1.xml"/><Relationship Id="rId2" Target="slides/slide2.xml"/></Relationships>"##,
    );
    add_file(
        &mut zip,
        "ppt/slides/_rels/slide1.xml.rels",
        br##"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdInk" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml" Target="../ink/ink1.xml"/><Relationship Id="rIdImg" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/><Relationship Id="rIdLate" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/package" Target="../embeddings/late.png"/></Relationships>"##,
    );
    add_file(
        &mut zip,
        "ppt/slides/slide1.xml",
        br##"<p:sld xmlns:p="p" xmlns:a="a" xmlns:r="r" xmlns:mc="mc" xmlns:p14="p14">
          <p:cSld><p:spTree>
            <p:nvGrpSpPr><p:cNvPr id="1" name=""/></p:nvGrpSpPr>
            <p:grpSpPr/>
            <mc:AlternateContent>
              <mc:Choice Requires="p14">
                <p:contentPart p14:bwMode="auto" r:id="rIdInk">
                  <p14:nvContentPartPr><p14:cNvPr id="10" name="Ink 9"/><p14:cNvContentPartPr/><p14:nvPr/></p14:nvContentPartPr>
                  <p14:xfrm><a:off x="4327309" y="4518427"/><a:ext cx="360" cy="360"/></p14:xfrm>
                </p:contentPart>
              </mc:Choice>
              <mc:Fallback>
                <p:pic>
                  <p:nvPicPr><p:cNvPr id="10" name="Ink 9"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr>
                  <p:blipFill><a:blip r:embed="rIdImg"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>
                  <p:spPr><a:xfrm><a:off x="4318309" y="4509787"/><a:ext cx="18000" cy="18000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
                </p:pic>
              </mc:Fallback>
            </mc:AlternateContent>
            <p:pic>
              <p:nvPicPr><p:cNvPr id="11" name="Late picture"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr>
              <p:blipFill><a:blip r:embed="rIdLate"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>
              <p:spPr><a:xfrm rot="2700000"><a:off x="1000000" y="1000000"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
            </p:pic>
            <p:sp>
              <p:nvSpPr><p:cNvPr id="12" name="Rectangle"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
              <p:spPr><a:xfrm><a:off x="2000000" y="1000000"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="roundRect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></p:spPr>
            </p:sp>
          </p:spTree></p:cSld>
        </p:sld>"##,
    );
    add_file(
        &mut zip,
        "ppt/slides/_rels/slide2.xml.rels",
        br##"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdLate" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../embeddings/late.png"/></Relationships>"##,
    );
    add_file(
        &mut zip,
        "ppt/slides/slide2.xml",
        br##"<p:sld xmlns:p="p" xmlns:a="a" xmlns:r="r">
          <p:cSld><p:spTree>
            <p:nvGrpSpPr><p:cNvPr id="1" name=""/></p:nvGrpSpPr>
            <p:grpSpPr/>
            <p:pic>
              <p:nvPicPr><p:cNvPr id="11" name="Late picture"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr>
              <p:blipFill><a:blip r:embed="rIdLate"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>
              <p:spPr><a:xfrm rot="2700000"><a:off x="1000000" y="1000000"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
            </p:pic>
          </p:spTree></p:cSld>
        </p:sld>"##,
    );
    add_file(&mut zip, "ppt/ink/ink1.xml", ink_xml.as_bytes());
    add_file(&mut zip, "ppt/media/image1.png", PNG);
    add_file(&mut zip, "ppt/embeddings/late.png", PNG);
    zip.finish().unwrap();
    output.into_inner()
}

fn image_node<'a>(nodes: &'a [SlideNode], id: &str) -> &'a SlideNode {
    nodes
        .iter()
        .find(|node| node_id(node) == id)
        .unwrap_or_else(|| panic!("no node {id} in {nodes:?}"))
}

#[test]
fn ink_renders_on_demand_and_its_fallback_choice_is_kept() {
    let limits = ParseLimits::default();
    let options = ParseOptions::default();
    let bytes = two_slide_pptx(INK);
    let eager = parse_presentation_with_options(&bytes, &limits, &options).unwrap();
    let retained = parse_presentation_retained(&bytes, &limits, &options).unwrap();
    let SlideNode::Image {
        asset_id,
        ink: Some(_),
        picture_svg: Some(svg),
        ..
    } = image_node(&eager.slides[0].nodes, "10")
    else {
        panic!("expected the rendered ink node, got {:?}", eager.slides[0].nodes[0])
    };
    assert_eq!(asset_id, "ppt/ink/ink1.xml");
    let SlideNode::Image {
        asset_id,
        ink: Some(_),
        picture_svg: None,
        ..
    } = image_node(&retained.document().slides[0].nodes, "10")
    else {
        panic!("expected the deferred ink node, got {:?}", retained.document().slides[0].nodes[0])
    };
    assert_eq!(asset_id, "ppt/ink/ink1.xml");
    let target = ArtworkTarget {
        slide_index: Some(0),
        node_id: "10".into(),
        ..Default::default()
    };
    let artwork = retained
        .render_node_artwork(&target)
        .unwrap()
        .expect("the ink node renders on demand");
    assert_eq!(artwork.svg, *svg);
    assert_eq!(Some(artwork), render_node_artwork(&bytes, &limits, &options, &target).unwrap());

    // An unrenderable ink keeps the saved preview in both parses, which has no
    // generated artwork of its own.
    let bytes = two_slide_pptx(&INK.replace(
        "<inkml:definitions>",
        "<inkml:canvasTransform/><inkml:definitions>",
    ));
    let eager = parse_presentation_with_options(&bytes, &limits, &options).unwrap();
    let retained = parse_presentation_retained(&bytes, &limits, &options).unwrap();
    for document in [&eager, retained.document()] {
        let SlideNode::Image {
            asset_id,
            ink: Some(ink),
            ..
        } = image_node(&document.slides[0].nodes, "10")
        else {
            panic!("expected the saved-preview picture carrying the ink")
        };
        assert_eq!(asset_id, "ppt/media/image1.png");
        assert_eq!(ink.asset_id.as_deref(), Some("ppt/ink/ink1.xml"));
    }
    assert_eq!(
        retained.render_node_artwork(&target).unwrap(),
        render_node_artwork(&bytes, &limits, &options, &target).unwrap()
    );
}

#[test]
fn a_part_sees_only_the_assets_loaded_when_it_was_parsed() {
    let limits = ParseLimits::default();
    let options = ParseOptions::default();
    let bytes = two_slide_pptx(INK);
    let eager = parse_presentation_with_options(&bytes, &limits, &options).unwrap();
    let retained = parse_presentation_retained(&bytes, &limits, &options).unwrap();
    assert!(
        eager.assets.contains_key("ppt/embeddings/late.png"),
        "the second slide loads the late image"
    );
    // Slide 1 references the late image through a non-asset relationship, so the
    // eager parse had not loaded it when slide 1 was parsed: the rotated picture
    // has no generated artwork there, while the same picture renders on slide 2.
    let early = ArtworkTarget {
        slide_index: Some(0),
        node_id: "11".into(),
        ..Default::default()
    };
    let late = ArtworkTarget {
        slide_index: Some(1),
        node_id: "11".into(),
        ..Default::default()
    };
    assert_eq!(eager_artwork(image_node(&eager.slides[0].nodes, "11")), None);
    let expected = eager_artwork(image_node(&eager.slides[1].nodes, "11"))
        .expect("the rotated picture renders where its image is loaded");
    assert_eq!(retained.render_node_artwork(&early).unwrap(), None);
    assert_eq!(render_node_artwork(&bytes, &limits, &options, &early).unwrap(), None);
    assert_eq!(retained.render_node_artwork(&late).unwrap(), Some(expected.clone()));
    assert_eq!(render_node_artwork(&bytes, &limits, &options, &late).unwrap(), Some(expected));
    // The plain shape on slide 1 renders either way.
    let shape = ArtworkTarget {
        slide_index: Some(0),
        node_id: "12".into(),
        ..Default::default()
    };
    let expected = eager_artwork(image_node(&eager.slides[0].nodes, "12")).expect("shape artwork");
    assert_eq!(retained.render_node_artwork(&shape).unwrap(), Some(expected));
}

fn legacy_record(version: u16, record_type: u16, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(payload.len() + 8);
    bytes.extend_from_slice(&version.to_le_bytes());
    bytes.extend_from_slice(&record_type.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

fn minimal_ppt() -> Vec<u8> {
    let utf16: Vec<u8> = "Legacy slide"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let text = legacy_record(0, 4000, &utf16);
    let slide = legacy_record(0x000f, 1006, &text);
    let mut compound = cfb::CompoundFile::create(Cursor::new(Vec::new())).unwrap();
    compound
        .create_stream("/PowerPoint Document")
        .unwrap()
        .write_all(&slide)
        .unwrap();
    compound.into_inner().into_inner()
}

#[test]
fn legacy_presentations_keep_their_model_and_render_nothing() {
    let bytes = &minimal_ppt();
    let limits = ParseLimits::default();
    let retained = parse_presentation_retained(bytes, &limits, &ParseOptions::default()).unwrap();
    let document = parse_presentation_with_options(bytes, &limits, &ParseOptions::default()).unwrap();
    assert_eq!(document.format, duke_slides::PresentationFormat::Ppt);
    assert_eq!(
        serde_json::to_value(retained.document()).unwrap(),
        serde_json::to_value(&document).unwrap()
    );
    let target = ArtworkTarget {
        slide_index: Some(0),
        node_id: "1".into(),
        ..Default::default()
    };
    assert_eq!(retained.render_node_artwork(&target).unwrap(), None);
}

use std::io::{Cursor, Seek, Write};

use duke_pptx::{
    parse_presentation_with_options, render_node_artwork, ArtworkTarget, ParseLimits,
    ParseOptions, SlideNode,
};
use serde_json::Value;
use zip::{write::SimpleFileOptions, ZipWriter};

const FIXTURE: &[u8] = include_bytes!("fixtures/pretext-layout-cases.pptx");
const ARTWORK_KEYS: [&str; 5] = ["shapeSvg", "pictureSvg", "chartSvg", "paintBounds", "fallbackPicture"];
const RENDERING_WARNINGS: [&str; 4] = [
    "unsupported-shape-appearance",
    "unsupported-picture-appearance",
    "unsupported-chart-rendering",
    "ink-fallback",
];

fn strip(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(key, _)| !ARTWORK_KEYS.contains(&key.as_str()))
                .map(|(key, child)| {
                    let child = if key == "warnings" {
                        match child {
                            Value::Array(items) => Value::Array(
                                items
                                    .iter()
                                    .filter(|item| {
                                        !item.get("code").and_then(Value::as_str).is_some_and(|code| {
                                            RENDERING_WARNINGS.contains(&code)
                                        })
                                    })
                                    .cloned()
                                    .collect(),
                            ),
                            other => other.clone(),
                        }
                    } else {
                        child.clone()
                    };
                    (key.clone(), strip(&child))
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(strip).collect()),
        other => other.clone(),
    }
}

fn first_artwork(nodes: &[SlideNode]) -> Option<(String, String)> {
    nodes.iter().find_map(|node| match node {
        SlideNode::Shape {
            id,
            shape_svg: Some(svg),
            ..
        } => Some((id.clone(), svg.clone())),
        SlideNode::Image {
            id,
            picture_svg: Some(svg),
            ..
        } => Some((id.clone(), svg.clone())),
        SlideNode::Group { children, .. } => first_artwork(children),
        _ => None,
    })
}

fn has_artwork(value: &Value) -> bool {
    match value {
        Value::Object(map) => map
            .iter()
            .any(|(key, child)| ARTWORK_KEYS.contains(&key.as_str()) || has_artwork(child)),
        Value::Array(items) => items.iter().any(has_artwork),
        _ => false,
    }
}

#[test]
fn deferred_artwork_keeps_everything_but_generated_artwork() {
    let limits = ParseLimits::default();
    let eager = parse_presentation_with_options(FIXTURE, &limits, &ParseOptions::default()).unwrap();
    let deferred = parse_presentation_with_options(
        FIXTURE,
        &limits,
        &ParseOptions {
            defer_artwork: true,
            ..ParseOptions::default()
        },
    )
    .unwrap();
    let eager_json = serde_json::to_value(&eager).unwrap();
    let deferred_json = serde_json::to_value(&deferred).unwrap();
    assert!(has_artwork(&eager_json), "fixture renders no artwork eagerly");
    assert!(!has_artwork(&deferred_json), "deferred parse still produced artwork");
    assert_eq!(strip(&eager_json), strip(&deferred_json));
}

#[test]
fn on_demand_render_matches_eager_artwork() {
    let limits = ParseLimits::default();
    let options = ParseOptions::default();
    let eager = parse_presentation_with_options(FIXTURE, &limits, &options).unwrap();
    let (slide_index, (node_id, svg)) = eager
        .slides
        .iter()
        .enumerate()
        .find_map(|(index, slide)| first_artwork(&slide.nodes).map(|found| (index, found)))
        .expect("fixture has a slide node with artwork");
    let target = ArtworkTarget {
        slide_index: Some(slide_index),
        node_id: node_id.clone(),
        ..ArtworkTarget::default()
    };
    let artwork = render_node_artwork(FIXTURE, &limits, &options, &target)
        .unwrap()
        .expect("target renders artwork");
    assert_eq!(artwork.svg, svg);
    let missing = ArtworkTarget {
        slide_index: Some(slide_index),
        node_id: "no-such-node".into(),
        ..ArtworkTarget::default()
    };
    assert!(render_node_artwork(FIXTURE, &limits, &options, &missing).unwrap().is_none());
}

const PNG: &[u8] = &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xff, 0xff, 0x3f, 0x00, 0x05, 0xfe, 0x02, 0xfe, 0x0d, 0xef, 0x46, 0xb8, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82];

/// A real two-point ink stroke as PowerPoint writes it.
const INK: &str = r##"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<inkml:ink xmlns:inkml="http://www.w3.org/2003/InkML"><inkml:definitions><inkml:context xml:id="ctx0"><inkml:inkSource xml:id="inkSrc0"><inkml:traceFormat><inkml:channel name="X" type="integer" min="-2.14748E9" max="2.14748E9" units="cm"/><inkml:channel name="Y" type="integer" min="-2.14748E9" max="2.14748E9" units="cm"/></inkml:traceFormat><inkml:channelProperties><inkml:channelProperty channel="X" name="resolution" value="1000" units="1/cm"/><inkml:channelProperty channel="Y" name="resolution" value="1000" units="1/cm"/></inkml:channelProperties></inkml:inkSource><inkml:timestamp xml:id="ts0" timeString="2020-01-17T15:16:03.382"/></inkml:context><inkml:brush xml:id="br0"><inkml:brushProperty name="width" value="0.05" units="cm"/><inkml:brushProperty name="height" value="0.05" units="cm"/><inkml:brushProperty name="ignorePressure" value="1"/></inkml:brush></inkml:definitions><inkml:trace contextRef="#ctx0" brushRef="#br0">0 1,'0'0</inkml:trace></inkml:ink>"##;

fn add_file<W: Write + Seek>(zip: &mut ZipWriter<W>, path: &str, contents: &[u8]) {
    zip.start_file(path, SimpleFileOptions::default()).unwrap();
    zip.write_all(contents).unwrap();
}

/// One slide whose ink content part sits in a markup-compatibility wrapper with
/// a saved-preview picture as the fallback, as PowerPoint saves ink.
fn ink_pptx(ink_xml: &str) -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut output);
    add_file(
        &mut zip,
        "ppt/presentation.xml",
        br##"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst><p:sldSz cx="12192000" cy="6858000"/></p:presentation>"##,
    );
    add_file(
        &mut zip,
        "ppt/_rels/presentation.xml.rels",
        br##"<Relationships><Relationship Id="rId1" Target="slides/slide1.xml"/></Relationships>"##,
    );
    add_file(
        &mut zip,
        "ppt/slides/_rels/slide1.xml.rels",
        br##"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdInk" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml" Target="../ink/ink1.xml"/><Relationship Id="rIdImg" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/></Relationships>"##,
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
          </p:spTree></p:cSld>
        </p:sld>"##,
    );
    add_file(&mut zip, "ppt/ink/ink1.xml", ink_xml.as_bytes());
    add_file(&mut zip, "ppt/media/image1.png", PNG);
    zip.finish().unwrap();
    output.into_inner()
}

fn has_warning(value: &Value, code: &str) -> bool {
    match value {
        Value::Object(map) => {
            map.get("code").and_then(Value::as_str) == Some(code)
                || map.values().any(|child| has_warning(child, code))
        }
        Value::Array(items) => items.iter().any(|item| has_warning(item, code)),
        _ => false,
    }
}

fn parse_both(bytes: &[u8]) -> (duke_pptx::PresentationDocument, duke_pptx::PresentationDocument) {
    let limits = ParseLimits::default();
    let eager = parse_presentation_with_options(bytes, &limits, &ParseOptions::default()).unwrap();
    let deferred = parse_presentation_with_options(
        bytes,
        &limits,
        &ParseOptions {
            defer_artwork: true,
            ..ParseOptions::default()
        },
    )
    .unwrap();
    (eager, deferred)
}

#[test]
fn deferred_ink_keeps_the_rendered_choice_and_drops_only_its_preview() {
    let bytes = ink_pptx(INK);
    let (eager, deferred) = parse_both(&bytes);
    let (
        SlideNode::Image {
            id,
            asset_id,
            ink: Some(_),
            picture_svg: Some(svg),
            ..
        },
        SlideNode::Image {
            id: deferred_id,
            asset_id: deferred_asset,
            ink: Some(_),
            picture_svg: None,
            paint_bounds: None,
            ..
        },
    ) = (&eager.slides[0].nodes[0], &deferred.slides[0].nodes[0])
    else {
        panic!(
            "expected the rendered ink node in both modes, got {:?} and {:?}",
            eager.slides[0].nodes[0], deferred.slides[0].nodes[0]
        )
    };
    assert_eq!(asset_id, "ppt/ink/ink1.xml");
    assert_eq!(deferred_asset, "ppt/ink/ink1.xml");
    assert_eq!(id, deferred_id);
    let eager_json = serde_json::to_value(&eager).unwrap();
    let deferred_json = serde_json::to_value(&deferred).unwrap();
    assert!(!has_warning(&eager_json, "ink-fallback"));
    assert!(!has_artwork(&deferred_json));
    assert_eq!(strip(&eager_json), strip(&deferred_json));
    let artwork = render_node_artwork(
        &bytes,
        &ParseLimits::default(),
        &ParseOptions::default(),
        &ArtworkTarget {
            slide_index: Some(0),
            node_id: id.clone(),
            ..ArtworkTarget::default()
        },
    )
    .unwrap()
    .expect("the ink node renders on demand");
    assert_eq!(artwork.svg, *svg);
}

#[test]
fn deferred_ink_falls_back_to_the_saved_preview_like_an_eager_parse() {
    // A canvas transform makes the ink unrenderable, so both modes must keep
    // the saved-preview picture carrying the ink source.
    let bytes = ink_pptx(&INK.replace(
        "<inkml:definitions>",
        "<inkml:canvasTransform/><inkml:definitions>",
    ));
    let (eager, deferred) = parse_both(&bytes);
    for node in [&eager.slides[0].nodes[0], &deferred.slides[0].nodes[0]] {
        let SlideNode::Image {
            asset_id,
            ink: Some(ink),
            ..
        } = node
        else {
            panic!("expected the saved-preview picture carrying the ink, got {node:?}")
        };
        assert_eq!(asset_id, "ppt/media/image1.png");
        assert_eq!(ink.asset_id.as_deref(), Some("ppt/ink/ink1.xml"));
    }
    let eager_json = serde_json::to_value(&eager).unwrap();
    let deferred_json = serde_json::to_value(&deferred).unwrap();
    assert!(has_warning(&eager_json, "ink-fallback"));
    assert!(has_warning(&deferred_json, "ink-fallback"));
    assert!(!has_artwork(&deferred_json));
    assert_eq!(strip(&eager_json), strip(&deferred_json));
}

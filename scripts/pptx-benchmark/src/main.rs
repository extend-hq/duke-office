use std::{fs, hint::black_box, io::{Cursor, Write}, time::Instant, sync::Arc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use flate2::{write::GzEncoder, Compression};

enum Parsed {
    Core(duke_pptx::PresentationDocument),
    Oxide(office_oxide::ir::DocumentIR),
    Undoc(undoc::Document),
}

fn parse(engine: &str, bytes: &Arc<[u8]>, legacy: bool) -> Result<Parsed, String> {
    match engine {
        "duke-pptx" => duke_pptx::parse_presentation(bytes, &Default::default()).map(Parsed::Core).map_err(|e| e.to_string()),
        "duke-pptx-deferred" => duke_pptx::parse_presentation_with_options(bytes, &Default::default(), &duke_pptx::ParseOptions { defer_artwork: true, ..Default::default() }).map(Parsed::Core).map_err(|e| e.to_string()),
        "office_oxide" => {
            let format = if legacy { office_oxide::format::DocumentFormat::Ppt } else { office_oxide::format::DocumentFormat::Pptx };
            let doc = office_oxide::Document::from_reader(Cursor::new(Arc::clone(bytes)), format).map_err(|e|e.to_string())?;
            Ok(Parsed::Oxide(doc.to_ir()))
        },
        "undoc" => undoc::parse_bytes(bytes).map(Parsed::Undoc).map_err(|e|e.to_string()),
        _ => Err("unknown engine".into()),
    }
}

fn compact(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for key in ["sourceXml", "shapeSvg", "pictureSvg", "chartXml", "chartSvg", "svg", "source_xml"] {
                if let Some(s) = m.remove(key) { m.insert(format!("{key}Bytes"), json!(s.as_str().map_or(0, str::len))); }
            }
            for key in ["data", "bytes"] {
                if let Some(Value::Array(a)) = m.get(key) {
                    if a.iter().all(|x|x.as_u64().is_some_and(|n| n<256)) {
                        let data: Vec<u8> = a.iter().map(|x|x.as_u64().unwrap() as u8).collect();
                        m.insert(format!("{key}Sha256"),json!(format!("{:x}",Sha256::digest(&data))));
                        m.remove(key);
                    }
                }
            }
            for child in m.values_mut() { compact(child); }
        },
        Value::Array(a) => for child in a { compact(child); },
        _ => (),
    }
}

fn take_editing(v: &mut Value, path: &str, nodes: &mut Vec<Value>) {
    match v {
        Value::Object(map) => {
            if map.contains_key("type") && map.contains_key("id") && map.contains_key("transform") {
                let mut node = serde_json::Map::new();
                for key in ["type", "id", "sourceId", "sourcePart", "sourceXml", "editing"] {
                    if let Some(value) = map.get(key) { node.insert(key.to_owned(), value.clone()); }
                }
                node.insert("transform".to_owned(), json!({}));
                nodes.push(json!({"path": path, "node": node}));
                map.remove("editing");
            }
            for (key, child) in map {
                if key != "data" && key != "bytes" { take_editing(child, &format!("{path}/{key}"), nodes); }
            }
        },
        Value::Array(array) => for (index, child) in array.iter_mut().enumerate() {
            take_editing(child, &format!("{path}/{index}"), nodes);
        },
        _ => (),
    }
}

/// The first `limit` nodes with eager artwork (first occurrence of each id per
/// node list, in model order) with that artwork, and the eager document.
fn artwork_targets(bytes: &[u8], limit: usize) -> Result<(duke_pptx::PresentationDocument, Vec<(duke_pptx::ArtworkTarget, duke_pptx::NodeArtwork)>), String> {
    use duke_pptx::{ArtworkTarget, NodeArtwork, SlideNode};
    use std::collections::HashSet;
    fn artwork(node: &SlideNode) -> Option<NodeArtwork> {
        match node {
            SlideNode::Shape { shape_svg: Some(svg), paint_bounds, .. } => Some(NodeArtwork { svg: svg.clone(), paint_bounds: paint_bounds.clone(), fallback_picture: None }),
            SlideNode::Image { picture_svg: Some(svg), paint_bounds, fallback_picture, .. } => Some(NodeArtwork { svg: svg.clone(), paint_bounds: paint_bounds.clone(), fallback_picture: fallback_picture.clone() }),
            SlideNode::Chart { chart_svg: Some(svg), .. } => Some(NodeArtwork { svg: svg.clone(), paint_bounds: None, fallback_picture: None }),
            _ => None,
        }
    }
    fn collect(nodes: &[SlideNode], seen: &mut HashSet<String>, make: &dyn Fn(String) -> ArtworkTarget, out: &mut Vec<(ArtworkTarget, NodeArtwork)>, limit: usize) {
        for node in nodes {
            let id = match node {
                SlideNode::Shape { id, .. } | SlideNode::Image { id, .. } | SlideNode::Group { id, .. } | SlideNode::Table { id, .. } | SlideNode::Chart { id, .. } | SlideNode::Media { id, .. } | SlideNode::Unknown { id, .. } => id.clone(),
            };
            if seen.insert(id.clone()) {
                if let Some(expected) = artwork(node) {
                    if out.len() < limit { out.push((make(id), expected)); }
                }
            }
            if let SlideNode::Group { children, .. } = node { collect(children, seen, make, out, limit); }
        }
    }
    let limits = duke_pptx::ParseLimits::default();
    let options = duke_pptx::ParseOptions::default();
    let document = duke_pptx::parse_presentation_with_options(bytes, &limits, &options).map_err(|e| e.to_string())?;
    let mut targets = Vec::new();
    for (index, slide) in document.slides.iter().enumerate() {
        let mut seen = HashSet::new();
        collect(&slide.nodes, &mut seen, &|node_id| ArtworkTarget { slide_index: Some(index), node_id, ..Default::default() }, &mut targets, limit);
    }
    for layout in &document.layouts {
        let mut seen = HashSet::new();
        collect(&layout.nodes, &mut seen, &|node_id| ArtworkTarget { layout_id: Some(layout.id.clone()), node_id, ..Default::default() }, &mut targets, limit);
    }
    for master in &document.masters {
        let mut seen = HashSet::new();
        collect(&master.nodes, &mut seen, &|node_id| ArtworkTarget { master_id: Some(master.id.clone()), node_id, ..Default::default() }, &mut targets, limit);
    }
    Ok((document, targets))
}

/// Compares on-demand renders of the first `limit` eager artwork nodes with the
/// eager artwork, both through `render_node_artwork` (one parse per render) and
/// through a retained `ParsedPresentation`, whose model must equal the deferred parse.
fn artwork_check(bytes: &[u8], limit: usize) -> Result<Value, String> {
    let limits = duke_pptx::ParseLimits::default();
    let options = duke_pptx::ParseOptions::default();
    let (_, targets) = artwork_targets(bytes, limit)?;
    let deferred = duke_pptx::parse_presentation_with_options(bytes, &limits, &duke_pptx::ParseOptions { defer_artwork: true, ..options }).map_err(|e| e.to_string())?;
    let retained = duke_pptx::parse_presentation_retained(bytes, &limits, &options).map_err(|e| e.to_string())?;
    let retained_document_equal = serde_json::to_vec(retained.document()).map_err(|e| e.to_string())? == serde_json::to_vec(&deferred).map_err(|e| e.to_string())?;
    drop(deferred);
    let (mut matched, mut reparse_matched, mut retained_matched) = (0, 0, 0);
    let mut mismatches = Vec::new();
    for (target, expected) in &targets {
        let reparsed = duke_pptx::render_node_artwork(bytes, &limits, &options, target).map_err(|e| e.to_string())?;
        let through_handle = retained.render_node_artwork(target).map_err(|e| e.to_string())?;
        let reparse_ok = reparsed.as_ref() == Some(expected);
        let retained_ok = through_handle.as_ref() == Some(expected);
        reparse_matched += usize::from(reparse_ok);
        retained_matched += usize::from(retained_ok);
        if reparse_ok && retained_ok { matched += 1; } else { mismatches.push(json!({"target": target, "expected_bytes": expected.svg.len(), "reparse_bytes": reparsed.map(|a| a.svg.len()), "retained_bytes": through_handle.map(|a| a.svg.len())})); }
    }
    Ok(json!({"status": "ok", "checked": targets.len(), "matched": matched, "reparse_matched": reparse_matched, "retained_matched": retained_matched, "retained_document_equal": retained_document_equal, "mismatches": mismatches}))
}

/// Times, interleaved within this process, `targets.len()` on-demand renders
/// through `render_node_artwork` against one `parse_presentation_retained` plus the
/// same renders through the handle. Each sample is one complete pass; the order
/// of the two methods alternates between samples.
fn render_timing(bytes: &[u8], limit: usize, samples: usize) -> Result<Value, String> {
    let limits = duke_pptx::ParseLimits::default();
    let options = duke_pptx::ParseOptions::default();
    let (document, targets) = artwork_targets(bytes, limit)?;
    let slides = document.slides.len();
    drop(document);
    let (mut reparse_ms, mut retained_ms, mut handle_ms, mut handle_render_ms) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for sample in 0..samples {
        for method in if sample % 2 == 0 { [0, 1] } else { [1, 0] } {
            if method == 0 {
                let start = Instant::now();
                for (target, _) in &targets {
                    black_box(duke_pptx::render_node_artwork(bytes, &limits, &options, target).map_err(|e| e.to_string())?);
                }
                reparse_ms.push(start.elapsed().as_secs_f64() * 1000.);
            } else {
                let start = Instant::now();
                let handle = duke_pptx::parse_presentation_retained(bytes, &limits, &options).map_err(|e| e.to_string())?;
                let created = start.elapsed().as_secs_f64() * 1000.;
                for (target, _) in &targets {
                    black_box(handle.render_node_artwork(target).map_err(|e| e.to_string())?);
                }
                let total = start.elapsed().as_secs_f64() * 1000.;
                drop(handle);
                handle_ms.push(created);
                handle_render_ms.push(total - created);
                retained_ms.push(total);
            }
        }
    }
    Ok(json!({"status": "ok", "targets": targets.len(), "slides": slides, "samples": samples, "reparse_ms": reparse_ms, "retained_ms": retained_ms, "handle_ms": handle_ms, "handle_render_ms": handle_render_ms}))
}

fn run(args: &[String]) -> Result<Value,String> {
    let bytes: Arc<[u8]>=fs::read(&args[2]).map_err(|e|e.to_string())?.into();
    let legacy=bytes.starts_with(b"\xd0\xcf\x11\xe0");
    let repeats:usize=args[4].parse().unwrap();
    if args[1]=="duke-pptx-artwork-check" { return artwork_check(&bytes, if repeats==0 {6} else {repeats}); }
    if args[1]=="duke-pptx-render-timing" {
        let limit=args.get(5).and_then(|value| value.parse().ok()).unwrap_or(12);
        return render_timing(&bytes, limit, if repeats==0 {7} else {repeats});
    }
    let start=Instant::now();
    let first=parse(&args[1],&bytes,legacy)?;
    let cold_ms=start.elapsed().as_secs_f64()*1000.;
    let loops=((10.0/cold_ms.max(0.01)).ceil() as usize).clamp(1,10);
    let mut samples=Vec::new();
    for _ in 0..repeats {
        let mut elapsed=0.;
        for _ in 0..loops {
            let start=Instant::now();
            let parsed=parse(&args[1],&bytes,legacy)?;
            elapsed+=start.elapsed().as_secs_f64()*1000.;
            black_box(&parsed);
            drop(parsed);
        }
        samples.push(elapsed/loops as f64);
    }
    let mut full_sha256=None;
    let mut without_editing_sha256=None;
    let mut without_document_identity_sha256=None;
    let mut source_identity_matches=None;
    if args[3]!="-" {
        let mut v=match first {
            Parsed::Core(d)=>serde_json::to_value(d),
            Parsed::Oxide(d)=>serde_json::to_value(d),
            Parsed::Undoc(d)=>{
                let hashes:std::collections::BTreeMap<_,_>=d.resources.iter().map(|(k,v)|(k.clone(),format!("{:x}",Sha256::digest(&v.data)))).collect();
                let mut v=serde_json::to_value(d).map_err(|e|e.to_string())?;
                v["resource_payload_sha256"]=json!(hashes);
                Ok(v)
            },
        }.map_err(|e:serde_json::Error|e.to_string())?;
        let canonical=serde_json::to_vec(&v).map_err(|e|e.to_string())?;
        full_sha256=Some(format!("{:x}",Sha256::digest(&canonical)));
        drop(canonical);
        if args.get(5).map(String::as_str)==Some("identity") {
            let identity=v.as_object_mut().and_then(|object| object.remove("documentId"));
            let expected=format!("pptx-v1:sha256:{:x}",Sha256::digest(&bytes));
            source_identity_matches=Some(identity.as_ref().and_then(Value::as_str)==Some(expected.as_str()));
            let canonical=serde_json::to_vec(&v).map_err(|e|e.to_string())?;
            without_document_identity_sha256=Some(format!("{:x}",Sha256::digest(&canonical)));
            if let Some(identity)=identity { v["documentId"]=identity; }
        }
        if args.get(5).map(String::as_str)==Some("editing") {
            let mut nodes=Vec::new();
            take_editing(&mut v, "", &mut nodes);
            let canonical=serde_json::to_vec(&v).map_err(|e|e.to_string())?;
            without_editing_sha256=Some(format!("{:x}",Sha256::digest(&canonical)));
            drop(canonical);
            compact(&mut v);
            v=json!({"model":v,"nodes":nodes});
        } else if args.get(5).map(String::as_str)!=Some("full") { compact(&mut v); }
        let mut writer=GzEncoder::new(fs::File::create(&args[3]).map_err(|e|e.to_string())?,Compression::fast());
        serde_json::to_writer(&mut writer,&v).map_err(|e|e.to_string())?;
        writer.flush().map_err(|e|e.to_string())?;
        writer.finish().map_err(|e|e.to_string())?;
    }
    let mut record=json!({"status":"ok","cold_ms":cold_ms,"loops":loops,"samples_ms":samples,"full_sha256":full_sha256});
    if let Some(hash)=without_editing_sha256 { record["without_editing_sha256"]=json!(hash); }
    if let Some(hash)=without_document_identity_sha256 { record["without_document_identity_sha256"]=json!(hash); }
    if let Some(matches)=source_identity_matches { record["source_identity_matches"]=json!(matches); }
    Ok(record)
}

fn main() {
    let args:Vec<_>=std::env::args().collect();
    std::panic::set_hook(Box::new(|_|{}));
    let result=std::thread::Builder::new().stack_size(64*1024*1024).spawn(move||std::panic::catch_unwind(||run(&args))).unwrap().join();
    let value=match result { Ok(Ok(Ok(v)))=>v, Ok(Ok(Err(e)))=>json!({"status":"error","error":e}), _=>json!({"status":"panic"}) };
    println!("{value}");
}

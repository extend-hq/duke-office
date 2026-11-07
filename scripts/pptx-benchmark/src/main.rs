use std::{fs, hint::black_box, io::{Cursor, Write}, time::Instant, sync::Arc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use flate2::{write::GzEncoder, Compression};

enum Parsed {
    Core(rust_pptx::PresentationDocument),
    Oxide(office_oxide::ir::DocumentIR),
    Undoc(undoc::Document),
}

fn parse(engine: &str, bytes: &Arc<[u8]>, legacy: bool) -> Result<Parsed, String> {
    match engine {
        "rust-pptx" => rust_pptx::parse_presentation(bytes, &Default::default()).map(Parsed::Core).map_err(|e| e.to_string()),
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

fn run(args: &[String]) -> Result<Value,String> {
    let bytes: Arc<[u8]>=fs::read(&args[2]).map_err(|e|e.to_string())?.into();
    let legacy=bytes.starts_with(b"\xd0\xcf\x11\xe0");
    let repeats:usize=args[4].parse().unwrap();
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
    Ok(record)
}

fn main() {
    let args:Vec<_>=std::env::args().collect();
    std::panic::set_hook(Box::new(|_|{}));
    let result=std::thread::Builder::new().stack_size(64*1024*1024).spawn(move||std::panic::catch_unwind(||run(&args))).unwrap().join();
    let value=match result { Ok(Ok(Ok(v)))=>v, Ok(Ok(Err(e)))=>json!({"status":"error","error":e}), _=>json!({"status":"panic"}) };
    println!("{value}");
}

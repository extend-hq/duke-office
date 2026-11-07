mod boss;

use std::{fs, hint::black_box, io::Write, sync::Arc, time::Instant};
use flate2::{write::GzEncoder, Compression};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

enum Parsed {
    Boss(boss::Materialized),
    Better(pptx_parse::PptxPackage),
    Rusty(rustypptx::PptxDocument),
}

fn parse(engine: &str, bytes: &Arc<[u8]>) -> Result<Parsed, String> {
    match engine {
        "pptxboss" => boss::parse(bytes).map(Parsed::Boss),
        "betteroffice" => pptx_parse::parse_pptx(bytes).map(Parsed::Better).map_err(|e| e.to_string()),
        "rustypptx" => rustypptx::parse_pptx_bytes(bytes).map(Parsed::Rusty).map_err(|e| e.to_string()),
        _ => Err("Unknown parser".into()),
    }
}

fn compact(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for key in ["data", "bytes"] {
                if let Some(Value::Array(values)) = object.get(key) {
                    if values.iter().all(|value| value.as_u64().is_some_and(|value| value < 256)) {
                        let bytes: Vec<u8> = values.iter().map(|value| value.as_u64().unwrap() as u8).collect();
                        object.insert(format!("{key}Sha256"), json!(format!("{:x}", Sha256::digest(&bytes))));
                        object.remove(key);
                    }
                }
            }
            for child in object.values_mut() { compact(child); }
        }
        Value::Array(values) => for child in values { compact(child); },
        _ => (),
    }
}

fn serialize(parsed: Parsed) -> Result<Value, String> {
    let mut value = match parsed {
        Parsed::Boss(document) => Ok(boss::serialize(document)),
        Parsed::Better(document) => {
            let mut value = serde_json::to_value(&document).map_err(|e| e.to_string())?;
            for (output, media) in value["media"].as_array_mut().unwrap().iter_mut().zip(&document.media) {
                output["bytesSha256"] = json!(format!("{:x}", Sha256::digest(&media.bytes)));
                output.as_object_mut().unwrap().remove("bytes");
            }
            Ok(value)
        },
        Parsed::Rusty(document) => {
            let mut value = serde_json::to_value(&document).map_err(|e| e.to_string())?;
            for (output, slide) in value["slides"].as_array_mut().unwrap().iter_mut().zip(&document.slides) {
                for (output, image) in output["images"].as_array_mut().unwrap().iter_mut().zip(&slide.images) {
                    output["dataSha256"] = json!(format!("{:x}", Sha256::digest(&image.data)));
                }
            }
            Ok(value)
        }
    }.map_err(|e: serde_json::Error| e.to_string())?;
    compact(&mut value);
    Ok(value)
}

fn run(args: &[String]) -> Result<Value, String> {
    let bytes: Arc<[u8]> = fs::read(&args[2]).map_err(|e| e.to_string())?.into();
    let repeats: usize = args[4].parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
    if args[1] == "rustypptx" {
        rayon::ThreadPoolBuilder::new().num_threads(1).build_global().map_err(|e| e.to_string())?;
    }
    let start = Instant::now();
    let first = parse(&args[1], &bytes)?;
    let cold_ms = start.elapsed().as_secs_f64() * 1000.;
    let loops = ((10.0 / cold_ms.max(0.01)).ceil() as usize).clamp(1, 10);
    let mut samples = Vec::new();
    for _ in 0..repeats {
        let mut elapsed = 0.;
        for _ in 0..loops {
            let start = Instant::now();
            let parsed = parse(&args[1], &bytes)?;
            elapsed += start.elapsed().as_secs_f64() * 1000.;
            black_box(&parsed);
            drop(parsed);
        }
        samples.push(elapsed / loops as f64);
    }
    let mut output_sha256 = None;
    if args[3] != "-" {
        let value = serialize(first)?;
        let canonical = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
        output_sha256 = Some(format!("{:x}", Sha256::digest(&canonical)));
        let mut writer = GzEncoder::new(fs::File::create(&args[3]).map_err(|e| e.to_string())?, Compression::fast());
        writer.write_all(&canonical).map_err(|e| e.to_string())?;
        writer.finish().map_err(|e| e.to_string())?;
    }
    Ok(json!({"status":"ok","cold_ms":cold_ms,"loops":loops,"samples_ms":samples,"output_sha256":output_sha256,"threads":1}))
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::thread::Builder::new().stack_size(64 * 1024 * 1024)
        .spawn(move || std::panic::catch_unwind(|| run(&args))).unwrap().join();
    let value = match result {
        Ok(Ok(Ok(value))) => value,
        Ok(Ok(Err(error))) => json!({"status":"error","error":error}),
        _ => json!({"status":"panic"}),
    };
    println!("{value}");
}

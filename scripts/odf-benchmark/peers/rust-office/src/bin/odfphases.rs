//! Splits `rust-odf`'s read time into inflating and everything after it.
//! Usage: odfphases <list file>

use std::time::Instant;

fn best<T>(mut call: impl FnMut() -> T) -> (f64, T) {
    let mut best = f64::INFINITY;
    let mut last = None;
    let begin = Instant::now();
    let mut reps = 0;
    while reps < 3 || (begin.elapsed().as_millis() < 40 && reps < 50) {
        let start = Instant::now();
        last = Some(call());
        best = best.min(start.elapsed().as_secs_f64() * 1000.0);
        reps += 1;
    }
    (best, last.unwrap())
}

fn main() {
    let list = std::env::args().nth(1).unwrap();
    let limits = rust_odf::Limits::default();
    let (mut inflate, mut total, mut bytes_xml, mut files) = (0.0, 0.0, 0usize, 0);
    for path in std::fs::read_to_string(list).unwrap().lines() {
        let Ok(bytes) = std::fs::read(path) else { continue };
        let (total_ms, parsed) = best(|| rust_odf::OdfDocument::parse(&bytes, &limits));
        if parsed.is_err() {
            continue;
        }
        let (inflate_ms, parts) = best(|| {
            let package = rust_odf::OdfPackage::open(&bytes, &limits).ok()?;
            Some([
                package.xml("content.xml").ok().flatten(),
                package.xml("styles.xml").ok().flatten(),
                package.xml("meta.xml").ok().flatten(),
            ])
        });
        let Some(parts) = parts else { continue };
        bytes_xml += parts.iter().flatten().map(String::len).sum::<usize>();
        inflate += inflate_ms;
        total += total_ms;
        files += 1;
    }
    println!(
        "files {files}: total {total:.1} ms = inflate {inflate:.1} + parse and build {:.1}; xml {:.1} MB",
        total - inflate,
        bytes_xml as f64 / 1e6
    );
}

//! Where this repository's OpenDocument import spends its time, per phase.
//! Usage: phases <list file>

use std::time::Instant;

fn time<T>(mut call: impl FnMut() -> T) -> (f64, T) {
    let mut best = f64::INFINITY;
    let mut last = None;
    let begin = Instant::now();
    let mut reps = 0;
    while reps < 3 || (begin.elapsed().as_millis() < 60 && reps < 50) {
        let start = Instant::now();
        last = Some(call());
        best = best.min(start.elapsed().as_secs_f64() * 1000.0);
        reps += 1;
    }
    (best, last.unwrap())
}

fn main() {
    let list = std::env::args().nth(1).unwrap();
    let mut totals = [0.0f64; 4];
    let mut rows = Vec::new();
    for path in std::fs::read_to_string(list).unwrap().lines() {
        let Ok(bytes) = std::fs::read(path) else { continue };
        let Some(kind) = duke_odf::sniff(&bytes) else { continue };
        let limits = duke_odf::Limits::default();
        let (odf, parsed) = time(|| duke_odf::OdfDocument::parse(&bytes, &limits));
        if parsed.is_err() {
            continue;
        }
        let mut phases = [odf, 0.0, 0.0, 0.0];
        match kind.family {
            duke_odf::OdfFamily::Text => {
                let (model_ms, model) = time(|| duke_docx::parse_odt(&bytes));
                let Ok(model) = model else { continue };
                let (blocks_ms, _) = time(|| duke_docx::extract_document_blocks(&model));
                // Everything after the shared read: building the model, the
                // identity hash and the block identifiers.
                phases[1] = (model_ms - odf).max(0.0);
                phases[2] = blocks_ms;
            }
            duke_odf::OdfFamily::Presentation | duke_odf::OdfFamily::Graphics => {
                let (total, document) =
                    time(|| duke_pptx::parse_presentation(&bytes, &duke_pptx::ParseLimits::server()));
                if document.is_err() {
                    continue;
                }
                phases[3] = total - odf;
            }
            _ => continue,
        }
        for (total, phase) in totals.iter_mut().zip(phases) {
            *total += phase;
        }
        rows.push((phases.iter().sum::<f64>(), phases, bytes.len(), path.rsplit('/').next().unwrap_or_default().to_owned()));
    }
    println!("files {}", rows.len());
    println!("totals ms: odf-read {:.1} | odt-model {:.1} | blocks {:.1} | odp-model {:.1}", totals[0], totals[1], totals[2], totals[3]);
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (total, phases, size, name) in rows.iter().take(12) {
        println!("{total:8.2} ms  odf {:.2} model {:.2} blocks {:.2} odp {:.2}  {size:>8} B  {name}", phases[0], phases[1], phases[2], phases[3]);
    }
    let mut medians: Vec<f64> = rows.iter().map(|row| row.0).collect();
    medians.sort_by(f64::total_cmp);
    println!("median total {:.3} ms; median odf-read {:.3}", medians[medians.len() / 2], {
        let mut odf: Vec<f64> = rows.iter().map(|row| row.1[0]).collect();
        odf.sort_by(f64::total_cmp);
        odf[odf.len() / 2]
    });
}

use duke_odf::{Body, Limits, OdfDocument};
use std::{hint::black_box, time::Instant};

fn best<T>(mut f: impl FnMut() -> T) -> f64 {
    let mut best = f64::INFINITY;
    for _ in 0..100 {
        let start = Instant::now();
        black_box(f());
        best = best.min(start.elapsed().as_secs_f64() * 1e6);
    }
    best
}

fn visit(doc: &OdfDocument, totals: &mut [f64; 3], depth: usize) {
    if depth > 16 {
        return;
    }
    if let Body::Chart(chart) = &doc.body {
        let direct = || duke_office_charts::odf::extract_chart_data(black_box(chart));
        let roundtrip = || {
            duke_office_charts::odf::chart_xml(black_box(chart))
                .and_then(|xml| duke_office_charts::parse_chart_xml(&xml))
        };
        assert_eq!(direct(), roundtrip());
        let a = best(roundtrip);
        let b = best(direct);
        println!("chart,{},{a:.4},{b:.4}", totals[0] as usize);
        totals[0] += 1.;
        totals[1] += a;
        totals[2] += b;
    }
    for object in doc.objects.values() {
        if let Ok(doc) = object.document(&Limits::default()) {
            visit(&doc, totals, depth + 1);
        }
    }
}

fn main() {
    let mut totals = [0.; 3];
    for path in std::fs::read_to_string(std::env::args().nth(1).expect("list path"))
        .unwrap()
        .lines()
    {
        let bytes = std::fs::read(path).unwrap();
        if let Ok(doc) = OdfDocument::parse(&bytes, &Limits::default()) {
            visit(&doc, &mut totals, 0);
        }
    }
    println!("total,{},{:.4},{:.4}", totals[0], totals[1], totals[2]);
}

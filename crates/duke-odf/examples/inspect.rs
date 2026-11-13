//! Prints what `duke-odf` reads from a file: `cargo run -p duke-odf --example inspect -- file.odt`.

use duke_odf::{text::plain_text, Body, Limits, OdfDocument};

fn main() {
    let path = std::env::args().nth(1).expect("usage: inspect <file>");
    let bytes = std::fs::read(&path).expect("read input");
    let document = match OdfDocument::parse(&bytes, &Limits::default()) {
        Ok(document) => document,
        Err(error) => {
            eprintln!("{path}: {error}");
            std::process::exit(1);
        }
    };
    println!("kind: {:?}", document.kind);
    println!("metadata: {:?}", document.metadata);
    println!("master pages: {}", document.styles.master_pages.len());
    println!("binaries: {:?}", document.binaries.iter().map(|(name, bytes)| (name, bytes.len())).collect::<Vec<_>>());
    println!("objects: {:?}", document.objects.keys().collect::<Vec<_>>());
    println!("warnings: {:?}", document.warnings);
    match &document.body {
        Body::Text(blocks) => println!("text blocks: {}\n{}", blocks.len(), plain_text(blocks)),
        Body::Pages(pages) => {
            for (index, page) in pages.iter().enumerate() {
                println!("page {index}: {:?} master {:?}", page.name, page.master_page);
                for shape in &page.shapes {
                    println!(
                        "  {:?} class {:?} at {:?}: {:?}",
                        std::mem::discriminant(&shape.kind),
                        shape.class,
                        shape.placement(),
                        plain_text(&shape.text)
                    );
                }
            }
        }
        Body::Spreadsheet(tables) => println!("sheets: {}", tables.len()),
        Body::Chart(chart) => println!(
            "chart: {} titled {:?}, {} categories, series {:?}",
            chart.class,
            chart.title,
            chart.categories.len(),
            chart.series.iter().map(|series| series.name.as_deref()).collect::<Vec<_>>()
        ),
        Body::Other => println!("no modelled body"),
    }
}

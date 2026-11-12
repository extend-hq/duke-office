#[path = "../../../harness.rs"]
mod harness;
use transmutation::converters::odt::OdtConverter;
use transmutation::DocumentConverter;

fn main() {
    // Path-only and async-only; a current-thread runtime drives it.
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let converter = OdtConverter::new();
    harness::run(|_, path| {
        let format = transmutation::OutputFormat::Markdown { split_pages: false, optimize_for_llm: true };
        let result = runtime
            .block_on(converter.convert(path, format, transmutation::ConversionOptions::default()))
            .map_err(|error| error.to_string())?;
        result
            .content
            .first()
            .map(|output| String::from_utf8_lossy(&output.data).into_owned())
            .ok_or_else(|| "no output".to_owned())
    });
}

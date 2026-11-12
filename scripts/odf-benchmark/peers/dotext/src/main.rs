#[path = "../../../harness.rs"]
mod harness;
use dotext::doc::OpenOfficeDoc;
use dotext::{Odp, Odt};
use std::io::Read;

fn main() {
    harness::run(|_, path| {
        let extension = path.extension().and_then(|extension| extension.to_str()).unwrap_or_default();
        let mut out = String::new();
        if extension == "odp" {
            Odp::open(path).and_then(|mut file| file.read_to_string(&mut out)).map_err(|error| error.to_string())?;
        } else {
            Odt::open(path).and_then(|mut file| file.read_to_string(&mut out)).map_err(|error| error.to_string())?;
        }
        Ok(out)
    });
}

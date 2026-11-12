//! Shared runner for one parser: reads a list of files, extracts text from
//! each, and reports warm in-process timings as JSON lines on stdout.
//!
//! Usage: `<runner> <list file> <output directory>`. Text is written to
//! `<output directory>/<family>/<file name>.txt`, where the family is the
//! name of the file's parent directory.

use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A single call may run this long before the process gives up on the file.
const FILE_TIMEOUT_SECONDS: u64 = 60;
/// Warm repeats continue until this much time has been spent on a file.
const REPEAT_BUDGET: Duration = Duration::from_millis(250);

fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

#[repr(C)]
struct Timespec {
    seconds: i64,
    nanoseconds: i64,
}

extern "C" {
    fn clock_gettime(clock: i32, time: *mut Timespec) -> i32;
}

/// `CLOCK_PROCESS_CPUTIME_ID`.
#[cfg(target_os = "macos")]
const PROCESS_CPU_CLOCK: i32 = 12;
#[cfg(not(target_os = "macos"))]
const PROCESS_CPU_CLOCK: i32 = 2;

/// CPU time this process has used, in milliseconds, summed over its threads.
/// Unlike elapsed time it does not grow while other programs hold the processor.
fn cpu_milliseconds() -> f64 {
    let mut time = Timespec {
        seconds: 0,
        nanoseconds: 0,
    };
    // SAFETY: `time` is a valid, writable `timespec` for the duration of the call.
    unsafe { clock_gettime(PROCESS_CPU_CLOCK, &mut time) };
    time.seconds as f64 * 1000.0 + time.nanoseconds as f64 / 1e6
}

fn now_seconds() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs())
}

pub fn run(extract: impl Fn(&[u8], &Path) -> Result<String, String>) {
    let mut arguments = std::env::args().skip(1);
    let list = arguments.next().expect("usage: <list file> <output directory>");
    let output = PathBuf::from(arguments.next().expect("usage: <list file> <output directory>"));
    if std::env::var_os("PEER_VERBOSE").is_none() {
        std::panic::set_hook(Box::new(|_| {}));
    }

    // Exit when one call hangs; the driver records the file and resumes after it.
    let started = Arc::new(AtomicU64::new(0));
    let watched = Arc::clone(&started);
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        let since = watched.load(Ordering::Relaxed);
        if since != 0 && now_seconds().saturating_sub(since) > FILE_TIMEOUT_SECONDS {
            eprintln!("TIMEOUT");
            std::process::exit(124);
        }
    });

    let stdout = std::io::stdout();
    for path in std::fs::read_to_string(&list).expect("read list").lines() {
        if path.is_empty() {
            continue;
        }
        let file = Path::new(path);
        let family = file
            .parent()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let key = format!("{family}/{name}");
        eprintln!("START\t{key}");
        let bytes = match std::fs::read(file) {
            Ok(bytes) => bytes,
            Err(error) => {
                let mut out = stdout.lock();
                writeln!(out, r#"{{"file":{},"ok":false,"error":{}}}"#, json_string(&key), json_string(&error.to_string())).unwrap();
                continue;
            }
        };
        started.store(now_seconds(), Ordering::Relaxed);
        let first_start = Instant::now();
        let first = catch_unwind(AssertUnwindSafe(|| extract(&bytes, file)));
        let first_elapsed = first_start.elapsed();
        let (text, error) = match first {
            Ok(Ok(text)) => (Some(text), String::new()),
            Ok(Err(error)) => (None, error),
            Err(_) => (None, "panic".to_owned()),
        };
        let mut samples: Vec<Duration> = Vec::new();
        let mut cpu = f64::INFINITY;
        if text.is_some() && first_elapsed < Duration::from_secs(5) {
            let begin = Instant::now();
            while samples.len() < 3 || (begin.elapsed() < REPEAT_BUDGET && samples.len() < 200) {
                started.store(now_seconds(), Ordering::Relaxed);
                let cpu_start = cpu_milliseconds();
                let start = Instant::now();
                let result = catch_unwind(AssertUnwindSafe(|| extract(&bytes, file)));
                samples.push(start.elapsed());
                cpu = cpu.min(cpu_milliseconds() - cpu_start);
                std::hint::black_box(&result);
                drop(result);
            }
        }
        started.store(0, Ordering::Relaxed);
        if samples.is_empty() {
            samples.push(first_elapsed);
        }
        let milliseconds = |duration: Duration| duration.as_secs_f64() * 1000.0;
        let mean = samples.iter().copied().map(milliseconds).sum::<f64>() / samples.len() as f64;
        let min = samples.iter().copied().map(milliseconds).fold(f64::INFINITY, f64::min);
        if let Some(text) = &text {
            let directory = output.join(&family);
            std::fs::create_dir_all(&directory).expect("create output directory");
            std::fs::write(directory.join(format!("{name}.txt")), text).expect("write text");
        }
        let mut out = stdout.lock();
        writeln!(
            out,
            r#"{{"file":{},"ok":{},"error":{},"bytes":{},"chars":{},"first_ms":{:.4},"mean_ms":{:.4},"min_ms":{:.4},"cpu_ms":{},"reps":{}}}"#,
            json_string(&key),
            text.is_some(),
            json_string(&error),
            bytes.len(),
            text.as_ref().map_or(0, String::len),
            milliseconds(first_elapsed),
            mean,
            min,
            if cpu.is_finite() { format!("{cpu:.4}") } else { "null".to_owned() },
            samples.len(),
        )
        .unwrap();
        out.flush().unwrap();
    }
}

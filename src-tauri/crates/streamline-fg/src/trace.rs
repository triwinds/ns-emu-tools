use serde_json::{json, Value};
use std::cell::Cell;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

thread_local! { static PHASE: Cell<u32> = const { Cell::new(0) }; }
static TRACE: OnceLock<Option<Mutex<File>>> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();

#[no_mangle]
pub extern "system" fn probeSetPhase(phase: u32) {
    PHASE.with(|p| p.set(phase));
}

pub fn authorized() -> bool {
    let expected = std::env::var_os("NS_STREAMLINE_PROBE_EXE");
    let actual = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok());
    let expected = expected.and_then(|p| std::path::PathBuf::from(p).canonicalize().ok());
    actual.is_some() && actual == expected && cfg!(all(windows, target_arch = "x86_64"))
}

pub fn event(name: &str, details: Value) {
    let sink = TRACE.get_or_init(|| {
        if !authorized() {
            return None;
        }
        let path = std::env::var_os("NS_STREAMLINE_PROBE_TRACE")?;
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .ok()
            .map(Mutex::new)
    });
    if let Some(sink) = sink {
        let row = json!({"event": name, "phase": PHASE.with(Cell::get), "thread": format!("{:?}", std::thread::current().id()), "elapsed_us": START.get_or_init(Instant::now).elapsed().as_micros(), "details": details});
        if let Ok(mut file) = sink.lock() {
            let _ = writeln!(file, "{row}");
            let _ = file.flush();
        }
    }
}

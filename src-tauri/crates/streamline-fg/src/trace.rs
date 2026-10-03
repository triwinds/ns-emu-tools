use serde_json::{json, Value};
use std::cell::Cell;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

thread_local! { static PHASE: Cell<u32> = const { Cell::new(0) }; }
static TRACE: OnceLock<Option<Mutex<BufWriter<File>>>> = OnceLock::new();
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

// Gate before JSON construction in hot Vulkan hooks. Diagnostic hosts retain
// full tracing; ordinary target FG sessions keep lifecycle/error and required present evidence.
pub fn verbose() -> bool {
    static VERBOSE: OnceLock<bool> = OnceLock::new();
    *VERBOSE.get_or_init(|| match std::env::var("NS_STREAMLINE_TRACE_VERBOSE") {
        Ok(v) => v == "1",
        Err(_) => {
            std::env::var("NS_STREAMLINE_TARGET_FG").as_deref() != Ok("1")
                && std::env::var("NS_STREAMLINE_NATIVE_NR").as_deref() != Ok("1")
        }
    })
}
fn hot_event(name: &str) -> bool {
    (name.starts_with("vk")
        && !name.starts_with("vkCreate")
        && !name.starts_with("vkDestroy")
        && name != "vkQueuePresentKHR")
        || matches!(name, "target_nvof_frame" | "target_nvof_confidence")
}
pub fn enabled(name: &str) -> bool {
    static FRAMES: OnceLock<bool> = OnceLock::new();
    let frames =
        *FRAMES.get_or_init(|| std::env::var("NS_STREAMLINE_TRACE_FRAMES").as_deref() != Ok("0"));
    if !frames
        && matches!(
            name,
            "vkQueuePresentKHR"
                | "route_present_retired"
                | "target_fg_frame"
                | "target_sr_frame"
                | "target_nr_frame"
                | "target_nr_submission"
                | "target_nr_completion"
                | "target_nr_profile"
                | "target_nr_evaluate"
        )
    {
        return false;
    }
    !hot_event(name) || verbose()
}
macro_rules! event {
    ($name:expr, $details:expr $(,)?) => {{
        let name = $name;
        if $crate::trace::enabled(name) {
            $crate::trace::record(name, $details);
        }
    }};
}
pub(crate) use event;

pub fn record(name: &str, details: Value) {
    let sink = TRACE.get_or_init(|| {
        if !authorized() {
            return None;
        }
        let path = std::env::var_os("NS_STREAMLINE_PROBE_TRACE")?;
        OpenOptions::new()
            // Qt's Vulkan suitability checks can unload and reload this DLL
            // before the game's instance. Keep evidence from every lifetime in
            // the launcher's private session instead of losing the later logs.
            .create(true)
            .append(true)
            .open(path)
            .ok()
            .map(|file| Mutex::new(BufWriter::with_capacity(64 * 1024, file)))
    });
    if let Some(sink) = sink {
        let row = json!({"event": name, "phase": PHASE.with(Cell::get), "thread": format!("{:?}", std::thread::current().id()), "elapsed_us": START.get_or_init(Instant::now).elapsed().as_micros(), "details": details});
        if let Ok(mut file) = sink.lock() {
            let _ = writeln!(file, "{row}");
            // Keep frame evidence for session_verify, but batch it off the hot path.
            // Lifecycle/error records and explicit verbose diagnostics flush promptly.
            if verbose()
                || !matches!(
                    name,
                    "vkQueuePresentKHR"
                        | "route_present_retired"
                        | "target_fg_frame"
                        | "target_sr_frame"
                        | "target_sr_profile"
                        | "target_nvof_gpu"
                        | "target_nvof_profile"
                )
            {
                let _ = file.flush();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hot_path_filter_keeps_lifecycle_and_failures() {
        for name in [
            "vkBeginCommandBuffer",
            "vkEndCommandBuffer",
            "vkQueueSubmit",
            "target_nvof_frame",
        ] {
            assert!(hot_event(name), "{name}");
        }
        for name in [
            "vkQueuePresentKHR",
            "route_present_retired",
            "target_fg_frame",
            "vkCreateDevice",
            "vkDestroyDevice",
            "target_fg_failure",
            "target_nvof_fallback",
            "target_nvof_ready",
            "target_nvof_gpu",
        ] {
            assert!(!hot_event(name), "{name}");
        }
    }
}

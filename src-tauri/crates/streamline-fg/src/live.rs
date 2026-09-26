//! Session-local control and measured rates. File I/O never runs on a Vulkan thread.
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex, OnceLock,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
static CONTROL: Mutex<(bool, u64)> = Mutex::new((true, 0));
static APP: AtomicU64 = AtomicU64::new(0);
static NATIVE: AtomicU64 = AtomicU64::new(0);
static LAST: Mutex<Option<(Instant, bool, String, u64)>> = Mutex::new(None);
static SR: Mutex<Option<Value>> = Mutex::new(None);
static SR_CONTROL: OnceLock<Mutex<(u32, u64)>> = OnceLock::new();
fn sr_control() -> &'static Mutex<(u32, u64)> {
    SR_CONTROL.get_or_init(|| Mutex::new((crate::target_sr::initial_mode(), 0)))
}
pub fn sr_requested() -> (u32, u64) {
    *sr_control().lock().unwrap()
}
pub fn sr_applied(mode: u32, revision: u64) {
    let mut status = SR.lock().unwrap();
    let value = status.get_or_insert_with(|| json!({"active":false,"reason":"waiting"}));
    value["mode"] = mode.into();
    value["appliedRevision"] = revision.into();
}
fn apply_sr_control(control: &mut (u32, u64), value: &Value) {
    let mode = match value["srMode"].as_str() {
        Some("off") => 0,
        Some("quality") => 3,
        Some("balanced") => 2,
        Some("performance") => 1,
        Some("dlaa") => 6,
        _ => return,
    };
    if let Some(revision) = value["srRevision"].as_u64() {
        if revision > control.1 {
            *control = (mode, revision);
        }
    }
}
pub fn sr(status: Value) {
    *SR.lock().unwrap() = Some(status);
}
static STOP: AtomicBool = AtomicBool::new(false);
static WORKER: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);
pub fn shutdown() {
    STOP.store(true, Ordering::Release);
    if let Some(worker) = WORKER.lock().unwrap().take() {
        let _ = worker.join();
    }
}
static START: OnceLock<()> = OnceLock::new();
pub fn requested() -> (bool, u64) {
    START.get_or_init(|| {
        if !crate::trace::authorized() {
            return;
        }
        let Some(path) = std::env::var_os("NS_STREAMLINE_LIVE_DIR") else {
            return;
        };
        let dir = std::path::PathBuf::from(path);
        let worker = std::thread::Builder::new()
            .name("fg-live".into())
            .spawn(move || worker(dir));
        *WORKER.lock().unwrap() = worker.ok();
    });
    *CONTROL.lock().unwrap()
}
fn worker(dir: std::path::PathBuf) {
    let mut sample_at = Instant::now();
    let mut samples = std::collections::VecDeque::<Value>::new();
    while !STOP.load(Ordering::Acquire) {
        if let Ok(bytes) = std::fs::read(dir.join("control.json")) {
            if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                apply_control(&mut CONTROL.lock().unwrap(), &v);
                apply_sr_control(&mut sr_control().lock().unwrap(), &v);
            }
        }
        if sample_at.elapsed() >= Duration::from_secs(1) {
            let seconds = sample_at.elapsed().as_secs_f64();
            sample_at = Instant::now();
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let app = APP.swap(0, Ordering::Relaxed) as f64 / seconds;
            let native = NATIVE.swap(0, Ordering::Relaxed) as f64 / seconds;
            let last = LAST.lock().unwrap().clone();
            let fresh = last
                .as_ref()
                .is_some_and(|v| v.0.elapsed() < Duration::from_secs(3));
            samples.push_back(json!({"time":now,"appFps":if fresh {Some(app)} else {None},"presentFps":if fresh {Some(native)} else {None}}));
            while samples.len() > 60 {
                samples.pop_front();
            }
            let (active, reason, applied) =
                last.map(|v| (v.1, v.2, v.3))
                    .unwrap_or((false, "waiting".into(), 0));
            let control = *CONTROL.lock().unwrap();
            let status = json!({"srLiveSupported":crate::target_sr::available(),"sr":SR.lock().unwrap().clone(),"protocol":1,"updatedAt":now,"fresh":fresh,"requested":control.0,"revision":control.1,"appliedRevision":applied,"active":active && fresh,"reason":if fresh {reason} else {"waiting".into()},"samples":samples});
            if let Ok(bytes) = serde_json::to_vec(&status) {
                if std::fs::write(dir.join("telemetry.tmp"), bytes).is_ok() {
                    let _ = std::fs::rename(dir.join("telemetry.tmp"), dir.join("telemetry.json"));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
pub fn frame(on: bool, reason: Option<&str>, revision: u64) {
    APP.fetch_add(1, Ordering::Relaxed);
    *LAST.lock().unwrap() = Some((
        Instant::now(),
        on,
        reason.unwrap_or("active").into(),
        revision,
    ));
}
pub fn native() {
    NATIVE.fetch_add(1, Ordering::Relaxed);
}

fn apply_control(control: &mut (bool, u64), v: &Value) {
    if let (Some(on), Some(revision)) = (v["enabled"].as_bool(), v["revision"].as_u64()) {
        if revision > control.1 {
            *control = (on, revision);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sr_revisions_are_independent_and_invalid_modes_are_ignored() {
        let mut sr = (0, 0);
        let mut fg = (false, 7);
        let value = json!({"srMode":"quality","srRevision":10,"enabled":true,"revision":6});
        apply_sr_control(&mut sr, &value);
        apply_control(&mut fg, &value);
        assert_eq!(sr, (3, 10));
        assert_eq!(fg, (false, 7));
        for value in [
            json!({"srMode":"off","srRevision":9}),
            json!({"srMode":"invalid","srRevision":11}),
            json!({"srMode":"off"}),
        ] {
            apply_sr_control(&mut sr, &value);
            assert_eq!(sr, (3, 10));
        }
        apply_sr_control(&mut sr, &json!({"srMode":"off","srRevision":11}));
        assert_eq!(sr, (0, 11));
        apply_sr_control(&mut sr, &json!({"srMode":"dlaa","srRevision":12}));
        assert_eq!(sr, (6, 12));
    }
    #[test]
    fn stale_or_incomplete_commands_cannot_reverse_manual_off() {
        let mut state = (true, 0);
        apply_control(&mut state, &json!({"enabled":false,"revision":7}));
        for value in [
            json!({"enabled":true,"revision":6}),
            json!({"enabled":true,"revision":7}),
            json!({"enabled":"true","revision":8}),
            json!({"revision":8}),
        ] {
            apply_control(&mut state, &value);
            assert_eq!(state, (false, 7));
        }
        apply_control(&mut state, &json!({"enabled":true,"revision":8}));
        assert_eq!(state, (true, 8));
    }
}

//! Session-local control and measured rates. File I/O never runs on a Vulkan thread.
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
static CONTROL: Mutex<(bool, u64)> = Mutex::new((true, 0));
static APP: AtomicU64 = AtomicU64::new(0);
static NATIVE: AtomicU64 = AtomicU64::new(0);
static LAST: Mutex<Option<(Instant, bool, String, u64)>> = Mutex::new(None);
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
        let _ = std::thread::Builder::new()
            .name("fg-live".into())
            .spawn(move || worker(dir));
    });
    *CONTROL.lock().unwrap()
}
fn worker(dir: std::path::PathBuf) {
    let mut sample_at = Instant::now();
    let mut samples = std::collections::VecDeque::<Value>::new();
    loop {
        if let Ok(bytes) = std::fs::read(dir.join("control.json")) {
            if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                apply_control(&mut CONTROL.lock().unwrap(), &v);
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
            let status = json!({"protocol":1,"updatedAt":now,"fresh":fresh,"requested":control.0,"revision":control.1,"appliedRevision":applied,"active":active && fresh,"reason":if fresh {reason} else {"waiting".into()},"samples":samples});
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

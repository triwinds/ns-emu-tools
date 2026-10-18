//! Small, process-owned live snapshot. File installation never implies active AA/FG.
use serde_json::{json, Value};
use std::{
    sync::{Mutex, OnceLock},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

struct State {
    value: Value,
    last_write: Instant,
    sample: Instant,
    source: u64,
    queued: u64,
}
static STATE: OnceLock<Mutex<State>> = OnceLock::new();
pub(super) fn event(name: &str, details: &Value) {
    if !matches!(
        name,
        "xess_input_scale"
            | "xess_native_frame"
            | "xess_fg_frame"
            | "xess_native_failure"
            | "xess_fg_failure"
            | "xess_fg_runtime"
    ) {
        return;
    }
    let mut state = STATE.get_or_init(|| Mutex::new(State {
        value: json!({"schemaVersion":1,"nativeAaRequested":false,"nativeAaActive":false,"fgRequested":false,"fgActive":false,"fgFramesQueued":0,"failed":false}),
        last_write: Instant::now(), sample: Instant::now(), source:0, queued:0,
    })).lock().unwrap();
    let mut urgent = false;
    match name {
        "xess_input_scale" => {
            state.value["inputScale"] = details.clone();
        }
        "xess_native_frame" => {
            state.value["nativeAaRequested"] = details["requested"].clone();
            state.value["nativeAaActive"] = details["active"].clone();
            if std::env::var("NS_EMU_XESS_FG").as_deref() != Ok("1") {
                state.source += 1;
                state.queued += 1;
            }
        }
        "xess_fg_frame" => {
            state.value["fgRequested"] = details["enabled"].clone();
            state.value["fgActive"] = json!(
                details["enabled"] == true
                    && details["fg_result"] == 0
                    && details["frames_queued"] == 2
            );
            state.value["fgFramesQueued"] = details["frames_queued"].clone();
            state.value["fgResult"] = details["fg_result"].clone();
            state.source += 1;
            state.queued += details["frames_queued"].as_u64().unwrap_or(0);
        }
        "xess_native_failure" | "xess_fg_failure" => urgent = true,
        "xess_fg_runtime" => {
            urgent = matches!(
                details["event"].as_str(),
                Some("fatal" | "game_runtime_failure")
            )
        }
        _ => {}
    }
    if urgent {
        state.value["failed"] = json!(true);
        state.value["error"] = details.clone();
    }
    if state.last_write.elapsed().as_secs_f64() < 1.0 && !urgent {
        return;
    }
    let seconds = state.sample.elapsed().as_secs_f64();
    if state.source > 0 && seconds > 0.0 {
        state.value["sourceFps"] = json!(state.source as f64 / seconds);
        state.value["sdkQueuedFps"] = json!(state.queued as f64 / seconds);
        state.sample = Instant::now();
        state.source = 0;
        state.queued = 0;
    }
    state.value["lastUpdateMs"] = json!(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis());
    if let Some(dir) = std::env::var_os("NS_STREAMLINE_LIVE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        let temp = dir.join("xess-status.tmp");
        if std::fs::write(&temp, state.value.to_string()).is_ok() {
            let _ = std::fs::rename(temp, dir.join("xess-status.json"));
        }
    }
    state.last_write = Instant::now();
}

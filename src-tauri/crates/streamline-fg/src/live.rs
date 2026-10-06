//! Session-local control and measured rates. File I/O never runs on a Vulkan thread.
use crate::advanced_settings::{AdvancedSettings, FgOptions, InputSizing, SrOptions};
use crate::sr_preset::StreamlineSrPreset;
use serde_json::{json, Value};
type SrControl = (u32, u64, u16, StreamlineSrPreset, SrOptions);
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex, OnceLock,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
static CONTROL: Mutex<(bool, u64)> = Mutex::new((true, 0));
static FG_OPTIONS: OnceLock<Mutex<FgOptions>> = OnceLock::new();
fn initial_advanced() -> AdvancedSettings {
    static INITIAL: OnceLock<AdvancedSettings> = OnceLock::new();
    *INITIAL.get_or_init(|| {
        std::env::var("NS_STREAMLINE_ADVANCED_SETTINGS")
            .ok()
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default()
    })
}
fn fg_options() -> &'static Mutex<FgOptions> {
    FG_OPTIONS.get_or_init(|| Mutex::new(initial_advanced().fg))
}
static APP: AtomicU64 = AtomicU64::new(0);
static NATIVE: AtomicU64 = AtomicU64::new(0);
static LAST: Mutex<Option<(Instant, bool, String, u64)>> = Mutex::new(None);
static SR: Mutex<Option<Value>> = Mutex::new(None);
static INPUT_SCALE: Mutex<Option<Value>> = Mutex::new(None);
static INPUT_CONTROL: OnceLock<Mutex<(InputSizing, u64)>> = OnceLock::new();
fn input_control() -> &'static Mutex<(InputSizing, u64)> {
    INPUT_CONTROL.get_or_init(|| {
        let sizing = InputSizing {
            scale_percent: std::env::var("NS_STREAMLINE_INPUT_SCALE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100),
            max_edge: std::env::var("NS_STREAMLINE_INPUT_MAX_EDGE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
        };
        Mutex::new((
            if sizing.valid() {
                sizing
            } else {
                InputSizing::default()
            },
            0,
        ))
    })
}
fn apply_input_control(control: &mut (InputSizing, u64), value: &Value) {
    let Some(revision) = value["inputScaleRevision"]
        .as_u64()
        .filter(|r| *r > control.1)
    else {
        return;
    };
    let Ok(sizing) = serde_json::from_value::<InputSizing>(value["inputSizing"].clone()) else {
        return;
    };
    if sizing.valid() {
        *control = (sizing, revision);
    }
}
pub fn input_scale(status: Value) {
    *INPUT_SCALE.lock().unwrap() = Some(status);
}
static NR: Mutex<Option<Value>> = Mutex::new(None);
static FG: Mutex<Option<(Instant, Value)>> = Mutex::new(None);
#[cfg(all(windows, feature = "sdk-bridge"))]
pub fn fg(status: Value) {
    *FG.lock().unwrap() = Some((Instant::now(), status));
}
static NR_CONTROL: OnceLock<Mutex<(crate::nr_history::Controls, u64)>> = OnceLock::new();
static CONTROL_UPDATE: Mutex<()> = Mutex::new(());
pub fn frame_controls() -> (
    (bool, u64, FgOptions),
    SrControl,
    (crate::nr_history::Controls, u64),
    (InputSizing, u64),
) {
    requested(); // Start the worker before taking its update lock.
    let _update = CONTROL_UPDATE.lock().unwrap();
    let control = *CONTROL.lock().unwrap();
    (
        (control.0, control.1, *fg_options().lock().unwrap()),
        sr_requested(),
        nr_requested(),
        *input_control().lock().unwrap(),
    )
}
fn nr_control() -> &'static Mutex<(crate::nr_history::Controls, u64)> {
    NR_CONTROL.get_or_init(|| {
        Mutex::new((
            crate::nr_history::Controls {
                enabled: cfg!(feature = "native-nr")
                    && std::env::var("NS_STREAMLINE_NATIVE_NR").as_deref() == Ok("1")
                    && std::env::var("NS_STREAMLINE_NR_INITIAL_ENABLED").as_deref() != Ok("0"),
                intensity: std::env::var("NS_STREAMLINE_NR_INTENSITY")
                    .ok()
                    .and_then(|v| v.parse::<f32>().ok())
                    .filter(|v| v.is_finite() && (0.0..=2.0).contains(v))
                    .unwrap_or(1.0),
                options: initial_advanced().nr,
            },
            0,
        ))
    })
}
pub fn nr_requested() -> (crate::nr_history::Controls, u64) {
    *nr_control().lock().unwrap()
}
#[cfg(feature = "native-nr")]
pub fn nr(status: Value) {
    *NR.lock().unwrap() = Some(status);
}
fn apply_nr_control(control: &mut (crate::nr_history::Controls, u64), v: &Value) {
    let (Some(enabled), Some(revision)) = (v["nrEnabled"].as_bool(), v["nrRevision"].as_u64())
    else {
        return;
    };
    if revision <= control.1 {
        return;
    }
    let intensity = match v.get("nrIntensity") {
        None => control.0.intensity,
        Some(v) => match v
            .as_f64()
            .filter(|v| v.is_finite() && (0.0..=2.0).contains(v))
        {
            Some(v) => v as f32,
            None => return,
        },
    };
    let options = match v.get("nrOptions") {
        None => control.0.options,
        Some(v) => match serde_json::from_value(v.clone()) {
            Ok(v) => v,
            Err(_) => return,
        },
    };
    *control = (
        crate::nr_history::Controls {
            enabled,
            intensity,
            options,
        },
        revision,
    );
}
static SR_CONTROL: OnceLock<Mutex<SrControl>> = OnceLock::new();
fn sr_control() -> &'static Mutex<SrControl> {
    SR_CONTROL.get_or_init(|| {
        Mutex::new((
            crate::target_sr::initial_mode(),
            0,
            crate::target_sr::initial_scale(),
            crate::target_sr::initial_preset(),
            initial_advanced().sr,
        ))
    })
}
pub fn sr_requested() -> SrControl {
    *sr_control().lock().unwrap()
}
pub fn sr_applied(mode: u32, revision: u64, preset: StreamlineSrPreset, options: SrOptions) {
    let mut status = SR.lock().unwrap();
    let value = status.get_or_insert_with(|| json!({"active":false,"reason":"waiting"}));
    value["mode"] = mode.into();
    value["preset"] = preset.as_str().into();
    value["appliedRevision"] = revision.into();
    value["options"] = json!(options);
}
fn apply_sr_control(control: &mut SrControl, value: &Value) {
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
            let scale = match value.get("srScale") {
                None => control.2,
                Some(v) => match v.as_u64().filter(|v| (50..=200).contains(v)) {
                    Some(v) => v as u16,
                    None => return,
                },
            };
            let preset = match value.get("srPreset") {
                None => control.3,
                Some(v) => match v.as_str().and_then(StreamlineSrPreset::parse) {
                    Some(p) => p,
                    None => return,
                },
            };
            let options = match value.get("srOptions") {
                None => control.4,
                Some(v) => match serde_json::from_value(v.clone()) {
                    Ok(v) => v,
                    Err(_) => return,
                },
            };
            *control = (mode, revision, scale, preset, options);
        }
    }
}
pub fn sr(status: Value) {
    update_sr_status(&mut SR.lock().unwrap(), status);
}
fn update_sr_status(previous: &mut Option<Value>, mut status: Value) {
    // Runtime status is published before end-of-frame confirmation. Preserve
    // the last completed revision until sr_applied confirms the next frame.
    if let Some(value) = previous.as_ref() {
        for field in ["appliedRevision", "mode", "preset", "options"] {
            if status.get(field).is_none() {
                if let Some(value) = value.get(field) {
                    status[field] = value.clone();
                }
            }
        }
    }
    *previous = Some(status);
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
        *CONTROL.lock().unwrap() = (
            std::env::var("NS_STREAMLINE_TARGET_FG").as_deref() == Ok("1"),
            0,
        );
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
                #[cfg(all(windows, feature = "sdk-bridge"))]
                crate::frame_capture::request(&v);
                if v["sourceTrackingStop"] == true {
                    crate::source_auto::stop_measurement_tracking();
                }
                let tracking = {
                    let _update = CONTROL_UPDATE.lock().unwrap();
                    apply_fg_control(
                        &mut CONTROL.lock().unwrap(),
                        &mut fg_options().lock().unwrap(),
                        &v,
                    );
                    apply_sr_control(&mut sr_control().lock().unwrap(), &v);
                    apply_nr_control(&mut nr_control().lock().unwrap(), &v);
                    apply_input_control(&mut input_control().lock().unwrap(), &v);
                    sr_requested().0 != 0
                        || CONTROL.lock().unwrap().0
                        || nr_requested().0.enabled
                        || input_control().lock().unwrap().0 != InputSizing::default()
                };
                crate::source_auto::set_requested(tracking);
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
            let status = json!({"srPresetSupported":true,"srScaleSupported":true,"srScaleBasis":"source_output","srLiveSupported":crate::target_sr::available(),"sr":SR.lock().unwrap().clone(),"protocol":1,"updatedAt":now,"fresh":fresh,"requested":control.0,"revision":control.1,"appliedRevision":applied,"active":active && fresh,"reason":if fresh {reason} else {"waiting".into()},"samples":samples});
            let mut status = status;
            status["advancedSettingsSupported"] = json!(true);
            status["inputScalingSupported"] = json!(true);
            status["inputScalingLiveSupported"] = json!(true);
            status["inputScale"] = INPUT_SCALE
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(serde_json::Value::Null);
            status["nrLookSupported"] = json!(cfg!(feature = "native-nr"));
            status["nrSpatialLookSupported"] = json!(cfg!(feature = "native-nr"));
            status["nrTemporalLookSupported"] = json!(cfg!(feature = "native-nr"));
            status["nrLookScopeSupported"] = json!(cfg!(feature = "native-nr"));
            status["nrTemporalModesSupported"] = json!(cfg!(feature = "native-nr"));
            status["nrPersistenceSupported"] = json!(cfg!(feature = "native-nr"));
            status["nrLookExperimentsSupported"] = json!(cfg!(feature = "native-nr"));
            status["nrTwoPassSupported"] = json!(cfg!(feature = "native-nr"));
            status["nrConsolidatedSupported"] = json!(cfg!(feature = "native-nr"));
            status["nrInferenceScaleSupported"] = json!(cfg!(feature = "native-nr"));
            status["nrInferenceCapSupported"] = json!(cfg!(feature = "native-nr"));
            status["fg"] = FG
                .lock()
                .unwrap()
                .as_ref()
                .filter(|(time, _)| time.elapsed() < Duration::from_secs(1))
                .map(|(_, value)| value.clone())
                .unwrap_or(json!({"state":"waiting","generationObserved":false}));
            status["nrLiveSupported"] = json!(
                cfg!(feature = "native-nr")
                    && std::env::var("NS_STREAMLINE_NATIVE_NR").as_deref() == Ok("1")
            );
            status["nr"] = NR
                .lock()
                .unwrap()
                .clone()
                .unwrap_or(json!({"active":false,"reason":"waiting"}));
            if crate::source_auto::measuring() {
                status["sourceMeasurement"] = crate::source_auto::measurement();
            }
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
fn apply_fg_control(control: &mut (bool, u64), options: &mut FgOptions, v: &Value) {
    if v["revision"]
        .as_u64()
        .is_none_or(|revision| revision <= control.1)
        || v["enabled"].as_bool().is_none()
    {
        return;
    }
    let next = match v.get("fgOptions") {
        None => *options,
        Some(value) => match serde_json::from_value(value.clone()) {
            Ok(v) => v,
            Err(_) => return,
        },
    };
    apply_control(control, v);
    *options = next;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advanced_updates_are_atomic_and_legacy_commands_keep_tuning() {
        let mut fg = (true, 1);
        let mut options = FgOptions::default();
        apply_fg_control(
            &mut fg,
            &mut options,
            &json!({"enabled":false,"revision":2,"fgOptions":{"multiplier":7}}),
        );
        assert_eq!(fg, (true, 1));
        assert_eq!(options, FgOptions::default());
        apply_fg_control(
            &mut fg,
            &mut options,
            &json!({"enabled":true,"revision":2,"fgOptions":{"mode":"dynamic","multiplier":4,"targetFps":144,"reflex":"boost","inputFps":60}}),
        );
        assert_eq!(
            (
                fg,
                options.multiplier,
                options.target_fps,
                options.frame_limit_us()
            ),
            ((true, 2), 4, 144, 16667)
        );
        apply_fg_control(
            &mut fg,
            &mut options,
            &json!({"enabled":false,"revision":3}),
        );
        assert_eq!(fg, (false, 3));
        assert_eq!(options.multiplier, 4);
        let mut sr = (2, 1, 172, StreamlineSrPreset::J, SrOptions::default());
        apply_sr_control(
            &mut sr,
            &json!({"srMode":"off","srRevision":2,"srOptions":{"exposure":401}}),
        );
        assert_eq!(sr.1, 1);
        apply_sr_control(
            &mut sr,
            &json!({"srMode":"quality","srRevision":2,"srOptions":{"autoExposure":false,"exposure":50}}),
        );
        assert_eq!(
            (sr.0, sr.1, sr.4),
            (
                3,
                2,
                SrOptions {
                    auto_exposure: false,
                    exposure: 50
                }
            )
        );
        let mut nr = (
            crate::nr_history::Controls {
                enabled: true,
                intensity: 1.0,
                options: Default::default(),
            },
            1,
        );
        apply_nr_control(
            &mut nr,
            &json!({"nrEnabled":false,"nrRevision":2,"nrOptions":{"style":"d"}}),
        );
        assert!(nr.0.enabled && nr.1 == 1);
        apply_nr_control(
            &mut nr,
            &json!({"nrEnabled":true,"nrRevision":2,"nrIntensity":2.0,"nrOptions":{"style":"b","localStructure":200,"autoMask":true}}),
        );
        assert_eq!(
            (nr.1, nr.0.intensity, nr.0.options.style),
            (2, 2.0, crate::advanced_settings::NrStyle::B)
        );
    }
    #[test]
    fn runtime_status_preserves_only_the_last_completed_sr_revision() {
        let mut status = Some(
            json!({"active":true,"appliedRevision":17,"mode":6,"preset":"k","input":[100,100]}),
        );
        update_sr_status(&mut status, json!({"active":false,"reason":"disabled"}));
        let value = status.as_ref().unwrap();
        assert_eq!(value["appliedRevision"], 17);
        assert_eq!(value["mode"], 6);
        assert_eq!(value["preset"], "k");
        assert_eq!(value["active"], false);
        assert!(value.get("input").is_none());
        update_sr_status(
            &mut status,
            json!({"active":false,"mode":0,"appliedRevision":18,"preset":"default"}),
        );
        update_sr_status(&mut status, json!({"active":true,"mode":6}));
        assert_eq!(status.as_ref().unwrap()["appliedRevision"], 18);
        assert_eq!(status.as_ref().unwrap()["mode"], 6);
    }
    #[test]
    fn look_live_updates_reject_invalid_imports_without_advancing_revision() {
        let mut nr = (
            crate::nr_history::Controls {
                enabled: true,
                intensity: 0.75,
                options: Default::default(),
            },
            1,
        );
        apply_nr_control(
            &mut nr,
            &json!({"nrEnabled":true,"nrRevision":2,"nrOptions":{"look":{"amount":50,"hue":0}}}),
        );
        assert_eq!(nr.0.options.look.amount, 50);
        assert_eq!(nr.0.options.look.hue, 0);
        assert_eq!(nr.1, 2);
        let previous = nr;
        for look in [
            json!({"schemaVersion":2}),
            json!({"brightenCap":1601}),
            json!({"amount":201}),
        ] {
            apply_nr_control(
                &mut nr,
                &json!({"nrEnabled":false,"nrRevision":3,"nrIntensity":1.5,"nrOptions":{"look":look}}),
            );
            assert_eq!(nr, previous);
        }
        apply_nr_control(&mut nr, &json!({"nrEnabled":false,"nrRevision":3}));
        assert_eq!(nr.0.options.look, previous.0.options.look);
        assert_eq!(nr.1, 3);
    }
    #[test]
    fn nr_controls_are_independent_atomic_and_reject_stale_or_invalid_strength() {
        let mut nr = (
            crate::nr_history::Controls {
                enabled: true,
                intensity: 0.5,
                options: Default::default(),
            },
            4,
        );
        let mut fg = (false, 7);
        apply_nr_control(
            &mut nr,
            &json!({"nrEnabled":false,"nrIntensity":0.25,"nrRevision":5,"enabled":true,"revision":6}),
        );
        assert!(!nr.0.enabled && nr.0.intensity == 0.25 && nr.1 == 5);
        apply_control(&mut fg, &json!({"nrEnabled":true,"nrRevision":6}));
        assert_eq!(fg, (false, 7));
        for value in [
            json!({"nrEnabled":true,"nrRevision":4}),
            json!({"nrEnabled":true,"nrRevision":6,"nrIntensity":2.1}),
            json!({"nrEnabled":true,"nrRevision":6,"nrIntensity":"1"}),
            json!({"nrEnabled":true}),
        ] {
            apply_nr_control(&mut nr, &value);
            assert_eq!(
                nr,
                (
                    crate::nr_history::Controls {
                        enabled: false,
                        intensity: 0.25,
                        options: Default::default(),
                    },
                    5
                )
            );
        }
        apply_nr_control(&mut nr, &json!({"nrEnabled":true,"nrRevision":6}));
        assert_eq!(
            nr,
            (
                crate::nr_history::Controls {
                    enabled: true,
                    intensity: 0.25,
                    options: Default::default(),
                },
                6
            )
        );
    }
    #[test]
    fn preset_updates_are_atomic_and_older_controls_preserve_selection() {
        let mut control = (6, 10, 100, StreamlineSrPreset::K, SrOptions::default());
        apply_sr_control(
            &mut control,
            &json!({"srMode":"dlaa","srPreset":"j","srRevision":11}),
        );
        assert_eq!(
            control,
            (6, 11, 100, StreamlineSrPreset::J, SrOptions::default())
        );
        for preset in [json!("invalid"), json!(10), json!(null)] {
            apply_sr_control(
                &mut control,
                &json!({"srMode":"off","srPreset":preset,"srScale":150,"srRevision":12}),
            );
            assert_eq!(
                control,
                (6, 11, 100, StreamlineSrPreset::J, SrOptions::default())
            );
        }
        apply_sr_control(&mut control, &json!({"srMode":"off","srRevision":12}));
        assert_eq!(
            control,
            (0, 12, 100, StreamlineSrPreset::J, SrOptions::default())
        );
        apply_sr_control(
            &mut control,
            &json!({"srMode":"dlaa","srPreset":"k","srRevision":11}),
        );
        assert_eq!(
            control,
            (0, 12, 100, StreamlineSrPreset::J, SrOptions::default())
        );
    }
    #[test]
    fn scale_revisions_are_atomic_and_reject_invalid_values() {
        let mut control = (3, 1, 150, StreamlineSrPreset::Default, SrOptions::default());
        for scale in [json!(49), json!(201), json!(-1), json!(1.5), json!("150")] {
            apply_sr_control(
                &mut control,
                &json!({"srMode":"quality","srScale":scale,"srRevision":2}),
            );
            assert_eq!(
                control,
                (3, 1, 150, StreamlineSrPreset::Default, SrOptions::default())
            );
        }
        apply_sr_control(
            &mut control,
            &json!({"srMode":"quality","srScale":50,"srRevision":2}),
        );
        assert_eq!(
            control,
            (3, 2, 50, StreamlineSrPreset::Default, SrOptions::default())
        );
        apply_sr_control(
            &mut control,
            &json!({"srMode":"quality","srScale":200,"srRevision":3}),
        );
        assert_eq!(
            control,
            (3, 3, 200, StreamlineSrPreset::Default, SrOptions::default())
        );
    }
    #[test]
    fn sr_revisions_are_independent_and_invalid_modes_are_ignored() {
        let mut sr = (0, 0, 150, StreamlineSrPreset::Default, SrOptions::default());
        let mut fg = (false, 7);
        let value = json!({"srMode":"quality","srRevision":10,"enabled":true,"revision":6});
        apply_sr_control(&mut sr, &value);
        apply_control(&mut fg, &value);
        assert_eq!(
            sr,
            (
                3,
                10,
                150,
                StreamlineSrPreset::Default,
                SrOptions::default()
            )
        );
        assert_eq!(fg, (false, 7));
        for value in [
            json!({"srMode":"off","srRevision":9}),
            json!({"srMode":"invalid","srRevision":11}),
            json!({"srMode":"off"}),
        ] {
            apply_sr_control(&mut sr, &value);
            assert_eq!(
                sr,
                (
                    3,
                    10,
                    150,
                    StreamlineSrPreset::Default,
                    SrOptions::default()
                )
            );
        }
        apply_sr_control(&mut sr, &json!({"srMode":"off","srRevision":11}));
        assert_eq!(
            sr,
            (
                0,
                11,
                150,
                StreamlineSrPreset::Default,
                SrOptions::default()
            )
        );
        apply_sr_control(&mut sr, &json!({"srMode":"dlaa","srRevision":12}));
        assert_eq!(
            sr,
            (
                6,
                12,
                150,
                StreamlineSrPreset::Default,
                SrOptions::default()
            )
        );
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

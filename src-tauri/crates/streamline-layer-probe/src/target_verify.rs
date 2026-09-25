//! Evidence gate for bounded target activation; never asserts display-quality acceptance.
use crate::host::Result;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufRead, BufReader},
    path::Path,
};

fn analyze(rows: &[Value], fg: bool, sdk: bool) -> Result<Value> {
    let events = |name: &str| {
        rows.iter()
            .filter(move |r| r["event"] == name)
            .collect::<Vec<_>>()
    };
    if rows.iter().any(|r| {
        matches!(
            r["event"].as_str(),
            Some("target_fg_failure" | "target_sdk_error")
        )
    }) {
        return Err("target recorded an SDK/FG failure".into());
    }
    let app = events("vkQueuePresentKHR");
    let native = events("route_present_retired");
    let frames = events("target_fg_frame");
    let threads: HashSet<_> = app.iter().filter_map(|r| r["thread"].as_str()).collect();
    let workers = native
        .iter()
        .filter(|r| !threads.contains(r["thread"].as_str().unwrap_or("")))
        .count();
    for (event, field) in [
        ("vkDestroyDevice", "remaining_devices"),
        ("vkDestroyInstance", "remaining_instances"),
    ] {
        if events(event)
            .last()
            .is_none_or(|r| r["details"][field] != 0)
        {
            return Err(format!("missing clean {event}").into());
        }
    }
    if app.is_empty() {
        return Err("no game presents".into());
    }
    if sdk
        && (events("target_sdk_shutdown")
            .last()
            .is_none_or(|r| r["details"]["result"] != 0)
            || native.is_empty()
            || native.iter().any(|r| {
                r["details"]["fence_wait_succeeded"] != true || r["details"]["result"] != 0
            }))
    {
        return Err("SDK shutdown or native present completion failed".into());
    }
    let on = frames
        .iter()
        .filter(|r| r["details"]["requested_on"] == true)
        .count();
    let doubled = frames
        .iter()
        .filter(|r| r["details"]["state"]["presented"] == 2)
        .count();
    let completion = frames
        .iter()
        .filter_map(|r| r["details"]["state"]["value"].as_u64())
        .max()
        .unwrap_or(0);
    // Timeline values are comparable only within the same swapchain and semaphore.
    let mut timelines = HashMap::new();
    let mut input_advances = 0;
    for row in &frames {
        let d = &row["details"];
        let value = d["state"]["value"]
            .as_u64()
            .ok_or("missing input timeline value")?;
        if value == 0 {
            continue;
        }
        let fence = d["state"]["fence"]
            .as_u64()
            .filter(|v| *v != 0)
            .ok_or("input completion without semaphore")?;
        let swapchain = d["swapchain"].as_u64().ok_or("missing input swapchain")?;
        if let Some(previous) = timelines.insert((swapchain, fence), value) {
            if value < previous {
                return Err("input timeline regressed".into());
            }
            if value > previous {
                input_advances += 1;
            }
        }
        if d["input_wait_completed"] == false {
            return Err("input wait failed".into());
        }
    }
    if fg
        && (frames.len() != app.len()
            || on == 0
            || doubled == 0
            || completion == 0
            || input_advances == 0
            || workers == 0
            || native.len() <= app.len()
            || frames
                .last()
                .is_none_or(|r| r["details"]["requested_on"] != false)
            || frames.iter().any(|r| r["details"]["state"]["status"] != 0)
            || events("target_fg_retired")
                .iter()
                .filter_map(|r| r["details"]["on_frames"].as_u64())
                .sum::<u64>()
                != on as u64)
    {
        return Err(
            "FG activation, worker routing, input completion, Off or retirement evidence missing"
                .into(),
        );
    }
    if sdk && !fg && native.len() != app.len() {
        return Err("FG-off present count mismatch".into());
    }
    Ok(
        json!({"bounded_activation_verified":fg,"application_presents":app.len(),"native_presents":native.len(),
        "native_worker_presents":workers,"requested_on_frames":on,"sdk_x2_reports":doubled,"maximum_input_completion":completion,"input_timeline_advances":input_advances,
        "native_present_fences_completed":sdk,"clean_shutdown":true,"p4_passed":false,
        "visual_intermediate_frames_verified":false,"scanout_cadence_verified":false,"latency_verified":false,
        "validation_layer_enabled":false}),
    )
}

pub(super) fn verify(session: &Path) -> Result<()> {
    let inputs: Value = serde_json::from_slice(&fs::read(session.join("target-inputs.json"))?)?;
    let exit: Value = serde_json::from_slice(&fs::read(session.join("target-exit.json"))?)?;
    if exit["success"] != true {
        return Err("target process did not exit successfully".into());
    }
    let fg = inputs["fg_experiment_requested"] == true;
    let sdk = inputs["layer_only"] == false;
    let mut rows = Vec::new();
    for line in BufReader::new(fs::File::open(session.join("layer.jsonl"))?).lines() {
        let row: Value = serde_json::from_str(&line?)?;
        if matches!(
            row["event"].as_str(),
            Some(
                "vkQueuePresentKHR"
                    | "route_present_retired"
                    | "target_fg_frame"
                    | "target_fg_failure"
                    | "target_sdk_error"
                    | "target_sdk_shutdown"
                    | "target_fg_retired"
                    | "vkDestroyDevice"
                    | "vkDestroyInstance"
            )
        ) {
            rows.push(row);
        }
    }
    let mut result = analyze(&rows, fg, sdk)?;
    if sdk {
        let log = fs::read_to_string(session.join("runtime/sl.log"))?;
        if log.lines().any(|l| l.contains("[error]")) {
            return Err("SDK logged an error".into());
        }
        result["sdk_warning_lines"] = json!(log.lines().filter(|l| l.contains("[warn]")).count());
    }
    // This is a derived report; explicit --verify may refresh it after stricter checks.
    fs::write(
        session.join("target-result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("{result}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn valid() -> Vec<Value> {
        let mut rows = Vec::new();
        for _ in 0..3 {
            rows.push(json!({"event":"vkQueuePresentKHR","thread":"app"}));
        }
        for _ in 0..5 {
            rows.push(json!({"event":"route_present_retired","thread":"worker","details":{"result":0,"fence_wait_succeeded":true}}));
        }
        for (on, value) in [(true, 1), (true, 2), (false, 2)] {
            rows.push(json!({"event":"target_fg_frame","details":{"swapchain":1,"requested_on":on,"input_wait_completed":true,"state":{"status":0,"presented":if on {2} else {1},"fence":100,"value":value}}}));
        }
        rows.extend([
            json!({"event":"target_fg_retired","details":{"on_frames":2}}),
            json!({"event":"target_sdk_shutdown","details":{"result":0}}),
            json!({"event":"vkDestroyDevice","details":{"remaining_devices":0}}),
            json!({"event":"vkDestroyInstance","details":{"remaining_instances":0}}),
        ]);
        rows
    }
    #[test]
    fn rejects_counter_only_activation_and_missing_cleanup() {
        let rows = valid();
        assert!(analyze(&rows, true, true).is_ok());
        for event in [
            "route_present_retired",
            "target_fg_frame",
            "target_fg_retired",
            "target_sdk_shutdown",
            "vkDestroyDevice",
            "vkDestroyInstance",
        ] {
            let missing = rows
                .iter()
                .filter(|r| r["event"] != event)
                .cloned()
                .collect::<Vec<_>>();
            assert!(analyze(&missing, true, true).is_err(), "{event}");
        }
        let mut altered = rows.clone();
        for row in &mut altered {
            row["thread"] = json!("app");
        }
        assert!(analyze(&altered, true, true).is_err());
        let mut altered = rows.clone();
        altered
            .iter_mut()
            .find(|r| r["event"] == "route_present_retired")
            .unwrap()["details"]["fence_wait_succeeded"] = json!(false);
        assert!(analyze(&altered, true, true).is_err());
        let mut altered = rows;
        altered
            .iter_mut()
            .rev()
            .find(|r| r["event"] == "target_fg_frame")
            .unwrap()["details"]["requested_on"] = json!(true);
        assert!(analyze(&altered, true, true).is_err());
    }
    #[test]
    fn rejects_stalled_regressed_or_missing_input_completion() {
        for values in [[0, 0, 0], [1, 1, 1], [2, 1, 3]] {
            let mut rows = valid();
            for (r, v) in rows
                .iter_mut()
                .filter(|r| r["event"] == "target_fg_frame")
                .zip(values)
            {
                r["details"]["state"]["value"] = json!(v);
            }
            assert!(analyze(&rows, true, true).is_err(), "{values:?}");
        }
        for field in ["fence", "value"] {
            let mut rows = valid();
            for r in rows.iter_mut().filter(|r| r["event"] == "target_fg_frame") {
                r["details"]["state"][field] = Value::Null;
            }
            assert!(analyze(&rows, true, true).is_err(), "{field}");
        }
        let mut rows = valid();
        rows.iter_mut()
            .find(|r| r["event"] == "target_fg_frame")
            .unwrap()["details"]["input_wait_completed"] = json!(false);
        assert!(analyze(&rows, true, true).is_err());
    }
}

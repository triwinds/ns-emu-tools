//! Evidence gate for bounded target activation; never asserts display-quality acceptance.
use crate::support::Result;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufRead, BufReader},
    path::Path,
};
#[path = "nr_combinations.rs"]
mod nr_combinations;
#[path = "nr_look_evidence.rs"]
mod nr_look_evidence;
#[path = "nr_pass_evidence.rs"]
mod nr_pass_evidence;

// NR execution and full game acceptance are separate gates. Keep evidence even
// when the emulator or the shared SDK reports a validation error.
fn analyze_temporal(frames: &[&Value]) -> Value {
    let source_frames = frames.iter().any(|r| {
        !r["details"]["source_frame_basis"].is_null() || !r["details"]["source_frame_id"].is_null()
    });
    let checked = frames
        .iter()
        .any(|r| !r["details"]["source_identity"].is_null());
    let mut valid = true;
    let mut previous: Option<&Value> = None;
    let mut continuous = 0;
    let mut longest = 0;
    let mut count = 0;
    let mut rotations = 0;
    let mut identities = HashSet::new();
    let mut allocations = HashMap::new();
    for row in frames {
        let d = &row["details"];
        if d["evaluated"] == true {
            if source_frames {
                valid &= d["active"] == true
                    && d["output_reused"] == false
                    && d["source_frame_basis"] == "exact_nr_gamma_input"
                    && d["source_frame_id"].as_u64().is_some_and(|id| id != 0);
                if let Some(old) = previous.filter(|old| {
                    old["source_identity"] == d["source_identity"]
                        && old["mapping"] == d["mapping"]
                        && (old["evaluated"] == true || old["output_reused"] == true)
                }) {
                    valid &= old["source_frame_id"]
                        .as_u64()
                        .and_then(|id| id.checked_add(1))
                        == d["source_frame_id"].as_u64();
                }
            }
            let identity = d["source_identity"].as_u64().filter(|v| *v != 0);
            if checked {
                valid &= identity.is_some();
            }
            if let Some(id) = identity {
                identities.insert(id);
                if let (Some(image), Some(generation)) =
                    (d["source_image"].as_u64(), d["source_generation"].as_u64())
                {
                    valid &= image != 0 && generation != 0;
                    if let Some(old) = allocations.insert((id, image), generation) {
                        valid &= old == generation;
                    }
                }
            }
            if d["reset"] == false {
                count += 1;
                continuous += 1;
                longest = longest.max(continuous);
                if checked {
                    valid &= matches!(
                        d["reset_reason"].as_str(),
                        Some("Continuous" | "LookChanged" | "SecondPassChanged")
                    ) && previous.is_some_and(|old| {
                        (old["evaluated"] == true || old["output_reused"] == true)
                            && if d["source_frame_id"].is_u64() {
                                old["source_frame_id"]
                                    .as_u64()
                                    .and_then(|f| f.checked_add(1))
                                    == d["source_frame_id"].as_u64()
                            } else {
                                old["frame"].as_u64().and_then(|f| f.checked_add(1))
                                    == d["frame"].as_u64()
                            }
                            && old["source_identity"] == d["source_identity"]
                            && old["mapping"] == d["mapping"]
                            && old["input"] == d["input"]
                            && if d["applied_intensity"].is_number() {
                                old["applied_intensity"] == d["applied_intensity"]
                            } else {
                                old["intensity"] == d["intensity"]
                            }
                    });
                    rotations += usize::from(
                        previous.is_some_and(|old| old["source_image"] != d["source_image"]),
                    );
                }
            } else {
                continuous = 0;
            }
        } else if d["output_reused"] == true {
            valid &= d["active"] == true
                && d["reset"] == false
                && d["motion_valid"] == true
                && d["source_frame_basis"] == "exact_nr_gamma_input"
                && d["source_frame_id"].as_u64().is_some_and(|id| id != 0)
                && previous.is_some_and(|old| {
                    (old["evaluated"] == true || old["output_reused"] == true)
                        && old["source_frame_id"] == d["source_frame_id"]
                        && old["source_identity"] == d["source_identity"]
                        && old["mapping"] == d["mapping"]
                        && old["input"] == d["input"]
                        && old["applied_intensity"] == d["applied_intensity"]
                });
        } else {
            continuous = 0;
        }
        previous = Some(d);
    }
    json!({"checked":checked,"valid":if checked {Some(valid)} else {None},"continuous_evaluations":count,"longest_without_reset":longest,"physical_rotations_without_reset":rotations,"logical_identities":identities.len(),"visual_quality_verified":false})
}
fn nr_submissions_completed(rows: &[Value]) -> bool {
    struct Pending {
        image: u64,
        semaphore: u64,
        consumed: bool,
        tail: bool,
        forwarded: bool,
        boundaries: u32,
        sr: Option<(u64, u64, u64)>,
    }
    let mut pending = None;
    let mut valid = true;
    let mut count = 0;
    for row in rows {
        let d = &row["details"];
        match row["event"].as_str() {
            Some("target_nr_submission") => {
                count += 1;
                valid &= pending.is_none();
                if d["fence_completed"] != true {
                    let image = d["output_image"].as_u64().unwrap_or(0);
                    let semaphore = d["ready_semaphore"].as_u64().unwrap_or(0);
                    valid &= image != 0 && semaphore != 0 && d["output_to"] == "SR";
                    let tail = d["tail_deferred"] == true;
                    valid &= !tail || d["borrowed_inputs_completed"] == true;
                    pending = Some(Pending {
                        image,
                        semaphore,
                        consumed: false,
                        tail,
                        forwarded: false,
                        boundaries: 0,
                        sr: None,
                    });
                }
            }
            Some("target_sr_frame") => {
                if let Some(p) = pending.as_mut() {
                    valid &= !p.consumed
                        && p.sr.is_none()
                        && d["source"] == "nr_output"
                        && d["color_image"].as_u64() == Some(p.image)
                        && d["consumed_waits"] == json!([p.semaphore]);
                    if p.tail {
                        let key = (
                            d["frame"].as_u64().unwrap_or(0),
                            d["fence"].as_u64().unwrap_or(0),
                            d["ready_semaphore"].as_u64().unwrap_or(0),
                        );
                        valid &= d["tail_deferred"] == true
                            && d["fence_completed"] == false
                            && key.0 != 0
                            && key.1 != 0
                            && key.2 != 0;
                        p.sr = Some(key);
                    } else {
                        valid &= d["fence_completed"] == true;
                        p.consumed = true;
                    }
                }
            }
            Some("target_fg_frame") => {
                if let Some(p) = pending.as_mut().filter(|p| p.tail) {
                    valid &= !p.forwarded
                        && p.sr.is_some_and(|(frame, _, ready)| {
                            d["frame"] == frame
                                && d["tail_deferred"] == true
                                && d["present_waits"] == json!([ready])
                                && d["requested_on"] == false
                        });
                    p.forwarded = true;
                }
            }
            Some("target_sr_completion") => {
                valid &= pending.as_mut().is_some_and(|p| {
                    let matches = p.forwarded
                        && !p.consumed
                        && p.sr.is_some_and(|(frame, fence, ready)| {
                            d["frame"] == frame
                                && d["fence"] == fence
                                && d["ready_semaphore"] == ready
                                && d["color_image"] == p.image
                                && d["fence_completed"] == true
                        });
                    p.consumed = true;
                    matches
                });
            }
            Some("target_nr_completion") => {
                valid &= pending.take().is_some_and(|p| {
                    p.consumed
                        && d["output_image"].as_u64() == Some(p.image)
                        && d["ready_semaphore"].as_u64() == Some(p.semaphore)
                        && d["fence_completed"] == true
                });
            }
            Some("vkQueuePresentKHR") => {
                if let Some(p) = pending.as_mut() {
                    p.boundaries += 1;
                    valid &= p.tail && p.sr.is_some() && p.boundaries <= 1;
                }
            }
            Some("target_nr_release_feature" | "target_fg_retired" | "vkDestroyDevice") => {
                valid &= pending.is_none()
            }
            _ => {}
        }
    }
    valid && count > 0 && pending.is_none()
}
fn present_retirements_valid(rows: &[Value]) -> bool {
    // Legacy fixtures predate submission serials; retain their existing checks.
    if !rows.iter().any(|r| r["event"] == "route_present_submitted") {
        return true;
    }
    let mut pending = HashMap::new();
    let mut seen = HashSet::new();
    for row in rows {
        let d = &row["details"];
        match row["event"].as_str() {
            Some("route_present_submitted") => {
                let Some(serial) = d["serial"].as_u64().filter(|v| *v != 0) else {
                    return false;
                };
                let key = (
                    d["queue"].clone(),
                    d["swapchain"].clone(),
                    d["image_index"].clone(),
                );
                if !seen.insert(serial) || !pending.is_empty() {
                    return false;
                }
                pending.insert(serial, key);
            }
            Some("route_present_retired") => {
                let Some(serial) = d["serial"].as_u64() else {
                    return false;
                };
                if d["fence_wait_succeeded"] != true
                    || pending.remove(&serial)
                        != Some((
                            d["queue"].clone(),
                            d["swapchain"].clone(),
                            d["image_index"].clone(),
                        ))
                {
                    return false;
                }
            }
            Some("vkDestroySwapchainKHR" | "vkDestroyDevice" | "target_sdk_shutdown")
                if !pending.is_empty() =>
            {
                return false
            }
            _ => {}
        }
    }
    pending.is_empty()
}
fn analyze_nr(rows: &[Value]) -> Value {
    let events = |name: &str| {
        rows.iter()
            .filter(move |r| r["event"] == name)
            .collect::<Vec<_>>()
    };
    let success = |name: &str| {
        !events(name).is_empty() && events(name).iter().all(|r| r["details"]["result"] == 1)
    };
    let frames = events("target_nr_frame");
    let combinations = nr_combinations::analyze(rows);
    let temporal = analyze_temporal(&frames);
    let passes = nr_pass_evidence::analyze(rows);
    let look_history = nr_look_evidence::analyze(rows);
    let submissions = events("target_nr_submission");
    let evaluations = events("target_nr_evaluate");
    let reads = events("target_nr_readback");
    let mut consecutive = 0;
    let mut longest = 0;
    let mut previous_frame = None;
    for row in &frames {
        let d = &row["details"];
        let frame = d["source_frame_id"]
            .as_u64()
            .or_else(|| d["frame"].as_u64());
        if d["evaluated"] == true {
            consecutive = if previous_frame.is_some_and(|v: u64| Some(v + 1) == frame) {
                consecutive + 1
            } else {
                1
            };
            longest = longest.max(consecutive);
            previous_frame = frame;
        } else if d["output_reused"] != true {
            consecutive = 0;
            previous_frame = None;
        }
    }
    let evaluated = frames
        .iter()
        .filter(|r| r["details"]["evaluated"] == true)
        .count();
    let valid_motion = frames
        .iter()
        .filter(|r| r["details"]["evaluated"] == true)
        .all(|r| r["details"]["motion_valid"] == true);
    let fenced = nr_submissions_completed(rows);
    let readbacks_valid = !reads.is_empty()
        && reads.iter().all(|r| {
            r["details"]["finite"] == true
                && r["details"]["sentinel_pixels"] == 0
                && r["details"]["fence_completed"] == true
        });
    let output_changed = reads.iter().any(|r| {
        r["details"]["intensity"].as_f64().is_some_and(|v| v > 0.0)
            && r["details"]["mean_absolute_input_difference"]
                .as_f64()
                .is_some_and(|v| v > 0.0)
    });
    let mut output = None;
    let mut sr_expected = false;
    let mut sr_reset = false;
    let mut handoffs = 0;
    let mut handoff_valid = true;
    for row in rows {
        let d = &row["details"];
        match row["event"].as_str() {
            Some("vkQueuePresentKHR") => {
                handoff_valid &= !sr_expected;
                output = None;
                sr_expected = false;
                sr_reset = false;
            }
            Some("target_nr_submission") => {
                output = d["output_image"].as_u64().filter(|v| *v != 0);
                sr_expected = d["output_to"] == "SR";
            }
            Some("target_nr_frame") => sr_reset = d["pending_sr_reset"] == true,
            Some("target_sr_frame") => {
                if output.is_some() {
                    handoff_valid &= sr_expected
                        && d["source"] == "nr_output"
                        && d["color_image"].as_u64() == output;
                    handoffs += 1;
                } else {
                    handoff_valid &= d["source"] != "nr_output";
                }
                handoff_valid &= !sr_reset || d["history_reset"] == true;
                sr_expected = false;
            }
            _ => {}
        }
    }
    handoff_valid &= !sr_expected;
    let position = |name: &str| rows.iter().rposition(|r| r["event"] == name);
    let clean_nr_shutdown = success("target_nr_release_feature")
        && success("target_nr_destroy_parameters")
        && success("target_nr_snippet_shutdown")
        && success("target_nr_destroy_capabilities")
        && events("target_nr_create_feature").len() == events("target_nr_release_feature").len()
        && events("target_nr_allocate_parameters").len()
            == events("target_nr_destroy_parameters").len()
        && position("target_nr_release_feature") < position("target_nr_snippet_shutdown")
        && position("target_nr_snippet_shutdown") < position("target_sdk_shutdown")
        && events("target_nr_core_deferred").last().is_some_and(|r| {
            r["details"]["all_features_released"] == true && r["details"]["owner"] == "Streamline"
        });
    let evaluated_submissions = submissions
        .iter()
        .filter(|r| r["details"]["evaluated"] != false)
        .collect::<Vec<_>>();
    let count_matches =
        evaluated > 0 && evaluated == evaluated_submissions.len() && evaluated == evaluations.len();
    let reset_matches = temporal["checked"] != true
        || frames
            .iter()
            .filter(|r| r["details"]["evaluated"] == true)
            .zip(&evaluated_submissions)
            .all(|(frame, submit)| {
                frame["details"]["reset"] == submit["details"]["history_reset"]
                    && (frame["details"]["source_frame_id"].is_null()
                        || frame["details"]["source_frame_id"]
                            == submit["details"]["source_frame_id"])
            });
    let call_chain = success("target_nr_core_init")
        && success("target_nr_snippet_init")
        && success("target_nr_create_feature")
        && success("target_nr_evaluate")
        && count_matches
        && valid_motion
        && fenced
        && clean_nr_shutdown
        && handoff_valid
        && reset_matches
        && combinations["valid"] != false
        && temporal["valid"] != false
        && passes["valid"] != false
        && look_history["valid"] != false;
    let execution = call_chain && readbacks_valid && output_changed;
    let mut reset_reasons = std::collections::BTreeMap::<String, usize>::new();
    for r in &frames {
        if r["details"]["reset"] == true {
            *reset_reasons
                .entry(
                    r["details"]["reset_reason"]
                        .as_str()
                        .unwrap_or("unknown")
                        .into(),
                )
                .or_default() += 1;
        }
    }
    json!({"execution_verified":execution,"call_chain_completed":call_chain,"temporal_history":temporal,"passes":passes,"look_history":look_history,"combinations":combinations,"evaluate_successes":evaluated,"count_matches":count_matches,"reset_matches_submission":reset_matches,"longest_consecutive_evaluations":longest,"valid_motion_only":valid_motion,"submissions_fenced":fenced,"readbacks":reads.len(),"readbacks_valid":readbacks_valid,"nonzero_intensity_changed_output":output_changed,"off_frames":frames.iter().filter(|r|r["details"]["requested"]==false).count(),"zero_motion_paused_frames":frames.iter().filter(|r|r["details"]["requested"]==true&&r["details"]["motion_valid"]==false&&r["details"]["evaluated"]==false).count(),"reset_reasons":reset_reasons,"nr_to_sr_frames":handoffs,"nr_to_sr_handoff_valid":handoff_valid,"clean_nr_shutdown":clean_nr_shutdown,"fg_requested_frames":events("target_fg_frame").iter().filter(|r|r["details"]["requested_on"]==true).count(),"synthetic_depth":true,"p1_passed":false,"p2_passed":false,"visual_quality_verified":false,"performance_verified":false})
}

fn analyze(rows: &[Value], fg: bool, sdk: bool) -> Result<Value> {
    if !present_retirements_valid(rows) {
        return Err("unbalanced or premature native present retirement".into());
    }
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

fn analyze_shutdown(rows: &[Value], sdk: bool) -> Result<Value> {
    if rows.iter().any(|r| {
        matches!(
            r["event"].as_str(),
            Some("target_fg_failure" | "target_sdk_error")
        )
    }) {
        return Err("target recorded an SDK/FG failure".into());
    }
    for (event, field) in [
        ("vkDestroyDevice", "remaining_devices"),
        ("vkDestroyInstance", "remaining_instances"),
    ] {
        if rows
            .iter()
            .rev()
            .find(|r| r["event"] == event)
            .is_none_or(|r| r["details"][field] != 0)
        {
            return Err(format!("missing clean {event}").into());
        }
    }
    if sdk
        && rows
            .iter()
            .rev()
            .find(|r| r["event"] == "target_sdk_shutdown")
            .is_none_or(|r| r["details"]["result"] != 0)
    {
        return Err("SDK shutdown failed".into());
    }
    Ok(
        json!({"clean_shutdown":true,"frame_trace_enabled":false,"bounded_activation_verified":false,
        "visual_intermediate_frames_verified":false,"scanout_cadence_verified":false,"latency_verified":false}),
    )
}

fn verification_event(event: Option<&str>) -> bool {
    event.is_some_and(|e| e.starts_with("target_nr_"))
        || matches!(
            event,
            Some(
                "vkQueuePresentKHR"
                    | "route_present_submitted"
                    | "route_present_retired"
                    | "target_fg_frame"
                    | "target_fg_failure"
                    | "target_sdk_error"
                    | "target_sdk_shutdown"
                    | "target_fg_retired"
                    | "vkDestroyDevice"
                    | "vkDestroyInstance"
                    | "vkDestroySwapchainKHR"
                    | "target_sr_frame"
                    | "target_sr_completion"
            )
        )
}
pub(super) fn verify(session: &Path) -> Result<()> {
    let inputs: Value = serde_json::from_slice(&fs::read(session.join("target-inputs.json"))?)?;
    let exit: Value = serde_json::from_slice(&fs::read(session.join("target-exit.json"))?)?;
    // A live NR session may start with NR disabled. Its initial control state
    // must not bypass NR evidence or discard the report after a failed exit.
    // Keep nr_requested for sessions recorded before nr_available existed.
    let native_nr = inputs["nr_available"] == true || inputs["nr_requested"] == true;
    if exit["success"] != true && !native_nr {
        return Err("target process did not exit successfully".into());
    }
    let fg = inputs["fg_experiment_requested"] == true;
    let sdk = inputs["layer_only"] == false;
    let mut rows = Vec::new();
    for line in BufReader::new(fs::File::open(session.join("layer.jsonl"))?).lines() {
        let row: Value = serde_json::from_str(&line?)?;
        if verification_event(row["event"].as_str()) {
            rows.push(row);
        }
    }
    if native_nr {
        let nr = analyze_nr(&rows);
        let mut chain = analyze(&rows, fg, sdk);
        if inputs["nr_performance"] == true {
            // Timing runs cannot satisfy the strict validation/output evidence gate.
            let result = json!({"target_exit":exit,"nr":nr,"present_chain":chain.as_ref().ok(),"present_chain_error":chain.as_ref().err().map(|e|e.to_string()),"nr_performance":true,"validation":{"enabled":false,"validation_passed":false},"session_verified":false,"p1_passed":false,"p2_passed":false});
            fs::write(
                session.join("target-result.json"),
                serde_json::to_vec_pretty(&result)?,
            )?;
            println!("{result}");
            return if exit["success"] == true && chain.is_ok() && nr["call_chain_completed"] == true
            {
                Ok(())
            } else {
                Err(
                    "NR timing process or presentation cleanup failed; see target-result.json"
                        .into(),
                )
            };
        }
        let mut validation: Value =
            serde_json::from_slice(&fs::read(session.join("nr-validation-result.json"))?)?;
        // An SDK helper instance can retire before the application's instance
        // exists. A crash may leave that early summary in place, so the final
        // gate also counts every retained message and requires process cleanup.
        let mut errors = 0u64;
        let mut warnings = 0u64;
        let mut unreviewed = 0u64;
        for line in BufReader::new(fs::File::open(session.join("nr-validation.jsonl"))?).lines() {
            let row: Value = serde_json::from_str(&line?)?;
            let severity = row["severity"]
                .as_u64()
                .ok_or("missing validation severity")?;
            errors += u64::from(severity & 4096 != 0);
            warnings += u64::from(severity & 256 != 0);
            unreviewed += u64::from(severity & 256 != 0 && row["review"].is_null());
        }
        let summary_matches = validation["errors"] == errors
            && validation["warnings"] == warnings
            && validation["unreviewed_warnings"] == unreviewed;
        validation["summary_matches_log"] = json!(summary_matches);
        validation["errors"] = json!(errors);
        validation["warnings"] = json!(warnings);
        validation["unreviewed_warnings"] = json!(unreviewed);
        validation["complete"] = json!(
            validation["complete"] == true
                && exit["success"] == true
                && chain.is_ok()
                && summary_matches
        );
        validation["validation_passed"] = json!(
            validation["complete"] == true
                && errors == 0
                && unreviewed == 0
                && validation["log_failed"] == false
        );
        let validation_enabled = rows.iter().any(|r| {
            r["event"] == "target_nr_validation"
                && r["details"]["core"] == true
                && r["details"]["synchronization"] == true
        });
        if let Ok(ref mut report) = chain {
            report["validation_layer_enabled"] = json!(validation_enabled);
        }
        let log = fs::read_to_string(session.join("runtime/sl.log"))?;
        let sdk_errors = log.lines().filter(|l| l.contains("[error]")).count();
        let verified = exit["success"] == true
            && validation_enabled
            && nr["execution_verified"] == true
            && chain.is_ok()
            && validation["complete"] == true
            && validation["validation_passed"] == true
            && sdk_errors == 0;
        let result = json!({"target_exit":exit,"nr":nr,"present_chain":chain.as_ref().ok(),"present_chain_error":chain.as_ref().err().map(|e|e.to_string()),"validation":validation,"sdk_error_lines":sdk_errors,"session_verified":verified,"p1_passed":false,"p2_passed":false});
        fs::write(
            session.join("target-result.json"),
            serde_json::to_vec_pretty(&result)?,
        )?;
        println!("{result}");
        return if verified {
            Ok(())
        } else {
            Err("native NR execution, shutdown or strict validation gate failed; see target-result.json".into())
        };
    }
    let mut result = if inputs["frame_trace_enabled"] == false {
        analyze_shutdown(&rows, sdk)?
    } else {
        analyze(&rows, fg, sdk)?
    };
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
    fn temporal_rows() -> Vec<Value> {
        (0..4).map(|f|json!({"details":{"frame":f,"evaluated":true,"source_identity":2,"source_image":if f%2==0 {42}else{52},"source_generation":if f%2==0 {1}else{2},"source_group_size":2,"mapping":7,"input":[1920,1080],"intensity":1.0,"reset":f==0,"reset_reason":if f==0 {"Created"}else{"Continuous"}}})).collect()
    }
    #[test]
    fn source_color_repeats_preserve_history_and_reject_forged_ids() {
        let mut rows = temporal_rows();
        for (i, row) in rows.iter_mut().enumerate() {
            let d = &mut row["details"];
            d["active"] = json!(true);
            d["source_frame_id"] = json!(i + 1);
            d["source_frame_basis"] = json!("exact_nr_gamma_input");
            d["output_reused"] = json!(false);
            d["motion_valid"] = json!(true);
            d["applied_intensity"] = json!(1.0);
        }
        let mut repeated = rows[1].clone();
        repeated["details"]["evaluated"] = json!(false);
        repeated["details"]["output_reused"] = json!(true);
        // Requested changes remain pending; the completed output still has
        // the old strength. Present gaps alone do not advance source history.
        repeated["details"]["intensity"] = json!(0.5);
        repeated["details"]["frame"] = json!(100);
        rows.insert(2, repeated);
        let report = analyze_temporal(&rows.iter().collect::<Vec<_>>());
        assert_eq!(report["valid"], true);
        assert_eq!(report["continuous_evaluations"], 3);
        assert_eq!(report["longest_without_reset"], 3);
        for (index, field, value) in [
            (2, "source_frame_id", json!(3)),
            (2, "reset", json!(true)),
            (2, "active", json!(false)),
            (2, "motion_valid", json!(false)),
            (2, "mapping", json!(9)),
            (2, "applied_intensity", json!(0.5)),
            (3, "source_frame_id", json!(2)),
            (3, "source_frame_id", Value::Null),
        ] {
            let mut invalid = rows.clone();
            invalid[index]["details"][field] = value;
            assert_eq!(
                analyze_temporal(&invalid.iter().collect::<Vec<_>>())["valid"],
                false,
                "{index}/{field}"
            );
        }
        let mut twice = rows.clone();
        twice[3]["details"]["source_frame_id"] = json!(2);
        twice[3]["details"]["reset"] = json!(true);
        assert_eq!(
            analyze_temporal(&twice.iter().collect::<Vec<_>>())["valid"],
            false
        );
    }
    #[test]
    fn cached_output_submissions_do_not_count_as_ngx_evaluations() {
        let mut rows = nr_valid();
        let position = rows
            .iter()
            .position(|r| r["event"] == "target_nr_release_feature")
            .unwrap();
        rows.splice(position..position, [
            json!({"event":"target_nr_submission","details":{"evaluated":false,"output_reused":true,"output_image":99,"output_to":"SR","fence_completed":true}}),
            json!({"event":"target_nr_frame","details":{"frame":11,"active":true,"evaluated":false,"output_reused":true,"pending_sr_reset":false}}),
            json!({"event":"target_sr_frame","details":{"source":"nr_output","color_image":99,"history_reset":false}}),
        ]);
        let report = analyze_nr(&rows);
        assert_eq!(report["evaluate_successes"], 1);
        assert_eq!(report["count_matches"], true);
        assert_eq!(report["submissions_fenced"], true);
        assert_eq!(report["nr_to_sr_frames"], 2);
        // Unknown legacy source identity cannot prove temporal continuity.
        assert_eq!(report["temporal_history"]["valid"], Value::Null);
        rows[position]["details"]["fence_completed"] = json!(false);
        assert_eq!(analyze_nr(&rows)["submissions_fenced"], false);
    }
    #[test]
    fn temporal_evidence_requires_consecutive_unchanged_logical_contracts() {
        let rows = temporal_rows();
        let report = analyze_temporal(&rows.iter().collect::<Vec<_>>());
        assert_eq!(report["valid"], true);
        assert_eq!(report["physical_rotations_without_reset"], 3);
        assert_eq!(report["longest_without_reset"], 3);
        for (field, value) in [
            ("frame", json!(8)),
            ("source_identity", json!(9)),
            ("mapping", json!(9)),
            ("input", json!([1280, 720])),
            ("intensity", json!(0.5)),
            ("evaluated", json!(false)),
            ("source_generation", json!(10)),
        ] {
            let mut changed = temporal_rows();
            changed[2]["details"][field] = value;
            assert_eq!(
                analyze_temporal(&changed.iter().collect::<Vec<_>>())["valid"],
                false,
                "{field}"
            );
        }
        let old = vec![json!({"details":{"evaluated":true,"reset":true}})];
        assert_eq!(
            analyze_temporal(&old.iter().collect::<Vec<_>>())["valid"],
            Value::Null
        );
    }
    #[test]
    fn changed_allocation_can_resume_only_in_a_new_reset_epoch() {
        let mut rows = temporal_rows();
        rows[2]["details"]["source_identity"] = json!(3);
        rows[2]["details"]["source_generation"] = json!(10);
        rows[2]["details"]["reset"] = json!(true);
        rows[2]["details"]["reset_reason"] = json!("SourceChanged");
        rows[3]["details"]["source_identity"] = json!(3);
        assert_eq!(
            analyze_temporal(&rows.iter().collect::<Vec<_>>())["valid"],
            true
        );
        rows[2]["details"]["reset"] = json!(false);
        assert_eq!(
            analyze_temporal(&rows.iter().collect::<Vec<_>>())["valid"],
            false
        );
    }
    fn nr_valid() -> Vec<Value> {
        let mut rows = [
            "target_nr_core_init",
            "target_nr_snippet_init",
            "target_nr_allocate_parameters",
            "target_nr_create_feature",
            "target_nr_evaluate",
        ]
        .map(|e| json!({"event":e,"details":{"result":1}}))
        .to_vec();
        rows.extend([
            json!({"event":"target_nr_submission","details":{"output_image":99,"output_to":"SR","fence_completed":true}}),
            json!({"event":"target_nr_frame","details":{"frame":10,"evaluated":true,"motion_valid":true,"pending_sr_reset":true}}),
            json!({"event":"target_nr_readback","details":{"finite":true,"sentinel_pixels":0,"fence_completed":true,"intensity":1.0,"mean_absolute_input_difference":0.01}}),
            json!({"event":"target_sr_frame","details":{"source":"nr_output","color_image":99,"history_reset":true}}),
        ]);
        rows.extend(
            [
                "target_nr_release_feature",
                "target_nr_destroy_parameters",
                "target_nr_snippet_shutdown",
                "target_nr_destroy_capabilities",
            ]
            .map(|e| json!({"event":e,"details":{"result":1}})),
        );
        rows.extend([
            json!({"event":"target_nr_core_deferred","details":{"all_features_released":true,"owner":"Streamline"}}),
            json!({"event":"target_sdk_shutdown","details":{"result":0}}),
        ]);
        rows
    }
    #[test]
    fn nr_timing_without_readback_cannot_pass_output_gate() {
        let rows: Vec<_> = nr_valid()
            .into_iter()
            .filter(|r| r["event"] != "target_nr_readback")
            .collect();
        let report = analyze_nr(&rows);
        assert_eq!(report["call_chain_completed"], true);
        assert_eq!(report["execution_verified"], false);
        assert_eq!(report["readbacks_valid"], false);
        assert_eq!(report["p1_passed"], false);
    }
    #[test]
    fn nr_gate_rejects_stale_handoff_unfenced_output_and_missing_reset() {
        assert_eq!(analyze_nr(&nr_valid())["execution_verified"], true);
        for (event, field, value) in [
            ("target_sr_frame", "color_image", json!(98)),
            ("target_sr_frame", "history_reset", json!(false)),
            ("target_nr_submission", "fence_completed", json!(false)),
            ("target_nr_readback", "sentinel_pixels", json!(1)),
            ("target_nr_frame", "motion_valid", json!(false)),
        ] {
            let mut rows = nr_valid();
            rows.iter_mut().find(|r| r["event"] == event).unwrap()["details"][field] = value;
            assert_eq!(
                analyze_nr(&rows)["execution_verified"],
                false,
                "{event}/{field}"
            );
        }
        let mut rows = nr_valid();
        rows.retain(|r| r["event"] != "target_sr_frame");
        assert_eq!(analyze_nr(&rows)["execution_verified"], false);
    }
    #[test]
    fn nr_deferred_completion_requires_consumed_semaphore_and_retired_sr() {
        let mut rows = nr_valid();
        let submit = rows
            .iter_mut()
            .find(|r| r["event"] == "target_nr_submission")
            .unwrap();
        submit["details"]["fence_completed"] = json!(false);
        submit["details"]["ready_semaphore"] = json!(77);
        let index = rows
            .iter()
            .position(|r| r["event"] == "target_sr_frame")
            .unwrap();
        rows[index]["details"]["consumed_waits"] = json!([77]);
        rows[index]["details"]["fence_completed"] = json!(true);
        let completion = json!({"event":"target_nr_completion","details":{"output_image":99,"ready_semaphore":77,"fence_completed":true}});
        rows.insert(index + 1, completion.clone());
        assert_eq!(analyze_nr(&rows)["execution_verified"], true);
        for (event, field, value) in [
            ("target_sr_frame", "consumed_waits", json!([])),
            ("target_sr_frame", "consumed_waits", json!([78])),
            ("target_sr_frame", "fence_completed", json!(false)),
            ("target_nr_completion", "output_image", json!(98)),
            ("target_nr_completion", "ready_semaphore", json!(78)),
            ("target_nr_completion", "fence_completed", json!(false)),
        ] {
            let mut bad = rows.clone();
            bad.iter_mut().find(|r| r["event"] == event).unwrap()["details"][field] = value;
            assert_eq!(
                analyze_nr(&bad)["execution_verified"],
                false,
                "{event}/{field}"
            );
        }
        for insertion in [0, index, index + 2] {
            let mut bad = rows.clone();
            bad.insert(insertion, completion.clone());
            assert!(!nr_submissions_completed(&bad));
        }
        let mut bad = rows.clone();
        bad.insert(index + 1, json!({"event":"vkQueuePresentKHR"}));
        assert!(!nr_submissions_completed(&bad));
        rows.retain(|r| r["event"] != "target_nr_completion");
        assert!(!nr_submissions_completed(&rows));
    }
    #[test]
    fn deferred_tail_requires_private_inputs_and_retires_before_second_present() {
        let rows = vec![
            json!({"event":"target_nr_submission","details":{"output_image":99,"ready_semaphore":77,"output_to":"SR","fence_completed":false,"tail_deferred":true,"borrowed_inputs_completed":true}}),
            json!({"event":"target_sr_frame","details":{"source":"nr_output","color_image":99,"consumed_waits":[77],"frame":10,"fence":55,"ready_semaphore":88,"tail_deferred":true,"fence_completed":false}}),
            json!({"event":"target_fg_frame","details":{"frame":10,"tail_deferred":true,"present_waits":[88],"requested_on":false}}),
            json!({"event":"vkQueuePresentKHR"}),
            json!({"event":"target_sr_completion","details":{"color_image":99,"frame":10,"fence":55,"ready_semaphore":88,"fence_completed":true}}),
            json!({"event":"target_nr_completion","details":{"output_image":99,"ready_semaphore":77,"fence_completed":true}}),
        ];
        assert!(nr_submissions_completed(&rows));
        let retained = rows
            .iter()
            .filter(|r| verification_event(r["event"].as_str()))
            .cloned()
            .collect::<Vec<_>>();
        assert!(nr_submissions_completed(&retained));
        for (index, field, value) in [
            (0, "borrowed_inputs_completed", json!(false)),
            (1, "consumed_waits", json!([])),
            (2, "present_waits", json!([])),
            (2, "requested_on", json!(true)),
            (4, "frame", json!(11)),
            (4, "fence", json!(56)),
            (4, "fence_completed", json!(false)),
        ] {
            let mut bad = rows.clone();
            bad[index]["details"][field] = value;
            assert!(!nr_submissions_completed(&bad), "{index}/{field}");
        }
        for event in [
            "vkQueuePresentKHR",
            "target_nr_release_feature",
            "target_fg_retired",
            "vkDestroyDevice",
        ] {
            let mut bad = rows.clone();
            bad.insert(4, json!({"event":event}));
            assert!(!nr_submissions_completed(&bad), "{event}");
        }
        let mut bad = rows.clone();
        bad.swap(4, 5);
        assert!(!nr_submissions_completed(&bad));
        for index in [2, 4, 5] {
            let mut bad = rows.clone();
            bad.remove(index);
            assert!(!nr_submissions_completed(&bad));
        }
    }
    #[test]
    fn present_retirement_rejects_reuse_wrong_image_and_pending_shutdown() {
        let submit = json!({"event":"route_present_submitted","details":{"serial":1,"queue":2,"swapchain":3,"image_index":0}});
        let done = json!({"event":"route_present_retired","details":{"serial":1,"queue":2,"swapchain":3,"image_index":0,"fence_wait_succeeded":true}});
        assert!(verification_event(submit["event"].as_str()));
        assert!(verification_event(done["event"].as_str()));
        assert!(present_retirements_valid(&[submit.clone(), done.clone()]));
        assert!(!present_retirements_valid(&[submit.clone()]));
        assert!(!present_retirements_valid(&[
            submit.clone(),
            submit.clone(),
            done.clone()
        ]));
        assert!(!present_retirements_valid(&[
            submit.clone(),
            done.clone(),
            done.clone()
        ]));
        assert!(!present_retirements_valid(&[
            submit.clone(),
            done.clone(),
            submit.clone(),
            done.clone()
        ]));
        for field in ["serial", "queue", "swapchain", "image_index"] {
            let mut bad = done.clone();
            bad["details"][field] = json!(8);
            assert!(!present_retirements_valid(&[submit.clone(), bad]));
        }
        for event in [
            "vkDestroySwapchainKHR",
            "vkDestroyDevice",
            "target_sdk_shutdown",
        ] {
            assert!(!present_retirements_valid(&[
                submit.clone(),
                json!({"event":event}),
                done.clone()
            ]));
        }
    }
    #[test]
    fn nr_gate_requires_balanced_resources_and_shared_core_shutdown_order() {
        for event in [
            "target_nr_allocate_parameters",
            "target_nr_release_feature",
            "target_nr_snippet_shutdown",
            "target_sdk_shutdown",
            "target_nr_core_deferred",
        ] {
            let rows = nr_valid()
                .into_iter()
                .filter(|r| r["event"] != event)
                .collect::<Vec<_>>();
            assert_eq!(analyze_nr(&rows)["execution_verified"], false, "{event}");
        }
        let mut rows = nr_valid();
        let sdk = rows.pop().unwrap();
        rows.insert(0, sdk);
        assert_eq!(analyze_nr(&rows)["execution_verified"], false);
        assert_eq!(analyze_nr(&nr_valid())["p1_passed"], false);
    }
    #[test]
    fn failed_nr_process_retains_result_and_rejects_stale_validation_summary() {
        let dir = std::env::temp_dir().join(format!(
            "ns-nr-verifier-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(dir.join("runtime")).unwrap();
        for (name, value) in [
            (
                "target-inputs.json",
                json!({"nr_requested":true,"layer_only":false}),
            ),
            (
                "target-exit.json",
                json!({"success":false,"code":-532462766}),
            ),
            (
                "nr-validation-result.json",
                json!({"complete":true,"errors":0,"warnings":0,"unreviewed_warnings":0,"log_failed":false,"validation_passed":true}),
            ),
        ] {
            fs::write(dir.join(name), serde_json::to_vec(&value).unwrap()).unwrap();
        }
        let rows = nr_valid()
            .iter()
            .map(|r| format!("{r}\n"))
            .collect::<String>();
        fs::write(dir.join("layer.jsonl"), rows).unwrap();
        fs::write(dir.join("runtime/sl.log"), "").unwrap();
        fs::write(
            dir.join("nr-validation.jsonl"),
            "{\"severity\":4096,\"review\":null}\n",
        )
        .unwrap();
        assert!(verify(&dir).is_err());
        let report: Value =
            serde_json::from_slice(&fs::read(dir.join("target-result.json")).unwrap()).unwrap();
        assert_eq!(report["validation"]["errors"], 1);
        assert_eq!(report["validation"]["summary_matches_log"], false);
        assert_eq!(report["validation"]["complete"], false);
        assert_eq!(report["session_verified"], false);
        assert_eq!(report["target_exit"]["success"], false);
        // Initial-off launches still use the strict NR path after live enable,
        // including when process exit succeeds but validation does not.
        for success in [false, true] {
            fs::write(
                dir.join("target-inputs.json"),
                b"{\"nr_requested\":false,\"nr_available\":true,\"layer_only\":false}",
            )
            .unwrap();
            fs::write(
                dir.join("target-exit.json"),
                serde_json::to_vec(&json!({"success":success})).unwrap(),
            )
            .unwrap();
            assert!(verify(&dir).is_err());
            let report: Value =
                serde_json::from_slice(&fs::read(dir.join("target-result.json")).unwrap()).unwrap();
            assert_eq!(report["nr"]["call_chain_completed"], true);
            assert_eq!(report["validation"]["errors"], 1);
            assert_eq!(report["session_verified"], false);
            assert_eq!(report["target_exit"]["success"], success);
        }
        fs::write(
            dir.join("target-exit.json"),
            b"{\"success\":false,\"code\":-532462766}",
        )
        .unwrap();
        // Even stale passing validation evidence cannot bless a timing run.
        fs::write(
            dir.join("target-inputs.json"),
            b"{\"nr_requested\":true,\"nr_performance\":true,\"layer_only\":false}",
        )
        .unwrap();
        assert!(verify(&dir).is_err());
        let report: Value =
            serde_json::from_slice(&fs::read(dir.join("target-result.json")).unwrap()).unwrap();
        assert_eq!(report["validation"]["enabled"], false);
        assert_eq!(report["validation"]["validation_passed"], false);
        assert_eq!(report["session_verified"], false);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn quiet_sessions_still_require_clean_shutdown_and_reject_errors() {
        let mut rows = vec![
            json!({"event":"vkDestroyDevice","details":{"remaining_devices":0}}),
            json!({"event":"vkDestroyInstance","details":{"remaining_instances":0}}),
            json!({"event":"target_sdk_shutdown","details":{"result":0}}),
        ];
        assert_eq!(
            analyze_shutdown(&rows, true).unwrap()["bounded_activation_verified"],
            false
        );
        assert!(analyze(&rows, true, true).is_err());
        assert!(analyze_shutdown(&rows[..2], true).is_err());
        rows.push(json!({"event":"target_fg_failure"}));
        assert!(analyze_shutdown(&rows, true).is_err());
    }
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

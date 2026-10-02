//! Per-frame composition evidence. Requests and background FG samples do not
//! prove activation, and completion counts do not establish display quality.
use serde_json::{json, Value};
use std::collections::HashMap;

pub(super) fn analyze(rows: &[Value]) -> Value {
    let checked = rows
        .iter()
        .any(|r| r["event"] == "target_fg_frame" && r["details"]["composition_version"] == 1);
    if !checked {
        return json!({"checked":false,"valid":null,"all_combinations_observed":false});
    }
    let mut valid = true;
    let mut nr_frames = HashMap::new();
    let mut sr_frames = HashMap::new();
    for row in rows {
        let map = match row["event"].as_str() {
            Some("target_nr_frame") => &mut nr_frames,
            Some("target_sr_frame") => &mut sr_frames,
            _ => continue,
        };
        if let Some(frame) = row["details"]["frame"].as_u64() {
            valid &= map.insert(frame, &row["details"]).is_none();
        } else {
            valid = false;
        }
    }
    let mut counts = [0u64; 8];
    let mut inactive = 0;
    let mut fg_enabled_frames = 0;
    let mut previous: Option<(&Value, &Value)> = None;
    let mut transitions = Vec::new();
    let mut reset_checks = [0u64; 2];
    let mut invalid_frames = Vec::new();
    for row in rows.iter().filter(|r| r["event"] == "target_fg_frame") {
        let d = &row["details"];
        let Some(nr) = d["frame"].as_u64().and_then(|f| nr_frames.get(&f)) else {
            valid = false;
            previous = None;
            continue;
        };
        let sr = d["frame"].as_u64().and_then(|f| sr_frames.get(&f));
        let nr_on = nr["evaluated"] == true;
        let sr_on = sr.is_some();
        let fg_on = d["requested_on"] == true;
        let fg_generated = fg_on
            && d["state"]["presented"] == 2
            && d["state"]["fence"].as_u64().is_some_and(|v| v != 0)
            && d["state"]["value"].as_u64().is_some_and(|v| v != 0)
            && d["input_wait_completed"] == true;
        let color = if sr_on {
            "sr_output"
        } else if nr_on {
            "nr_output"
        } else {
            "original_present"
        };
        let mut frame_valid = d["composition_version"] == 1
            && d["nr_evaluated"] == nr_on
            && d["sr_evaluated"] == sr_on
            && d["color_source"] == color
            && d["fg_control_requested"].is_boolean()
            && d["fg_revision"].is_u64()
            && d["sr_mode"].is_u64()
            && d["sr_revision"].is_u64()
            && d["requested_on"].is_boolean()
            && d["history_reset"].is_boolean()
            && d["input_wait_completed"] == true
            && nr["requested"].is_boolean()
            && nr["evaluated"].is_boolean()
            && nr["pending_sr_reset"].is_boolean()
            && nr["pending_fg_reset"].is_boolean()
            && nr["intensity"]
                .as_f64()
                .is_some_and(|v| (0.0..=1.0).contains(&v))
            && d["state"]["status"] == 0
            && (!fg_on || d["fg_control_requested"] == true);
        if let Some(sr) = sr {
            frame_valid &= sr["evaluated"] == true
                && sr["history_reset"].is_boolean()
                && if nr_on {
                    sr["source"] == "nr_output"
                } else {
                    matches!(
                        sr["source"].as_str(),
                        Some("native_source" | "present_source")
                    )
                };
            if nr["pending_sr_reset"] == true {
                reset_checks[0] += 1;
                frame_valid &= sr["history_reset"] == true;
            }
        }
        if fg_on && nr["pending_fg_reset"] == true {
            reset_checks[1] += 1;
            frame_valid &= d["history_reset"] == true;
        }
        if !frame_valid && invalid_frames.len() < 16 {
            invalid_frames.push(d["frame"].clone());
        }
        valid &= frame_valid;
        fg_enabled_frames += u64::from(fg_on);
        // Ignore requested consumers that have not actually run. In particular,
        // background/warmup FG must not be counted as an FG-off test phase.
        if frame_valid
            && nr["requested"] == nr_on
            && (d["sr_mode"].as_u64().unwrap_or(0) != 0) == sr_on
            && d["fg_control_requested"] == fg_generated
            && (fg_generated || (!fg_on && d["state"]["presented"] == 1))
        {
            let mask =
                usize::from(nr_on) | (usize::from(sr_on) << 1) | (usize::from(fg_generated) << 2);
            counts[mask] += 1;
        } else {
            inactive += 1;
        }
        if let Some((old_d, old_nr)) = previous {
            if old_d["frame"].as_u64().and_then(|f| f.checked_add(1)) == d["frame"].as_u64()
                && old_d["swapchain"] == d["swapchain"]
                && (old_nr["requested"] != nr["requested"]
                    || old_nr["intensity"] != nr["intensity"])
            {
                let sr_continuous = old_d["sr_evaluated"] == true
                    && old_d["sr_revision"] == d["sr_revision"]
                    && sr.is_some_and(|current| {
                        current["fence"].as_u64().is_some_and(|v| v != 0)
                            && old_d["frame"]
                                .as_u64()
                                .and_then(|f| sr_frames.get(&f))
                                .is_some_and(|old| old["fence"] == current["fence"])
                    });
                let fg_continuous = old_d["requested_on"] == true
                    && old_d["fg_revision"] == d["fg_revision"]
                    && fg_on
                    && old_d["state"]["presented"] == 2
                    && old_d["state"]["fence"] == d["state"]["fence"]
                    && old_d["state"]["value"].as_u64().is_some_and(|old| {
                        old != 0 && d["state"]["value"].as_u64().is_some_and(|new| new > old)
                    });
                transitions.push(json!({"frame":d["frame"],"enabled_changed":old_nr["requested"]!=nr["requested"],"strength_changed":old_nr["intensity"]!=nr["intensity"],"sr_continuous":sr_continuous,"fg_continuous":fg_continuous,"fg_generated_on_transition":fg_generated,"reset_valid":frame_valid}));
            }
        }
        previous = Some((d, nr));
    }
    let combinations: Vec<_> = (0..8)
        .map(|mask| json!({"nr":mask&1!=0,"sr":mask&2!=0,"fg":mask&4!=0,"frames":counts[mask]}))
        .collect();
    json!({"checked":true,"valid":valid,"combinations":combinations,"all_combinations_observed":valid&&counts.iter().all(|&v|v>0),"inactive_requested_frames":inactive,"fg_enabled_frames":fg_enabled_frames,"sr_reset_checks":reset_checks[0],"fg_reset_checks":reset_checks[1],"nr_control_transitions":transitions,"invalid_frames":invalid_frames,"safe_failure_tested":false,"visual_quality_verified":false})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(id: u64, mask: u32) -> Vec<Value> {
        let nr = mask & 1 != 0;
        let sr = mask & 2 != 0;
        let fg = mask & 4 != 0;
        let mut rows = vec![
            json!({"event":"target_nr_frame","details":{"frame":id,"requested":nr,"evaluated":nr,"intensity":1.0,"pending_sr_reset":true,"pending_fg_reset":true}}),
        ];
        if sr {
            rows.push(json!({"event":"target_sr_frame","details":{"frame":id,"fence":55,"source":if nr {"nr_output"} else {"native_source"},"evaluated":true,"history_reset":true}}));
        }
        rows.push(json!({"event":"target_fg_frame","details":{"frame":id,"swapchain":9,"composition_version":1,"fg_control_requested":fg,"fg_revision":1,"sr_mode":if sr {6} else {0},"sr_revision":1,"nr_evaluated":nr,"sr_evaluated":sr,"color_source":if sr {"sr_output"} else if nr {"nr_output"} else {"original_present"},"requested_on":fg,"history_reset":true,"input_wait_completed":true,"state":{"presented":if fg {2} else {1},"status":0,"fence":10,"value":id+1}}}));
        rows
    }
    #[test]
    fn all_eight_combinations_require_real_consumers() {
        let rows: Vec<_> = (0..8).flat_map(|m| frame(m as u64, m)).collect();
        let report = analyze(&rows);
        assert_eq!(report["valid"], true);
        assert_eq!(report["all_combinations_observed"], true);
        for (field, value) in [
            ("requested_on", json!(false)),
            (
                "state",
                json!({"status":0,"presented":1,"fence":0,"value":0}),
            ),
        ] {
            let mut rows = frame(1, 7);
            rows.last_mut().unwrap()["details"][field] = value;
            let report = analyze(&rows);
            assert_eq!(report["combinations"][7]["frames"], 0);
            assert_eq!(report["combinations"][3]["frames"], 0);
        }
    }
    #[test]
    fn missing_resets_stale_color_and_mismatched_frames_fail() {
        for (event, field, value) in [
            ("target_sr_frame", "history_reset", json!(false)),
            ("target_fg_frame", "history_reset", json!(false)),
            ("target_sr_frame", "source", json!("native_source")),
            ("target_sr_frame", "frame", json!(99)),
            ("target_nr_frame", "frame", json!(99)),
            ("target_fg_frame", "color_source", json!("nr_output")),
            ("target_fg_frame", "input_wait_completed", json!(false)),
            ("target_nr_frame", "pending_fg_reset", Value::Null),
            ("target_nr_frame", "pending_sr_reset", Value::Null),
        ] {
            let mut rows = frame(1, 7);
            rows.iter_mut().find(|r| r["event"] == event).unwrap()["details"][field] = value;
            assert_eq!(analyze(&rows)["valid"], false, "{event}/{field}");
        }
        let mut rows = frame(1, 7);
        rows.push(rows[0].clone());
        assert_eq!(analyze(&rows)["valid"], false);
    }
    #[test]
    fn strength_changes_preserve_pending_reset_until_fg_resumes() {
        let mut rows = frame(1, 3);
        rows.extend(frame(2, 3));
        rows[3]["details"]["intensity"] = json!(0.5);
        rows.extend(frame(3, 7));
        rows[6]["details"]["intensity"] = json!(0.5);
        let report = analyze(&rows);
        assert_eq!(report["valid"], true);
        assert_eq!(report["fg_reset_checks"], 1);
        assert_eq!(report["nr_control_transitions"][0]["sr_continuous"], true);
        assert_eq!(report["nr_control_transitions"][0]["fg_continuous"], false);
        rows.last_mut().unwrap()["details"]["history_reset"] = json!(false);
        assert_eq!(analyze(&rows)["valid"], false);
        assert_eq!(analyze(&[])["checked"], false);
    }
    #[test]
    fn recreated_consumers_do_not_prove_continuous_toggle_coverage() {
        let mut rows = frame(1, 7);
        rows.extend(frame(2, 7));
        rows[3]["details"]["intensity"] = json!(0.5);
        let report = analyze(&rows);
        assert_eq!(report["nr_control_transitions"][0]["sr_continuous"], true);
        assert_eq!(report["nr_control_transitions"][0]["fg_continuous"], true);
        rows[4]["details"]["fence"] = json!(99);
        rows[5]["details"]["state"]["fence"] = json!(99);
        let report = analyze(&rows);
        assert_eq!(report["nr_control_transitions"][0]["sr_continuous"], false);
        assert_eq!(report["nr_control_transitions"][0]["fg_continuous"], false);
    }
}

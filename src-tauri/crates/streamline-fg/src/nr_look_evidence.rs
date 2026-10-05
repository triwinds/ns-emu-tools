//! Check Look history claims separately from NGX model history.
use serde_json::{json, Value};
use std::collections::HashMap;
pub(super) fn analyze(rows: &[Value]) -> Value {
    let profiles = rows
        .iter()
        .filter(|r| r["event"] == "target_nr_profile")
        .map(|r| &r["details"])
        .collect::<Vec<_>>();
    let checked = profiles.iter().any(|p| p["look"]["temporal"].is_object());
    let mut valid = true;
    let mut previous = HashMap::<u64, &Value>::new();
    let mut evaluations = 0;
    let mut repeats = 0;
    for p in profiles {
        let t = &p["look"]["temporal"];
        if t["active"] != true {
            continue;
        }
        let instance = p["pipeline"]["passes"]
            .as_array()
            .and_then(|passes| passes.last())
            .and_then(|pass| pass["instance"].as_u64())
            .unwrap_or(0);
        let weight = t["maximumHistoryWeight"].as_f64();
        valid &= instance != 0
            && t["requested"] == true
            && t["historyReady"] == true
            && t["textureCount"] == 3
            && t["history"] == "raw_log_model_delta"
            && t["sourceFrameId"] == p["source_frame_id"]
            && weight.is_some_and(|v| v.is_finite() && (0.0..=0.9).contains(&v));
        let old = previous.get(&instance).copied();
        if p["evaluated"] == true {
            evaluations += 1;
            let reason = t["resetReason"].as_str();
            valid &= matches!(
                reason,
                Some(
                    "first_frame"
                        | "upstream_reset"
                        | "source_gap"
                        | "look_changed"
                        | "nonpositive_interval"
                        | "long_interval"
                        | "continuous"
                )
            );
            if reason == Some("continuous") {
                valid &= t["intervalMs"]
                    .as_u64()
                    .is_some_and(|ms| (1..=250).contains(&ms))
                    && old.is_some_and(|old| {
                        old["sourceFrameId"]
                            .as_u64()
                            .and_then(|id| id.checked_add(1))
                            == t["sourceFrameId"].as_u64()
                    });
            } else {
                valid &= weight == Some(0.0);
            }
        } else {
            repeats += 1;
            valid &= p["output_reused"] == true
                && old.is_some_and(|old| {
                    old["sourceFrameId"] == t["sourceFrameId"]
                        && old["maximumHistoryWeight"] == t["maximumHistoryWeight"]
                        && old["resetReason"] == t["resetReason"]
                        && old["intervalMs"] == t["intervalMs"]
                });
        }
        previous.insert(instance, t);
    }
    json!({"checked":checked,"valid":if checked {Some(valid)} else {None},"history_updates":evaluations,"cached_outputs":repeats,"pixel_acceptance_measured":false,"video_quality_verified":false})
}
#[cfg(test)]
mod tests {
    use super::*;
    fn profile(id: u64, evaluated: bool, weight: f64, reason: &str) -> Value {
        json!({"event":"target_nr_profile","details":{"source_frame_id":id,"evaluated":evaluated,"output_reused":!evaluated,"pipeline":{"passes":[{"instance":7}]},
            "look":{"temporal":{"active":true,"requested":true,"historyReady":true,"textureCount":3,"history":"raw_log_model_delta","sourceFrameId":id,
                "maximumHistoryWeight":weight,"intervalMs":if reason=="first_frame" {0}else{16},"resetReason":reason}}}})
    }
    #[test]
    fn delta_history_and_cached_outputs_are_independent_claims() {
        let rows = vec![
            profile(1, true, 0.0, "first_frame"),
            profile(2, true, 0.75, "continuous"),
            profile(2, false, 0.75, "continuous"),
        ];
        assert_eq!(analyze(&rows)["valid"], true);
        for (key, value) in [
            ("sourceFrameId", json!(3)),
            ("history", json!("rgb_output")),
            ("maximumHistoryWeight", json!(0.95)),
            ("intervalMs", json!(300)),
            ("resetReason", json!("first_frame")),
        ] {
            let mut invalid = rows.clone();
            invalid[1]["details"]["look"]["temporal"][key] = value;
            assert_eq!(analyze(&invalid)["valid"], false, "{key}");
        }
        let mut invalid = rows.clone();
        invalid[2]["details"]["look"]["temporal"]["intervalMs"] = json!(17);
        assert_eq!(analyze(&invalid)["valid"], false);
        assert_eq!(analyze(&[])["valid"], Value::Null);
    }
    #[test]
    fn rejected_gap_seeds_without_claiming_blending() {
        let rows = vec![
            profile(1, true, 0.0, "first_frame"),
            profile(5, true, 0.0, "source_gap"),
        ];
        assert_eq!(analyze(&rows)["valid"], true);
        let mut invalid = rows;
        invalid[1]["details"]["look"]["temporal"]["maximumHistoryWeight"] = json!(0.5);
        assert_eq!(analyze(&invalid)["valid"], false);
    }
}

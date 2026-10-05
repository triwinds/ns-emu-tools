//! Audit actual submitted passes rather than the requested pipeline setting.
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

pub(super) fn analyze(rows: &[Value]) -> Value {
    let events = |name: &str| {
        rows.iter()
            .filter(move |r| r["event"] == name)
            .collect::<Vec<_>>()
            .into_iter()
    };
    let records = events("target_nr_pass_frame").collect::<Vec<_>>();
    let checked = !records.is_empty()
        || events("target_nr_frame").any(|r| !r["details"]["pipeline"]["actualPasses"].is_null());
    let mut valid = true;
    let mut consumed = HashSet::new();
    let mut histories = HashMap::<u64, &Value>::new();
    let mut second_frames = 0;
    for frame in events("target_nr_frame").filter(|r| r["details"]["evaluated"] == true) {
        let f = &frame["details"];
        let passes = records
            .iter()
            .filter(|r| r["details"]["presentFrame"] == f["frame"])
            .collect::<Vec<_>>();
        let count = f["pipeline"]["actualPasses"].as_u64().unwrap_or(0);
        valid &= matches!(count, 1 | 2) && passes.len() == count as usize;
        let mut indices = HashSet::new();
        for row in &passes {
            let d = &row["details"];
            let pass = d["pass"].as_u64().unwrap_or(0);
            valid &= indices.insert(pass) && pass > 0 && pass <= count;
            valid &= consumed.insert((d["presentFrame"].as_u64(), pass));
            valid &= d["evaluated"] == true
                && d["outputReused"] == false
                && d["sourceFrameId"] == f["source_frame_id"]
                && d["sourceIdentity"] == f["source_identity"]
                && d["mapping"] == f["mapping"];
            for key in [
                "instance",
                "parameters",
                "feature",
                "inputImage",
                "outputImage",
                "depthImage",
                "motionImage",
                "sourceFrameId",
                "sourceIdentity",
            ] {
                valid &= d[key].as_u64().is_some_and(|v| v != 0);
            }
            valid &= d["inputImage"] != d["outputImage"];
            if pass == 1 {
                valid &= d["reset"] == f["reset"];
            } else if pass == 2 {
                second_frames += 1;
                let old = histories.get(&d["instance"].as_u64().unwrap_or(0)).copied();
                let expected_reset = f["reset"] == true
                    || old.is_none_or(|old| {
                        old["sourceFrameId"]
                            .as_u64()
                            .and_then(|id| id.checked_add(1))
                            != d["sourceFrameId"].as_u64()
                            || old["sourceIdentity"] != d["sourceIdentity"]
                            || old["mapping"] != d["mapping"]
                            || old["intensity"] != d["intensity"]
                            || old["options"] != d["options"]
                    });
                valid &= d["reset"] == expected_reset;
                histories.insert(d["instance"].as_u64().unwrap_or(0), d);
            }
        }
        if count == 2 {
            if let (Some(first), Some(second)) = (
                passes.iter().find(|r| r["details"]["pass"] == 1),
                passes.iter().find(|r| r["details"]["pass"] == 2),
            ) {
                let (a, b) = (&first["details"], &second["details"]);
                valid &= a["outputImage"] == b["inputImage"]
                    && a["inputImage"] != b["outputImage"]
                    && a["outputImage"] != b["outputImage"]
                    && a["depthImage"] == b["depthImage"]
                    && a["motionImage"] == b["motionImage"];
                for key in ["instance", "parameters", "feature"] {
                    valid &= a[key] != b[key];
                }
            } else {
                valid = false;
            }
        }
        // The last successful pass must be the image submitted to Present/SR.
        let output = passes
            .iter()
            .find(|r| r["details"]["pass"] == count)
            .map(|r| &r["details"]["outputImage"]);
        let submissions = events("target_nr_submission")
            .filter(|r| r["details"]["present_frame"] == f["frame"])
            .collect::<Vec<_>>();
        valid &= submissions.len() == 1
            && output.is_some_and(|output| {
                submissions[0]["details"]["output_image"] == *output
                    && submissions[0]["details"]["actual_passes"] == count
                    && submissions[0]["details"]["source_frame_id"] == f["source_frame_id"]
                    && submissions[0]["details"]["evaluated"] == true
            });
    }
    valid &= consumed.len() == records.len();
    let successful = |name: &str| events(name).filter(|r| r["details"]["result"] == 1).count();
    valid &= second_frames == successful("target_nr_second_evaluate");
    let created = successful("target_nr_second_create_feature");
    let allocated = successful("target_nr_second_allocate_parameters");
    let clean = created == successful("target_nr_second_release_feature")
        && allocated
            == successful("target_nr_second_destroy_parameters")
                + successful("target_nr_second_destroy_unused_parameters")
        && [
            "target_nr_second_release_feature",
            "target_nr_second_destroy_parameters",
            "target_nr_second_destroy_unused_parameters",
        ]
        .iter()
        .all(|name| {
            events(name).all(|r| r["details"]["result"] == 1)
                && rows
                    .iter()
                    .rposition(|r| r["event"] == *name)
                    .is_none_or(|last| {
                        rows.iter()
                            .rposition(|r| r["event"] == "target_nr_snippet_shutdown")
                            .is_some_and(|shutdown| last < shutdown)
                    })
        });
    valid &= clean;
    let mut fallbacks = 0;
    for row in events("target_nr_second_fallback") {
        let d = &row["details"];
        fallbacks += 1;
        valid &= d["output"] == "first_pass"
            && d["retry"] == "explicit_retry_toggle_or_recreation"
            && (d["stage"] == "prepare_before_submit"
                || (d["stage"] == "discard_unsubmitted_recording"
                    && d["first_pass_fenced"] == true
                    && d["second_submitted"] == false));
        if d["stage"] == "discard_unsubmitted_recording" {
            valid &= events("target_nr_frame").any(|r| {
                r["details"]["source_frame_id"] == d["source_frame_id"]
                    && r["details"]["evaluated"] == true
                    && r["details"]["pipeline"]["actualPasses"] == 1
            });
        }
    }
    json!({"checked":checked,"valid":if checked {Some(valid)} else {None},"submitted_passes":records.len(),
        "two_pass_frames":second_frames,"fallbacks":fallbacks,"second_instances_created":created,"second_instances_released":successful("target_nr_second_release_feature"),
        "clean_second_shutdown":clean,"game_quality_verified":false})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(name: &str, details: Value) -> Value {
        json!({"event":name,"details":details})
    }
    fn fixture() -> Vec<Value> {
        let mut rows = ["allocate_parameters", "create_feature"]
            .map(|name| event(&format!("target_nr_second_{name}"), json!({"result":1})))
            .to_vec();
        for frame in 1..=3 {
            for pass in 1..=2 {
                rows.push(event("target_nr_pass_frame", json!({"presentFrame":frame,"sourceFrameId":frame,"sourceIdentity":99,"mapping":7,
                    "instance":pass,"parameters":100+pass,"feature":200+pass,"pass":pass,"evaluated":true,"outputReused":false,
                    "reset":frame==1||(frame==3&&pass==2),"intensity":if frame==3&&pass==2 {0.5} else {1.0},"options":{"style":"a"},
                    "inputImage":10+pass,"outputImage":11+pass,"depthImage":20,"motionImage":21})));
            }
            rows.push(event("target_nr_second_evaluate", json!({"result":1})));
            rows.push(event(
                "target_nr_submission",
                json!({"present_frame":frame,"source_frame_id":frame,"actual_passes":2,"evaluated":true,"output_image":13}),
            ));
            rows.push(event(
                "target_nr_frame",
                json!({"frame":frame,"source_frame_id":frame,"source_identity":99,"mapping":7,
                "evaluated":true,"reset":frame==1,"pipeline":{"actualPasses":2}}),
            ));
        }
        for name in [
            "target_nr_second_release_feature",
            "target_nr_second_destroy_parameters",
            "target_nr_snippet_shutdown",
        ] {
            rows.push(event(name, json!({"result":1})));
        }
        rows
    }
    #[test]
    fn independent_history_and_final_output_are_required() {
        let rows = fixture();
        assert_eq!(analyze(&rows)["valid"], true);
        for (key, value) in [
            ("parameters", json!(101)),
            ("feature", json!(201)),
            ("inputImage", json!(11)),
            ("outputImage", json!(11)),
            ("depthImage", json!(90)),
            ("motionImage", json!(90)),
            ("sourceFrameId", json!(2)),
            ("sourceIdentity", json!(90)),
            ("reset", json!(false)),
        ] {
            let mut invalid = rows.clone();
            invalid[3]["details"][key] = value;
            assert_eq!(analyze(&invalid)["valid"], false, "{key}");
        }
        let mut invalid = rows.clone();
        invalid[8]["details"]["reset"] = json!(true);
        assert_eq!(analyze(&invalid)["valid"], false, "per-frame reset");
        let mut invalid = rows.clone();
        invalid[5]["details"]["output_image"] = json!(12);
        assert_eq!(analyze(&invalid)["valid"], false, "wrong final output");
        let mut invalid = rows.clone();
        invalid.retain(|r| r["event"] != "target_nr_second_release_feature");
        assert_eq!(analyze(&invalid)["valid"], false, "leaked second feature");
        assert_eq!(analyze(&[])["valid"], Value::Null);
    }
    #[test]
    fn repeated_frames_cannot_claim_another_evaluation() {
        let mut rows = fixture();
        let mut repeat = rows[6].clone();
        repeat["details"]["frame"] = json!(4);
        repeat["details"]["evaluated"] = json!(false);
        repeat["details"]["output_reused"] = json!(true);
        rows.push(repeat);
        assert_eq!(analyze(&rows)["valid"], true);
        rows.push(rows[3].clone());
        assert_eq!(analyze(&rows)["valid"], false);
    }
    #[test]
    fn fallback_requires_discard_before_submission() {
        let source = fixture();
        let mut rows = vec![source[2].clone(), source[5].clone(), source[6].clone()];
        rows[1]["details"]["output_image"] = json!(12);
        rows[1]["details"]["actual_passes"] = json!(1);
        rows[2]["details"]["pipeline"]["actualPasses"] = json!(1);
        rows.push(event("target_nr_second_fallback", json!({"stage":"discard_unsubmitted_recording","source_frame_id":1,"first_pass_fenced":true,
            "second_submitted":false,"output":"first_pass","retry":"explicit_retry_toggle_or_recreation"})));
        assert_eq!(analyze(&rows)["valid"], true);
        rows.last_mut().unwrap()["details"]["second_submitted"] = json!(true);
        assert_eq!(analyze(&rows)["valid"], false);
    }
}

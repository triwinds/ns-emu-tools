//! Offline candidate report.
use crate::scale_model::analyze;
use crate::support::{write_json, Result};
use serde_json::{json, Value};
use std::{fs, path::Path};
pub(super) fn run(path: &Path) -> Result<()> {
    let text = fs::read_to_string(path.join("layer.jsonl"))?;
    let mut rows = vec![];
    let mut complete = false;
    for line in text.lines() {
        let r: Value = serde_json::from_str(line)?;
        if r["event"] == "scale_probe" {
            rows.push(r["details"].clone());
        }
        if r["event"] == "scale_probe_complete" && r["details"]["reason"] == "frame_limit" {
            complete = true;
        }
    }
    rows.sort_by_key(|r| r["seq"].as_u64().unwrap_or(0));
    let report = analyze(&rows, complete);
    let output = path.join("scale-analysis-v2.json");
    if output.exists() {
        let previous: Value = serde_json::from_slice(&fs::read(&output)?)?;
        if previous != report {
            return Err(
                "existing scale-analysis-v2.json differs; preserve it before reanalysis".into(),
            );
        }
    } else {
        write_json(&output, &report)?;
    }
    println!(
        "{}",
        json!({"present_count":report["present_count"],"unique_submitted_draw_and_source_count":report["unique_submitted_draw_and_source_count"],"capture_complete":complete,"replacement_verified":false})
    );
    Ok(())
}

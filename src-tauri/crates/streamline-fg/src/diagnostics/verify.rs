use serde_json::Value;

pub fn validate_trace(events: &[Value]) -> Result<(), String> {
    for (name, phase, expected) in [
        ("vkGetDeviceQueue", 2, 2),
        ("vkGetDeviceQueue", 4, 2),
        ("vkGetDeviceQueue", 3, 0),
        ("vkQueuePresentKHR", 1, 6),
        ("vkCreateWin32SurfaceKHR", 2, 2),
        ("vkAcquireNextImage2KHR", 1, 2),
        ("vkGetDeviceQueue2", 1, 2),
    ] {
        let count = events
            .iter()
            .filter(|r| r["event"] == name && r["phase"] == phase)
            .count();
        if count != expected {
            return Err(format!(
                "{name} phase {phase}: expected {expected}, observed {count}"
            ));
        }
    }
    let main = events
        .iter()
        .find(|r| r["event"] == "vkGetDeviceQueue" && r["phase"] == 2)
        .unwrap();
    for worker in events
        .iter()
        .filter(|r| r["event"] == "vkGetDeviceQueue" && r["phase"] == 4)
    {
        if worker["thread"].as_str().is_none() || worker["thread"] == main["thread"] {
            return Err("worker did not use a distinct thread".into());
        }
    }
    for (event, field) in [
        ("vkDestroyDevice", "remaining_devices"),
        ("vkDestroyInstance", "remaining_instances"),
    ] {
        let rows: Vec<_> = events.iter().filter(|r| r["event"] == event).collect();
        if rows.len() != 2 || rows.iter().any(|r| r["details"][field] != 0) {
            return Err(format!("incomplete cleanup: {event}"));
        }
    }
    Ok(())
}

pub fn validate_runs(baseline: &Value, layered: &Value) -> Result<(), String> {
    let a = baseline["generations"]
        .as_array()
        .ok_or("missing baseline generations")?;
    let b = layered["generations"]
        .as_array()
        .ok_or("missing layered generations")?;
    if a.len() != 2 || b.len() != 2 {
        return Err("two generations required".into());
    }
    for (a, b) in a.iter().zip(b) {
        for field in [
            "generation",
            "gpu",
            "queue_family",
            "width",
            "height",
            "native_present_count",
        ] {
            if a[field].is_null() || a[field] != b[field] {
                return Err(format!("baseline/layered mismatch: {field}"));
            }
        }
        let addresses = b["addresses"].as_array().ok_or("missing addresses")?;
        for name in [
            "vkGetDeviceQueue",
            "vkGetDeviceQueue2",
            "vkAcquireNextImage2KHR",
            "vkCreateWin32SurfaceKHR",
            "vkDestroySurfaceKHR",
        ] {
            let row = addresses
                .iter()
                .find(|r| r["name"] == name)
                .ok_or("missing fallback address")?;
            if row["sdk"].as_u64().unwrap_or(0) == 0
                || row["next"].as_u64().unwrap_or(0) == 0
                || row["sdk"] != row["system"]
                || row["sdk"] == row["next"]
            {
                return Err(format!(
                    "SDK fallback route differs from hypothesis: {name}"
                ));
            }
        }
        for name in ["vkCreateWin32SurfaceKHR", "vkDestroySurfaceKHR"] {
            let row = addresses.iter().find(|r| r["name"] == name).unwrap();
            if row["direct_sdk_export"].as_u64().unwrap_or(0) == 0
                || row["direct_sdk_export"] == row["sdk"]
            {
                return Err(format!("surface export distinction missing: {name}"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn trace() -> Vec<Value> {
        let mut rows = Vec::new();
        for _ in 0..2 {
            for (name, phase, count) in [
                ("vkGetDeviceQueue", 2, 1),
                ("vkGetDeviceQueue", 4, 1),
                ("vkQueuePresentKHR", 1, 3),
                ("vkCreateWin32SurfaceKHR", 2, 1),
                ("vkAcquireNextImage2KHR", 1, 1),
                ("vkGetDeviceQueue2", 1, 1),
            ] {
                for _ in 0..count {
                    rows.push(json!({"event": name, "phase": phase, "thread": if phase == 4 {"worker"} else {"main"}}));
                }
            }
            rows.push(json!({"event": "vkDestroyDevice", "details": {"remaining_devices": 0}}));
            rows.push(json!({"event": "vkDestroyInstance", "details": {"remaining_instances": 0}}));
        }
        rows
    }
    #[test]
    fn rejects_partial_or_recursive_or_wrong_thread_evidence() {
        let good = trace();
        assert!(validate_trace(&good).is_ok());
        assert!(validate_trace(&good[..good.len() - 1]).is_err());
        let mut duplicate = good.clone();
        duplicate.push(good[0].clone());
        assert!(validate_trace(&duplicate).is_err());
        let mut same_thread = good.clone();
        for row in &mut same_thread {
            row["thread"] = json!("main");
        }
        assert!(validate_trace(&same_thread).is_err());
        let mut reentry = good;
        reentry.push(json!({"event":"vkGetDeviceQueue", "phase":3}));
        assert!(validate_trace(&reentry).is_err());
    }
    #[test]
    fn rejects_empty_run_reports() {
        assert!(validate_runs(&json!({}), &json!({})).is_err());
    }
}

pub const PLUGINS: &[&str] = &[
    "sl.interposer.dll",
    "sl.common.dll",
    "sl.dlss_g.dll",
    "sl.reflex.dll",
    "sl.pcl.dll",
    "nvngx_dlssg.dll",
    "NvLowLatencyVk.dll",
];
use crate::support::{hash, verify, Result};
use serde_json::Value;
use std::path::Path;

pub fn verify_runtime(path: &Path, name: &str, experimental: bool) -> Result<String> {
    if !experimental {
        return verify(path, &format!("dlss/{name}"));
    }
    let manifest: Value = serde_json::from_str(include_str!("../sdk-route/runtime-v3.json"))?;
    let expected = expected_hash(&manifest, name)?;
    let actual = hash(path)?;
    if actual != expected {
        return Err(format!("experimental runtime hash mismatch: {}", path.display()).into());
    }
    Ok(actual)
}

fn expected_hash<'a>(manifest: &'a Value, name: &str) -> Result<&'a str> {
    manifest["files"]
        .as_array()
        .ok_or("missing runtime files")?
        .iter()
        .find(|v| v["name"] == name)
        .and_then(|v| v["sha256"].as_str())
        .ok_or_else(|| "unknown runtime DLL".into())
}

#[cfg(all(test, feature = "sdk-bridge"))]
mod tests {
    use super::*;
    #[test]
    fn experiment_changes_only_the_two_rebuilt_dlls() {
        let manifest: Value =
            serde_json::from_str(include_str!("../sdk-route/runtime-v3.json")).unwrap();
        let frozen: Value = serde_json::from_str(include_str!("../sdk/baseline.json")).unwrap();
        assert_eq!(manifest["files"].as_array().unwrap().len(), 7);
        for name in crate::runtime::PLUGINS {
            let old = frozen["artifacts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["path"] == format!("dlss/{name}"))
                .unwrap()["sha256"]
                .as_str()
                .unwrap();
            let new = expected_hash(&manifest, name).unwrap();
            assert_eq!(
                new != old,
                ["sl.interposer.dll", "sl.common.dll"].contains(name)
            );
        }
        assert!(expected_hash(&manifest, "../sl.interposer.dll").is_err());
    }
}

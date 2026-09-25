use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path, process::ExitCode};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    path: String,
    sha256_lf: String,
    markers: Vec<String>,
}

fn inspect(rule: &Rule, source: &str) -> Value {
    let normalized = source.replace("\r\n", "\n");
    let actual = format!("{:x}", Sha256::digest(normalized.as_bytes()));
    let matches = actual == rule.sha256_lf;
    let sites: Vec<_> = rule
        .markers
        .iter()
        .map(|marker| {
            let lines: Vec<_> = normalized
                .lines()
                .enumerate()
                .filter(|(_, line)| line.contains(marker))
                .map(|(index, line)| json!({"line": index + 1, "text": line.trim()}))
                .collect();
            json!({"marker": marker, "found": !lines.is_empty(), "sites": lines})
        })
        .collect();
    let markers_found = sites.iter().all(|v| v["found"] == true);
    json!({
        "path": rule.path, "expected_sha256_lf": rule.sha256_lf,
        "actual_sha256_lf": actual, "hash_matches": matches,
        "evidence_matches": matches && markers_found, "markers": sites
    })
}

fn audit(root: &Path, rules: &[Rule]) -> Value {
    let files: Vec<_> = rules
        .iter()
        .map(|rule| match fs::read_to_string(root.join(&rule.path)) {
            Ok(source) => inspect(rule, &source),
            Err(error) => {
                json!({"path": rule.path, "evidence_matches": false, "error": error.to_string()})
            }
        })
        .collect();
    // Presence inventory only: this neither verifies dependencies nor establishes build readiness.
    let inventory: Vec<_> = [
        "tools/premake5/premake5.exe",
        "external/vulkan/Include/vulkan/vulkan.h",
        "external/nvapi/amd64/nvapi64.lib",
        "external/slang/bin/slangc.exe",
        "external/ngx-sdk/Lib/Windows_x86_64/nvsdk_ngx_d.lib",
        "external/reflex-sdk-vk/lib/NvLowLatencyVk.lib",
        "source/plugins/sl.dlss_g",
    ]
    .iter()
    .map(|path| json!({"path": path, "present": root.join(path).exists()}))
    .collect();
    json!({
        "schema_version": 1,
        "sdk_commit": streamline_fg_preflight::SDK_COMMIT,
        "sdk_root": root,
        "source_evidence_matches": files.iter().all(|v| v["evidence_matches"] == true),
        "files": files,
        "dependency_presence_only": inventory,
        "required_adaptation": [
            "install next GIPA/GDPA before processVulkanInterface maps tables and before initializePlugins",
            "route both sl.common NGX Vulkan initialization branches through supplied resolvers",
            "audit direct system exports separately for SDK-owned and host-owned instances",
            "keep resolver bindings alive until SDK workers, NGX and shutdown finish",
            "rebuild interposer and common in an isolated, independently hashed runtime"
        ],
        "sdk_rebuilt": false,
        "runtime_routing_verified": false,
        "sdk_owned_worker_tested": false,
        "fg_enabled": false,
        "p0_passed": false
    })
}

fn run() -> Result<u8, Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--help" {
        println!("sdk-route-audit --sdk-dir <absolute directory>\nRead-only frozen-source audit; JSON on stdout.\nExit 1: invalid/mismatching evidence. Exit 2: evidence matches, runtime route remains unverified.");
        return Ok(0);
    }
    if args.len() != 2 || args[0] != "--sdk-dir" || !Path::new(&args[1]).is_absolute() {
        return Err("expected --sdk-dir <absolute directory>".into());
    }
    let rules: Vec<Rule> = serde_json::from_str(include_str!("../../route-baseline.json"))?;
    if rules.is_empty() {
        return Err("empty source baseline".into());
    }
    let report = audit(Path::new(&args[1]), &rules);
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, &report)?;
    writeln!(stdout)?;
    Ok(if report["source_evidence_matches"] == true {
        2
    } else {
        1
    })
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rule() -> Rule {
        Rule {
            path: "sample.cpp".into(),
            sha256_lf: format!("{:x}", Sha256::digest(b"header\nresolver\n")),
            markers: vec!["resolver".into()],
        }
    }

    #[test]
    fn line_endings_are_normalized_but_mutation_is_rejected() {
        assert_eq!(
            inspect(&rule(), "header\r\nresolver\r\n")["evidence_matches"],
            true
        );
        assert_eq!(
            inspect(&rule(), "modified\nresolver\n")["evidence_matches"],
            false
        );
        let mut missing_marker = rule();
        missing_marker.markers.push("absent".into());
        assert_eq!(
            inspect(&missing_marker, "header\nresolver\n")["evidence_matches"],
            false
        );
    }

    #[test]
    fn missing_source_is_reported_and_never_passes_p0() {
        let dir = tempfile::tempdir().unwrap();
        let report = audit(dir.path(), &[rule()]);
        assert_eq!(report["source_evidence_matches"], false);
        assert_eq!(report["p0_passed"], false);
        assert!(report["files"][0]["error"].is_string());
        fs::write(dir.path().join("sample.cpp"), "header\nresolver\n").unwrap();
        let report = audit(dir.path(), &[rule()]);
        assert_eq!(report["source_evidence_matches"], true);
        assert_eq!(report["runtime_routing_verified"], false);
        assert_eq!(report["p0_passed"], false);
    }
}

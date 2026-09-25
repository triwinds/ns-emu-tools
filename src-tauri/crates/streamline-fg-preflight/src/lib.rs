//! Read-only P0 evidence verification. This crate never loads a graphics DLL.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

pub const SDK_COMMIT: &str = "e8aaa6eaac968711fb62473d4ae8256dde20919b";
pub const BLOCKERS: &[&str] = &[
    "loader_reentry: stock SDK resolves Vulkan through the system loader; no proven next-layer dispatch route",
    "window_pre_disable: no proven notification, serialization and completion path before all supported window operations",
    "feature_requirements: per-feature runtime requirements and queue allocations have not been queried or validated",
    "abi: header versions are pinned but compiled C++ layout assertions and a minimal C ABI bridge are outstanding",
    "target_identity: only the reference fork is pinned; an original Ryubing build has not been selected",
];

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Reference,
    Sdk,
    Loader,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HashMode {
    Binary,
    Utf8Lf,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub scope: Scope,
    pub path: String,
    pub mode: HashMode,
    pub sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Baseline {
    pub schema_version: u32,
    pub sdk_commit: String,
    pub reference_build: String,
    pub loader_version: String,
    pub artifacts: Vec<Artifact>,
}

pub fn baseline() -> Result<Baseline, Box<dyn std::error::Error>> {
    let baseline: Baseline = serde_json::from_str(include_str!("../baseline.json"))?;
    if baseline.schema_version != 1
        || baseline.sdk_commit != SDK_COMMIT
        || baseline.artifacts.is_empty()
    {
        return Err("invalid built-in baseline".into());
    }
    for item in &baseline.artifacts {
        if item.path.is_empty()
            || item.path.contains('\\')
            || item.path.contains(':')
            || Path::new(&item.path)
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
            || item.sha256.len() != 64
            || !item.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(format!("invalid baseline entry: {}", item.path).into());
        }
    }
    Ok(baseline)
}

pub struct Roots {
    pub reference: PathBuf,
    pub sdk: PathBuf,
    /// Exact loader file, not a DLL search directory.
    pub loader: PathBuf,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactStatus {
    Match,
    Mismatch,
    Unreadable,
}

#[derive(Debug, Serialize)]
pub struct Observation {
    pub scope: Scope,
    pub path: PathBuf,
    pub hash_mode: HashMode,
    pub expected_sha256: String,
    pub actual_sha256: Option<String>,
    pub status: ArtifactStatus,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub sdk_commit: String,
    pub reference_build: String,
    pub baseline_loader_version: String,
    pub supported_host: bool,
    pub evidence_matches: bool,
    pub p0_passed: bool,
    pub fg_enabled: bool,
    pub runtime_queries_performed: bool,
    pub blockers: Vec<String>,
    pub observations: Vec<Observation>,
}

impl Report {
    /// Matching files never imply that the integration gate has passed.
    pub fn exit_code(&self) -> u8 {
        if !self.evidence_matches || !self.supported_host {
            1
        } else {
            2
        }
    }
}

pub fn audit(baseline: &Baseline, roots: &Roots) -> Report {
    let observations: Vec<_> = baseline
        .artifacts
        .iter()
        .map(|item| {
            let path = match item.scope {
                Scope::Reference => roots.reference.join(&item.path),
                Scope::Sdk => roots.sdk.join(&item.path),
                Scope::Loader => roots.loader.clone(),
            };
            let (actual_sha256, status, error) = match digest(&path, item.mode) {
                Ok(hash) => {
                    let status = if hash.eq_ignore_ascii_case(&item.sha256) {
                        ArtifactStatus::Match
                    } else {
                        ArtifactStatus::Mismatch
                    };
                    (Some(hash), status, None)
                }
                Err(error) => (None, ArtifactStatus::Unreadable, Some(error.to_string())),
            };
            Observation {
                scope: item.scope,
                path,
                hash_mode: item.mode,
                expected_sha256: item.sha256.clone(),
                actual_sha256,
                status,
                error,
            }
        })
        .collect();
    let supported_host = cfg!(all(target_os = "windows", target_arch = "x86_64"));
    let mut blockers: Vec<String> = BLOCKERS.iter().map(|s| (*s).to_owned()).collect();
    if !supported_host {
        blockers.push("unsupported_host: first prototype requires Windows x64".into());
    }
    Report {
        schema_version: 1,
        sdk_commit: baseline.sdk_commit.clone(),
        reference_build: baseline.reference_build.clone(),
        baseline_loader_version: baseline.loader_version.clone(),
        supported_host,
        evidence_matches: observations
            .iter()
            .all(|o| o.status == ArtifactStatus::Match),
        p0_passed: false,
        fg_enabled: false,
        runtime_queries_performed: false,
        blockers,
        observations,
    }
}

fn digest(path: &Path, mode: HashMode) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    match mode {
        HashMode::Binary => {
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let count = file.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                hasher.update(&buffer[..count]);
            }
        }
        HashMode::Utf8Lf => {
            // Source evidence only: ignore Git's CRLF conversion, no other normalization.
            let mut source = String::new();
            file.read_to_string(&mut source)?;
            hasher.update(source.replace("\r\n", "\n").as_bytes());
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_profile_contains_reference_sdk_and_loader() {
        let profile = baseline().unwrap();
        for scope in [Scope::Reference, Scope::Sdk, Scope::Loader] {
            assert!(profile
                .artifacts
                .iter()
                .any(|a| { std::mem::discriminant(&a.scope) == std::mem::discriminant(&scope) }));
        }
    }

    #[test]
    fn text_line_endings_are_normalized_but_binary_bytes_are_not() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::write(&a, b"abc\r\n").unwrap();
        std::fs::write(&b, b"abc\n").unwrap();
        assert_eq!(
            digest(&a, HashMode::Utf8Lf).unwrap(),
            digest(&b, HashMode::Utf8Lf).unwrap()
        );
        assert_ne!(
            digest(&a, HashMode::Binary).unwrap(),
            digest(&b, HashMode::Binary).unwrap()
        );
        std::fs::write(&a, b"abc").unwrap();
        assert_eq!(
            digest(&a, HashMode::Binary).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn changed_and_missing_evidence_never_pass_and_all_results_are_retained() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sample");
        std::fs::write(&file, b"frozen evidence").unwrap();
        let mut profile = Baseline {
            schema_version: 1,
            sdk_commit: SDK_COMMIT.into(),
            reference_build: "test".into(),
            loader_version: "test".into(),
            artifacts: vec![Artifact {
                scope: Scope::Reference,
                path: "sample".into(),
                mode: HashMode::Binary,
                sha256: digest(&file, HashMode::Binary).unwrap(),
            }],
        };
        let roots = Roots {
            reference: dir.path().into(),
            sdk: dir.path().into(),
            loader: file.clone(),
        };
        let matching = audit(&profile, &roots);
        assert!(matching.evidence_matches);
        assert!(!matching.p0_passed && !matching.fg_enabled && !matching.runtime_queries_performed);
        assert_ne!(matching.exit_code(), 0);
        std::fs::write(&file, b"changed evidence").unwrap();
        profile.artifacts.push(Artifact {
            scope: Scope::Sdk,
            path: "missing".into(),
            mode: HashMode::Binary,
            sha256: "0".repeat(64),
        });
        let failed = audit(&profile, &roots);
        assert_eq!(failed.exit_code(), 1);
        assert_eq!(failed.observations.len(), 2);
        assert_eq!(failed.observations[0].status, ArtifactStatus::Mismatch);
        assert_eq!(failed.observations[1].status, ArtifactStatus::Unreadable);
        assert_eq!(std::fs::read(&file).unwrap(), b"changed evidence");
    }
}

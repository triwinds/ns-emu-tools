//! Launcher lifecycle and disposal of verified, session-owned runtime snapshots.
use super::streamline_install::{atomic_json, plain, read_live_json, Artifact, Package};
use serde_json::{json, Value};
use std::{fs, path::Path, process::Child};

fn exited(session: &Path) -> bool {
    session.join("run/target-exit.json").is_file()
        || (session.join("launcher-exit.json").is_file()
            && !session.join("run/target-spawning.json").exists()
            && !session.join("run/target-process.json").exists())
}

/// A launcher failure after spawning a target does not prove the target exited.
/// Never dispose that target's loaded DLLs without its completion record.
pub(super) fn cleanup(base: &Path, session: &Path) -> Result<(), String> {
    if session.parent() != Some(base)
        || !session
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("game-"))
    {
        return Err("拒绝回收非工具箱会话".into());
    }
    super::runtime_package::safe_path(session)?;
    plain(session, true)?;
    if !exited(session) {
        return Err("会话进程尚未确认退出".into());
    }
    let receipt = read_live_json(&session.join("package-manifest.json"))?;
    let manifest: Package = serde_json::from_value(receipt.clone()).map_err(|e| e.to_string())?;
    let runtime: Vec<_> = manifest
        .files
        .iter()
        .filter(|f| f.name.ends_with(".dll"))
        .cloned()
        .collect();
    let nr: Vec<Artifact> = receipt
        .get("nrFiles")
        .map(|files| serde_json::from_value(files.clone()))
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    // The launcher makes additional private copies of Streamline and NR DLLs.
    // Retain SDK logs in those directories, but reclaim every owned binary copy.
    cleanup_members(&session.join("run/runtime"), &runtime, false)?;
    cleanup_members(&session.join("run/nr"), &nr, false)?;
    cleanup_members(&session.join("package"), &manifest.files, true)
}

fn cleanup_members(bundle: &Path, files: &[Artifact], exact: bool) -> Result<(), String> {
    super::runtime_package::safe_path(bundle)?;
    if !bundle.try_exists().map_err(|e| e.to_string())? {
        return Ok(());
    }
    plain(bundle, true)?;
    // Validate every remaining member before removing anything. Missing files
    // are allowed so a Windows file-in-use failure can be retried next launch.
    for artifact in files {
        if artifact.name.is_empty()
            || artifact.name.contains(['/', '\\', ':'])
            || matches!(artifact.name.as_str(), "." | "..")
        {
            return Err("会话组件文件名无效".into());
        }
        let path = bundle.join(&artifact.name);
        if path.try_exists().map_err(|e| e.to_string())? {
            plain(&path, false)?;
            if super::streamline_fg::file_digest(&path)? != artifact.sha256 {
                return Err(format!("会话组件已改变，保留文件：{}", artifact.name));
            }
        }
    }
    for entry in fs::read_dir(bundle).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if exact && !files.iter().any(|f| entry.file_name() == f.name.as_str()) {
            return Err("会话组件目录包含额外文件，保留目录".into());
        }
    }
    for artifact in files {
        let path = bundle.join(&artifact.name);
        if path.try_exists().map_err(|e| e.to_string())? {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    if fs::read_dir(bundle)
        .map_err(|e| e.to_string())?
        .next()
        .is_none()
    {
        fs::remove_dir(bundle).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(super) fn reap_completed(base: &Path) {
    let Ok(entries) = fs::read_dir(base) else {
        return;
    };
    for entry in entries.flatten() {
        let session = entry.path();
        // Legacy snapshots without our ownership manifest remain untouched.
        if session.join("package-manifest.json").is_file() && exited(&session) {
            if let Err(error) = cleanup(base, &session) {
                tracing::warn!(%error, path = %session.display(), "会话组件回收未完成");
            }
        }
    }
}

pub(super) fn monitor(
    mut child: Child,
    session: std::path::PathBuf,
    base: std::path::PathBuf,
    runtime_files: Vec<fs::File>,
) {
    std::thread::spawn(move || {
        let _runtime_files = runtime_files;
        let exit = match child.wait() {
            Ok(status) => json!({"success":status.success(), "code":status.code()}),
            Err(error) => {
                tracing::warn!(%error, "无法确认画面增强启动器退出，保留会话组件");
                return;
            }
        };
        if let Err(error) = atomic_json(&session.join("launcher-exit.json"), &exit) {
            tracing::warn!(%error, "无法保存画面增强启动器退出状态");
        }
        if session.join("package-manifest.json").is_file() && exited(&session) {
            if let Err(error) = cleanup(&base, &session) {
                tracing::warn!(%error, "会话组件回收未完成，将在下次启动时重试");
            }
        }
    });
}

pub(super) fn status(session: &Path) -> Result<Value, String> {
    super::runtime_package::safe_path(session)?;
    let launcher = session.join("launcher-exit.json");
    let target = session.join("run/target-exit.json");
    let exit = if launcher.exists() {
        Some(read_live_json(&launcher)?)
    } else if target.exists() {
        Some(read_live_json(&target)?)
    } else {
        None
    };
    let (state, message) = if let Some(exit) = exit {
        if exit["success"] == true {
            ("finished", "本次模拟器会话已结束。".to_owned())
        } else {
            (
                "failed",
                format!(
                    "本次启动或模拟器运行失败（退出码 {}）。请查看 {} 和 {}。",
                    exit["code"],
                    session.join("launcher.error.log").display(),
                    session.join("run/target.stderr.log").display()
                ),
            )
        }
    } else if session.join("run/target-process.json").exists() {
        (
            "started",
            "模拟器进程已启动，等待游戏画面与增强状态。".to_owned(),
        )
    } else {
        (
            "launching",
            "启动器正在准备，尚未确认模拟器进程启动。".to_owned(),
        )
    };
    Ok(json!({"connected":false, "launchState":state, "launchMessage":message}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        let base = tempfile::tempdir().unwrap();
        let session = base.path().join("game-test");
        fs::create_dir_all(session.join("package")).unwrap();
        fs::create_dir(session.join("run")).unwrap();
        fs::write(session.join("package/layer.dll"), b"verified").unwrap();
        fs::write(session.join("launcher.log"), b"keep evidence").unwrap();
        atomic_json(
            &session.join("package-manifest.json"),
            &json!({
                "version":"test", "files":[{"name":"layer.dll",
                "sha256":super::super::runtime_package::hash(b"verified")}]
            }),
        )
        .unwrap();
        (base, session)
    }

    #[test]
    fn pending_start_late_failure_and_normal_exit_are_distinct() {
        let (_base, session) = fixture();
        assert_eq!(status(&session).unwrap()["launchState"], "launching");
        atomic_json(&session.join("run/target-process.json"), &json!({"pid":42})).unwrap();
        assert_eq!(status(&session).unwrap()["launchState"], "started");
        atomic_json(
            &session.join("run/target-exit.json"),
            &json!({"success":true,"code":0}),
        )
        .unwrap();
        assert_eq!(status(&session).unwrap()["launchState"], "finished");
        atomic_json(
            &session.join("launcher-exit.json"),
            &json!({"success":false,"code":1}),
        )
        .unwrap();
        assert_eq!(status(&session).unwrap()["launchState"], "failed");
    }

    #[test]
    fn cleanup_requires_target_completion_and_preserves_evidence() {
        let (base, session) = fixture();
        assert!(cleanup(base.path(), &session).is_err());
        atomic_json(&session.join("run/target-process.json"), &json!({"pid":42})).unwrap();
        atomic_json(
            &session.join("launcher-exit.json"),
            &json!({"success":false,"code":1}),
        )
        .unwrap();
        assert!(cleanup(base.path(), &session).is_err());
        atomic_json(
            &session.join("run/target-exit.json"),
            &json!({"success":false,"code":1}),
        )
        .unwrap();
        cleanup(base.path(), &session).unwrap();
        assert!(!session.join("package").exists());
        assert_eq!(
            fs::read(session.join("launcher.log")).unwrap(),
            b"keep evidence"
        );
        cleanup(base.path(), &session).unwrap();
    }

    #[test]
    fn cleanup_rejects_changed_extra_and_out_of_root_files() {
        let (base, session) = fixture();
        atomic_json(
            &session.join("launcher-exit.json"),
            &json!({"success":false,"code":1}),
        )
        .unwrap();
        fs::write(session.join("package/layer.dll"), b"modified").unwrap();
        assert!(cleanup(base.path(), &session).is_err());
        fs::write(session.join("package/layer.dll"), b"verified").unwrap();
        fs::write(session.join("package/user.txt"), b"keep").unwrap();
        assert!(cleanup(base.path(), &session).is_err());
        assert!(session.join("package/layer.dll").exists());
        assert!(cleanup(&base.path().join("other"), &session).is_err());
    }

    #[test]
    fn completed_snapshots_are_reaped_after_restart() {
        let (base, session) = fixture();
        atomic_json(
            &session.join("run/target-exit.json"),
            &json!({"success":true,"code":0}),
        )
        .unwrap();
        reap_completed(base.path());
        assert!(!session.join("package").exists());
    }

    #[test]
    fn launcher_private_copies_are_reclaimed_but_sdk_logs_remain() {
        let (base, session) = fixture();
        for directory in ["runtime", "nr"] {
            fs::create_dir(session.join("run").join(directory)).unwrap();
            fs::write(
                session.join("run").join(directory).join("layer.dll"),
                b"verified",
            )
            .unwrap();
        }
        fs::write(session.join("run/runtime/sdk.log"), b"evidence").unwrap();
        let path = session.join("package-manifest.json");
        let mut receipt = read_live_json(&path).unwrap();
        receipt["nrFiles"] = receipt["files"].clone();
        atomic_json(&path, &receipt).unwrap();
        atomic_json(
            &session.join("run/target-exit.json"),
            &json!({"success":true,"code":0}),
        )
        .unwrap();
        cleanup(base.path(), &session).unwrap();
        assert!(!session.join("package").exists());
        assert!(!session.join("run/nr").exists());
        assert!(!session.join("run/runtime/layer.dll").exists());
        assert_eq!(
            fs::read(session.join("run/runtime/sdk.log")).unwrap(),
            b"evidence"
        );
    }

    #[test]
    fn failed_pid_record_and_partial_cleanup_never_delete_unconfirmed_target_files() {
        let (base, session) = fixture();
        atomic_json(
            &session.join("run/target-spawning.json"),
            &json!({"requested":true}),
        )
        .unwrap();
        atomic_json(
            &session.join("launcher-exit.json"),
            &json!({"success":false,"code":1}),
        )
        .unwrap();
        assert!(cleanup(base.path(), &session).is_err());
        assert!(session.join("package/layer.dll").exists());
        atomic_json(
            &session.join("run/target-exit.json"),
            &json!({"success":false,"code":1}),
        )
        .unwrap();
        fs::remove_file(session.join("package/layer.dll")).unwrap();
        cleanup(base.path(), &session).unwrap();
        assert!(!session.join("package").exists());
    }
}

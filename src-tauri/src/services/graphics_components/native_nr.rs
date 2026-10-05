//! Immutable NR installs from the pinned runtime Release. Games load private snapshots.
use super::streamline_fg::file_digest;
use super::streamline_install::{self, atomic_json, plain, safe_dir};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
#[path = "../../../crates/streamline-nr-contract.rs"]
mod contract;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    schema_version: u32,
    runtime_sha256: String,
    source: PathBuf,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    pub supported: bool,
    pub package_ready: bool,
    pub installed: bool,
    pub runtime_sha256: Option<String>,
    pub runtime_version: &'static str,
    pub architecture: &'static str,
    pub depth: &'static str,
    pub message: String,
}
fn base() -> PathBuf {
    streamline_install::root().join("nr-runtimes")
}
pub(super) fn supported_package(package: &streamline_install::Package) -> bool {
    package
        .files
        .iter()
        .any(|f| f.name == "nvngx_dlssnr.dll" && contract::RUNTIMES.contains(&f.sha256.as_str()))
}
fn digest(path: &Path, allowed: &[&str]) -> Result<String, String> {
    if !path.is_absolute() {
        return Err("NR 组件缓存路径无效".into());
    }
    plain(path, false)?;
    let hash = file_digest(path)?;
    if !allowed.contains(&hash.as_str()) {
        return Err("NR 运行库未通过校验；目前只支持已实测的 Windows x64 310.8.0 运行库".into());
    }
    Ok(hash)
}
fn owned(dir: &Path, expected: &str, allowed: &[&str]) -> Result<PathBuf, String> {
    plain(dir, true)?;
    let record = dir.join("installation.json");
    plain(&record, false)?;
    let receipt: Receipt = serde_json::from_slice(&fs::read(record).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if receipt.schema_version != 1 || receipt.runtime_sha256 != expected {
        return Err("NR 安装记录不匹配，请检查组件文件".into());
    }
    let runtime = dir.join("nvngx_dlssnr.dll");
    if digest(&runtime, allowed)? != expected {
        return Err("NR 组件已改变".into());
    }
    Ok(runtime)
}
fn current_at(base: &Path, allowed: &[&str]) -> Result<Option<PathBuf>, String> {
    if !base.exists() {
        return Ok(None);
    }
    plain(base, true)?;
    let pointer = base.join("current.json");
    if !pointer.exists() {
        return Ok(None);
    }
    plain(&pointer, false)?;
    let v: serde_json::Value =
        serde_json::from_slice(&fs::read(pointer).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let hash = v["runtimeSha256"]
        .as_str()
        .filter(|h| allowed.contains(h))
        .ok_or("NR 组件记录无效")?;
    owned(&base.join(hash), hash, allowed).map(Some)
}
pub(super) fn current_runtime() -> Result<Option<PathBuf>, String> {
    current_at(&base(), &contract::RUNTIMES)
}
pub fn status() -> Component {
    let supported = cfg!(all(windows, target_arch = "x86_64"));
    let bridge = streamline_install::native_bridge().and_then(|p| digest(&p, &[contract::BRIDGE]));
    let runtime = current_runtime();
    let installed = matches!(runtime, Ok(Some(_)));
    let runtime_sha256 = runtime
        .as_ref()
        .ok()
        .and_then(|p| p.as_ref())
        .and_then(|p| file_digest(p).ok());
    let message = if !supported {
        "NR 仅支持 Windows x64".into()
    } else if let Err(e) = runtime {
        e
    } else if installed {
        "已安装 NR（DLSS 5）运行库。使用合成深度和硬件光流，画质兼容性仍属实验。".into()
    } else if bridge.is_err() && !super::streamline_download::available() {
        bridge.as_ref().unwrap_err().clone()
    } else {
        "点击“下载并安装 NR 组件”，工具将自动下载、校验并安装运行库。".into()
    };
    Component {
        supported,
        package_ready: installed || bridge.is_ok() || super::streamline_download::available(),
        installed,
        runtime_sha256,
        runtime_version: "310.8.0",
        architecture: "x64",
        depth: "synthetic_constant",
        message,
    }
}
fn import_at(base: &Path, source: &Path, allowed: &[&str]) -> Result<(), String> {
    let hash = digest(source, allowed)?;
    safe_dir(base)?;
    let destination = base.join(&hash);
    if destination.exists() {
        owned(&destination, &hash, allowed)?;
    } else {
        let stage = tempfile::Builder::new()
            .prefix(".import-")
            .tempdir_in(base)
            .map_err(|e| e.to_string())?;
        fs::copy(source, stage.path().join("nvngx_dlssnr.dll")).map_err(|e| e.to_string())?;
        if digest(&stage.path().join("nvngx_dlssnr.dll"), allowed)? != hash {
            return Err("安装期间运行库已改变，请重新下载并安装".into());
        }
        let receipt = Receipt {
            schema_version: 1,
            runtime_sha256: hash.clone(),
            source: source.to_owned(),
        };
        fs::write(
            stage.path().join("installation.json"),
            serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::rename(stage.path(), &destination).map_err(|e| e.to_string())?;
    }
    atomic_json(
        &base.join("current.json"),
        &serde_json::json!({"runtimeSha256":hash}),
    )
}
pub fn import(source: PathBuf) -> Result<Component, String> {
    let _lock = streamline_install::OPERATION
        .lock()
        .map_err(|e| e.to_string())?;
    let _store = streamline_install::lock_store()?;
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Err("NR 仅支持 Windows x64".into());
    }
    let bridge = streamline_install::native_bridge()?;
    digest(&bridge, &[contract::BRIDGE])?;
    import_at(&base(), &source, &contract::RUNTIMES)?;
    Ok(status())
}
pub async fn install(
    reporter: crate::services::installer::InstallReporter,
) -> Result<Component, String> {
    let package = super::streamline_update::candidate();
    let source = super::streamline_download::ensure_package(&package, reporter).await?;
    tauri::async_runtime::spawn_blocking(move || import_package(&source, &package))
        .await
        .map_err(|e| e.to_string())?
}
pub(super) fn import_package(
    source: &Path,
    package: &streamline_install::Package,
) -> Result<Component, String> {
    let _lock = streamline_install::OPERATION
        .lock()
        .map_err(|e| e.to_string())?;
    let _store = streamline_install::lock_store()?;
    super::streamline_update::validate_selected(package)?;
    streamline_install::verify_artifacts(source, &package.files)?;
    if !package.native_nr {
        return Err("组件包未包含 NR".into());
    }
    import_at(
        &base(),
        &source.join("nvngx_dlssnr.dll"),
        &contract::RUNTIMES,
    )?;
    Ok(status())
}
fn uninstall_at(base: &Path, allowed: &[&str]) -> Result<(), String> {
    if !base.exists() {
        return Ok(());
    }
    plain(base, true)?;
    // Inspect every owned version before deleting anything; retain modified/user files.
    let mut versions = Vec::new();
    for entry in fs::read_dir(base).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.file_name().and_then(|s| s.to_str()) == Some("current.json") {
            plain(&path, false)?;
            continue;
        }
        let hash = path
            .file_name()
            .and_then(|s| s.to_str())
            .filter(|h| allowed.contains(h))
            .ok_or("NR 目录包含未知文件，保留原文件并停止卸载")?;
        owned(&path, hash, allowed)?;
        for entry in fs::read_dir(&path).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if !matches!(
                path.file_name().and_then(|s| s.to_str()),
                Some("installation.json" | "nvngx_dlssnr.dll")
            ) {
                return Err("NR 目录包含用户文件，停止卸载".into());
            }
            plain(&path, false)?;
        }
        versions.push(path);
    }
    for dir in versions {
        fs::remove_file(dir.join("nvngx_dlssnr.dll")).map_err(|e| e.to_string())?;
        fs::remove_file(dir.join("installation.json")).map_err(|e| e.to_string())?;
        fs::remove_dir(dir).map_err(|e| e.to_string())?;
    }
    let pointer = base.join("current.json");
    if pointer.exists() {
        fs::remove_file(pointer).map_err(|e| e.to_string())?;
    }
    fs::remove_dir(base).map_err(|e| e.to_string())
}
pub fn uninstall() -> Result<Component, String> {
    let _lock = streamline_install::OPERATION
        .lock()
        .map_err(|e| e.to_string())?;
    let _store = streamline_install::lock_store()?;
    uninstall_at(&base(), &contract::RUNTIMES)?;
    Ok(status())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn online_bundle_matches_the_gpu_tested_nr_and_bridge_contract() {
        let package = streamline_install::package();
        if !package.online() {
            return;
        }
        let nr = package
            .files
            .iter()
            .find(|f| f.name == "nvngx_dlssnr.dll")
            .unwrap();
        assert!(contract::RUNTIMES.contains(&nr.sha256.as_str()));
        let bridge = package
            .files
            .iter()
            .find(|f| f.name == "nvngx.dll")
            .unwrap();
        assert_eq!(bridge.sha256, contract::BRIDGE);
    }
    #[test]
    fn immutable_import_rejects_modified_or_unowned_files_and_keeps_source() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.dll");
        fs::write(&source, b"verified test runtime").unwrap();
        let hash = file_digest(&source).unwrap();
        let allowed = [hash.as_str()];
        let base = temp.path().join("managed");
        import_at(&base, &source, &allowed).unwrap();
        let runtime = current_at(&base, &allowed).unwrap().unwrap();
        fs::write(&runtime, b"modified").unwrap();
        assert!(import_at(&base, &source, &allowed).is_err());
        assert!(uninstall_at(&base, &allowed).is_err());
        assert_eq!(fs::read(&runtime).unwrap(), b"modified");
        fs::copy(&source, &runtime).unwrap();
        let user = runtime.parent().unwrap().join("user.txt");
        fs::write(&user, b"keep").unwrap();
        assert!(uninstall_at(&base, &allowed).is_err());
        assert!(base.join("current.json").exists());
        fs::remove_file(user).unwrap();
        uninstall_at(&base, &allowed).unwrap();
        assert!(!base.exists());
        assert!(source.is_file());
    }
    #[test]
    fn unknown_runtime_or_pointer_never_selects_external_libraries() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("nr.dll");
        fs::write(&source, b"unknown").unwrap();
        let base = temp.path().join("managed");
        assert!(import_at(&base, &source, &contract::RUNTIMES).is_err());
        assert!(!base.exists());
        fs::create_dir(&base).unwrap();
        fs::write(
            base.join("current.json"),
            br#"{"runtimeSha256":"../outside"}"#,
        )
        .unwrap();
        assert!(current_at(&base, &contract::RUNTIMES).is_err());
    }
}

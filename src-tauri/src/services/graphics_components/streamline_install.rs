//! Local, pinned experimental package. Each launch uses a private snapshot.
use super::streamline_fg::{file_digest, target_policy};
use crate::{config::effective_config_dir, models::graphics_components::GraphicsApi};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
};
static OPERATION: Mutex<()> = Mutex::new(());
fn lock_store() -> Result<fs::File, String> {
    safe_dir(&root())?;
    let path = root().join(".operation.lock");
    if path.exists() {
        plain(&path, false)?;
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    fs2::FileExt::try_lock_exclusive(&file)
        .map_err(|_| "另一个组件操作正在进行，请稍后重试".to_owned())?;
    Ok(file)
}
#[derive(Deserialize)]
struct Package {
    version: String,
    files: Vec<Artifact>,
}
#[derive(Deserialize)]
struct Artifact {
    name: String,
    sha256: String,
}
#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Receipt {
    executable: PathBuf,
    target_sha256: String,
    version: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub message: String,
    pub session: Option<PathBuf>,
}
fn package() -> Package {
    serde_json::from_str(include_str!("streamline-package.json")).expect("embedded package")
}
fn root() -> PathBuf {
    effective_config_dir()
        .join("graphics")
        .join("streamline-fg")
}
fn key(exe: &Path) -> String {
    format!(
        "{:x}",
        Sha256::digest(exe.to_string_lossy().to_lowercase().as_bytes())
    )
}
fn destination(base: &Path, exe: &Path) -> PathBuf {
    base.join(key(exe)).join(package().version)
}
fn plain(path: &Path, directory: bool) -> Result<(), String> {
    let m = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return Err("拒绝操作重解析点".into());
        }
    }
    if m.file_type().is_symlink() || m.is_dir() != directory {
        return Err(format!("无效组件路径：{}", path.display()));
    }
    Ok(())
}
fn safe_dir(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            safe_dir(parent)?;
        }
    }
    if !path.exists() {
        fs::create_dir(path).map_err(|e| e.to_string())?;
    }
    plain(path, true)
}
fn verify(dir: &Path) -> Result<(), String> {
    plain(dir, true)?;
    for f in package().files {
        let path = dir.join(&f.name);
        plain(&path, false).map_err(|e| format!("组件文件不可用 {}：{e}", path.display()))?;
        if file_digest(&path)? != f.sha256 {
            return Err(format!("组件校验失败：{}", f.name));
        }
    }
    Ok(())
}
fn source() -> PathBuf {
    if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/streamline-fg-package")
    } else {
        std::env::current_exe()
            .unwrap_or_default()
            .parent()
            .unwrap_or(Path::new("."))
            .join("streamline-fg-package")
    }
}
fn package_availability_at(path: &Path) -> Result<(), String> {
    verify(path).map_err(|error| format!("组件包未就绪：{}。{error}。请将与此工具版本配套的 streamline-fg-package 文件夹放到工具 EXE 同级目录，然后重新检查。", path.display()))
}
pub(super) fn availability() -> Result<(), String> {
    package_availability_at(&source())
}
pub(super) fn planned(exe: &Path) -> PathBuf {
    destination(&root(), exe)
}
fn receipt(exe: &Path) -> Result<Receipt, String> {
    Ok(Receipt {
        executable: exe.to_owned(),
        target_sha256: file_digest(exe)?,
        version: package().version,
    })
}
fn owned(dir: &Path, exe: &Path, current: bool) -> Result<(), String> {
    let mut ancestor = Some(dir);
    while let Some(path) = ancestor {
        plain(path, true)?;
        ancestor = path.parent();
    }
    let record = dir.join("installation.json");
    plain(&record, false)?;
    let r: Receipt = serde_json::from_slice(&fs::read(record).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if r.executable != exe || r.version != package().version {
        return Err("安装记录不属于当前目标".into());
    }
    if current && r != receipt(exe)? {
        return Err("模拟器已更新，请卸载组件后重新检测、安装".into());
    }
    verify(dir)
}
pub(super) fn state(exe: &Path) -> &'static str {
    let dir = planned(exe);
    if !dir.exists() {
        "unmanaged"
    } else if owned(&dir, exe, true).is_ok() {
        "installed"
    } else {
        "damaged"
    }
}
fn authorize(
    exe: &Path,
    api: GraphicsApi,
    consent: bool,
    expected: &str,
) -> Result<PathBuf, String> {
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Err("仅支持 Windows x64".into());
    }
    if api != GraphicsApi::Vulkan {
        return Err("需要选择 Vulkan".into());
    }
    let exe = exe.canonicalize().map_err(|e| e.to_string())?;
    target_policy::validate_executable(&exe)?;
    let hash = file_digest(&exe)?;
    if hash != expected {
        return Err("主程序已改变，请重新检查安装条件".into());
    }
    target_policy::classify(&hash, true).authorize(consent)?;
    Ok(exe)
}
fn copy_package(from: &Path, to: &Path) -> Result<(), String> {
    verify(from)?;
    for f in package().files {
        fs::copy(from.join(&f.name), to.join(&f.name)).map_err(|e| e.to_string())?;
    }
    verify(to)
}
fn install_at(base: &Path, src: &Path, exe: &Path, expected: &str) -> Result<(), String> {
    if file_digest(exe)? != expected {
        return Err("主程序已改变，请重新检测".into());
    }
    let dest = destination(base, exe);
    if dest.exists() {
        return owned(&dest, exe, true);
    }
    let parent = dest.parent().ok_or("无安装目录")?;
    safe_dir(parent)?;
    let stage = tempfile::Builder::new()
        .prefix(".staging-")
        .tempdir_in(parent)
        .map_err(|e| e.to_string())?;
    copy_package(src, stage.path())?;
    if file_digest(exe)? != expected {
        return Err("安装期间主程序发生变化".into());
    }
    fs::write(
        stage.path().join("installation.json"),
        serde_json::to_vec_pretty(&receipt(exe)?).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(stage.path(), &dest).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn install(
    exe: PathBuf,
    api: GraphicsApi,
    consent: bool,
    expected: String,
) -> Result<Operation, String> {
    let _lock = OPERATION.lock().map_err(|e| e.to_string())?;
    let _store_lock = lock_store()?;
    let exe = authorize(&exe, api, consent, &expected)?;
    install_at(&root(), &source(), &exe, &expected)?;
    Ok(Operation {
        message: "组件已安装。请通过“以 FG 启动”打开模拟器。".into(),
        session: None,
    })
}
fn uninstall_at(base: &Path, exe: &Path) -> Result<(), String> {
    let dir = destination(base, exe);
    if !dir.exists() {
        return Ok(());
    }
    owned(&dir, exe, false)?;
    let mut names: Vec<String> = package().files.into_iter().map(|f| f.name).collect();
    names.push("installation.json".into());
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let e = entry.map_err(|e| e.to_string())?;
        if !names.iter().any(|n| e.file_name() == n.as_str()) {
            return Err("目录存在额外文件，保留安装目录，请手动检查".into());
        }
    }
    // Only the exact owned files are removed. Session snapshots and user files remain.
    for name in names {
        fs::remove_file(dir.join(name)).map_err(|e| e.to_string())?;
    }
    fs::remove_dir(dir).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn uninstall(exe: PathBuf) -> Result<Operation, String> {
    let _lock = OPERATION.lock().map_err(|e| e.to_string())?;
    let _store_lock = lock_store()?;
    let exe = exe.canonicalize().map_err(|e| e.to_string())?;
    uninstall_at(&root(), &exe)?;
    Ok(Operation {
        message: "已卸载组件；当前游戏会话和诊断记录保留，普通启动不加载 FG。".into(),
        session: None,
    })
}
pub fn launch(
    exe: PathBuf,
    api: GraphicsApi,
    consent: bool,
    expected: String,
    game: Option<PathBuf>,
) -> Result<Operation, String> {
    let _lock = OPERATION.lock().map_err(|e| e.to_string())?;
    let _store_lock = lock_store()?;
    let exe = authorize(&exe, api, consent, &expected)?;
    if let Some(path) = &game {
        if !path.is_absolute() || !path.is_file() {
            return Err("游戏路径无效".into());
        }
    }
    let installed = planned(&exe);
    owned(&installed, &exe, true)?;
    let sessions = root().join("sessions");
    safe_dir(&sessions)?;
    let stage = tempfile::Builder::new()
        .prefix("game-")
        .tempdir_in(&sessions)
        .map_err(|e| e.to_string())?;
    let bundle = stage.path().join("package");
    fs::create_dir(&bundle).map_err(|e| e.to_string())?;
    copy_package(&installed, &bundle)?;
    let mut cmd = Command::new(bundle.join("streamline-layer-probe.exe"));
    cmd.arg("--target-probe")
        .arg("--fg")
        .arg("--reference-params")
        .arg("--expected-target-sha256")
        .arg(&expected)
        .arg("--target")
        .arg(&exe)
        .arg("--layer")
        .arg(bundle.join("streamline_probe_layer.dll"))
        .arg("--runtime")
        .arg(&bundle)
        .arg("--session")
        .arg(stage.path().join("run"));
    if let Some(path) = game {
        cmd.arg("--game").arg(path);
    }
    if consent {
        cmd.arg("--allow-unverified-target");
    }
    cmd.stdin(Stdio::null())
        .stdout(fs::File::create(stage.path().join("launcher.log")).map_err(|e| e.to_string())?)
        .stderr(
            fs::File::create(stage.path().join("launcher.error.log")).map_err(|e| e.to_string())?,
        );
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let session = stage.keep();
    let pointer = root().join(key(&exe)).join("live-session.json");
    atomic_json(&pointer, &serde_json::json!({"session":session}))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !session.join("run/target-process.json").exists() && std::time::Instant::now() < deadline
    {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Err(format!(
                "启动失败（{status}）。请查看 {}",
                session.join("launcher.error.log").display()
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(Operation{message:"已发出 FG 启动请求，请在模拟器中打开游戏。实际 FG 状态由运行时检查；失焦时暂停 FG，回到前台可恢复；调整窗口仍可能使本次会话停用 FG。".into(),session:Some(session)})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Explicit local GPU smoke; launches the game named in FG_SMOKE_GAME"]
    fn local_game_launch() {
        let exe = PathBuf::from(std::env::var_os("FG_SMOKE_TARGET").expect("FG_SMOKE_TARGET"));
        let game = std::env::var_os("FG_SMOKE_GAME").map(PathBuf::from);
        let expected = file_digest(&exe).unwrap();
        install(exe.clone(), GraphicsApi::Vulkan, false, expected.clone()).unwrap();
        let result = launch(exe, GraphicsApi::Vulkan, false, expected, game).unwrap();
        println!("FG_SMOKE_SESSION={}", result.session.unwrap().display());
    }
    #[test]
    #[ignore = "Explicit local live-session control; target and desired mode supplied by caller"]
    fn local_live_control() {
        let exe = PathBuf::from(std::env::var_os("FG_SMOKE_TARGET").expect("FG_SMOKE_TARGET"));
        let enabled = std::env::var("FG_SMOKE_ENABLED").ok().map(|v| v == "1");
        let result = live(exe, enabled).unwrap();
        println!("FG_LIVE={result}");
        assert_eq!(result["connected"], true);
    }
    #[test]
    fn missing_package_reports_location_and_missing_artifact() {
        let temp = tempfile::tempdir().unwrap();
        let absent = temp.path().join("streamline-fg-package");
        let error = package_availability_at(&absent).unwrap_err();
        assert!(error.contains(&absent.display().to_string()));
        fs::create_dir(&absent).unwrap();
        let error = package_availability_at(&absent).unwrap_err();
        assert!(error.contains(&package().files[0].name));
        assert!(error.contains("重新检查"));
    }
    #[test]
    fn live_snapshots_replace_atomically_and_reject_oversize_records() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("control.json");
        atomic_json(&path, &serde_json::json!({"enabled":false,"revision":1})).unwrap();
        atomic_json(&path, &serde_json::json!({"enabled":true,"revision":2})).unwrap();
        assert_eq!(read_live_json(&path).unwrap()["revision"], 2);
        fs::write(&path, vec![b' '; 131073]).unwrap();
        assert!(read_live_json(&path).is_err());
        assert!(read_live_json(temp.path()).is_err());
    }
    #[test]
    fn pinned_install_roundtrip_and_tamper_are_safe() {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path().join("test.exe");
        fs::write(&exe, b"target").unwrap();
        let src = source();
        if !src.exists() {
            return;
        } // Local package is not redistributed in source checkouts.
        let base = temp.path().join("managed");
        install_at(&base, &src, &exe, &file_digest(&exe).unwrap()).unwrap();
        let dest = destination(&base, &exe);
        owned(&dest, &exe, true).unwrap();
        let artifact = package().files[0].name.clone();
        fs::write(dest.join(&artifact), b"tampered").unwrap();
        assert!(owned(&dest, &exe, true).is_err());
        assert!(uninstall_at(&base, &exe).is_err());
        fs::copy(src.join(&artifact), dest.join(&artifact)).unwrap();
        fs::write(dest.join("user.txt"), b"keep").unwrap();
        assert!(uninstall_at(&base, &exe).is_err());
        assert!(dest.join("installation.json").exists());
        fs::remove_file(dest.join("user.txt")).unwrap();
        fs::write(&exe, b"updated").unwrap();
        assert!(owned(&dest, &exe, true).is_err());
        uninstall_at(&base, &exe).unwrap();
        assert!(!dest.exists());
        assert_eq!(fs::read(&exe).unwrap(), b"updated");
        let broken = temp.path().join("broken");
        fs::create_dir(&broken).unwrap();
        assert!(install_at(&base, &broken, &exe, &file_digest(&exe).unwrap()).is_err());
        assert!(!dest.exists());
    }
}

// Only toolbox-created sessions can be controlled; the frontend never supplies a file path.
fn atomic_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    use std::io::Write;
    if path.exists() {
        plain(path, false)?;
    }
    let mut tmp = tempfile::NamedTempFile::new_in(path.parent().ok_or("无效会话路径")?)
        .map_err(|e| e.to_string())?;
    tmp.write_all(&serde_json::to_vec(value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    tmp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
fn read_live_json(path: &Path) -> Result<serde_json::Value, String> {
    plain(path, false)?;
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > 131072 {
        return Err("运行记录过大".into());
    }
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
pub fn live(exe: PathBuf, enabled: Option<bool>) -> Result<serde_json::Value, String> {
    let _lock = OPERATION.lock().map_err(|e| e.to_string())?;
    let _store_lock = lock_store()?;
    let exe = exe.canonicalize().map_err(|e| e.to_string())?;
    let target = root().join(key(&exe));
    if !target.exists() {
        return Ok(serde_json::json!({"connected":false}));
    }
    safe_dir(&target)?;
    let pointer = target.join("live-session.json");
    if !pointer.exists() {
        return Ok(serde_json::json!({"connected":false}));
    }
    let pointer = read_live_json(&pointer)?;
    let session = PathBuf::from(pointer["session"].as_str().ok_or("会话记录无效")?);
    if session.parent() != Some(root().join("sessions").as_path()) {
        return Err("会话不属于工具箱".into());
    }
    safe_dir(&session)?;
    let run = session.join("run");
    if !run.exists() {
        return Ok(serde_json::json!({"connected":false}));
    }
    safe_dir(&run)?;
    let path = run.join("telemetry.json");
    if !path.exists() {
        return Ok(serde_json::json!({"connected":false}));
    }
    let mut v = read_live_json(&path)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis() as u64;
    let updated = v["updatedAt"].as_u64().unwrap_or(0);
    let connected = v["protocol"] == 1
        && updated <= now
        && now - updated < 3500
        && !run.join("target-exit.json").exists();
    v["connected"] = connected.into();
    if !connected {
        v["active"] = false.into();
        v["fresh"] = false.into();
    }
    if let Some(on) = enabled {
        if !connected {
            return Err("游戏未连接，请通过工具箱重新启动游戏".into());
        }
        let command = run.join("control.json");
        let previous = if command.exists() {
            read_live_json(&command)?["revision"].as_u64().unwrap_or(0)
        } else {
            0
        };
        let revision = now.max(previous.saturating_add(1));
        atomic_json(
            &command,
            &serde_json::json!({"enabled":on,"revision":revision}),
        )?;
        v["sentRevision"] = revision.into();
    }
    Ok(v)
}

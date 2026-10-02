//! Pinned experimental package from the managed download cache. Each launch uses a private snapshot.
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
pub(super) static OPERATION: Mutex<()> = Mutex::new(());
pub(super) fn lock_store() -> Result<fs::File, String> {
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
#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Package {
    pub version: String,
    pub files: Vec<Artifact>,
    #[serde(default)]
    pub download: Option<super::runtime_package::Asset>,
    #[serde(default)]
    pub parts: Vec<PackagePart>,
    #[serde(default)]
    pub native_nr: bool,
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub launcher_protocol: u32,
}
#[derive(Clone, Deserialize, Serialize)]
pub(super) struct PackagePart {
    pub download: super::runtime_package::Asset,
    pub files: Vec<String>,
}
impl Package {
    pub(super) fn online(&self) -> bool {
        self.download.is_some() || !self.parts.is_empty()
    }
}
#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Artifact {
    pub name: String,
    pub sha256: String,
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
pub(super) fn package() -> Package {
    serde_json::from_str(include_str!("streamline-package.json")).expect("embedded package")
}
pub(super) fn root() -> PathBuf {
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
fn selected_at(base: &Path, exe: &Path) -> Result<(PathBuf, Package), String> {
    let pointer = base.join(key(exe)).join("current-package.json");
    super::runtime_package::safe_path(&pointer)?;
    if !pointer.exists() {
        return Ok((destination(base, exe), package()));
    }
    let bytes = read_live_json(&pointer)?;
    let package: Package = serde_json::from_value(bytes).map_err(|e| e.to_string())?;
    super::streamline_update::validate_selected(&package)?;
    Ok((base.join(key(exe)).join(&package.version), package))
}
pub(super) fn installed_package(exe: &Path) -> Result<Package, String> {
    selected_at(&root(), exe).map(|(_, p)| p)
}
pub(super) fn plain(path: &Path, directory: bool) -> Result<(), String> {
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
pub(super) fn safe_dir(path: &Path) -> Result<(), String> {
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
pub(super) fn verify(dir: &Path) -> Result<(), String> {
    verify_artifacts(dir, &package().files)
}
pub(super) fn verify_artifacts(dir: &Path, files: &[Artifact]) -> Result<(), String> {
    plain(dir, true)?;
    for f in files {
        let path = dir.join(&f.name);
        plain(&path, false).map_err(|e| format!("组件文件不可用 {}：{e}", path.display()))?;
        if file_digest(&path)? != f.sha256 {
            return Err(format!("组件校验失败：{}", f.name));
        }
    }
    Ok(())
}
pub(super) fn source() -> PathBuf {
    if package().online() {
        return super::streamline_download::cached_source();
    }
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
    verify(path).map_err(|error| {
        format!(
            "组件包未就绪：{}。{error}。请点击“下载并安装”，工具将自动下载并校验配套组件。",
            path.display()
        )
    })
}
pub(super) fn availability() -> Result<(), String> {
    package_availability_at(&source())
}
pub(super) fn native_bridge() -> Result<PathBuf, String> {
    if !package().native_nr {
        return Err("当前组件包未包含原生 NR，请更新画面增强组件包".into());
    }
    availability()?;
    let bridge = source().join("nvngx.dll");
    plain(&bridge, false)?;
    Ok(bridge)
}
pub(super) fn planned(exe: &Path) -> PathBuf {
    selected_at(&root(), exe)
        .map(|(p, _)| p)
        .unwrap_or_else(|_| destination(&root(), exe))
}
pub(super) fn planned_package(exe: &Path, package: &Package) -> PathBuf {
    root().join(key(exe)).join(&package.version)
}
fn receipt_for(exe: &Path, package: &Package) -> Result<Receipt, String> {
    Ok(Receipt {
        executable: exe.to_owned(),
        target_sha256: file_digest(exe)?,
        version: package.version.clone(),
    })
}
#[cfg(test)]
fn owned(dir: &Path, exe: &Path, current: bool) -> Result<(), String> {
    owned_for(dir, exe, current, &package())
}
fn owned_for(dir: &Path, exe: &Path, current: bool, package: &Package) -> Result<(), String> {
    let mut ancestor = Some(dir);
    while let Some(path) = ancestor {
        plain(path, true)?;
        ancestor = path.parent();
    }
    let record = dir.join("installation.json");
    plain(&record, false)?;
    let r: Receipt = serde_json::from_slice(&fs::read(record).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if r.executable != exe || r.version != package.version {
        return Err("安装记录不属于当前目标".into());
    }
    if current && r != receipt_for(exe, package)? {
        return Err("模拟器已更新，请卸载组件后重新检测、安装".into());
    }
    verify_artifacts(dir, &package.files)
}
pub(super) fn state(exe: &Path) -> &'static str {
    let Ok((dir, package)) = selected_at(&root(), exe) else {
        return "damaged";
    };
    if !dir.exists() {
        "unmanaged"
    } else if owned_for(&dir, exe, true, &package).is_ok() {
        "installed"
    } else {
        "damaged"
    }
}
pub(crate) fn authorize(
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
fn copy_package_for(from: &Path, to: &Path, package: &Package) -> Result<(), String> {
    verify_artifacts(from, &package.files)?;
    for f in &package.files {
        fs::copy(from.join(&f.name), to.join(&f.name)).map_err(|e| e.to_string())?;
    }
    verify_artifacts(to, &package.files)
}
#[cfg(test)]
pub(super) fn install_at(
    base: &Path,
    src: &Path,
    exe: &Path,
    expected: &str,
) -> Result<(), String> {
    install_package_at(base, src, exe, expected, &package(), false)
}
#[cfg(test)]
pub(super) fn install_release_at(
    base: &Path,
    src: &Path,
    exe: &Path,
    expected: &str,
    package: &Package,
) -> Result<(), String> {
    install_package_at(base, src, exe, expected, package, true)
}
fn install_package_at(
    base: &Path,
    src: &Path,
    exe: &Path,
    expected: &str,
    package: &Package,
    select: bool,
) -> Result<(), String> {
    super::streamline_update::validate_selected(package)?;
    if file_digest(exe)? != expected {
        return Err("主程序已改变，请重新检测".into());
    }
    let dest = base.join(key(exe)).join(&package.version);
    if dest.exists() {
        owned_for(&dest, exe, true, package)?;
        if select {
            atomic_json(
                &base.join(key(exe)).join("current-package.json"),
                &serde_json::to_value(package).map_err(|e| e.to_string())?,
            )?;
        }
        return Ok(());
    }
    let parent = dest.parent().ok_or("无安装目录")?;
    safe_dir(parent)?;
    let stage = tempfile::Builder::new()
        .prefix(".staging-")
        .tempdir_in(parent)
        .map_err(|e| e.to_string())?;
    copy_package_for(src, stage.path(), package)?;
    if file_digest(exe)? != expected {
        return Err("安装期间主程序发生变化".into());
    }
    fs::write(
        stage.path().join("installation.json"),
        serde_json::to_vec_pretty(&receipt_for(exe, package)?).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(stage.path(), &dest).map_err(|e| e.to_string())?;
    if select {
        atomic_json(
            &base.join(key(exe)).join("current-package.json"),
            &serde_json::to_value(package).map_err(|e| e.to_string())?,
        )?;
    }
    Ok(())
}
pub fn install(
    exe: PathBuf,
    api: GraphicsApi,
    consent: bool,
    expected: String,
) -> Result<Operation, String> {
    install_package(exe, api, consent, expected, source(), package())
}
pub async fn install_latest(
    exe: PathBuf,
    api: GraphicsApi,
    consent: bool,
    expected: String,
    reporter: crate::services::installer::InstallReporter,
) -> Result<Operation, String> {
    let canonical = exe.canonicalize().map_err(|e| e.to_string())?;
    let package = super::streamline_update::candidate_for(&canonical);
    let source = super::streamline_download::ensure_package(&package, reporter).await?;
    tauri::async_runtime::spawn_blocking(move || {
        super::native_nr::import_package(&source, &package)?;
        install_package(exe, api, consent, expected, source, package)
    })
    .await
    .map_err(|e| e.to_string())?
}
pub(super) fn install_package(
    exe: PathBuf,
    api: GraphicsApi,
    consent: bool,
    expected: String,
    source: PathBuf,
    package: Package,
) -> Result<Operation, String> {
    let _lock = OPERATION.lock().map_err(|e| e.to_string())?;
    let _store_lock = lock_store()?;
    let exe = authorize(&exe, api, consent, &expected)?;
    install_package_at(&root(), &source, &exe, &expected, &package, true)?;
    Ok(Operation {
        message: "组件已安装。请通过“以画面增强启动”打开模拟器。".into(),
        session: None,
    })
}
pub(super) fn uninstall_at(base: &Path, exe: &Path) -> Result<(), String> {
    let (dir, package) = selected_at(base, exe)?;
    if !dir.exists() {
        return Ok(());
    }
    owned_for(&dir, exe, false, &package)?;
    let mut names: Vec<String> = package.files.iter().map(|f| f.name.clone()).collect();
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
    // Keep the selected version after uninstall so a retained older version is
    // never silently reactivated by the legacy fallback.
    atomic_json(
        &base.join(key(exe)).join("current-package.json"),
        &serde_json::to_value(package).map_err(|e| e.to_string())?,
    )?;
    Ok(())
}
pub fn uninstall(exe: PathBuf) -> Result<Operation, String> {
    let _lock = OPERATION.lock().map_err(|e| e.to_string())?;
    let _store_lock = lock_store()?;
    let exe = exe.canonicalize().map_err(|e| e.to_string())?;
    uninstall_at(&root(), &exe)?;
    Ok(Operation {
        message: "已卸载组件；当前游戏会话和诊断记录保留，普通启动不加载画面增强图层。".into(),
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
    let (installed, package) = selected_at(&root(), &exe)?;
    owned_for(&installed, &exe, true, &package)?;
    let graphics_settings = crate::config::CONFIG.read().setting.other.clone();
    if graphics_settings.streamline_nr_intensity > 100 {
        return Err("NR 强度必须为 0～100".into());
    }
    let nr_runtime = match super::native_nr::current_runtime() {
        Ok(runtime) => runtime,
        Err(error) if graphics_settings.streamline_nr => return Err(error),
        Err(_) => None,
    };
    if graphics_settings.streamline_nr && nr_runtime.is_none() {
        return Err("请先下载并安装原生 NR 组件，再启用神经渲染".into());
    }
    let sessions = root().join("sessions");
    safe_dir(&sessions)?;
    let stage = tempfile::Builder::new()
        .prefix("game-")
        .tempdir_in(&sessions)
        .map_err(|e| e.to_string())?;
    let bundle = stage.path().join("package");
    fs::create_dir(&bundle).map_err(|e| e.to_string())?;
    copy_package_for(&installed, &bundle, &package)?;
    let mut cmd = Command::new(bundle.join("streamline-layer-probe.exe"));
    // Diagnostic flags must never leak into a normal game launch from the parent.
    for name in [
        "NS_STREAMLINE_SR_TIMING",
        "NS_STREAMLINE_NVOF_TIMING",
        "NS_STREAMLINE_SOURCE_MEASURE",
        "NS_STREAMLINE_SOURCE_TRACK_ONLY",
        "NS_STREAMLINE_SOURCE_BENCH_OFF",
        "NS_STREAMLINE_NR_TIMING",
        "NS_STREAMLINE_NR_READBACK",
        "NS_STREAMLINE_NR_VALIDATION",
        "NS_STREAMLINE_SDK_VALIDATION",
        "NS_STREAMLINE_SDK_LAYOUT_TRACE",
        "NS_STREAMLINE_NR_GPU_HANDOFF",
        "NS_STREAMLINE_DEFER_PRESENT",
        "NS_STREAMLINE_SDK_OUTPUT_INIT",
        "NS_STREAMLINE_SDK_TRANSFER_ACCESS",
    ] {
        cmd.env_remove(name);
    }
    cmd.env("NS_STREAMLINE_TRACE_VERBOSE", "0");
    cmd.env("NS_STREAMLINE_TRACE_FRAMES", "0");
    cmd.arg("--target-probe")
        .arg("--graphics-launch")
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
    if graphics_settings.streamline_fg {
        cmd.args(["--fg", "--reference-params"]);
    }
    if let Some(runtime) = nr_runtime {
        if !package.native_nr {
            return Err("当前安装未包含 NR 调用桥，请更新画面增强组件".into());
        }
        cmd.arg("--native-nr")
            .arg("--nr-runtime")
            .arg(runtime)
            .arg("--nr-bridge")
            .arg(bundle.join("nvngx.dll"))
            .arg("--nr-intensity")
            .arg((f32::from(graphics_settings.streamline_nr_intensity) / 100.0).to_string());
        if !graphics_settings.streamline_nr {
            cmd.arg("--nr-initial-off");
        }
    }
    cmd.arg("--sr-mode")
        .arg(if graphics_settings.streamline_sr {
            graphics_settings.streamline_sr_mode.as_str()
        } else {
            "off"
        });
    cmd.arg("--sr-preset")
        .arg(graphics_settings.streamline_sr_preset.as_str());
    if let Some(scale) = graphics_settings.streamline_sr_scale {
        if !(50..=200).contains(&scale) {
            return Err("SR 倍率必须为 0.5～2.0".into());
        }
        cmd.arg("--sr-scale").arg(scale.to_string());
    }
    if graphics_settings.streamline_nvof {
        cmd.arg("--nvof");
    }
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
    Ok(Operation{message:"画面增强专用启动已就绪。NR、SR 和 FG 独立控制；NR 使用合成深度和硬件光流，未取得有效运动时暂停。失焦会暂停 FG，窗口操作可能停用本次 FG。".into(),session:Some(session)})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upgrades_select_only_after_verification_and_uninstall_never_reactivates_old_versions() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("managed");
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        let exe = temp.path().join("test.exe");
        fs::write(&exe, b"unchanged target").unwrap();
        let expected = file_digest(&exe).unwrap();
        let make_package = |version: &str, bytes: &[u8]| Package {
            version: version.into(),
            files: vec![Artifact {
                name: "layer.txt".into(),
                sha256: super::super::runtime_package::hash(bytes),
            }],
            download: None,
            parts: vec![],
            native_nr: false,
            schema_version: 0,
            launcher_protocol: 0,
        };
        let old = make_package("local-old", b"first release");
        fs::write(source.join("layer.txt"), b"first release").unwrap();
        install_package_at(&base, &source, &exe, &expected, &old, true).unwrap();
        let (old_dir, saved) = selected_at(&base, &exe).unwrap();
        assert_eq!(saved.version, old.version);
        let new = make_package("local-new", b"second release");
        assert!(install_package_at(&base, &source, &exe, &expected, &new, true).is_err());
        assert_eq!(selected_at(&base, &exe).unwrap().1.version, old.version);
        owned_for(&old_dir, &exe, true, &old).unwrap();
        fs::write(source.join("layer.txt"), b"second release").unwrap();
        install_package_at(&base, &source, &exe, &expected, &new, true).unwrap();
        // Selection comes from disk, independent of the network candidate or embedded version.
        let (new_dir, saved) = selected_at(&base, &exe).unwrap();
        assert_eq!(saved.version, new.version);
        owned_for(&new_dir, &exe, true, &saved).unwrap();
        assert_eq!(
            fs::read(old_dir.join("layer.txt")).unwrap(),
            b"first release"
        );
        fs::write(new_dir.join("layer.txt"), b"tampered").unwrap();
        assert!(uninstall_at(&base, &exe).is_err());
        fs::copy(source.join("layer.txt"), new_dir.join("layer.txt")).unwrap();
        uninstall_at(&base, &exe).unwrap();
        assert!(!selected_at(&base, &exe).unwrap().0.exists());
        assert!(old_dir.exists());
    }
    #[test]
    #[ignore = "Explicit local package selection; target, hash and store supplied by caller"]
    fn local_package_install() {
        let exe = PathBuf::from(std::env::var_os("FG_SMOKE_TARGET").expect("FG_SMOKE_TARGET"));
        let expected = std::env::var("FG_SMOKE_TARGET_SHA256").expect("FG_SMOKE_TARGET_SHA256");
        let base = PathBuf::from(std::env::var_os("FG_INSTALL_ROOT").expect("FG_INSTALL_ROOT"));
        let exe = authorize(&exe, GraphicsApi::Vulkan, false, &expected).unwrap();
        let src = source();
        install_package_at(&base, &src, &exe, &expected, &package(), true).unwrap();
        let (selected, package) = selected_at(&base, &exe).unwrap();
        owned_for(&selected, &exe, true, &package).unwrap();
        println!("FG_INSTALLED={}", selected.display());
    }
    #[test]
    #[ignore = "Explicit local GPU smoke; launches the game named in FG_SMOKE_GAME"]
    fn local_game_launch() {
        let exe = PathBuf::from(std::env::var_os("FG_SMOKE_TARGET").expect("FG_SMOKE_TARGET"));
        let game = std::env::var_os("FG_SMOKE_GAME").map(PathBuf::from);
        let expected = file_digest(&exe).unwrap();
        if let Some(runtime) = std::env::var_os("FG_SMOKE_NR_RUNTIME") {
            let component = super::super::native_nr::import(PathBuf::from(runtime)).unwrap();
            assert!(component.installed && component.package_ready);
            let mut config = crate::config::CONFIG.write();
            config.setting.other.streamline_fg = false;
            config.setting.other.streamline_sr = false;
            config.setting.other.streamline_nr = false;
        }
        install(exe.clone(), GraphicsApi::Vulkan, false, expected.clone()).unwrap();
        let result = launch(exe, GraphicsApi::Vulkan, false, expected, game).unwrap();
        println!("FG_SMOKE_SESSION={}", result.session.unwrap().display());
    }
    #[test]
    #[ignore = "Explicit local live-session control; target and desired mode supplied by caller"]
    fn local_live_control() {
        let exe = PathBuf::from(std::env::var_os("FG_SMOKE_TARGET").expect("FG_SMOKE_TARGET"));
        let enabled = std::env::var("FG_SMOKE_ENABLED").ok().map(|v| v == "1");
        let nr_enabled = std::env::var("FG_SMOKE_NR_ENABLED").ok().map(|v| v == "1");
        let nr_intensity = std::env::var("FG_SMOKE_NR_INTENSITY")
            .ok()
            .map(|v| v.parse().unwrap());
        let sr_mode = std::env::var("FG_SMOKE_SR_MODE").ok();
        let result = live(exe, enabled, sr_mode, None, None, nr_enabled, nr_intensity).unwrap();
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
        assert!(error.contains("自动下载"));
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
pub(super) fn atomic_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
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
pub fn live(
    exe: PathBuf,
    enabled: Option<bool>,
    sr_mode: Option<String>,
    sr_scale: Option<u16>,
    sr_preset: Option<String>,
    nr_enabled: Option<bool>,
    nr_intensity: Option<f32>,
) -> Result<serde_json::Value, String> {
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
        if v["nr"].is_object() {
            v["nr"]["active"] = false.into();
        }
        if v["sr"].is_object() {
            v["sr"]["active"] = false.into();
        }
    }
    if sr_scale.is_some_and(|v| !(50..=200).contains(&v)) {
        return Err("SR 倍率必须为 0.5～2.0".into());
    }
    if sr_preset
        .as_deref()
        .is_some_and(|p| crate::config::StreamlineSrPreset::parse(p).is_none())
    {
        return Err("无效的 SR 模型预设".into());
    }
    if nr_intensity.is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v)) {
        return Err("NR 强度必须为 0～1".into());
    }
    if enabled.is_some()
        || sr_mode.is_some()
        || sr_scale.is_some()
        || sr_preset.is_some()
        || nr_enabled.is_some()
        || nr_intensity.is_some()
    {
        if !connected {
            return Err("游戏未连接，请通过工具箱重新启动游戏".into());
        }
        let command = run.join("control.json");
        let mut control = if command.exists() {
            read_live_json(&command)?
        } else {
            serde_json::json!({})
        };
        if sr_scale.is_some()
            && (v["srScaleSupported"] != true || v["srScaleBasis"] != "source_output")
        {
            return Err("当前会话不支持倍率滑块，请更新组件并重新专用启动一次".into());
        }
        if sr_preset.is_some() && v["srPresetSupported"] != true {
            return Err("当前会话不支持模型切换，请更新组件并重新启动游戏".into());
        }
        if (sr_scale.is_some() || sr_preset.is_some()) && sr_mode.is_none() {
            return Err("倍率更新必须同时指定 SR 模式".into());
        }
        if let Some(mode) = sr_mode {
            if !matches!(
                mode.as_str(),
                "off" | "quality" | "balanced" | "performance" | "dlaa"
            ) {
                return Err("无效的 SR 模式".into());
            }
            if v["srLiveSupported"] != true {
                return Err("当前会话不支持 SR 实时切换，请更新组件并重新专用启动一次".into());
            }
            let revision = now.max(
                control["srRevision"]
                    .as_u64()
                    .unwrap_or(0)
                    .saturating_add(1),
            );
            control["srMode"] = mode.into();
            if let Some(scale) = sr_scale {
                control["srScale"] = scale.into();
            }
            if let Some(preset) = sr_preset {
                control["srPreset"] = preset.into();
            }
            control["srRevision"] = revision.into();
            v["sentSrRevision"] = revision.into();
        }
        if let Some(on) = enabled {
            let revision = now.max(control["revision"].as_u64().unwrap_or(0).saturating_add(1));
            control["enabled"] = on.into();
            control["revision"] = revision.into();
            v["sentRevision"] = revision.into();
        }
        if nr_enabled.is_some() || nr_intensity.is_some() {
            if v["nrLiveSupported"] != true {
                return Err("当前会话未准备原生 NR；请下载并安装 NR 组件后重新专用启动".into());
            }
            let revision = now.max(
                control["nrRevision"]
                    .as_u64()
                    .unwrap_or(0)
                    .saturating_add(1),
            );
            let on = nr_enabled.map(serde_json::Value::from).unwrap_or_else(|| {
                control
                    .get("nrEnabled")
                    .cloned()
                    .or_else(|| v["nr"]["requested"].as_bool().map(serde_json::Value::from))
                    .unwrap_or(false.into())
            });
            control["nrEnabled"] = on;
            if let Some(intensity) = nr_intensity {
                control["nrIntensity"] = intensity.into();
            }
            control["nrRevision"] = revision.into();
            v["sentNrRevision"] = revision.into();
        }
        atomic_json(&command, &control)?;
    }
    Ok(v)
}

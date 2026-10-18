//! Pinned experimental package; launches share immutable installed versions.
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
pub(super) fn hold_runtime_files(
    paths: &[PathBuf],
    removing: bool,
) -> Result<Vec<fs::File>, String> {
    for path in paths {
        super::runtime_package::safe_path(path)?;
        plain(path, false)?;
    }
    use super::runtime_files::{hold, Access};
    hold(
        paths,
        if removing {
            Access::Removal
        } else {
            Access::Runtime
        },
    )
    .map_err(|error| {
        if matches!(error.raw_os_error(), Some(32 | 33)) {
            "组件文件正在使用中，请先关闭通过画面增强启动的模拟器，再重试更新或卸载".into()
        } else {
            format!("无法保护组件文件：{error}")
        }
    })
}
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
fn destination(base: &Path, _exe: &Path) -> PathBuf {
    super::shared_runtime::destination(base)
}
fn selected_at(base: &Path, exe: &Path) -> Result<(PathBuf, Package), String> {
    let shared = destination(base, exe);
    if shared.exists() {
        return super::shared_runtime::manifest(&shared).map(|package| (shared, package));
    }
    let global = base.join("current-package.json");
    if global.exists() {
        let package =
            serde_json::from_value(read_live_json(&global)?).map_err(|e| e.to_string())?;
        super::streamline_update::validate_selected(&package)?;
        return Ok((shared, package));
    }
    legacy_at(base, exe).map(|legacy| legacy.unwrap_or_else(|| (shared, package())))
}
fn legacy_at(base: &Path, exe: &Path) -> Result<Option<(PathBuf, Package)>, String> {
    let pointer = base.join(key(exe)).join("current-package.json");
    super::runtime_package::safe_path(&pointer)?;
    if !pointer.exists() {
        let package = package();
        let dir = base.join(key(exe)).join(&package.version);
        return Ok(dir.exists().then_some((dir, package)));
    }
    let bytes = read_live_json(&pointer)?;
    let package: Package = serde_json::from_value(bytes).map_err(|e| e.to_string())?;
    super::streamline_update::validate_selected(&package)?;
    Ok(Some((base.join(key(exe)).join(&package.version), package)))
}
pub(super) fn installed_package(exe: &Path) -> Result<Package, String> {
    selected_at(&root(), exe).map(|(_, p)| p)
}
/// Preset metadata must not report the embedded fallback as an installed build.
pub(super) fn verified_preset_package(exe: &Path) -> Result<Package, String> {
    verified_preset_package_at(&root(), exe)
}
fn verified_preset_package_at(base: &Path, exe: &Path) -> Result<Package, String> {
    let (dir, package) = selected_at(base, exe)?;
    owned_for(&dir, exe, true, &package)?;
    Ok(package)
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
        return Err("当前组件包未包含 NR，请更新画面增强组件包".into());
    }
    availability()?;
    let bridge = source().join("nvngx.dll");
    plain(&bridge, false)?;
    Ok(bridge)
}
pub(super) fn planned(exe: &Path) -> PathBuf {
    destination(&root(), exe)
}
pub(super) fn planned_package(exe: &Path, _package: &Package) -> PathBuf {
    destination(&root(), exe)
}
fn receipt_for(exe: &Path, package: &Package) -> Result<Receipt, String> {
    Ok(Receipt {
        executable: exe.to_owned(),
        target_sha256: file_digest(exe)?,
        version: package.version.clone(),
    })
}
fn target_record(base: &Path, exe: &Path) -> PathBuf {
    base.join("targets").join(format!("{}.json", key(exe)))
}
fn live_pointer(base: &Path, exe: &Path) -> PathBuf {
    base.join("live").join(format!("{}.json", key(exe)))
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
    let shared = dir.file_name().and_then(|s| s.to_str()) == Some("runtime");
    if shared {
        super::shared_runtime::verify(dir, package)?;
        if !current {
            return Ok(());
        }
    }
    let record = if shared {
        target_record(dir.parent().ok_or("无组件存储目录")?, exe)
    } else {
        dir.join("installation.json")
    };
    plain(&record, false)?;
    let r: Receipt = serde_json::from_slice(&fs::read(record).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if r.executable != exe || (!shared && r.version != package.version) {
        return Err("安装记录不属于当前目标".into());
    }
    if current && r.target_sha256 != file_digest(exe)? {
        return Err("模拟器已更新，请重新检测并安装组件".into());
    }
    if shared {
        Ok(())
    } else {
        verify_artifacts(dir, &package.files)
    }
}
pub(super) fn state(exe: &Path) -> &'static str {
    let Ok((dir, package)) = selected_at(&root(), exe) else {
        return "damaged";
    };
    if !dir.exists() {
        "unmanaged"
    } else if dir == destination(&root(), exe) {
        let registered = target_record(&root(), exe).exists();
        if registered && owned_for(&dir, exe, true, &package).is_ok() {
            "installed"
        } else if super::shared_runtime::verify(&dir, &package).is_err() {
            "damaged"
        } else if registered
            || legacy_at(&root(), exe)
                .ok()
                .flatten()
                .is_some_and(|(dir, _)| dir.exists())
        {
            "outdated"
        } else {
            "unmanaged"
        }
    } else if owned_for(&dir, exe, true, &package).is_ok() {
        "outdated"
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
    target_policy::classify(&exe, &hash, true).authorize(consent)?;
    Ok(exe)
}
pub(super) fn copy_package_for(from: &Path, to: &Path, package: &Package) -> Result<(), String> {
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
    _select: bool,
) -> Result<(), String> {
    super::streamline_update::validate_selected(package)?;
    if file_digest(exe)? != expected {
        return Err("主程序已改变，请重新检测".into());
    }
    let legacy = legacy_at(base, exe).ok().flatten();
    super::shared_runtime::install(base, src, package, || {
        if file_digest(exe)? != expected {
            return Err("安装期间主程序发生变化".into());
        }
        Ok(())
    })?;
    safe_dir(&base.join("targets"))?;
    atomic_json(
        &target_record(base, exe),
        &serde_json::to_value(receipt_for(exe, package)?).map_err(|e| e.to_string())?,
    )?;
    atomic_json(
        &base.join("current-package.json"),
        &serde_json::to_value(package).map_err(|e| e.to_string())?,
    )?;
    // Migrate only the selected legacy payload whose receipt and all hashes
    // can be verified. Active or unknown old installations remain untouched.
    if let Some((dir, legacy)) = legacy.filter(|(dir, _)| dir.exists()) {
        let result =
            owned_for(&dir, exe, false, &legacy).and_then(|_| remove_legacy(&dir, &legacy));
        if let Err(error) = result {
            tracing::warn!(%error, path = %dir.display(), "保留未能验证或仍在使用的旧组件安装");
        }
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
    let source = super::streamline_download::ensure_package(&package, reporter.clone()).await?;
    reporter
        .run_blocking_step("install", "安装画面增强组件", move || {
            super::native_nr::import_package(&source, &package)?;
            install_package(exe, api, consent, expected, source, package)
        })
        .await
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
        message: "共享组件已安装。请通过“以画面增强启动”打开模拟器。更新和卸载前请先关闭使用组件的模拟器。".into(),
        session: None,
    })
}
fn remove_legacy(dir: &Path, package: &Package) -> Result<(), String> {
    let mut names: Vec<String> = package.files.iter().map(|f| f.name.clone()).collect();
    names.push("installation.json".into());
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let e = entry.map_err(|e| e.to_string())?;
        if !names.iter().any(|n| e.file_name() == n.as_str()) {
            return Err("目录存在额外文件，保留安装目录，请手动检查".into());
        }
    }
    // Acquire every removal handle before deleting anything. Active launchers
    // and target layers hold reader handles even after the toolbox closes.
    let _files = hold_runtime_files(
        &names.iter().map(|name| dir.join(name)).collect::<Vec<_>>(),
        true,
    )?;
    // Only the exact owned files are removed. Diagnostics and user files remain.
    for name in names {
        fs::remove_file(dir.join(name)).map_err(|e| e.to_string())?;
    }
    drop(_files); // Finish Windows delete-pending files before removing the directory.
    fs::remove_dir(dir).map_err(|e| e.to_string())
}
pub(super) fn uninstall_at(base: &Path, exe: &Path) -> Result<(), String> {
    let (dir, package) = selected_at(base, exe)?;
    if !dir.exists() {
        return Ok(());
    }
    owned_for(&dir, exe, false, &package)?;
    if dir == destination(base, exe) {
        super::shared_runtime::remove(&dir, &package)?;
    } else {
        remove_legacy(&dir, &package)?;
    }
    // Keep the selected version after uninstall so a retained older version is
    // never silently reactivated by the legacy fallback.
    atomic_json(
        &base.join("current-package.json"),
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
        message: "已卸载共享组件；所有模拟器的画面增强入口需重新安装组件后使用，诊断记录保留。"
            .into(),
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
    let gpu = super::gpu::detect()?;
    if !gpu.has_nvidia {
        return Err("未检测到 NVIDIA 显卡，无法以画面增强启动".into());
    }
    if graphics_settings.streamline_nr && !gpu.nr_supported {
        return Err("DLSS 5 需要 GeForce RTX 50 系列，请先关闭 NR".into());
    }
    if graphics_settings.streamline_sr && !gpu.sr_supported {
        return Err("SR / DLAA 需要 GeForce RTX 20／30／40／50 系列，请先关闭 SR".into());
    }
    if graphics_settings.streamline_fg {
        let fg = graphics_settings.streamline_advanced.fg;
        if gpu.fg_max_multiplier < 2
            || (fg.backend == crate::config::advanced_settings::FgBackend::Dlss
                && (fg.multiplier > gpu.fg_max_multiplier
                    || (matches!(fg.mode, crate::config::advanced_settings::FgMode::Dynamic)
                        && gpu.fg_max_multiplier < 3)))
        {
            return Err("当前显卡不支持所选 FG 配置：2× 需要 RTX 40／50 系列，更高倍数及动态模式需要 RTX 50 系列".into());
        }
    }
    if graphics_settings.streamline_nr_intensity > 200 {
        return Err("NR 强度必须为 0～200".into());
    }
    if !(50..=100).contains(&graphics_settings.streamline_input_scale) {
        return Err("输入尺寸比例必须为 50～100".into());
    }
    if !crate::config::advanced_settings::valid_nr_inference_max_edge(
        graphics_settings.streamline_input_max_edge,
    ) {
        return Err("输入长边上限必须为 0 或 320～8192 像素".into());
    }
    let nr_experiments =
        graphics_settings.streamline_nr_consolidated && graphics_settings.streamline_nr;
    let input_scaled = graphics_settings.streamline_input_max_edge != 0
        || graphics_settings.streamline_input_scale != 100;
    if nr_experiments || input_scaled {
        let bytes =
            fs::read(installed.join("streamline_probe_layer.dll")).map_err(|e| e.to_string())?;
        for (needed, marker, name) in [
            (
                graphics_settings.streamline_nr_consolidated,
                crate::config::advanced_settings::NR_CONSOLIDATED_MARKER,
                "NR 集中提交",
            ),
            (
                input_scaled,
                crate::config::advanced_settings::INPUT_SCALING_MARKER,
                "输入尺寸缩放",
            ),
        ] {
            if needed && !bytes.windows(marker.len()).any(|v| v == marker) {
                return Err(format!(
                    "当前组件不支持 {name}，请更新画面增强组件或恢复启动实验默认配置"
                ));
            }
        }
    }
    let advanced = graphics_settings.streamline_advanced;
    let hybrid = advanced.fg.backend == crate::config::advanced_settings::FgBackend::Xess;
    let layer_bytes =
        fs::read(installed.join("streamline_probe_layer.dll")).map_err(|e| e.to_string())?;
    let switch_marker = b"NS_EMU_FG_SWITCH_V1";
    let switch_capable = gpu.fg_max_multiplier >= 2
        && layer_bytes
            .windows(switch_marker.len())
            .any(|v| v == switch_marker);
    let xess_runtime = if hybrid {
        Some(super::xess::hybrid_runtime()?)
    } else if switch_capable {
        super::xess::hybrid_runtime().ok()
    } else {
        None
    };
    let fg_switchable = switch_capable && xess_runtime.is_some();
    let (hybrid_runtime, hybrid_files) = if let Some((dir, files)) = xess_runtime {
        (Some(dir), files)
    } else {
        (None, Vec::new())
    };
    // Older verified packages ignore this environment variable. Reject tuned
    // launches rather than silently starting with different settings.
    if advanced != Default::default() || graphics_settings.streamline_nr_intensity > 100 {
        let bytes =
            fs::read(installed.join("streamline_probe_layer.dll")).map_err(|e| e.to_string())?;
        let marker = crate::config::advanced_settings::CAPABILITY_MARKER;
        if !bytes.windows(marker.len()).any(|v| v == marker) {
            return Err("当前画面增强组件不支持新增高级配置，请更新组件后再专用启动；也可先在三个弹窗恢复默认配置".into());
        }
        let marker = crate::config::advanced_settings::NR_LOOK_MARKER;
        let two_pass_marker = crate::config::advanced_settings::NR_TWO_PASS_MARKER;
        let temporal_marker = crate::config::advanced_settings::NR_TEMPORAL_LOOK_MARKER;
        for (needed, marker, name) in [
            (
                hybrid,
                crate::config::advanced_settings::HYBRID_FG_MARKER,
                "DLSS SR + XeSS FG 混合模式",
            ),
            (
                !advanced.optical_flow.is_default(),
                crate::config::advanced_settings::OPTICAL_FLOW_MARKER,
                "光流高级配置",
            ),
            (
                advanced.nr.look.needs_experiments(),
                crate::config::advanced_settings::NR_LOOK_EXPERIMENTS_MARKER,
                "Look 颜色保护/诊断实验",
            ),
            (
                !advanced.nr.second_pass.interpass.is_default(),
                crate::config::advanced_settings::NR_INTERPASS_MARKER,
                "NR 遍间预处理",
            ),
            (
                advanced.nr.look.temporal.mode.persistent(),
                crate::config::advanced_settings::NR_PERSISTENCE_MARKER,
                "光流累积＋",
            ),
            (
                !advanced.nr.look.scope.is_default(),
                crate::config::advanced_settings::NR_LOOK_SCOPE_MARKER,
                "Look 作用范围",
            ),
            (
                advanced.nr.look.temporal.needs_extended_support(),
                crate::config::advanced_settings::NR_TEMPORAL_MODES_MARKER,
                "时间模式/历史采样",
            ),
        ] {
            if needed && !bytes.windows(marker.len()).any(|v| v == marker) {
                return Err(format!(
                    "当前组件不支持 {name}，请更新组件并重新专用启动，或恢复默认配置"
                ));
            }
        }
        if !advanced.nr.look.temporal.is_default()
            && !bytes
                .windows(temporal_marker.len())
                .any(|v| v == temporal_marker)
        {
            return Err(
                "当前组件不支持时间 Look，请更新组件后重新专用启动，或恢复时间平滑默认配置".into(),
            );
        }
        if !advanced.nr.second_pass.is_default()
            && !bytes
                .windows(two_pass_marker.len())
                .any(|v| v == two_pass_marker)
        {
            return Err(
                "当前组件不支持两遍 NR，请更新组件后重新专用启动，或恢复第二遍默认配置".into(),
            );
        }
        if !advanced.nr.look.is_default() && !bytes.windows(marker.len()).any(|v| v == marker) {
            return Err(
                "当前画面增强组件不支持 NR Look，请更新组件后重新专用启动，或恢复 Look 默认配置"
                    .into(),
            );
        }
        let marker = crate::config::advanced_settings::NR_SPATIAL_LOOK_MARKER;
        if !advanced.nr.look.spatial.is_default()
            && !bytes.windows(marker.len()).any(|v| v == marker)
        {
            return Err(
                "当前组件不支持空间 Look，请更新组件后重新专用启动，或恢复空间 Look 默认配置"
                    .into(),
            );
        }
    }
    let nr_runtime = match super::native_nr::current_runtime() {
        Ok(runtime) => runtime,
        Err(error) if graphics_settings.streamline_nr || nr_experiments => return Err(error),
        Err(_) => None,
    };
    if graphics_settings.streamline_nr && nr_runtime.is_none() {
        return Err("请先下载并安装 NR 组件，再启用神经渲染".into());
    }
    if nr_experiments && nr_runtime.is_none() {
        return Err("NR 启动实验需要已安装的 NR 组件；请安装组件或恢复启动实验默认配置".into());
    }
    let sessions = root().join("sessions");
    safe_dir(&sessions)?;
    super::sessions::reap_completed(&sessions);
    let stage = tempfile::Builder::new()
        .prefix("game-")
        .tempdir_in(&sessions)
        .map_err(|e| e.to_string())?;
    let bundle = installed;
    let mut paths: Vec<_> = package.files.iter().map(|f| bundle.join(&f.name)).collect();
    if let Some(runtime) = &nr_runtime {
        paths.push(runtime.clone());
    }
    let mut runtime_files = hold_runtime_files(&paths, false)?;
    runtime_files.extend(hybrid_files);
    let marker = b"NS_EMU_FIXED_RUNTIME_V1";
    for name in ["streamline-layer-probe.exe", "streamline_probe_layer.dll"] {
        let bytes = fs::read(bundle.join(name)).map_err(|e| e.to_string())?;
        if !bytes.windows(marker.len()).any(|v| v == marker) {
            return Err("当前组件不支持固定版本目录启动，请更新画面增强组件".into());
        }
    }
    atomic_json(
        &stage.path().join("runtime-reference.json"),
        &serde_json::json!({"package":bundle,"version":package.version,"files":package.files,"nrRuntime":nr_runtime}),
    )?;
    let mut cmd = Command::new(bundle.join("streamline-layer-probe.exe"));
    // Diagnostic flags must never leak into a normal game launch from the parent.
    for name in [
        "NS_STREAMLINE_SR_TIMING",
        "NS_STREAMLINE_NVOF_TIMING",
        "NS_STREAMLINE_NVOF_TEMPORAL_HINTS",
        "NS_STREAMLINE_NVOF_STATIC_GUARD",
        "NS_STREAMLINE_SOURCE_MEASURE",
        "NS_STREAMLINE_SOURCE_TRACK_ONLY",
        "NS_STREAMLINE_SOURCE_BENCH_OFF",
        "NS_STREAMLINE_NR_TIMING",
        "NS_STREAMLINE_NR_CONSOLIDATED",
        "NS_STREAMLINE_NR_INFERENCE_SCALE",
        "NS_STREAMLINE_NR_INFERENCE_MAX_EDGE",
        "NS_STREAMLINE_INPUT_SCALE",
        "NS_STREAMLINE_INPUT_MAX_EDGE",
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
    cmd.env_remove("NS_EMU_XESS_HYBRID");
    cmd.env_remove("NS_EMU_FG_SWITCHABLE");
    cmd.env_remove("NS_EMU_XESS_FG");
    cmd.env(
        "NS_STREAMLINE_ADVANCED_SETTINGS",
        serde_json::to_string(&advanced).map_err(|e| e.to_string())?,
    );
    cmd.arg("--target-probe")
        .arg("--graphics-launch")
        .arg("--fixed-runtime")
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
    if let Some(runtime) = hybrid_runtime {
        cmd.arg("--xess-runtime").arg(runtime);
    }
    if fg_switchable {
        cmd.arg("--fg-switchable");
    }
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
        if nr_experiments {
            cmd.arg("--nr-consolidated");
        }
    }
    if graphics_settings.streamline_input_max_edge != 0 {
        cmd.arg("--input-max-edge")
            .arg(graphics_settings.streamline_input_max_edge.to_string());
    } else if graphics_settings.streamline_input_scale != 100 {
        cmd.arg("--input-scale")
            .arg(graphics_settings.streamline_input_scale.to_string());
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
    // Protocol-1 launchers predate family adaptation. Permit their generic path
    // for recognized emulators while retaining the exact target/hash checks.
    if consent
        || target_policy::classify(&exe, &expected, true) == target_policy::Compatibility::Adapted
    {
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
    let child = cmd.spawn().map_err(|e| e.to_string())?;
    let session = stage.keep();
    super::sessions::monitor(child, session.clone(), sessions, runtime_files);
    safe_dir(&root().join("live"))?;
    let pointer = live_pointer(&root(), &exe);
    atomic_json(&pointer, &serde_json::json!({"session":session}))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let status = super::sessions::status(&session)?;
        if status["launchState"] == "failed" || status["launchState"] == "finished" {
            return Err(format!(
                "模拟器启动后已退出。{}",
                status["launchMessage"].as_str().unwrap_or_default()
            ));
        }
        if status["launchState"] == "started" {
            return Ok(Operation {
                message: "模拟器进程已启动，等待游戏画面与增强运行状态确认。".into(),
                session: Some(session),
            });
        }
        if std::time::Instant::now() >= deadline {
            return Ok(Operation {
                message: "启动器仍在准备，尚未确认模拟器进程启动。请查看下方运行状态；启动失败时会显示原因。".into(),
                session: Some(session),
            });
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preset_metadata_never_treats_embedded_manifest_as_an_installed_build() {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path().join("eden.exe");
        fs::write(&exe, b"target").unwrap();
        let base = temp.path().join("empty-store");
        let (destination, candidate) = selected_at(&base, &exe).unwrap();
        assert_eq!(candidate.version, package().version);
        assert!(!destination.exists());
        assert!(verified_preset_package_at(&base, &exe).is_err());
        // An installation receipt without the verified payload is still unknown.
        fs::create_dir_all(&destination).unwrap();
        fs::write(
            destination.join("installation.json"),
            serde_json::to_vec(&receipt_for(&exe, &candidate).unwrap()).unwrap(),
        )
        .unwrap();
        assert!(verified_preset_package_at(&base, &exe).is_err());
    }
    #[test]
    #[cfg(all(windows, target_arch = "x86_64"))]
    fn adapted_launches_still_enforce_vulkan_architecture_and_current_target_hash() {
        let temp = tempfile::tempdir().unwrap();
        let bytes = super::super::tests::pe(0x8664, 0x20b);
        for name in ["eden.exe", "citron.exe", "Ryujinx.exe", "other.exe"] {
            let exe = temp.path().join(name);
            fs::write(&exe, &bytes).unwrap();
            let expected = file_digest(&exe).unwrap();
            assert_eq!(
                authorize(&exe, GraphicsApi::Vulkan, false, &expected).is_ok(),
                name != "other.exe"
            );
            assert!(authorize(&exe, GraphicsApi::Vulkan, true, &expected).is_ok());
            assert!(authorize(&exe, GraphicsApi::OpenGl, true, &expected).is_err());
            assert!(authorize(&exe, GraphicsApi::Vulkan, true, "stale hash").is_err());
            fs::write(&exe, super::super::tests::pe(0x14c, 0x10b)).unwrap();
            let current = file_digest(&exe).unwrap();
            assert!(authorize(&exe, GraphicsApi::Vulkan, true, &current).is_err());
        }
    }
    #[test]
    fn shared_runtime_reuses_one_directory_and_replaces_only_when_idle() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("managed");
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        let exe = temp.path().join("first.exe");
        let second = temp.path().join("second.exe");
        fs::write(&exe, b"first target").unwrap();
        fs::write(&second, b"second target").unwrap();
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
        let dir = destination(&base, &exe);
        assert_eq!(dir, base.join("runtime"));
        // Registration of another emulator reuses the verified directory and
        // does not need the source or another copy of the package.
        install_package_at(
            &base,
            &temp.path().join("missing-source"),
            &second,
            &file_digest(&second).unwrap(),
            &old,
            true,
        )
        .unwrap();
        assert_eq!(selected_at(&base, &second).unwrap().0, dir);
        owned_for(&dir, &second, true, &old).unwrap();
        assert!(!base.join(key(&exe)).exists());
        assert!(!base.join(key(&second)).exists());
        let new = make_package("local-new", b"second release");
        assert!(install_package_at(&base, &source, &exe, &expected, &new, true).is_err());
        assert_eq!(selected_at(&base, &exe).unwrap().1.version, old.version);
        fs::write(source.join("layer.txt"), b"second release").unwrap();
        #[cfg(windows)]
        {
            let guard = hold_runtime_files(&[dir.join("layer.txt")], false).unwrap();
            assert!(
                install_package_at(&base, &source, &exe, &expected, &new, true)
                    .unwrap_err()
                    .contains("正在使用")
            );
            assert!(uninstall_at(&base, &exe).unwrap_err().contains("正在使用"));
            owned_for(&dir, &exe, true, &old).unwrap();
            drop(guard);
        }
        install_package_at(&base, &source, &exe, &expected, &new, true).unwrap();
        assert_eq!(selected_at(&base, &exe).unwrap().0, dir);
        assert_eq!(selected_at(&base, &second).unwrap().1.version, new.version);
        owned_for(&dir, &second, true, &new).unwrap();
        assert_eq!(fs::read(dir.join("layer.txt")).unwrap(), b"second release");
        assert!(!fs::read_dir(&base).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".retired-")));
        // Retry an interrupted backup cleanup without deleting unknown files.
        let retired = base.join(".retired-test");
        fs::create_dir(&retired).unwrap();
        atomic_json(
            &retired.join("installation.json"),
            &serde_json::to_value(&new).unwrap(),
        )
        .unwrap();
        fs::write(retired.join("user.txt"), b"keep").unwrap();
        install_package_at(&base, &source, &exe, &expected, &new, true).unwrap();
        assert_eq!(fs::read(retired.join("user.txt")).unwrap(), b"keep");
        fs::remove_file(retired.join("user.txt")).unwrap();
        install_package_at(&base, &source, &exe, &expected, &new, true).unwrap();
        assert!(!retired.exists());
        fs::write(dir.join("user.txt"), b"keep").unwrap();
        assert!(uninstall_at(&base, &exe).is_err());
        assert!(install_package_at(&base, &source, &exe, &expected, &old, true).is_err());
        assert_eq!(fs::read(dir.join("user.txt")).unwrap(), b"keep");
        fs::remove_file(dir.join("user.txt")).unwrap();
        uninstall_at(&base, &exe).unwrap();
        assert!(!selected_at(&base, &second).unwrap().0.exists());
        // The shared uninstall marker never falls back to a legacy installation.
        let legacy = base.join(key(&second)).join(&old.version);
        fs::create_dir_all(&legacy).unwrap();
        atomic_json(
            &legacy.parent().unwrap().join("current-package.json"),
            &serde_json::to_value(&old).unwrap(),
        )
        .unwrap();
        assert_eq!(selected_at(&base, &second).unwrap().0, dir);
        assert!(!dir.exists());
    }
    #[test]
    fn verified_legacy_install_migrates_without_deleting_active_or_unknown_files() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("managed");
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("layer.txt"), b"legacy layer").unwrap();
        let package = Package {
            version: "local-migration".into(),
            files: vec![Artifact {
                name: "layer.txt".into(),
                sha256: file_digest(&source.join("layer.txt")).unwrap(),
            }],
            download: None,
            parts: vec![],
            native_nr: false,
            schema_version: 0,
            launcher_protocol: 0,
        };
        let exe = temp.path().join("eden.exe");
        fs::write(&exe, b"target").unwrap();
        let legacy = base.join(key(&exe)).join(&package.version);
        fs::create_dir_all(&legacy).unwrap();
        copy_package_for(&source, &legacy, &package).unwrap();
        atomic_json(
            &legacy.join("installation.json"),
            &serde_json::to_value(receipt_for(&exe, &package).unwrap()).unwrap(),
        )
        .unwrap();
        atomic_json(
            &legacy.parent().unwrap().join("current-package.json"),
            &serde_json::to_value(&package).unwrap(),
        )
        .unwrap();
        assert_eq!(selected_at(&base, &exe).unwrap().0, legacy);
        #[cfg(windows)]
        let guard = hold_runtime_files(&[legacy.join("layer.txt")], false).unwrap();
        #[cfg(not(windows))]
        fs::write(legacy.join("user.txt"), b"keep").unwrap();
        install_package_at(
            &base,
            &source,
            &exe,
            &file_digest(&exe).unwrap(),
            &package,
            true,
        )
        .unwrap();
        assert!(legacy.join("layer.txt").exists());
        assert_eq!(selected_at(&base, &exe).unwrap().0, base.join("runtime"));
        owned_for(&base.join("runtime"), &exe, true, &package).unwrap();
        #[cfg(windows)]
        drop(guard);
        #[cfg(not(windows))]
        fs::remove_file(legacy.join("user.txt")).unwrap();
        fs::write(legacy.join("user.txt"), b"keep").unwrap();
        install_package_at(
            &base,
            &source,
            &exe,
            &file_digest(&exe).unwrap(),
            &package,
            true,
        )
        .unwrap();
        assert_eq!(fs::read(legacy.join("user.txt")).unwrap(), b"keep");
        fs::remove_file(legacy.join("user.txt")).unwrap();
        install_package_at(
            &base,
            &source,
            &exe,
            &file_digest(&exe).unwrap(),
            &package,
            true,
        )
        .unwrap();
        assert!(!legacy.exists());
        let mut next = package.clone();
        next.version = "local-next".into();
        // A target change after preparing an update leaves the old package intact.
        assert!(
            super::super::shared_runtime::install(&base, &source, &next, || Err(
                "target changed".into()
            ))
            .is_err()
        );
        assert_eq!(selected_at(&base, &exe).unwrap().1.version, package.version);
        owned_for(&base.join("runtime"), &exe, true, &package).unwrap();
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
        let result = live(
            exe,
            enabled,
            sr_mode,
            None,
            None,
            nr_enabled,
            nr_intensity,
            None,
            None,
        )
        .unwrap();
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
pub(super) fn read_live_json(path: &Path) -> Result<serde_json::Value, String> {
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
    advanced: Option<crate::config::advanced_settings::AdvancedUpdate>,
    input_sizing: Option<crate::config::advanced_settings::InputSizing>,
) -> Result<serde_json::Value, String> {
    let _lock = OPERATION.lock().map_err(|e| e.to_string())?;
    let _store_lock = lock_store()?;
    let exe = exe.canonicalize().map_err(|e| e.to_string())?;
    let pointer = live_pointer(&root(), &exe);
    let pointer = if pointer.exists() {
        pointer
    } else {
        root().join(key(&exe)).join("live-session.json")
    };
    super::runtime_package::safe_path(&pointer)?;
    if !pointer.exists() {
        return Ok(serde_json::json!({"connected":false}));
    }
    let pointer = read_live_json(&pointer)?;
    let session = PathBuf::from(pointer["session"].as_str().ok_or("会话记录无效")?);
    if session.parent() != Some(root().join("sessions").as_path()) {
        return Err("会话不属于工具箱".into());
    }
    safe_dir(&session)?;
    let launch = super::sessions::status(&session)?;
    let run = session.join("run");
    if !run.exists() {
        return Ok(launch);
    }
    safe_dir(&run)?;
    let path = run.join("telemetry.json");
    if !path.exists() {
        return Ok(launch);
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
        && launch["launchState"] != "failed"
        && launch["launchState"] != "finished";
    v["launchState"] = if connected {
        "running".into()
    } else {
        launch["launchState"].clone()
    };
    v["launchMessage"] = launch["launchMessage"].clone();
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
        if v["inputScale"].is_object() {
            v["inputScale"]["active"] = false.into();
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
    if nr_intensity.is_some_and(|v| !v.is_finite() || !(0.0..=2.0).contains(&v)) {
        return Err("NR 强度必须为 0～2".into());
    }
    let advanced = advanced.unwrap_or_default();
    if input_sizing.is_some_and(|s| !s.valid()) {
        return Err("输入比例必须为 50～100，上限必须为 0 或 320～8192 的整数像素".into());
    }
    if enabled.is_some()
        || sr_mode.is_some()
        || sr_scale.is_some()
        || sr_preset.is_some()
        || nr_enabled.is_some()
        || nr_intensity.is_some()
        || advanced.nr.is_some()
        || advanced.sr.is_some()
        || advanced.fg.is_some()
        || input_sizing.is_some()
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
        if (advanced.nr.is_some()
            || advanced.sr.is_some()
            || advanced.fg.is_some()
            || nr_intensity.is_some_and(|v| v > 1.0))
            && v["advancedSettingsSupported"] != true
        {
            return Err("当前会话不支持新增高级配置，请更新画面增强组件并重新专用启动".into());
        }
        if let Some(fg) = advanced.fg {
            let current = v["fg"]["backend"].as_str().unwrap_or("dlss");
            let wanted = if fg.backend == crate::config::advanced_settings::FgBackend::Xess {
                "xess"
            } else {
                "dlss"
            };
            if current != wanted && v["fgBackendSwitchSupported"] != true {
                return Err("切换帧生成引擎需要重新启动游戏；设置已保存供下次启动使用".into());
            }
            if enabled.is_none() {
                return Err("FG 配置更新必须同时指定开关状态".into());
            }
            if let Some(maximum) = v["fg"]["dlssMaximumGenerated"]
                .as_u64()
                .or_else(|| v["fg"]["maximumGenerated"].as_u64())
            {
                if maximum > 0
                    && fg.backend == crate::config::advanced_settings::FgBackend::Dlss
                    && !fg.supported(maximum as u32)
                {
                    return Err(format!("当前 GPU / 运行库最高支持 {}× 帧生成", maximum + 1));
                }
            }
            control["fgOptions"] = serde_json::to_value(fg).map_err(|e| e.to_string())?;
        }
        if let Some(sr) = advanced.sr {
            if sr_mode.is_none() {
                return Err("SR 配置更新必须同时指定重建方式".into());
            }
            control["srOptions"] = serde_json::to_value(sr).map_err(|e| e.to_string())?;
        }
        if let Some(nr) = advanced.nr {
            if !nr.look.scope.is_default() && v["nrLookScopeSupported"] != true {
                return Err("当前会话不支持 Look 作用范围，请更新组件并重新专用启动".into());
            }
            if nr.look.needs_experiments() && v["nrLookExperimentsSupported"] != true {
                return Err(
                    "当前组件不支持 Look 颜色保护或诊断实验，请更新组件后重新专用启动".into(),
                );
            }
            if nr.look.temporal.mode.persistent() && v["nrPersistenceSupported"] != true {
                return Err("当前组件不支持光流累积＋，请更新组件后重新专用启动".into());
            }
            if nr.look.temporal.needs_extended_support() && v["nrTemporalModesSupported"] != true {
                return Err("当前会话不支持时间模式/历史采样，请更新组件并重新专用启动".into());
            }
            if !nr.look.temporal.is_default() && v["nrTemporalLookSupported"] != true {
                return Err("当前会话不支持时间 Look，请更新组件并重新专用启动".into());
            }
            if !nr.second_pass.is_default() && v["nrTwoPassSupported"] != true {
                return Err("当前会话不支持两遍 NR，请更新组件并重新专用启动".into());
            }
            if !nr.second_pass.interpass.is_default() && v["nrInterpassSupported"] != true {
                return Err("当前会话不支持 NR 遍间预处理，请更新组件并重新专用启动".into());
            }
            if !nr.look.spatial.is_default() && v["nrSpatialLookSupported"] != true {
                return Err("当前会话不支持空间 Look，请更新组件并重新专用启动".into());
            }
            if !nr.look.is_default() && v["nrLookSupported"] != true {
                return Err("当前会话不支持 NR Look，请更新组件并重新专用启动".into());
            }
            control["nrOptions"] = serde_json::to_value(nr).map_err(|e| e.to_string())?;
        }
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
        if nr_enabled.is_some() || nr_intensity.is_some() || advanced.nr.is_some() {
            if v["nrLiveSupported"] != true {
                return Err("当前会话未准备 NR；请下载并安装 NR 组件后重新专用启动".into());
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
        if let Some(sizing) = input_sizing {
            if v["inputScalingLiveSupported"] != true {
                return Err(
                    "当前会话不支持实时输入缩放，请更新组件并重新以画面增强启动一次".into(),
                );
            }
            let revision = now.max(
                control["inputScaleRevision"]
                    .as_u64()
                    .unwrap_or(0)
                    .saturating_add(1),
            );
            control["inputSizing"] = serde_json::to_value(sizing).map_err(|e| e.to_string())?;
            control["inputScaleRevision"] = revision.into();
            v["sentInputScaleRevision"] = revision.into();
        }
        atomic_json(&command, &control)?;
    }
    Ok(v)
}

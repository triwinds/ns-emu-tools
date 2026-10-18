//! Independent XeSS component store and process-scoped launches. No NVIDIA runtime.
use super::{
    runtime_package,
    streamline_fg::{file_digest, target_policy},
    streamline_install::{atomic_json, plain, read_live_json, safe_dir},
};
use crate::config::advanced_settings::InputSizing;
use crate::{config::effective_config_dir, models::graphics_components::GraphicsApi};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
};

#[path = "xess_download.rs"]
mod download;

static OPERATION: Mutex<()> = Mutex::new(());
const MEMBERS: &[&str] = &[
    "xess-component.exe",
    "xess-layer.dll",
    "libxess.dll",
    "bin/libxess_fg.dll",
    "bin/libxell.dll",
    "D3D12/D3D12Core.dll",
    "D3D12/d3d12SDKLayers.dll",
    "D3D12/D3D12StateObjectCompiler.dll",
    "licenses/Intel-LICENSE.txt",
    "licenses/Intel-third-party-programs.txt",
    "licenses/Microsoft-LICENSE.txt",
    "licenses/Microsoft-LICENSE-CODE.txt",
    "licenses/NS-Emu-Tools-LICENSE.txt",
];
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Artifact {
    name: String,
    sha256: String,
}
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Package {
    schema_version: u32,
    version: String,
    files: Vec<Artifact>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub native_aa: bool,
    pub fg: bool,
    pub input_sizing: InputSizing,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            native_aa: true,
            fg: false,
            input_sizing: InputSizing::default(),
        }
    }
}
impl Settings {
    fn validate(&self) -> Result<(), String> {
        if !self.input_sizing.valid() {
            return Err("输入比例必须为 50～100%，长边上限为 0 或 320～8192 像素".into());
        }
        Ok(())
    }
}
fn package() -> Package {
    serde_json::from_str(include_str!("xess-package.json")).expect("embedded XeSS manifest")
}
fn root() -> PathBuf {
    effective_config_dir().join("graphics/xess")
}
fn bundled() -> Result<PathBuf, String> {
    if package().schema_version == 2 {
        return super::distribution::bundled("xess");
    }
    Ok(std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("主程序目录无效")?
        .join("xess-package"))
}
fn destination(base: &Path, p: &Package) -> PathBuf {
    base.join("runtime").join(&p.version)
}
pub(super) fn hybrid_runtime() -> Result<(PathBuf, Vec<fs::File>), String> {
    let p = package();
    let dir = destination(&root(), &p);
    let files = verify(&dir, &p).map_err(|e| {
        format!("混合模式需要已安装且通过校验的 XeSS 组件，请在 XeSS 页签安装组件：{e}")
    })?;
    Ok((dir, files))
}
fn manifest_valid(p: &Package) -> Result<(), String> {
    let expected: BTreeSet<_> = MEMBERS
        .iter()
        .copied()
        .filter(|name| {
            p.schema_version != 2 || !matches!(*name, "xess-component.exe" | "xess-layer.dll")
        })
        .collect();
    if !matches!(p.schema_version, 1 | 2)
        || p.version.is_empty()
        || p.version.len() > 96
        || !p
            .version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
        || p.files.len() != expected.len()
    {
        return Err("XeSS 组件清单无效，请使用主程序配套版本".into());
    }
    let names: BTreeSet<_> = p.files.iter().map(|f| f.name.as_str()).collect();
    if names != expected
        || p.files
            .iter()
            .any(|f| f.sha256.len() != 64 || !f.sha256.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("XeSS 组件成员或校验值无效".into());
    }
    Ok(())
}
fn members(dir: &Path, p: &Package) -> Vec<PathBuf> {
    p.files
        .iter()
        .map(|f| dir.join(&f.name))
        .chain(Some(dir.join("package.json")))
        .collect()
}
fn verify(dir: &Path, p: &Package) -> Result<Vec<fs::File>, String> {
    manifest_valid(p)?;
    runtime_package::safe_path(dir)?;
    plain(dir, true)?;
    let paths = members(dir, p);
    let handles = super::streamline_install::hold_runtime_files(&paths, false)?;
    let actual: Package = serde_json::from_value(read_live_json(&dir.join("package.json"))?)
        .map_err(|e| e.to_string())?;
    if &actual != p {
        return Err("XeSS 组件清单与主程序不匹配".into());
    }
    let mut allowed: BTreeSet<PathBuf> = paths.iter().cloned().collect();
    for path in &paths {
        for parent in path.ancestors().skip(1).take_while(|a| a.starts_with(dir)) {
            allowed.insert(parent.to_owned());
        }
    }
    for entry in walkdir::WalkDir::new(dir).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        runtime_package::safe_path(entry.path())?;
        if !allowed.contains(entry.path()) {
            return Err("XeSS 目录有额外文件，保留目录，请先检查".into());
        }
    }
    for f in &p.files {
        if file_digest(&dir.join(&f.name))? != f.sha256 {
            return Err(format!("XeSS 文件未通过校验：{}", f.name));
        }
    }
    Ok(handles)
}
fn lock(base: &Path) -> Result<fs::File, String> {
    safe_dir(base)?;
    let path = base.join(".operation.lock");
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
        .map_err(|_| "另一个 XeSS 组件操作正在进行".to_owned())?;
    Ok(file)
}
fn settings_at(base: &Path) -> Result<Settings, String> {
    let path = base.join("settings.json");
    runtime_package::safe_path(&path)?;
    let settings: Settings = if path.exists() {
        {
            let value = read_live_json(&path)?;
            if value.get("fgMaxEdge").is_some() {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase", deny_unknown_fields)]
                struct Legacy {
                    native_aa: bool,
                    fg: bool,
                    fg_max_edge: u32,
                }
                let old: Legacy = serde_json::from_value(value).map_err(|e| e.to_string())?;
                if !(320..=8192).contains(&old.fg_max_edge) {
                    return Err("旧 XeSS 尺寸配置无效".into());
                }
                Settings {
                    native_aa: old.native_aa,
                    fg: old.fg,
                    input_sizing: InputSizing::default(),
                }
            } else {
                serde_json::from_value(value).map_err(|e| e.to_string())?
            }
        }
    } else {
        Settings::default()
    };
    settings.validate()?;
    Ok(settings)
}
pub fn save_settings(settings: Settings) -> Result<Settings, String> {
    settings.validate()?;
    let _guard = OPERATION.lock().map_err(|e| e.to_string())?;
    let base = root();
    let _store = lock(&base)?;
    atomic_json(
        &base.join("settings.json"),
        &serde_json::to_value(&settings).map_err(|e| e.to_string())?,
    )?;
    Ok(settings)
}
fn target(
    exe: &Path,
    api: GraphicsApi,
    expected: Option<&str>,
) -> Result<(PathBuf, String), String> {
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Err("XeSS 组件仅支持 Windows x64".into());
    }
    if api != GraphicsApi::Vulkan {
        return Err("XeSS 需要 Vulkan，请在模拟器中选择 Vulkan".into());
    }
    let exe = exe.canonicalize().map_err(|e| e.to_string())?;
    target_policy::validate_executable(&exe)?;
    let hash = file_digest(&exe)?;
    if expected.is_some_and(|v| v != hash) {
        return Err("模拟器文件已改变，请重新检查".into());
    }
    target_policy::classify(&exe, &hash, true).authorize(false)?;
    Ok((exe, hash))
}
pub fn status(executable: Option<PathBuf>, api: GraphicsApi) -> Value {
    let p = package();
    let base = root();
    let source = bundled().and_then(|d| verify(&d, &p).map(|_| d));
    let installed = verify(&destination(&base, &p), &p).and_then(|files| {
        if p.schema_version == 2 {
            super::distribution::programs(&super::distribution::common_version()?)?;
        }
        Ok(files)
    });
    let downloadable = if p.schema_version == 2 {
        super::distribution::available("xess")
    } else {
        download::release(&p).is_ok()
    };
    let target = executable.as_deref().map(|e| target(e, api, None));
    json!({"supported":cfg!(all(windows,target_arch="x86_64")),"version":p.version,
        "bundledReady":source.is_ok(),"installed":installed.is_ok(),
        "downloadAvailable":downloadable,
        "message":if installed.is_ok(){"XeSS 运行库和公共程序已安装并通过校验".to_owned()}else if source.is_ok() && (p.schema_version != 2 || super::distribution::local_ready("xess")){"配套 XeSS 运行库和公共程序已就绪，可离线安装".into()}else if downloadable{"点击下载并安装 XeSS 运行库及公共程序，下载完成后自动校验并安装".into()}else{format!("XeSS 组件不可用：{}",source.err().unwrap_or_default())},
        "targetReady":matches!(target,Some(Ok(_))),"targetHash":target.as_ref().and_then(|r|r.as_ref().ok()).map(|v|&v.1),
        "targetMessage":match &target {Some(Ok(_))=>"Vulkan x64 目标可启动；兼容性以实际运行结果为准".to_owned(),Some(Err(e))=>e.clone(),None=>"请先选择模拟器".into()},
        "settings":settings_at(&base).unwrap_or_default()})
}
fn install_at(base: &Path, source: &Path, p: &Package) -> Result<(), String> {
    let _source = verify(source, p)?;
    let dir = destination(base, p);
    if dir.exists() {
        verify(&dir, p)?;
        return Ok(());
    }
    safe_dir(dir.parent().ok_or("组件路径无效")?)?;
    let stage = tempfile::Builder::new()
        .prefix(".xess-")
        .tempdir_in(dir.parent().unwrap())
        .map_err(|e| e.to_string())?;
    for f in &p.files {
        let out = stage.path().join(&f.name);
        safe_dir(out.parent().unwrap())?;
        fs::copy(source.join(&f.name), out).map_err(|e| e.to_string())?;
    }
    atomic_json(
        &stage.path().join("package.json"),
        &serde_json::to_value(p).map_err(|e| e.to_string())?,
    )?;
    drop(verify(stage.path(), p)?);
    fs::rename(stage.path(), &dir).map_err(|e| e.to_string())?;
    Ok(())
}
pub async fn install(
    reporter: crate::services::installer::InstallReporter,
) -> Result<Value, String> {
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Err("XeSS 组件仅支持 Windows x64".into());
    }
    let source = download::ensure(reporter.clone()).await?;
    reporter
        .run_blocking_step("install", "安装 XeSS 组件", move || {
            install_source(&source)
        })
        .await
}
fn install_source(source: &Path) -> Result<Value, String> {
    let _guard = OPERATION.lock().map_err(|e| e.to_string())?;
    let base = root();
    let _store = lock(&base)?;
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Err("XeSS 组件仅支持 Windows x64".into());
    }
    install_at(&base, source, &package())?;
    Ok(json!({"message":"XeSS 组件已独立安装；DLSS 和光流组件保持独立"}))
}
fn remove_at(base: &Path, p: &Package) -> Result<(), String> {
    let dir = destination(base, p);
    if !dir.exists() {
        return Ok(());
    }
    drop(verify(&dir, p)?);
    let paths = members(&dir, p);
    let handles = super::streamline_install::hold_runtime_files(&paths, true)?;
    for path in &paths {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    drop(handles);
    let mut dirs: Vec<_> = walkdir::WalkDir::new(&dir)
        .min_depth(0)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
        .map(|e| e.into_path())
        .collect();
    dirs.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    for dir in dirs {
        fs::remove_dir(dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub fn uninstall() -> Result<Value, String> {
    let _guard = OPERATION.lock().map_err(|e| e.to_string())?;
    let base = root();
    let _store = lock(&base)?;
    remove_at(&base, &package())?;
    Ok(json!({"message":"XeSS 组件已卸载，设置和运行记录保留"}))
}
fn key(exe: &Path) -> String {
    runtime_package::hash(exe.to_string_lossy().to_lowercase().as_bytes())
}
fn session_at(base: &Path, exe: &Path) -> Result<Option<PathBuf>, String> {
    let pointer = base.join("live").join(format!("{}.json", key(exe)));
    runtime_package::safe_path(&pointer)?;
    if !pointer.exists() {
        return Ok(None);
    }
    let value = read_live_json(&pointer)?;
    let session = PathBuf::from(value["session"].as_str().ok_or("XeSS 会话记录无效")?);
    if session.parent() != Some(base.join("sessions").as_path())
        || !session
            .file_name()
            .is_some_and(|s| s.to_string_lossy().starts_with("game-"))
    {
        return Err("XeSS 会话不属于本组件".into());
    }
    runtime_package::safe_path(&session)?;
    plain(&session, true)?;
    Ok(Some(session))
}
pub fn live(
    executable: PathBuf,
    aa: Option<bool>,
    fg: Option<bool>,
    input_sizing: Option<InputSizing>,
) -> Result<Value, String> {
    if input_sizing.is_some_and(|v| !v.valid()) {
        return Err("XeSS 输入尺寸配置无效".into());
    }
    let _guard = OPERATION.lock().map_err(|e| e.to_string())?;
    let base = root();
    let exe = executable.canonicalize().map_err(|e| e.to_string())?;
    let Some(session) = session_at(&base, &exe)? else {
        return Ok(json!({"connected":false,"launchState":"idle"}));
    };
    let mut lifecycle = super::sessions::status(&session)?;
    lifecycle["session"] = json!(session);
    let launch = read_live_json(&session.join("session.json"))?;
    lifecycle["availableNativeAa"] = launch["settings"]["nativeAa"].clone();
    lifecycle["availableFg"] = launch["settings"]["fg"].clone();
    lifecycle["inputScalingLiveSupported"] = json!(launch["inputSizingSupported"] == true);
    if matches!(
        lifecycle["launchState"].as_str(),
        Some("failed" | "finished")
    ) {
        return Ok(lifecycle);
    }
    if let Some(on) = aa {
        if launch["settings"]["nativeAa"] != true {
            return Err("本次未加载 Native AA，请在下次启动时启用".into());
        }
        atomic_json(
            &session.join("run/xess-control.json"),
            &json!({"enabled":on}),
        )?;
    }
    if let Some(on) = fg {
        if launch["settings"]["fg"] != true {
            return Err("本次未加载 FG，请在下次启动时启用".into());
        }
        atomic_json(
            &session.join("run/xess-fg-control.json"),
            &json!({"enabled":on}),
        )?;
    }
    if let Some(input) = input_sizing {
        if launch["inputSizingSupported"] != true {
            return Err("当前会话尚不支持输入缩放，请使用新版 XeSS 组件重新启动一次".into());
        }
        let path = session.join("run/xess-input-control.json");
        let last = if path.exists() {
            read_live_json(&path)?["revision"].as_u64().unwrap_or(0)
        } else {
            0
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;
        let revision = now.max(last.checked_add(1).ok_or("XeSS 输入缩放版本溢出")?);
        atomic_json(&path, &json!({"inputSizing":input,"revision":revision}))?;
        lifecycle["sentInputScaleRevision"] = json!(revision);
    }
    let path = session.join("run/xess-status.json");
    if path.exists() {
        let runtime = read_live_json(&path)?;
        if runtime["schemaVersion"] != 1 {
            return Err("XeSS 运行状态版本不匹配".into());
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let fresh = runtime["lastUpdateMs"]
            .as_u64()
            .is_some_and(|v| now.abs_diff(v) <= 5000);
        lifecycle["connected"] = json!(fresh);
        lifecycle["runtime"] = runtime;
    }
    Ok(lifecycle)
}
fn clean_environment(command: &mut Command) {
    for (key, _) in std::env::vars_os() {
        let key_text = key.to_string_lossy().to_ascii_uppercase();
        if key_text.starts_with("NS_STREAMLINE_")
            || key_text.starts_with("NS_EMU_")
            || key_text.starts_with("VK_")
            || key_text == "RESHADE_BASE_PATH_OVERRIDE"
        {
            command.env_remove(key);
        }
    }
}
pub fn launch(
    executable: PathBuf,
    api: GraphicsApi,
    expected: String,
    game: Option<PathBuf>,
) -> Result<Value, String> {
    let _guard = OPERATION.lock().map_err(|e| e.to_string())?;
    let base = root();
    let _store = lock(&base)?;
    let (exe, hash) = target(&executable, api, Some(&expected))?;
    let settings = settings_at(&base)?;
    if !settings.native_aa && !settings.fg {
        return Err("请至少选择 Native AA 或 FG".into());
    }
    if let Some(path) = &game {
        if !path.is_absolute() || !path.is_file() {
            return Err("游戏路径无效".into());
        }
    }
    if let Some(previous) = session_at(&base, &exe)? {
        let state = super::sessions::status(&previous)?;
        if !matches!(state["launchState"].as_str(), Some("failed" | "finished")) {
            return Err("该模拟器已有 XeSS 会话，请先正常退出".into());
        }
    }
    let p = package();
    let dir = destination(&base, &p);
    let mut files = verify(&dir, &p)?;
    let programs = if p.schema_version == 2 {
        let (programs, handles) =
            super::distribution::programs(&super::distribution::common_version()?)?;
        files.extend(handles);
        programs
    } else {
        dir.clone()
    };
    let sessions = base.join("sessions");
    safe_dir(&sessions)?;
    let session = sessions.join(format!("game-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&session).map_err(|e| e.to_string())?;
    atomic_json(
        &session.join("session.json"),
        &json!({"executable":exe,"targetSha256":hash,"componentVersion":p.version,"inputSizingSupported":true,"settings":settings}),
    )?;
    let mut command = Command::new(programs.join(if p.schema_version == 2 {
        "graphics-launcher.exe"
    } else {
        "xess-component.exe"
    }));
    clean_environment(&mut command);
    command
        .args(["--target-probe", "--graphics-launch", "--target"])
        .arg(&exe)
        .arg("--layer")
        .arg(programs.join("xess-layer.dll"))
        .arg("--session")
        .arg(session.join("run"));
    if p.schema_version == 2 {
        command.args(["--backend", "xess"]);
    }
    command
        .env("NS_EMU_XESS_COMPONENT_ROOT", &dir)
        .env(
            "NS_EMU_XESS_NATIVE_AA",
            if settings.native_aa { "1" } else { "0" },
        )
        .env("NS_EMU_XESS_DLL", dir.join("libxess.dll"))
        .env("NS_EMU_XESS_FG", if settings.fg { "1" } else { "0" })
        .env("NS_EMU_XESS_FG_SDK", &dir)
        .env("NS_EMU_XESS_FG_AGILITY", dir.join("D3D12"))
        .env(
            "NS_EMU_XESS_INPUT_SIZING",
            serde_json::to_string(&settings.input_sizing).map_err(|e| e.to_string())?,
        )
        .env("NS_STREAMLINE_TRACE_VERBOSE", "0")
        .env("NS_STREAMLINE_TRACE_FRAMES", "1")
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(fs::File::create(session.join("launcher.log")).map_err(|e| e.to_string())?)
        .stderr(fs::File::create(session.join("launcher.error.log")).map_err(|e| e.to_string())?);
    if let Some(game) = game {
        command.arg("--game").arg(game);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let child = command.spawn().map_err(|e| e.to_string())?;
    super::sessions::monitor(child, session.clone(), sessions, files);
    safe_dir(&base.join("live"))?;
    atomic_json(
        &base.join("live").join(format!("{}.json", key(&exe))),
        &json!({"session":session}),
    )?;
    Ok(json!({"message":"XeSS 启动器已启动，等待模拟器和实际画面处理结果","session":session}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, Package) {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let base = temp.path().join("xess");
        let p = Package {
            schema_version: 1,
            version: "local-test".into(),
            files: MEMBERS
                .iter()
                .map(|name| {
                    let path = source.join(name);
                    fs::create_dir_all(path.parent().unwrap()).unwrap();
                    fs::write(path, name).unwrap();
                    Artifact {
                        name: (*name).into(),
                        sha256: runtime_package::hash(name.as_bytes()),
                    }
                })
                .collect(),
        };
        atomic_json(
            &source.join("package.json"),
            &serde_json::to_value(&p).unwrap(),
        )
        .unwrap();
        (temp, base, source, p)
    }
    #[test]
    fn independent_install_is_verified_and_tampering_is_retained() {
        let (_temp, base, source, p) = fixture();
        install_at(&base, &source, &p).unwrap();
        let dir = destination(&base, &p);
        assert!(verify(&dir, &p).is_ok());
        fs::write(dir.join("libxess.dll"), b"changed").unwrap();
        assert!(remove_at(&base, &p).is_err());
        assert!(dir.join("xess-component.exe").exists());
    }
    #[test]
    fn extra_files_and_foreign_manifests_are_never_removed() {
        let (_temp, base, source, p) = fixture();
        install_at(&base, &source, &p).unwrap();
        let dir = destination(&base, &p);
        fs::write(dir.join("user.txt"), b"retain").unwrap();
        assert!(remove_at(&base, &p).is_err());
        assert!(dir.join("libxess.dll").exists());
        let mut invalid = p.clone();
        invalid.files[0].name = "../escape".into();
        assert!(manifest_valid(&invalid).is_err());
    }
    #[test]
    fn uninstall_keeps_settings_and_other_components() {
        let (temp, base, source, p) = fixture();
        install_at(&base, &source, &p).unwrap();
        fs::write(base.join("settings.json"), b"settings").unwrap();
        fs::write(temp.path().join("dlss.txt"), b"other").unwrap();
        remove_at(&base, &p).unwrap();
        assert!(base.join("settings.json").exists());
        assert!(temp.path().join("dlss.txt").exists());
    }
    #[test]
    fn settings_reject_sr_and_unsupported_dimensions() {
        assert!(serde_json::from_value::<Settings>(json!({"nativeAa":true,"fg":true,"inputSizing":{"scalePercent":100,"maxEdge":0},"sr":true})).is_err());
        for input_sizing in [
            InputSizing {
                scale_percent: 49,
                max_edge: 0,
            },
            InputSizing {
                scale_percent: 101,
                max_edge: 0,
            },
            InputSizing {
                scale_percent: 100,
                max_edge: 319,
            },
            InputSizing {
                scale_percent: 100,
                max_edge: 8193,
            },
        ] {
            assert!(Settings {
                input_sizing,
                ..Settings::default()
            }
            .validate()
            .is_err());
        }
        for input_sizing in [
            InputSizing::default(),
            InputSizing {
                scale_percent: 50,
                max_edge: 0,
            },
            InputSizing {
                scale_percent: 75,
                max_edge: 1000,
            },
            InputSizing {
                scale_percent: 100,
                max_edge: 8192,
            },
        ] {
            assert!(Settings {
                input_sizing,
                ..Settings::default()
            }
            .validate()
            .is_ok());
        }
    }
    #[test]
    fn legacy_fg_cap_is_retired_without_changing_effects() {
        let (_temp, base, _source, _p) = fixture();
        safe_dir(&base).unwrap();
        atomic_json(
            &base.join("settings.json"),
            &json!({"nativeAa":false,"fg":true,"fgMaxEdge":640}),
        )
        .unwrap();
        let migrated = settings_at(&base).unwrap();
        assert!(!migrated.native_aa && migrated.fg);
        assert_eq!(migrated.input_sizing, InputSizing::default());
        atomic_json(
            &base.join("settings.json"),
            &json!({"nativeAa":false,"fg":true,"fgMaxEdge":640,"sr":true}),
        )
        .unwrap();
        assert!(settings_at(&base).is_err());
    }
    #[test]
    #[cfg(windows)]
    fn active_readers_block_uninstall_before_any_component_is_removed() {
        let (_temp, base, source, p) = fixture();
        install_at(&base, &source, &p).unwrap();
        let dir = destination(&base, &p);
        let active = verify(&dir, &p).unwrap();
        assert!(remove_at(&base, &p).is_err());
        assert!(p.files.iter().all(|f| dir.join(&f.name).is_file()));
        drop(active);
        remove_at(&base, &p).unwrap();
    }
    #[test]
    fn session_pointer_cannot_escape_xess_store() {
        let (temp, base, _source, _p) = fixture();
        let exe = temp.path().join("Ryujinx.exe");
        safe_dir(&base.join("live")).unwrap();
        atomic_json(
            &base.join("live").join(format!("{}.json", key(&exe))),
            &json!({"session":temp.path()}),
        )
        .unwrap();
        assert!(session_at(&base, &exe).is_err());
    }
}

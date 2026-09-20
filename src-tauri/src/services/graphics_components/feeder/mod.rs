//! Experimental Feeder + RHI bundle. Deployment is never a runtime compatibility claim.
use super::*;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use transaction::{file_hash, hash, read_optional};
pub mod packages;
mod store;
pub use store::Record as Installation;

pub fn installation(exe: PathBuf) -> Result<Option<Installation>, String> {
    transaction::safe_path(&exe)?;
    let exe = exe.canonicalize().map_err(|e| e.to_string())?;
    store::record(exe.parent().ok_or("目标没有父目录")?)
}

pub const BUNDLE: &str = "feeder-1.16.0-beta.6_sr-310.9.1_nr-310.8.0_renodx-4.70";
const PRESET: &str = "ns-emu-tools-feeder/FeederPreset.ini";
const FILES: &[&str] = &[
    "dlss5-feed.addon64",
    "renodx-dlss5.addon64",
    "nvngx_dlss.dll",
    "nvngx_dlssnr.dll",
    "ns-emu-tools-feeder/Shaders/DLSS5_Feed.fx",
    "ns-emu-tools-feeder/Shaders/lumenite_Kernel.fx",
    "ns-emu-tools-feeder/Shaders/ReShade.fxh",
    "ns-emu-tools-feeder/Shaders/include/lumenite_ColorManagement.fxh",
    "ns-emu-tools-feeder/Shaders/include/lumenite_Compute.fxh",
    "ns-emu-tools-feeder/Shaders/include/lumenite_Helpers.fxh",
    "ns-emu-tools-feeder/Shaders/include/lumenite_Projections.fxh",
    "ns-emu-tools-feeder/Textures/lumenite_bluenoise256.png",
    PRESET,
    "ReShade.ini",
];
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub plan_id: Option<String>,
    pub bundle: String,
    pub executable: PathBuf,
    pub graphics_api: GraphicsApi,
    pub files: Vec<String>,
    pub sources: Vec<packages::Asset>,
    pub blockers: Vec<String>,
    pub diagnostics: Vec<String>,
    pub compatibility_verified: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub message: String,
    pub preserved_files: Vec<String>,
    pub compatibility_verified: bool,
}
struct Plan {
    created: Instant,
    exe: PathBuf,
    api: GraphicsApi,
    cache: PathBuf,
    exe_hash: String,
    target: BTreeMap<String, Option<String>>,
    payload: BTreeMap<String, String>,
    reshade: String,
}
static PLANS: Lazy<Mutex<HashMap<String, Plan>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Conservative INI update: preserve unrelated lines; ambiguous duplicate keys/sections are blocked.
fn set_ini(
    text: &str,
    section: &str,
    key: &str,
    value: &str,
    append: bool,
) -> Result<String, String> {
    let text = text.trim_start_matches('\u{feff}');
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let mut starts = vec![];
    for (i, l) in lines.iter().enumerate() {
        if l.trim().eq_ignore_ascii_case(&format!("[{section}]")) {
            starts.push(i);
        }
    }
    if starts.len() > 1 {
        return Err(format!("ReShade.ini 存在重复 [{section}]"));
    }
    if starts.is_empty() {
        lines.push(format!("[{section}]"));
        starts.push(lines.len() - 1);
    }
    let start = starts[0] + 1;
    let end = (start..lines.len())
        .find(|i| lines[*i].trim().starts_with('['))
        .unwrap_or(lines.len());
    let matches: Vec<usize> = (start..end)
        .filter(|i| {
            lines[*i]
                .split_once('=')
                .is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(key))
        })
        .collect();
    if matches.len() > 1 {
        return Err(format!("ReShade.ini 存在重复 {section}/{key}"));
    }
    if let Some(i) = matches.first() {
        let old = lines[*i].split_once('=').unwrap().1.trim();
        let value = if append
            && !old.is_empty()
            && !old.split(',').any(|v| v.trim().eq_ignore_ascii_case(value))
        {
            format!("{old},{value}")
        } else if append && !old.is_empty() {
            old.into()
        } else {
            value.into()
        };
        lines[*i] = format!("{key}={value}");
    } else {
        lines.insert(end, format!("{key}={value}"));
    }
    Ok(lines.join("\r\n") + "\r\n")
}
fn configured(cache: &Path, dir: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut files = packages::payload(cache)?;
    let bytes = read_optional(&dir.join("ReShade.ini"))?.unwrap_or_default();
    let mut ini = String::from_utf8(bytes).map_err(|_| "ReShade.ini 非 UTF-8，暂不自动修改")?;
    for (section, key, value, append) in [
        ("ADDON", "AddonPath", r".\", true),
        (
            "GENERAL",
            "EffectSearchPaths",
            r".\ns-emu-tools-feeder\Shaders",
            true,
        ),
        (
            "GENERAL",
            "TextureSearchPaths",
            r".\ns-emu-tools-feeder\Textures",
            true,
        ),
        (
            "GENERAL",
            "PresetPath",
            r".\ns-emu-tools-feeder\FeederPreset.ini",
            false,
        ),
    ] {
        ini = set_ini(&ini, section, key, value, append)?;
    }
    files.insert("ReShade.ini".into(), ini.into_bytes());
    files.insert(PRESET.into(), b"PreprocessorDefinitions=DLSS5_MV_PROVIDER=3\r\nTechniques=Lumenite_Kernel@lumenite_Kernel.fx,DLSS5_Feed@DLSS5_Feed.fx\r\nTechniqueSorting=Lumenite_Kernel@lumenite_Kernel.fx,DLSS5_Feed@DLSS5_Feed.fx\r\n".to_vec());
    Ok(files)
}
fn reshade_snapshot(exe: &Path, api: GraphicsApi) -> Result<String, String> {
    let report = super::detect(exe.into(), Some(api));
    if report.reshade_state != GraphicsComponentState::Installed {
        return Err("需要本工具管理且校验完整的 ReShade Addon 安装".into());
    }
    let record = match api {
        GraphicsApi::Vulkan => vulkan::read_record(exe)?,
        GraphicsApi::OpenGl => transaction::read_record(exe.parent().ok_or("缺少目录")?)?,
    }
    .ok_or("缺少 ReShade 记录")?;
    if record.source_url.as_deref().is_none_or(|u| {
        !u.starts_with("https://reshade.me/downloads/ReShade_Setup_") || !u.ends_with("_Addon.exe")
    }) {
        return Err("无法确认 ReShade 为官方 Addon 版".into());
    }
    let version = record
        .version
        .as_deref()
        .unwrap_or("")
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "ReShade 版本无效")?;
    if version.len() != 3 || version.as_slice() < [6, 8, 0].as_slice() {
        return Err("Feeder 需要 ReShade 6.8.0 或更新版本".into());
    }
    Ok(hash(
        &serde_json::to_vec(&record).map_err(|e| e.to_string())?,
    ))
}
fn conflicts(dir: &Path) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let name = entry
            .map_err(|e| e.to_string())?
            .file_name()
            .to_string_lossy()
            .to_ascii_lowercase();
        if name.starts_with("deep-fried-chicken")
            || name.starts_with("alexs-toolkit")
            || name.starts_with("optiscaler")
            || name == "renodx-dlss.addon64"
            || (name.starts_with("renodx-dlss5")
                && name.ends_with(".addon64")
                && name != "renodx-dlss5.addon64")
            || [
                "dxgi.dll",
                "d3d11.dll",
                "d3d12.dll",
                "winmm.dll",
                "version.dll",
                "nvngx.dll",
                "_nvngx.dll",
            ]
            .contains(&name.as_str())
        {
            return Err(format!("发现可能冲突的消费者/代理，请先处理: {name}"));
        }
    }
    Ok(())
}
pub async fn prepare(
    exe: PathBuf,
    api: GraphicsApi,
    reporter: crate::services::installer::InstallReporter,
) -> Result<Preview, String> {
    transaction::target_directory(&exe)?;
    let cache = packages::download(reporter).await?;
    tokio::task::spawn_blocking(move || preview(exe, api, cache))
        .await
        .map_err(|e| e.to_string())?
}
pub fn preview(exe: PathBuf, api: GraphicsApi, cache: PathBuf) -> Result<Preview, String> {
    let dir = transaction::target_directory(&exe)?;
    let exe = exe.canonicalize().map_err(|e| e.to_string())?;
    let _guard = transaction::Store::open(&dir)?;
    let mut blockers = vec![];
    if let Err(e) = conflicts(&dir) {
        blockers.push(e);
    }
    if store::pending(&dir)? {
        blockers.push("存在未恢复的 Feeder 事务".into());
    }
    let reshade = match reshade_snapshot(&exe, api) {
        Ok(s) => s,
        Err(e) => {
            blockers.push(e);
            String::new()
        }
    };
    let old = store::record(&dir)?;
    if old
        .as_ref()
        .is_some_and(|r| r.executable != exe || r.graphics_api != api)
    {
        blockers.push("当前目录已有其他目标/API 的 Feeder 记录，请先卸载".into());
    }
    let target = store::snapshot(&dir)?;
    for p in FILES {
        if let Some(r) = &old {
            if target[*p].as_deref() != Some(r.files[*p].deployed.as_str()) {
                blockers.push(format!("已安装文件被修改或缺失: {p}"));
            }
        } else if *p != "ReShade.ini" && target[*p].is_some() {
            blockers.push(format!("保留外部文件，拒绝覆盖: {p}"));
        }
    }
    let files = configured(&cache, &dir)?;
    let plan_id = if blockers.is_empty() {
        let id = uuid::Uuid::new_v4().to_string();
        let mut plans = PLANS.lock();
        plans.retain(|_, p| p.created.elapsed() < Duration::from_secs(900));
        if plans.len() >= 16 {
            return Err("待执行 Feeder 计划过多".into());
        }
        plans.insert(
            id.clone(),
            Plan {
                created: Instant::now(),
                exe: exe.clone(),
                api,
                cache,
                exe_hash: file_hash(&exe)?.ok_or("EXE 缺失")?,
                target,
                payload: files.iter().map(|(p, b)| (p.clone(), hash(b))).collect(),
                reshade,
            },
        );
        Some(id)
    } else {
        None
    };
    Ok(Preview { plan_id, bundle: BUNDLE.into(), executable: exe, graphics_api: api, files: FILES.iter().map(|s|s.to_string()).collect(), sources: packages::assets()?, blockers,
        diagnostics: vec!["实验性组合：SR/NR/RenoDX 来自 RHI 社区镜像；不代表 NVIDIA 官方发布或模拟器兼容性认证。Ryujinx 1.3.351 实测已能执行 NR，但深度采样为零，窗口尺寸变化后发生异常退出；该组合未通过兼容性验收。".into(),
            "切换到专属 preset，保留原 preset；运行后检查深度、运动向量、DLAA evaluate 与神经消费者输出。".into(),
            "Vulkan 测试需关闭 NVIDIA Smooth Motion；本安装器不修改驱动设置。".into()], compatibility_verified: false })
}
pub fn install(id: String) -> Result<Operation, String> {
    let plan = PLANS
        .lock()
        .remove(&id)
        .ok_or("Feeder 计划已过期、已使用或不存在")?;
    if plan.created.elapsed() >= Duration::from_secs(900) {
        return Err("Feeder 计划已过期".into());
    }
    let dir = transaction::target_directory(&plan.exe)?;
    let _guard = transaction::Store::open(&dir)?;
    let _vk_guard = if plan.api == GraphicsApi::Vulkan {
        Some(vulkan::lock_for_feeder()?)
    } else {
        None
    };
    conflicts(&dir)?;
    if file_hash(&plan.exe)?.as_deref() != Some(plan.exe_hash.as_str())
        || reshade_snapshot(&plan.exe, plan.api)? != plan.reshade
    {
        return Err("模拟器或 ReShade 已变化，请重新预检".into());
    }
    let files = configured(&plan.cache, &dir)?;
    let hashes: BTreeMap<_, _> = files.iter().map(|(p, b)| (p.clone(), hash(b))).collect();
    if hashes != plan.payload {
        return Err("组件或配置已变化，请重新预检".into());
    }
    store::deploy(&plan.exe, plan.api, files, &plan.target)?;
    Ok(Operation {
        message: "Feeder、RHI 依赖与独立 preset 已部署；运行效果尚未验证。".into(),
        preserved_files: vec![],
        compatibility_verified: false,
    })
}
pub fn remove_or_repair(exe: PathBuf, repair: bool) -> Result<Operation, String> {
    let dir = transaction::target_directory(&exe)?;
    let _guard = transaction::Store::open(&dir)?;
    let needs_vk_lock = store::record(&dir)?.map_or(store::pending(&dir)?, |r| {
        r.graphics_api == GraphicsApi::Vulkan
    });
    let _vk_guard = if needs_vk_lock {
        Some(vulkan::lock_for_feeder()?)
    } else {
        None
    };
    let preserved_files = store::remove_or_repair(&dir, repair)?;
    Ok(Operation {
        message: if repair {
            "Feeder 未完成事务已回滚。"
        } else {
            "Feeder 已卸载；普通 ReShade 保留，修改过的配置和运行时生成的日志/配置保留。"
        }
        .into(),
        preserved_files,
        compatibility_verified: false,
    })
}
pub(super) fn detection(dir: &Path) -> Result<GraphicsComponentState, String> {
    if store::pending(dir)? {
        return Ok(GraphicsComponentState::Incomplete);
    }
    if let Some(r) = store::record(dir)? {
        let mut modified = false;
        for (p, f) in r.files {
            match file_hash(&dir.join(p))? {
                None => return Ok(GraphicsComponentState::Incomplete),
                Some(h) if h != f.deployed => modified = true,
                _ => {}
            }
        }
        return Ok(if modified {
            GraphicsComponentState::Modified
        } else {
            GraphicsComponentState::Installed
        });
    }
    for p in ["dlss5-feed.addon64", "renodx-dlss5.addon64"] {
        if fs::symlink_metadata(dir.join(p)).is_ok() {
            return Ok(GraphicsComponentState::External);
        }
    }
    Ok(GraphicsComponentState::NotInstalled)
}
pub(super) fn has_record_or_pending(dir: &Path) -> Result<bool, String> {
    Ok(store::pending(dir)? || store::record(dir)?.is_some())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ini_keeps_user_settings_and_blocks_ambiguity() {
        let ini = "[GENERAL]\r\nPresetPath=user.ini\r\nOther=42\r\nEffectSearchPaths=.\\user\r\n[INPUT]\r\nKeyMenu=36\r\n";
        let merged = set_ini(ini, "GENERAL", "EffectSearchPaths", r".\managed", true).unwrap();
        assert!(merged.contains("Other=42"));
        assert!(merged.contains(r"EffectSearchPaths=.\user,.\managed"));
        assert_eq!(
            set_ini(&merged, "GENERAL", "EffectSearchPaths", r".\managed", true).unwrap(),
            merged
        );
        assert!(set_ini(
            "[GENERAL]\n[GENERAL]\n",
            "GENERAL",
            "PresetPath",
            "x",
            false
        )
        .is_err());
    }
}

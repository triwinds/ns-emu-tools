//! 只读检测：不联网，不修改目标文件或 Vulkan 注册信息。

use crate::config::{effective_config_dir, get_config};
use crate::models::graphics_components::*;
use crate::models::storage::Storage;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};

pub mod feeder;
pub mod packages;
pub mod planning;
pub mod streamline_fg;
pub mod transaction;
pub mod vulkan;
use std::path::{Path, PathBuf};

const YUZU_NAMES: &[&str] = &["yuzu.exe", "eden.exe", "citron.exe", "suzu.exe", "cemu.exe"];
const RYUJINX_NAMES: &[&str] = &["Ryujinx.exe", "Ryujinx.Ava.exe"];
const COMPONENT_FILES: &[&str] = &[
    "opengl32.dll",
    "ReShade64.dll",
    "ReShade.ini",
    "ReShadePreset.ini",
    "dlss5-feed.addon64",
    "renodx-dlss5.addon64",
    "nvngx_dlss.dll",
    "nvngx_dlssnr.dll",
];

pub fn list_targets() -> Result<Vec<GraphicsTargetCandidate>, String> {
    let config = get_config();
    // 不调用 Storage::load：首次加载会保存默认配置。
    let storage = match fs::read(effective_config_dir().join("storage.json")) {
        Ok(bytes) => serde_json::from_slice::<Storage>(&bytes)
            .map_err(|error| format!("无法解析历史配置: {error}"))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Storage::default(),
        Err(error) => return Err(format!("无法读取历史配置: {error}")),
    };
    let mut directories = vec![
        ("yuzu", config.yuzu.yuzu_path),
        ("ryujinx", config.ryujinx.path),
    ];
    directories.extend(
        storage
            .yuzu_history
            .into_values()
            .map(|c| ("yuzu", c.yuzu_path)),
    );
    directories.extend(
        storage
            .ryujinx_history
            .into_values()
            .map(|c| ("ryujinx", c.path)),
    );
    collect_targets(directories)
}

fn collect_targets(
    directories: Vec<(&str, PathBuf)>,
) -> Result<Vec<GraphicsTargetCandidate>, String> {
    let mut targets = BTreeMap::new();
    for (family, directory) in directories {
        if directory.as_os_str().is_empty() {
            continue;
        }
        let names = if family == "yuzu" {
            YUZU_NAMES
        } else {
            RYUJINX_NAMES
        };
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("无法检查 {}: {error}", directory.display())),
        };
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            if !names.iter().any(|name| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(name)
            }) {
                continue;
            }
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let executable = path.canonicalize().map_err(|error| error.to_string())?;
            let key = executable.to_string_lossy().into_owned();
            let key = if cfg!(windows) {
                key.to_lowercase()
            } else {
                key
            };
            targets.entry(key).or_insert(GraphicsTargetCandidate {
                family: family.to_string(),
                executable,
            });
        }
    }
    Ok(targets.into_values().collect())
}

/// 有界读取头部；不把只有 MZ 签名或伪装成 exe 的 DLL 当作有效目标。
fn executable_architecture(path: &Path) -> Result<ExecutableArchitecture, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("目标不是普通文件".into());
    }
    pe_architecture(&mut file, metadata.len(), false)
}

fn pe_architecture(
    file: &mut (impl Read + Seek),
    length: u64,
    expect_dll: bool,
) -> Result<ExecutableArchitecture, String> {
    let mut dos = [0u8; 64];
    file.read_exact(&mut dos).map_err(|_| "DOS 文件头不完整")?;
    if &dos[..2] != b"MZ" {
        return Err("目标不是 Windows PE 文件".into());
    }
    let offset = u64::from(u32::from_le_bytes(dos[60..64].try_into().unwrap()));
    if offset < 64 || offset + 24 > length {
        return Err("PE 文件头偏移无效".into());
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| error.to_string())?;
    let mut header = [0u8; 24];
    file.read_exact(&mut header)
        .map_err(|_| "PE 文件头不完整")?;
    if &header[..4] != b"PE\0\0" {
        return Err("PE 签名无效".into());
    }
    let machine = u16::from_le_bytes([header[4], header[5]]);
    let sections = u16::from_le_bytes([header[6], header[7]]);
    let optional_size = u16::from_le_bytes([header[20], header[21]]);
    let characteristics = u16::from_le_bytes([header[22], header[23]]);
    if characteristics & 2 == 0 || (characteristics & 0x2000 != 0) != expect_dll {
        return Err("PE 文件类型与要求的 EXE/DLL 不一致".into());
    }
    if sections == 0
        || optional_size < 2
        || offset + 24 + u64::from(optional_size) + u64::from(sections) * 40 > length
    {
        return Err("PE 可选头或节表不完整".into());
    }
    let mut magic = [0u8; 2];
    file.read_exact(&mut magic).map_err(|_| "PE 可选头不完整")?;
    let magic = u16::from_le_bytes(magic);
    if !matches!(magic, 0x10b | 0x20b) || optional_size < if magic == 0x20b { 112 } else { 96 } {
        return Err("PE 可选头无效".into());
    }
    match (machine, magic) {
        (0x8664, 0x20b) => Ok(ExecutableArchitecture::X64),
        (0x014c, 0x10b) => Ok(ExecutableArchitecture::X86),
        (0xaa64, 0x20b) => Ok(ExecutableArchitecture::Arm64),
        (0x8664 | 0x014c | 0xaa64, _) => Err("PE 架构与可选头不一致".into()),
        _ => Ok(ExecutableArchitecture::Unknown),
    }
}

pub fn detect(
    executable: PathBuf,
    graphics_api: Option<GraphicsApi>,
) -> GraphicsComponentDetection {
    let mut report = GraphicsComponentDetection {
        executable: executable.clone(),
        architecture: ExecutableArchitecture::Unknown,
        graphics_api,
        supported_target: false,
        installation_available: false,
        reshade_state: GraphicsComponentState::Unknown,
        feeder_state: GraphicsComponentState::Unknown,
        compatibility_verified: false,
        evidence: Vec::new(),
        diagnostics: vec![
            "作用范围是模拟器安装目录，同一安装下的多个游戏可能共用配置。".into(),
            "DLSS Feeder 尚未验证；文件存在不代表已加载或效果可用。".into(),
            "OpenGL 支持计划驱动的安装与恢复；Vulkan 支持当前用户共享 layer；Feeder 支持实验性 RHI 组合部署，效果尚未验证。"
                .into(),
        ],
    };
    if !cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        report.reshade_state = GraphicsComponentState::Unsupported;
        report.feeder_state = GraphicsComponentState::Unsupported;
        report
            .diagnostics
            .push("首期仅支持 Windows x64 宿主。".into());
        return report;
    }
    let result = (|| -> Result<(), String> {
        report.executable = executable
            .canonicalize()
            .map_err(|error| format!("无法访问目标: {error}"))?;
        report.architecture = executable_architecture(&report.executable)?;
        report.supported_target = report.architecture == ExecutableArchitecture::X64;
        if !report.supported_target {
            report.reshade_state = GraphicsComponentState::Unsupported;
            report.feeder_state = GraphicsComponentState::Unsupported;
            report
                .diagnostics
                .push("仅支持 x64 PE 模拟器；不支持 x86、ARM64 或未知架构。".into());
            return Ok(());
        }
        let directory = report.executable.parent().ok_or("目标没有父目录")?;
        for name in COMPONENT_FILES {
            match fs::symlink_metadata(directory.join(name)) {
                Ok(metadata) => report.evidence.push(GraphicsComponentEvidence {
                    relative_path: (*name).to_string(),
                    is_regular_file: metadata.is_file() && !metadata.file_type().is_symlink(),
                }),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("无法检查 {name}: {error}")),
            }
        }
        let has = |name: &str| {
            report
                .evidence
                .iter()
                .any(|entry| entry.relative_path == name)
        };
        if graphics_api == Some(GraphicsApi::OpenGl) {
            report.reshade_state = if has("opengl32.dll") {
                GraphicsComponentState::External
            } else {
                GraphicsComponentState::NotInstalled
            };
        } else {
            report
                .diagnostics
                .push("Vulkan layer 使用共享注册，不能仅根据目标目录判断状态。".into());
        }
        if has("dlss5-feed.addon64") || has("renodx-dlss5.addon64") {
            report.feeder_state = GraphicsComponentState::External;
        }
        if !report.evidence.is_empty() {
            report.diagnostics.push(
                "发现外部或未知文件，文件存在本身不能确认组件版本或安装成功，所有权须另行核对安装记录。".into(),
            );
        }
        if graphics_api == Some(GraphicsApi::OpenGl) {
            report.installation_available = true;
            if transaction::pending(directory)? {
                report.reshade_state = GraphicsComponentState::Incomplete;
                report.installation_available = false;
                report
                    .diagnostics
                    .push("存在未完成事务，请先执行恢复。".into());
            } else if let Some(record) = transaction::read_record(directory)? {
                report.reshade_state =
                    match transaction::file_hash(&directory.join("opengl32.dll"))? {
                        Some(hash) if hash == record.deployed_sha256 => {
                            GraphicsComponentState::Installed
                        }
                        Some(_) => GraphicsComponentState::Modified,
                        None => GraphicsComponentState::Incomplete,
                    };
                report.installation_available =
                    report.reshade_state == GraphicsComponentState::Installed;
                report
                    .diagnostics
                    .push("所有权来自独立安装记录和部署哈希；运行加载仍未验证。".into());
            }
        }
        report.feeder_state = feeder::detection(directory)?;
        if graphics_api == Some(GraphicsApi::Vulkan) {
            vulkan::detect(&mut report)?;
        }

        if graphics_api.is_none() {
            report
                .diagnostics
                .push("图形 API 未确定，请选择模拟器实际配置的 Vulkan 或 OpenGL。".into());
        }
        Ok(())
    })();
    if let Err(error) = result {
        report.supported_target = false;
        report.installation_available = false;
        report.reshade_state = GraphicsComponentState::Error;
        report.feeder_state = GraphicsComponentState::Error;
        report.diagnostics.push(error);
    }
    report
}

#[cfg(test)]
mod tests;

pub mod streamline_install;

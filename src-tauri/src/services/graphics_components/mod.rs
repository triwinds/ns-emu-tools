//! 模拟器目标枚举与原生画面增强组件。

use crate::config::{effective_config_dir, get_config};
use crate::models::graphics_components::*;
use crate::models::storage::Storage;
use std::collections::BTreeMap;
use std::fs;
#[cfg(test)]
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub mod native_nr;
pub(super) mod runtime_package;
pub mod streamline_download;
pub mod streamline_fg;
pub(super) mod streamline_update;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

const YUZU_NAMES: &[&str] = &["yuzu.exe", "eden.exe", "citron.exe", "suzu.exe", "cemu.exe"];
const RYUJINX_NAMES: &[&str] = &["Ryujinx.exe", "Ryujinx.Ava.exe"];

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
#[cfg(test)]
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

#[cfg(test)]
mod tests;

pub mod streamline_install;

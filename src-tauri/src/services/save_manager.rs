//! Yuzu 存档管理服务
//!
//! 提供存档备份、还原、列表管理等功能

use crate::error::{AppError, AppResult};
use crate::models::storage::STORAGE;
use crate::services::yuzu::get_yuzu_nand_path;
use crate::utils::archive::{compress_folder_to_7z, is_7z_file};
use crate::utils::common::normalize_path;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// 游戏 ID 正则表达式 (16 位十六进制)
const GAME_ID_PATTERN: &str = r"^[0-9A-Fa-f]{16}$";

/// 用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    /// 用户 ID (UUID 格式)
    pub user_id: String,
    /// 用户文件夹名 (32 位十六进制)
    pub folder: String,
}

/// 游戏存档信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSaveInfo {
    /// 游戏 Title ID
    pub title_id: String,
    /// 存档文件夹路径
    pub folder: String,
}

/// 备份信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    /// 文件名
    pub filename: String,
    /// 文件路径
    pub path: String,
    /// 游戏 Title ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_id: Option<String>,
    /// 备份时间 (Unix timestamp 毫秒)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bak_time: Option<i64>,
}

/// 获取 Yuzu 存档路径
///
/// 路径格式: {yuzu_nand_path}/user/save/0000000000000000
pub fn get_yuzu_save_path() -> PathBuf {
    let nand_path = get_yuzu_nand_path();
    nand_path.join("user/save/0000000000000000")
}

/// 获取所有用户文件夹名（32位十六进制）
fn get_all_user_ids() -> AppResult<Vec<String>> {
    debug!("获取所有用户 ID");
    let save_path = get_yuzu_save_path();
    debug!("存档路径: {}", save_path.display());

    if !save_path.exists() {
        warn!("存档路径不存在: {}", save_path.display());
        return Ok(Vec::new());
    }

    let mut user_ids = Vec::new();

    for entry in fs::read_dir(&save_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                // 检查是否为 32 位十六进制字符串
                if name.len() == 32 && name.chars().all(|c| c.is_ascii_hexdigit()) {
                    debug!("找到用户文件夹: {}", name);
                    user_ids.push(name.to_uppercase());
                }
            }
        }
    }

    info!("找到 {} 个用户", user_ids.len());
    Ok(user_ids)
}

/// 将用户文件夹名转换为 UUID 格式
///
/// 例如: 97A1DAE861CD445AB9645267B3AB99BE -> be99abb3-6752-64b9-5a44-cd61e8daa197
fn convert_to_uuid(user_id: &str) -> String {
    let mut tmp = String::new();

    // 反转字节顺序
    for i in 0..16 {
        let start = i * 2;
        let end = start + 2;
        if end <= user_id.len() {
            tmp = format!("{}{}", &user_id[start..end], tmp);
        }
    }

    // 格式化为 UUID
    if tmp.len() >= 32 {
        format!(
            "{}-{}-{}-{}-{}",
            &tmp[0..8],
            &tmp[8..12],
            &tmp[12..16],
            &tmp[16..20],
            &tmp[20..]
        )
        .to_lowercase()
    } else {
        user_id.to_lowercase()
    }
}

/// 获取所有存档中的用户
pub fn get_users_in_save() -> AppResult<Vec<UserInfo>> {
    let user_ids = get_all_user_ids()?;

    let users: Vec<UserInfo> = user_ids
        .into_iter()
        .map(|folder| UserInfo {
            user_id: convert_to_uuid(&folder),
            folder,
        })
        .collect();

    Ok(users)
}

/// 列出指定用户的所有游戏存档
pub fn list_all_games_by_user_folder(user_folder_name: &str) -> AppResult<Vec<GameSaveInfo>> {
    info!("列出用户 {} 的所有游戏存档", user_folder_name);
    let save_path = get_yuzu_save_path();
    let user_save_folder = save_path.join(user_folder_name);

    debug!("用户存档目录: {}", user_save_folder.display());

    if !user_save_folder.exists() {
        warn!("用户存档目录不存在: {}", user_save_folder.display());
        return Ok(Vec::new());
    }

    let game_id_re = Regex::new(GAME_ID_PATTERN).unwrap();
    let mut games = Vec::new();

    for entry in fs::read_dir(&user_save_folder)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if game_id_re.is_match(name) {
                    debug!("找到游戏存档: {}", name);
                    games.push(GameSaveInfo {
                        title_id: name.to_uppercase(),
                        folder: path.to_string_lossy().to_string(),
                    });
                }
            }
        }
    }

    info!("找到 {} 个游戏存档", games.len());
    Ok(games)
}

/// 格式化文件大小
pub fn sizeof_fmt(num: u64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB", "ZiB"];
    let mut size = num as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < units.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", size as u64, units[unit_index])
    } else {
        format!("{:.1} {}", size, units[unit_index])
    }
}

/// 备份存档文件夹
pub fn backup_folder(folder_path: &str) -> AppResult<(PathBuf, u64)> {
    info!("开始备份存档文件夹: {}", folder_path);
    let storage = STORAGE.read();
    let yuzu_save_backup_path = &storage.yuzu_save_backup_path;
    debug!("备份目标路径: {}", yuzu_save_backup_path.display());

    // 确保备份目录存在
    if !yuzu_save_backup_path.exists() {
        info!("创建备份目录: {}", yuzu_save_backup_path.display());
        fs::create_dir_all(yuzu_save_backup_path)?;
    } else {
        debug!("备份目录已存在");
    }

    let folder_path = PathBuf::from(folder_path);
    debug!("源文件夹路径: {}", folder_path.display());

    // 生成备份文件名: yuzu_{title_id}_{timestamp}.7z
    let folder_name = folder_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| {
            warn!("无效的文件夹路径: {}", folder_path.display());
            AppError::Unknown("无效的文件夹路径".to_string())
        })?;

    debug!("文件夹名称 (Title ID): {}", folder_name);

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let backup_filename = format!("yuzu_{}_{}.7z", folder_name, timestamp);
    let backup_filepath = yuzu_save_backup_path.join(&backup_filename);

    info!(
        "备份文件夹 [{}] 至 {}",
        folder_path.display(),
        backup_filepath.display()
    );
    debug!("备份文件名: {}, 时间戳: {}", backup_filename, timestamp);

    // 压缩文件夹
    debug!("开始压缩文件夹到 7z 格式");
    compress_folder_to_7z(&folder_path, &backup_filepath)?;
    debug!("压缩完成");

    // 获取备份文件大小
    let file_size = fs::metadata(&backup_filepath)?.len();
    debug!("备份文件大小: {} bytes", file_size);

    info!(
        "{} 备份完成, 大小: {}",
        backup_filepath.display(),
        sizeof_fmt(file_size)
    );

    Ok((backup_filepath, file_size))
}

/// 更新 Yuzu 存档备份文件夹
pub fn update_yuzu_save_backup_folder(folder: &str) -> AppResult<()> {
    let new_path = PathBuf::from(folder);

    let mut storage = STORAGE.write();
    let current_path = storage.yuzu_save_backup_path.clone();

    // 规范化路径并去除 Windows 长路径前缀
    let new_path_abs = normalize_path(&new_path);
    let current_path_abs = normalize_path(&current_path);

    if new_path_abs == current_path_abs {
        return Err(AppError::Unknown("文件夹未发生变动，取消变更".to_string()));
    }

    storage.yuzu_save_backup_path = new_path_abs.clone();
    storage.save()?;

    info!(
        "yuzu 存档备份文件夹更改为: {}",
        new_path_abs.display()
    );

    Ok(())
}

/// 获取当前 Yuzu 存档备份文件夹
pub fn get_yuzu_save_backup_folder() -> AppResult<String> {
    let storage = STORAGE.read();
    Ok(storage
        .yuzu_save_backup_path
        .to_string_lossy()
        .to_string())
}

/// 解析备份文件信息
///
/// 从文件名解析 title_id 和备份时间
/// 文件名格式: yuzu_{title_id}_{timestamp}.7z
fn parse_backup_info(file: &Path) -> BackupInfo {
    let filename = file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let mut info = BackupInfo {
        filename: filename.clone(),
        path: file.to_string_lossy().to_string(),
        title_id: None,
        bak_time: None,
    };

    // 解析文件名: yuzu_{title_id}_{timestamp}.7z
    if filename.starts_with("yuzu_") && filename.ends_with(".7z") {
        let name_part = &filename[5..filename.len() - 3]; // 去掉 "yuzu_" 和 ".7z"

        if let Some(last_underscore) = name_part.rfind('_') {
            let title_id = &name_part[..last_underscore];
            let timestamp_str = &name_part[last_underscore + 1..];

            info.title_id = Some(title_id.to_string());

            if let Ok(timestamp) = timestamp_str.parse::<i64>() {
                info.bak_time = Some(timestamp * 1000); // 转换为毫秒
            }
        }
    }

    info
}

/// 列出所有 Yuzu 备份
pub fn list_all_yuzu_backups() -> AppResult<Vec<BackupInfo>> {
    debug!("列出所有 Yuzu 备份");
    let storage = STORAGE.read();
    let backup_path = &storage.yuzu_save_backup_path;
    debug!("备份路径: {}", backup_path.display());

    if !backup_path.exists() {
        warn!("备份路径不存在: {}", backup_path.display());
        return Ok(Vec::new());
    }

    let mut backups = Vec::new();

    for entry in fs::read_dir(backup_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("yuzu_") && name.ends_with(".7z") {
                    debug!("找到备份文件: {}", name);
                    let backup_info = parse_backup_info(&path);
                    debug!("解析备份信息: title_id={:?}, bak_time={:?}",
                        backup_info.title_id, backup_info.bak_time);
                    backups.push(backup_info);
                }
            }
        }
    }

    // 按备份时间倒序排序
    debug!("按时间倒序排序备份列表");
    backups.sort_by(|a, b| {
        let time_a = a.bak_time.unwrap_or(0);
        let time_b = b.bak_time.unwrap_or(0);
        time_b.cmp(&time_a)
    });

    info!("找到 {} 个备份文件", backups.len());
    Ok(backups)
}

/// 从备份还原 Yuzu 存档
pub fn restore_yuzu_save_from_backup(
    user_folder_name: &str,
    backup_path: &str,
) -> AppResult<()> {
    info!("开始还原存档，用户: {}, 备份: {}", user_folder_name, backup_path);
    let backup_path = PathBuf::from(backup_path);
    debug!("备份文件路径: {}", backup_path.display());

    // 检查是否为有效的 7z 文件
    debug!("验证 7z 文件完整性");
    if !is_7z_file(&backup_path) {
        warn!("{} 看起来不是一个完整的 7z 文件", backup_path.display());
        return Err(AppError::Unknown(format!(
            "{} 看起来不是一个完整的 7z 文件，跳过还原",
            backup_path.display()
        )));
    }
    debug!("7z 文件验证通过");

    let backup_info = parse_backup_info(&backup_path);
    debug!("备份信息: {:?}", backup_info);

    let title_id = backup_info.title_id.ok_or_else(|| {
        warn!("无法从备份文件名解析 title_id: {}", backup_path.display());
        AppError::Unknown("无法从备份文件名解析 title_id".to_string())
    })?;

    debug!("解析到 Title ID: {}", title_id);

    let save_path = get_yuzu_save_path();
    let user_save_path = save_path.join(user_folder_name);
    let target_game_save_path = user_save_path.join(&title_id);

    debug!("目标存档路径: {}", target_game_save_path.display());

    // 删除旧的存档目录
    if target_game_save_path.exists() {
        info!("正在清空目录 {}", target_game_save_path.display());
        fs::remove_dir_all(&target_game_save_path)?;
        debug!("旧存档已删除");
    } else {
        debug!("目标目录不存在，无需删除");
    }

    info!(
        "正在解压备份至 {}",
        user_save_path.display()
    );

    // 解压备份
    debug!("开始解压 7z 文件");
    crate::utils::archive::extract_7z(&backup_path, &user_save_path)?;
    debug!("解压完成");

    info!("{} 还原完成", backup_path.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_to_uuid() {
        let user_id = "97A1DAE861CD445AB9645267B3AB99BE";
        let uuid = convert_to_uuid(user_id);
        assert_eq!(uuid, "be99abb3-6752-64b9-5a44-cd61e8daa197");
    }

    #[test]
    fn test_parse_backup_info() {
        let path = PathBuf::from("D:\\yuzu_save_backup\\yuzu_0100F2C0115B6000_1685114415.7z");
        let info = parse_backup_info(&path);

        assert_eq!(info.filename, "yuzu_0100F2C0115B6000_1685114415.7z");
        assert_eq!(info.title_id, Some("0100F2C0115B6000".to_string()));
        assert_eq!(info.bak_time, Some(1685114415000));
    }

    #[test]
    fn test_sizeof_fmt() {
        assert_eq!(sizeof_fmt(0), "0 B");
        assert_eq!(sizeof_fmt(1023), "1023 B");
        assert_eq!(sizeof_fmt(1024), "1.0 KiB");
        assert_eq!(sizeof_fmt(1536), "1.5 KiB");
        assert_eq!(sizeof_fmt(1048576), "1.0 MiB");
        assert_eq!(sizeof_fmt(1073741824), "1.0 GiB");
    }
}

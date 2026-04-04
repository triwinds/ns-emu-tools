//! 存储模型
//!
//! 用于持久化存储历史记录等数据

use crate::config::{effective_config_dir, RyujinxConfig, YuzuConfig};
use crate::error::AppResult;
use crate::utils::common::normalize_path;
use crate::utils::write_string_atomic;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// 全局存储实例
pub static STORAGE: Lazy<RwLock<Storage>> = Lazy::new(|| {
    RwLock::new(Storage::load().unwrap_or_else(|e| {
        warn!("加载存储失败，使用默认存储: {}", e);
        Storage::default()
    }))
});

static STORAGE_LAST_SAVED: Lazy<RwLock<Option<Storage>>> = Lazy::new(|| RwLock::new(None));

/// 获取存储文件路径
pub fn storage_path() -> PathBuf {
    let dir = effective_config_dir();

    if let Err(e) = std::fs::create_dir_all(&dir) {
        warn!("创建存储目录失败：{}", e);
    }

    dir.join("storage.json")
}

/// 持久化存储
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Storage {
    /// Yuzu 历史配置
    #[serde(default)]
    pub yuzu_history: HashMap<String, YuzuConfig>,
    /// Ryujinx 历史配置
    #[serde(default)]
    pub ryujinx_history: HashMap<String, RyujinxConfig>,
    /// Yuzu 存档备份路径
    #[serde(default = "default_yuzu_save_backup_path")]
    pub yuzu_save_backup_path: PathBuf,
}

fn default_yuzu_save_backup_path() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|dirs| {
            dirs.document_dir()
                .map(|path| path.join("yuzu_save_backup"))
        })
        .unwrap_or_else(|| PathBuf::from("yuzu_save_backup"))
}

impl Storage {
    /// 从文件加载存储
    pub fn load() -> AppResult<Self> {
        let path = storage_path();
        if path.exists() {
            info!("从 {} 加载存储", path.display());
            let content = std::fs::read_to_string(&path)?;
            let storage: Storage = serde_json::from_str(&content)?;
            *STORAGE_LAST_SAVED.write() = Some(storage.clone());
            debug!("存储加载成功");
            Ok(storage)
        } else {
            info!("存储文件不存在，使用默认存储");
            let storage = Self::default();
            storage.save()?;
            Ok(storage)
        }
    }

    /// 保存存储到文件
    pub fn save(&self) -> AppResult<()> {
        if STORAGE_LAST_SAVED.read().as_ref() == Some(self) {
            debug!("存储未变化，跳过写盘");
            return Ok(());
        }

        let path = storage_path();
        info!("保存存储到 {}", path.display());
        let content = serde_json::to_string_pretty(self)?;
        write_string_atomic(&path, &content)?;
        *STORAGE_LAST_SAVED.write() = Some(self.clone());
        debug!("存储保存成功");
        Ok(())
    }
}

/// 添加 Yuzu 历史记录
pub fn add_yuzu_history(config: YuzuConfig, dump: bool) -> AppResult<()> {
    let mut storage = STORAGE.write();
    let path = normalize_path(&config.yuzu_path);
    let path_str = path.to_string_lossy().to_string();
    if storage.yuzu_history.get(&path_str) == Some(&config) {
        debug!("Yuzu 历史记录未变化，跳过更新: {}", path_str);
        return Ok(());
    }
    info!("添加 Yuzu 历史记录: {}", path_str);
    storage.yuzu_history.insert(path_str, config);
    if dump {
        storage.save()?;
    }
    Ok(())
}

/// 添加 Ryujinx 历史记录
pub fn add_ryujinx_history(config: RyujinxConfig, dump: bool) -> AppResult<()> {
    let mut storage = STORAGE.write();
    let path = normalize_path(&config.path);
    let path_str = path.to_string_lossy().to_string();
    if storage.ryujinx_history.get(&path_str) == Some(&config) {
        debug!("Ryujinx 历史记录未变化，跳过更新: {}", path_str);
        return Ok(());
    }
    info!("添加 Ryujinx 历史记录: {}", path_str);
    storage.ryujinx_history.insert(path_str, config);
    if dump {
        storage.save()?;
    }
    Ok(())
}

/// 删除历史记录路径
pub fn delete_history_path(emu_type: &str, path_to_delete: &str) -> AppResult<()> {
    let mut storage = STORAGE.write();
    let abs_path = normalize_path(&PathBuf::from(path_to_delete))
        .to_string_lossy()
        .to_string();

    let removed = if emu_type == "yuzu" {
        storage.yuzu_history.remove(&abs_path).is_some()
    } else {
        storage.ryujinx_history.remove(&abs_path).is_some()
    };

    if removed {
        info!("{} 路径 {} 已删除", emu_type, abs_path);
        storage.save()?;
    }
    Ok(())
}

/// 获取当前存储的克隆
pub fn get_storage() -> Storage {
    STORAGE.read().clone()
}

/// 加载历史路径列表
pub fn load_history_path(emu_type: &str) -> AppResult<Vec<String>> {
    use crate::config::CONFIG;

    let storage = STORAGE.read();
    let config = CONFIG.read();

    let mut paths: std::collections::HashSet<String> = if emu_type == "yuzu" {
        storage.yuzu_history.keys().cloned().collect()
    } else {
        storage.ryujinx_history.keys().cloned().collect()
    };

    // 添加当前配置的路径
    let current_path = if emu_type == "yuzu" {
        config.yuzu.yuzu_path.to_string_lossy().to_string()
    } else {
        config.ryujinx.path.to_string_lossy().to_string()
    };

    if !current_path.is_empty() {
        paths.insert(current_path);
    }

    let mut result: Vec<String> = paths.into_iter().collect();
    result.sort();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_storage() {
        let storage = Storage::default();
        assert!(storage.yuzu_history.is_empty());
        assert!(storage.ryujinx_history.is_empty());
    }

    #[test]
    fn test_storage_serialization() {
        let storage = Storage::default();
        let json = serde_json::to_string_pretty(&storage).unwrap();
        let parsed: Storage = serde_json::from_str(&json).unwrap();
        assert_eq!(storage.yuzu_save_backup_path, parsed.yuzu_save_backup_path);
    }

    #[test]
    fn test_storage_path_uses_config_directory() {
        let storage_path = storage_path();
        let config_path = crate::config::config_path();

        assert_eq!(storage_path.file_name().unwrap(), "storage.json");
        assert_eq!(storage_path.parent(), config_path.parent());
    }
}

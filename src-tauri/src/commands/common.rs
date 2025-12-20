//! 通用命令模块
//!
//! 提供通用的 Tauri 命令，如配置管理、系统操作等

use crate::config::{self, CommonSetting, Config, CURRENT_VERSION};
use crate::error::AppError;
use crate::models::storage::{self, Storage};
use crate::repositories::app_info::{self, UpdateCheckResult};
use crate::repositories::config_data;
use tauri::command;
use tracing::info;

/// 获取当前配置
#[command]
pub fn get_config() -> Result<Config, String> {
    Ok(config::get_config())
}

/// 保存配置
#[command]
pub fn save_config(config: Config) -> Result<(), String> {
    let mut current = crate::config::CONFIG.write();
    *current = config;
    current.save().map_err(|e| e.to_string())
}

/// 获取存储数据
#[command]
pub fn get_storage() -> Result<Storage, String> {
    Ok(storage::get_storage())
}

/// 获取应用版本
#[command]
pub fn get_app_version() -> String {
    CURRENT_VERSION.to_string()
}

/// 打开文件夹
#[command]
pub async fn open_folder(path: String) -> Result<(), String> {
    info!("打开文件夹: {}", path);
    let path = std::path::Path::new(&path);

    if !path.exists() {
        return Err(AppError::DirectoryNotFound(path.to_string_lossy().to_string()).to_string());
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// 打开 URL
#[command]
pub async fn open_url(url: String) -> Result<(), String> {
    info!("打开 URL: {}", url);

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", &url])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// 更新设置
#[command]
pub fn update_setting(setting: CommonSetting) -> Result<(), String> {
    config::update_setting(setting).map_err(|e| e.to_string())
}

/// 更新上次打开的模拟器页面
#[command]
pub fn update_last_open_emu_page(page: String) -> Result<(), String> {
    config::update_last_open_emu_page(&page).map_err(|e| e.to_string())
}

/// 更新深色模式状态
#[command]
pub fn update_dark_state(dark: bool) -> Result<(), String> {
    config::update_dark_state(dark).map_err(|e| e.to_string())
}

/// 删除历史路径
#[command]
pub fn delete_history_path(emu_type: String, path: String) -> Result<(), String> {
    storage::delete_history_path(&emu_type, &path).map_err(|e| e.to_string())
}

/// 检查应用更新
#[command]
pub async fn check_update(include_prerelease: bool) -> Result<UpdateCheckResult, String> {
    app_info::check_update(include_prerelease)
        .await
        .map_err(|e| e.to_string())
}

/// 加载变更日志
#[command]
pub async fn load_change_log() -> Result<String, String> {
    app_info::load_change_log()
        .await
        .map_err(|e| e.to_string())
}

/// 获取可用的固件下载源
#[command]
pub fn get_available_firmware_sources() -> Vec<(String, String)> {
    config_data::get_available_firmware_sources()
}

/// 获取 GitHub 镜像列表
#[command]
pub fn get_github_mirrors() -> Vec<(String, String, String)> {
    config_data::get_github_mirrors()
}

/// 获取游戏数据映射
#[command]
pub async fn get_game_data() -> Result<std::collections::HashMap<String, serde_json::Value>, String> {
    config_data::get_game_data()
        .await
        .map_err(|e| e.to_string())
}

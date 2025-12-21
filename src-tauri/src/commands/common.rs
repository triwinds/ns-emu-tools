//! 通用命令模块
//!
//! 提供通用的 Tauri 命令，如配置管理、系统操作等

use crate::config::{self, CommonSetting, Config, CURRENT_VERSION};
use crate::error::AppError;
use crate::models::storage::{self, Storage};
use crate::repositories::app_info::{self, UpdateCheckResult};
use crate::repositories::config_data;
use tauri::{command, Emitter, Window};
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

/// 更新窗口大小
#[command]
pub fn update_window_size(width: u32, height: u32) -> Result<(), String> {
    config::update_window_size(width, height).map_err(|e| e.to_string())
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

/// 获取可用固件信息列表
#[command]
pub async fn get_available_firmware_infos() -> Result<Vec<crate::services::firmware::FirmwareInfo>, String> {
    crate::services::firmware::get_firmware_infos()
        .await
        .map_err(|e| e.to_string())
}

/// 加载历史路径列表
#[command]
pub fn load_history_path(emu_type: String) -> Result<Vec<String>, String> {
    storage::load_history_path(&emu_type).map_err(|e| e.to_string())
}

/// 检测固件版本
#[command]
pub async fn detect_firmware_version(emu_type: String, window: Window) -> Result<String, String> {
    use crate::models::{ProgressEvent, ProgressStep, ProgressStatus};

    info!("检测 {} 固件版本", emu_type);

    // Define steps
    let steps = vec![
        ProgressStep {
            id: "load_keys".to_string(),
            title: "加载密钥文件".to_string(),
            status: ProgressStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: String::new(),
            eta: String::new(),
            error: None,
        },
        ProgressStep {
            id: "find_nca".to_string(),
            title: "查找固件文件".to_string(),
            status: ProgressStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: String::new(),
            eta: String::new(),
            error: None,
        },
        ProgressStep {
            id: "extract_version".to_string(),
            title: "提取版本信息".to_string(),
            status: ProgressStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: String::new(),
            eta: String::new(),
            error: None,
        },
    ];

    // Send started event
    let _ = window.emit("installation-event", ProgressEvent::Started { steps: steps.clone() });

    // Step 1: Load keys
    let _ = window.emit("installation-event", ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "load_keys".to_string(),
            title: "加载密钥文件".to_string(),
            status: ProgressStatus::Running,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: String::new(),
            eta: String::new(),
            error: None,
        },
    });

    // 尝试自动加载密钥（如果尚未加载）
    if !crate::services::keys::is_keys_loaded() {
        if let Err(e) = auto_load_keys(&emu_type) {
            info!("自动加载密钥失败: {}，将尝试不解密读取", e);
            let _ = window.emit("installation-event", ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "load_keys".to_string(),
                    title: "加载密钥文件".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: String::new(),
                    eta: String::new(),
                    error: Some(format!("加载密钥失败: {}", e)),
                },
            });
            let _ = window.emit("installation-event", ProgressEvent::Finished {
                success: false,
                message: Some(format!("加载密钥失败: {}", e))
            });
            return Err(e);
        }
    }

    let _ = window.emit("installation-event", ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "load_keys".to_string(),
            title: "加载密钥文件".to_string(),
            status: ProgressStatus::Success,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: String::new(),
            eta: String::new(),
            error: None,
        },
    });

    // Step 2 & 3: Find NCA and extract version
    let _ = window.emit("installation-event", ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "find_nca".to_string(),
            title: "查找固件文件".to_string(),
            status: ProgressStatus::Running,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: String::new(),
            eta: String::new(),
            error: None,
        },
    });

    let result = match emu_type.as_str() {
        "yuzu" => {
            crate::services::firmware::detect_yuzu_firmware_version()
                .await
                .map_err(|e| e.to_string())
        }
        "ryujinx" => {
            crate::services::firmware::detect_ryujinx_firmware_version()
                .await
                .map_err(|e| e.to_string())
        }
        _ => Err(format!("不支持的模拟器类型: {}", emu_type)),
    };

    match result {
        Ok(version) => {
            let _ = window.emit("installation-event", ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "find_nca".to_string(),
                    title: "查找固件文件".to_string(),
                    status: ProgressStatus::Success,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: String::new(),
                    eta: String::new(),
                    error: None,
                },
            });

            let _ = window.emit("installation-event", ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "extract_version".to_string(),
                    title: "提取版本信息".to_string(),
                    status: ProgressStatus::Success,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: String::new(),
                    eta: String::new(),
                    error: None,
                },
            });

            let _ = window.emit("installation-event", ProgressEvent::Finished {
                success: true,
                message: Some(format!("检测到固件版本: {}", version))
            });

            Ok(version)
        }
        Err(e) => {
            let _ = window.emit("installation-event", ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "find_nca".to_string(),
                    title: "查找固件文件".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: String::new(),
                    eta: String::new(),
                    error: Some(e.clone()),
                },
            });

            let _ = window.emit("installation-event", ProgressEvent::Finished {
                success: false,
                message: Some(e.clone())
            });

            Err(e)
        }
    }
}

/// 加载密钥文件
#[command]
pub fn load_keys(path: String) -> Result<(), String> {
    info!("加载密钥文件: {}", path);
    crate::services::keys::load_keys(&path).map_err(|e| e.to_string())
}

/// 检查密钥是否已加载
#[command]
pub fn is_keys_loaded() -> bool {
    crate::services::keys::is_keys_loaded()
}

/// 自动查找并加载密钥文件
fn auto_load_keys(emu_type: &str) -> Result<(), String> {
    let possible_paths = get_possible_key_paths(emu_type);

    for path in possible_paths {
        if path.exists() {
            info!("找到密钥文件: {}", path.display());
            match crate::services::keys::load_keys(&path) {
                Ok(_) => {
                    info!("成功加载密钥文件");
                    return Ok(());
                }
                Err(e) => {
                    info!("加载密钥文件失败: {}", e);
                    continue;
                }
            }
        }
    }

    Err("未找到有效的密钥文件".to_string())
}

/// 获取可能的密钥文件路径
fn get_possible_key_paths(emu_type: &str) -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();

    // Yuzu/Eden/Citron 密钥路径
    if emu_type == "yuzu" {
        // Windows: %APPDATA%/yuzu/keys/prod.keys
        if let Some(data_local) = dirs::data_local_dir() {
            paths.push(data_local.join("yuzu").join("keys").join("prod.keys"));
        }
        // Linux: ~/.local/share/yuzu/keys/prod.keys
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".local/share/yuzu/keys/prod.keys"));
        }
        // Portable mode
        let config = crate::config::CONFIG.read();
        let yuzu_path = &config.yuzu.yuzu_path;
        paths.push(yuzu_path.join("user").join("keys").join("prod.keys"));
    }

    // Ryujinx 密钥路径
    if emu_type == "ryujinx" {
        // Windows: %APPDATA%/Ryujinx/system/prod.keys
        if let Some(data_dir) = dirs::data_dir() {
            paths.push(data_dir.join("Ryujinx").join("system").join("prod.keys"));
        }
        // Linux: ~/.config/Ryujinx/system/prod.keys
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".config/Ryujinx/system/prod.keys"));
        }
        // Portable mode
        let config = crate::config::CONFIG.read();
        let ryujinx_path = &config.ryujinx.path;
        paths.push(ryujinx_path.join("portable").join("system").join("prod.keys"));
    }

    // 通用路径（两种模拟器都会尝试）
    if let Some(home) = dirs::home_dir() {
        // ~/.switch/prod.keys (常见的密钥存放位置)
        paths.push(home.join(".switch").join("prod.keys"));
    }

    paths
}

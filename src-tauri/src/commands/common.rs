//! 通用命令模块
//!
//! 提供通用的 Tauri 命令，如配置管理、系统操作等

use crate::config::{self, CommonSetting, Config, CURRENT_VERSION};
use crate::error::AppError;
use crate::models::storage::{self, Storage};
use crate::repositories::app_info::{self, UpdateCheckResult};
use crate::repositories::config_data;
use tauri::{command, Emitter, Window};
use tracing::{error, info};

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

    // Step 2 & 3: Find NCA and extract version (keys are loaded in detect functions)
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
            crate::services::firmware::detect_yuzu_firmware_version(Some(&window))
                .await
                .map_err(|e| e.to_string())
        }
        "ryujinx" => {
            crate::services::firmware::detect_ryujinx_firmware_version(Some(&window))
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

/// 下载应用更新
#[command]
pub async fn download_app_update(
    window: Window,
    include_prerelease: bool,
    download_url: Option<String>,
) -> Result<String, String> {
    info!(
        "开始下载应用更新 (包含预发布: {}, download_url: {:?})",
        include_prerelease, download_url
    );

    match crate::services::updater::download_update(&window, include_prerelease, download_url)
        .await
    {
        Ok(path) => Ok(path.to_string_lossy().to_string()),
        Err(e) => Err(e.to_string()),
    }
}

/// 安装应用更新并重启
#[command]
pub async fn install_app_update(update_file: String) -> Result<(), String> {
    info!("开始安装应用更新: {}", update_file);

    let update_path = std::path::Path::new(&update_file);
    match crate::services::updater::install_update(update_path).await {
        Ok(_) => {
            info!("更新脚本已启动，程序即将退出");
            // 等待一小段时间确保脚本启动
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            // 退出程序
            std::process::exit(0);
        }
        Err(e) => Err(e.to_string()),
    }
}

/// 根据 tag 更新自身（一体化更新流程）
#[command]
pub async fn update_self_by_tag(tag: String, window: Window) -> Result<(), String> {
    info!("开始根据 tag 更新自身: {}", tag);

    // 1. 下载并解压更新文件
    let update_path = match crate::services::updater::update_self_by_tag(&window, &tag).await {
        Ok(path) => path,
        Err(e) => {
            let err_msg = format!("下载更新失败: {}", e);
            error!("{}", err_msg);
            return Err(err_msg);
        }
    };

    info!("更新文件已准备完成: {}", update_path.display());

    // 2. 安装更新（生成并执行更新脚本）
    match crate::services::updater::install_update(&update_path).await {
        Ok(_) => {
            info!("更新脚本已启动，程序即将退出");
            // 等待一小段时间确保脚本启动
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            // 退出程序
            std::process::exit(0);
        }
        Err(e) => {
            let err_msg = format!("安装更新失败: {}", e);
            error!("{}", err_msg);
            Err(err_msg)
        }
    }
}


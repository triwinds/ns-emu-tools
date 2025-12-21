//! Ryujinx 相关 Tauri 命令
//!
//! 暴露给前端的 Ryujinx 管理命令

use crate::models::response::ApiResponse;
use crate::services::ryujinx::*;
use crate::services::notifier::send_notify;
use tauri::{Emitter, Window};
use tracing::{error, info};

/// 获取所有 Ryujinx 版本列表
#[tauri::command]
pub async fn get_all_ryujinx_versions_command(
    branch: String,
) -> Result<ApiResponse<Vec<String>>, String> {
    info!("获取 Ryujinx {} 所有版本", branch);

    match get_all_ryujinx_versions(&branch).await {
        Ok(versions) => Ok(ApiResponse::success(versions)),
        Err(e) => {
            error!("获取版本列表失败: {}", e);
            Err(e.to_string())
        }
    }
}

/// 安装 Ryujinx
#[tauri::command]
pub async fn install_ryujinx_by_version_command(
    target_version: String,
    branch: String,
    window: Window,
) -> Result<ApiResponse<()>, String> {
    use crate::models::{ProgressEvent, ProgressStep, ProgressStatus};

    info!("安装 Ryujinx {} 版本: {}", branch, target_version);

    // Initial steps
    let steps = vec![
        ProgressStep {
            id: "fetch_version".to_string(),
            title: "获取版本信息".to_string(),
            status: ProgressStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        },
        ProgressStep {
            id: "download".to_string(),
            title: format!("下载 Ryujinx {}", branch),
            status: ProgressStatus::Pending,
            step_type: "download".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        },
        ProgressStep {
            id: "extract".to_string(),
            title: "解压文件".to_string(),
            status: ProgressStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        },
        ProgressStep {
            id: "install".to_string(),
            title: "安装文件".to_string(),
            status: ProgressStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        },
        ProgressStep {
            id: "check_env".to_string(),
            title: "检查运行环境".to_string(),
            status: ProgressStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        },
    ];

    // Emit initial event to open dialog and show steps
    let _ = window.emit("installation-event", ProgressEvent::Started { steps });

    // 安装
    let window_clone = window.clone();
    let result = install_ryujinx_by_version(&target_version, &branch, move |event| {
        // 发送事件到前端
        let _ = window_clone.emit("installation-event", event);
    })
    .await;

    match result {
        Ok(_) => {
            let _ = window.emit("installation-event", ProgressEvent::Finished { success: true, message: None });
            let _ = send_notify(&window, &format!("Ryujinx {} 安装成功", branch));
            Ok(ApiResponse::success(()))
        }
        Err(e) => {
            error!("安装失败: {}", e);
            let _ = window.emit("installation-event", ProgressEvent::Finished { success: false, message: Some(e.to_string()) });
            let _ = send_notify(&window, &format!("安装失败: {}", e));
            Err(e.to_string())
        }
    }
}

/// 启动 Ryujinx
#[tauri::command]
pub fn start_ryujinx_command() -> Result<ApiResponse<()>, String> {
    info!("启动 Ryujinx");

    match start_ryujinx() {
        Ok(_) => Ok(ApiResponse::success(())),
        Err(e) => {
            error!("启动失败: {}", e);
            Err(e.to_string())
        }
    }
}

/// 打开 Ryujinx keys 文件夹
#[tauri::command]
pub fn open_ryujinx_keys_folder_command() -> Result<ApiResponse<()>, String> {
    info!("打开 Ryujinx keys 文件夹");

    match open_ryujinx_keys_folder() {
        Ok(_) => Ok(ApiResponse::success(())),
        Err(e) => {
            error!("打开文件夹失败: {}", e);
            Err(e.to_string())
        }
    }
}

/// 获取 Ryujinx 用户文件夹路径
#[tauri::command]
pub fn get_ryujinx_user_folder_command() -> Result<ApiResponse<String>, String> {
    let path = get_ryujinx_user_folder();
    Ok(ApiResponse::success(path.to_string_lossy().to_string()))
}

/// 更新 Ryujinx 安装路径
#[tauri::command]
pub fn update_ryujinx_path_command(new_path: String) -> Result<ApiResponse<()>, String> {
    info!("更新 Ryujinx 路径: {}", new_path);

    match update_ryujinx_path(&new_path) {
        Ok(_) => Ok(ApiResponse::success(())),
        Err(e) => {
            error!("更新路径失败: {}", e);
            Err(e.to_string())
        }
    }
}

/// 获取 Ryujinx 变更日志
#[tauri::command]
pub async fn get_ryujinx_change_logs_command(
    branch: String,
) -> Result<ApiResponse<String>, String> {
    info!("获取 Ryujinx {} 变更日志", branch);

    match get_ryujinx_change_logs(&branch).await {
        Ok(changelog) => Ok(ApiResponse::success(changelog)),
        Err(e) => {
            error!("获取变更日志失败: {}", e);
            Err(e.to_string())
        }
    }
}

/// 安装固件到 Ryujinx
#[tauri::command]
pub async fn install_firmware_to_ryujinx_command(
    firmware_version: Option<String>,
    window: Window,
) -> Result<ApiResponse<()>, String> {
    info!("安装固件到 Ryujinx");

    let _ = send_notify(&window, "开始安装固件...");

    let window_clone = window.clone();
    match install_firmware_to_ryujinx(firmware_version.as_deref(), move |event| {
        // 发送安装事件到前端
        let _ = window_clone.emit("installation-event", event);
    }).await {
        Ok(_) => {
            let _ = send_notify(&window, "固件安装成功");
            Ok(ApiResponse::success(()))
        }
        Err(e) => {
            error!("安装固件失败: {}", e);
            let _ = send_notify(&window, &format!("安装固件失败: {}", e));
            Err(e.to_string())
        }
    }
}

/// 检测当前 Ryujinx 分支
#[tauri::command]
pub fn detect_ryujinx_branch_command() -> Result<ApiResponse<String>, String> {
    let branch = detect_current_branch();
    Ok(ApiResponse::success(branch))
}

/// 选择并更新 Ryujinx 路径
#[tauri::command]
pub async fn ask_and_update_ryujinx_path_command(
    window: Window,
) -> Result<ApiResponse<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    info!("打开文件夹选择对话框");

    // 使用 Tauri 对话框选择文件夹
    let folder = window
        .dialog()
        .file()
        .blocking_pick_folder();

    match folder {
        Some(path) => {
            let path_str = path.to_string();
            info!("用户选择的文件夹: {}", path_str);

            match update_ryujinx_path(&path_str) {
                Ok(_) => Ok(ApiResponse::success(path_str)),
                Err(e) => {
                    error!("更新路径失败: {}", e);
                    Err(e.to_string())
                }
            }
        }
        None => {
            info!("用户取消了文件夹选择");
            Err("用户取消了操作".to_string())
        }
    }
}

/// 检测 Ryujinx 版本
#[tauri::command]
pub fn detect_ryujinx_version_command() -> Result<ApiResponse<Option<String>>, String> {
    info!("检测 Ryujinx 版本");
    // TODO: 实现完整的版本检测逻辑
    // 目前仅返回分支信息作为简化实现
    let branch = detect_current_branch();
    Ok(ApiResponse::success(Some(branch)))
}

/// 取消 Ryujinx 下载
#[tauri::command]
pub async fn cancel_ryujinx_download_command() -> Result<ApiResponse<()>, String> {
    info!("取消 Ryujinx 下载");

    match cancel_ryujinx_download().await {
        Ok(_) => Ok(ApiResponse::success(())),
        Err(e) => {
            error!("取消下载失败: {}", e);
            Err(e.to_string())
        }
    }
}

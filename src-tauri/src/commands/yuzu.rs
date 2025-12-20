//! Yuzu 相关 Tauri 命令
//!
//! 暴露给前端的 Yuzu/Eden/Citron 管理命令

use crate::models::response::ApiResponse;
use crate::repositories::yuzu::get_yuzu_all_release_info;
use crate::services::yuzu::*;
use crate::services::notifier::send_notify;
use tauri::{Emitter, Window};
use tracing::{error, info};

/// 获取所有 Yuzu/Eden/Citron 版本列表
#[tauri::command]
pub async fn get_all_yuzu_versions(branch: String) -> Result<ApiResponse<Vec<String>>, String> {
    info!("获取 {} 所有版本", get_emu_name(&branch));

    match get_yuzu_all_release_info(&branch).await {
        Ok(releases) => {
            let versions: Vec<String> = releases.iter().map(|r| r.tag_name.clone()).collect();
            Ok(ApiResponse::success(versions))
        }
        Err(e) => {
            error!("获取版本列表失败: {}", e);
            Err(e.to_string())
        }
    }
}

/// 安装 Yuzu/Eden/Citron
#[tauri::command]
pub async fn install_yuzu_by_version(
    target_version: String,
    branch: String,
    window: Window,
) -> Result<ApiResponse<()>, String> {
    info!("安装 {} 版本: {}", get_emu_name(&branch), target_version);

    // 发送通知
    let _ = send_notify(&window, &format!("开始安装 {} {}...", get_emu_name(&branch), target_version));

    // 安装
    let window_clone = window.clone();
    let result = install_yuzu(&target_version, &branch, move |progress| {
        // 发送进度事件到前端
        let _ = window_clone.emit("download-progress", &progress);

        // 发送文本通知
        if progress.percentage > 0.0 {
            let msg = format!(
                "下载进度: {:.1}% - {} / {} @ {}",
                progress.percentage,
                progress.downloaded_string(),
                progress.total_string(),
                progress.speed_string()
            );
            let _ = send_notify(&window_clone, &msg);
        }
    })
    .await;

    match result {
        Ok(_) => {
            let _ = send_notify(&window, &format!("{} 安装成功", get_emu_name(&branch)));
            Ok(ApiResponse::success(()))
        }
        Err(e) => {
            error!("安装失败: {}", e);
            let _ = send_notify(&window, &format!("安装失败: {}", e));
            Err(e.to_string())
        }
    }
}

/// 检测 Yuzu 版本
#[tauri::command]
pub async fn detect_yuzu_version_command(
    window: Window,
) -> Result<ApiResponse<Option<String>>, String> {
    info!("检测 Yuzu 版本");

    let _ = send_notify(&window, "正在检测模拟器版本...");

    match detect_yuzu_version().await {
        Ok(version) => {
            if let Some(ref v) = version {
                let _ = send_notify(&window, &format!("当前版本: {}", v));
            } else {
                let _ = send_notify(&window, "未能检测到版本");
            }
            Ok(ApiResponse::success(version))
        }
        Err(e) => {
            error!("版本检测失败: {}", e);
            let _ = send_notify(&window, &format!("检测失败: {}", e));
            Err(e.to_string())
        }
    }
}

/// 启动 Yuzu
#[tauri::command]
pub fn start_yuzu_command() -> Result<ApiResponse<()>, String> {
    info!("启动 Yuzu");

    match start_yuzu() {
        Ok(_) => Ok(ApiResponse::success(())),
        Err(e) => {
            error!("启动失败: {}", e);
            Err(e.to_string())
        }
    }
}

/// 获取 Yuzu 可执行文件路径
#[tauri::command]
pub fn get_yuzu_exe_path_command() -> Result<ApiResponse<String>, String> {
    let path = get_yuzu_exe_path();
    Ok(ApiResponse::success(path.to_string_lossy().to_string()))
}

/// 打开 Yuzu keys 文件夹
#[tauri::command]
pub fn open_yuzu_keys_folder_command() -> Result<ApiResponse<()>, String> {
    info!("打开 Yuzu keys 文件夹");

    match open_yuzu_keys_folder() {
        Ok(_) => Ok(ApiResponse::success(())),
        Err(e) => {
            error!("打开文件夹失败: {}", e);
            Err(e.to_string())
        }
    }
}

/// 获取 Yuzu 用户数据路径
#[tauri::command]
pub fn get_yuzu_user_path_command() -> Result<ApiResponse<String>, String> {
    let path = get_yuzu_user_path();
    Ok(ApiResponse::success(path.to_string_lossy().to_string()))
}

/// 获取 Yuzu NAND 路径
#[tauri::command]
pub fn get_yuzu_nand_path_command() -> Result<ApiResponse<String>, String> {
    let path = get_yuzu_nand_path();
    Ok(ApiResponse::success(path.to_string_lossy().to_string()))
}

/// 获取 Yuzu load 路径（用于 mods/cheats）
#[tauri::command]
pub fn get_yuzu_load_path_command() -> Result<ApiResponse<String>, String> {
    let path = get_yuzu_load_path();
    Ok(ApiResponse::success(path.to_string_lossy().to_string()))
}

/// 更新 Yuzu 安装路径
#[tauri::command]
pub fn update_yuzu_path_command(new_path: String) -> Result<ApiResponse<()>, String> {
    info!("更新 Yuzu 路径: {}", new_path);

    match update_yuzu_path(&new_path) {
        Ok(_) => Ok(ApiResponse::success(())),
        Err(e) => {
            error!("更新路径失败: {}", e);
            Err(e.to_string())
        }
    }
}

/// 获取 Yuzu 变更日志
#[tauri::command]
pub async fn get_yuzu_change_logs_command() -> Result<ApiResponse<String>, String> {
    info!("获取 Yuzu 变更日志");

    match get_yuzu_change_logs().await {
        Ok(changelog) => Ok(ApiResponse::success(changelog)),
        Err(e) => {
            error!("获取变更日志失败: {}", e);
            Err(e.to_string())
        }
    }
}

/// 安装固件到 Yuzu
#[tauri::command]
pub async fn install_firmware_to_yuzu_command(
    firmware_version: Option<String>,
    window: Window,
) -> Result<ApiResponse<()>, String> {
    info!("安装固件到 Yuzu");

    let _ = send_notify(&window, "开始安装固件...");

    match install_firmware_to_yuzu(firmware_version.as_deref()).await {
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

/// 切换 Yuzu 分支
#[tauri::command]
pub fn switch_yuzu_branch(branch: String) -> Result<ApiResponse<()>, String> {
    use crate::config::CONFIG;
    info!("切换 Yuzu 分支到: {}", branch);

    let mut cfg = CONFIG.write();
    cfg.yuzu.branch = branch;
    cfg.save().map_err(|e| e.to_string())?;

    Ok(ApiResponse::success(()))
}

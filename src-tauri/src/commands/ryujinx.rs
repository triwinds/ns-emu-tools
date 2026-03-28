//! Ryujinx 相关 Tauri 命令
//!
//! 暴露给前端的 Ryujinx 管理命令

use crate::models::response::ApiResponse;
use crate::services::installer::{
    error_step, install_steps, pending_step, running_step, success_step, InstallReporter, StepKind,
};
use crate::services::notifier::send_notify;
use crate::services::ryujinx::*;
use tauri::Window;
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
    info!("安装 Ryujinx {} 版本: {}", branch, target_version);
    let reporter = InstallReporter::from_window(window.clone());
    reporter.start(install_steps(format!("下载 Ryujinx {}", branch)));

    // Windows: 检查并安装 aria2（如果需要）
    #[cfg(target_os = "windows")]
    {
        use crate::services::downloader::check_and_install_aria2_with_ui;

        if let Err(e) = check_and_install_aria2_with_ui(window.clone()).await {
            let error_message = e.to_string();
            if crate::services::installer::is_cancelled_error_message(&error_message) {
                info!("aria2 安装已取消");
                return Err(error_message);
            }
            error!("aria2 安装失败: {}", error_message);
            return Err(format!("aria2 安装失败: {}", error_message));
        }
    }

    // 安装
    let reporter_clone = reporter.clone();
    let result = install_ryujinx_by_version(&target_version, &branch, move |event| {
        reporter_clone.emit(event);
    })
    .await;

    match result {
        Ok(_) => {
            reporter.finish_success();
            let _ = send_notify(&window, &format!("Ryujinx {} 安装成功", branch));
            Ok(ApiResponse::success(()))
        }
        Err(e) => {
            let error_message = e.to_string();
            if crate::services::installer::is_cancelled_error_message(&error_message) {
                info!("安装已取消: {}", error_message);
                reporter.finish(false, Some("安装已取消".to_string()));
                return Err(error_message);
            }

            error!("安装失败: {}", error_message);
            reporter.finish_error(error_message.clone());
            let _ = send_notify(&window, &format!("安装失败: {}", error_message));
            Err(error_message)
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

/// 打开 Ryujinx 的密钥目录
#[tauri::command]
pub fn open_ryujinx_keys_folder_command() -> Result<ApiResponse<()>, String> {
    info!("打开 Ryujinx 的密钥目录");

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
    let reporter = InstallReporter::from_window(window.clone());
    info!("安装固件到 Ryujinx");

    let _ = send_notify(&window, "开始安装固件...");

    #[cfg(target_os = "windows")]
    {
        use crate::services::downloader::{check_and_install_aria2_with_ui, should_use_aria2};

        if should_use_aria2() {
            reporter.start(crate::services::downloader::create_installation_steps());

            if let Err(e) = check_and_install_aria2_with_ui(window.clone()).await {
                let error_message = e.to_string();
                if crate::services::installer::is_cancelled_error_message(&error_message) {
                    info!("aria2 安装已取消");
                    return Err(error_message);
                }
                error!("aria2 安装失败: {}", error_message);
                return Err(format!("aria2 安装失败: {}", error_message));
            }

            reporter.finish_success();
        }
    }

    let reporter_clone = reporter.clone();
    match install_firmware_to_ryujinx(firmware_version.as_deref(), move |event| {
        reporter_clone.emit(event);
    })
    .await
    {
        Ok(_) => {
            reporter.finish_success();
            let _ = send_notify(&window, "固件安装成功");
            Ok(ApiResponse::success(()))
        }
        Err(e) => {
            let error_message = e.to_string();
            if crate::services::installer::is_cancelled_error_message(&error_message) {
                info!("固件安装已取消: {}", error_message);
                reporter.finish(false, Some("固件安装已取消".to_string()));
                return Err(error_message);
            }

            error!("安装固件失败: {}", error_message);
            reporter.finish_error(error_message.clone());
            let _ = send_notify(&window, &format!("安装固件失败: {}", error_message));
            Err(error_message)
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
    let folder = window.dialog().file().blocking_pick_folder();

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
pub async fn detect_ryujinx_version_command(
    window: Window,
) -> Result<ApiResponse<Option<String>>, String> {
    use crate::services::ryujinx::detect_ryujinx_version;

    info!("检测 Ryujinx 版本");
    let reporter = InstallReporter::from_window(window);
    reporter.start(vec![
        pending_step("check_exe", "检查模拟器程序"),
        pending_step("start_program", "启动程序检测版本"),
        pending_step("detect_version", "读取版本信息"),
    ]);
    reporter.step(running_step("check_exe", "检查模拟器程序"));

    let config = crate::config::get_config();
    let ryujinx_path = std::path::PathBuf::from(&config.ryujinx.path);

    let exe_path = {
        #[cfg(target_os = "macos")]
        {
            let app_path = ryujinx_path.join("Ryujinx.app");
            if app_path.exists() {
                Some(app_path.join("Contents/MacOS/Ryujinx"))
            } else {
                None
            }
        }

        #[cfg(target_os = "windows")]
        {
            let ava_exe = ryujinx_path.join("Ryujinx.Ava.exe");
            let normal_exe = ryujinx_path.join("Ryujinx.exe");
            if ava_exe.exists() {
                Some(ava_exe)
            } else if normal_exe.exists() {
                Some(normal_exe)
            } else {
                None
            }
        }

        #[cfg(target_os = "linux")]
        {
            let ava_exe = ryujinx_path.join("Ryujinx.Ava");
            let normal_exe = ryujinx_path.join("Ryujinx");
            if ava_exe.exists() {
                Some(ava_exe)
            } else if normal_exe.exists() {
                Some(normal_exe)
            } else {
                None
            }
        }
    };

    if exe_path.is_none() || !exe_path.as_ref().is_some_and(|path| path.exists()) {
        let error_msg = if let Some(path) = exe_path {
            format!("未找到模拟器程序: {}", path.display())
        } else {
            format!("未找到模拟器程序在: {}", ryujinx_path.display())
        };
        reporter.step(error_step(
            "check_exe",
            "检查模拟器程序",
            StepKind::Normal,
            error_msg.clone(),
        ));
        reporter.finish_error(error_msg.clone());
        return Err(error_msg);
    }

    reporter.step(success_step("check_exe", "检查模拟器程序"));
    reporter.step(running_step("start_program", "启动程序检测版本"));

    match detect_ryujinx_version().await {
        Ok((version, branch)) => {
            reporter.step(success_step("start_program", "启动程序检测版本"));
            reporter.step(running_step("detect_version", "读取版本信息"));

            if let Some(ref v) = version {
                reporter.step(success_step(
                    "detect_version",
                    format!("检测到版本: {} ({})", v, branch),
                ));
                reporter.finish_success();
            } else {
                let message = "未能检测到版本".to_string();
                reporter.step(error_step(
                    "detect_version",
                    "未能检测到版本",
                    StepKind::Normal,
                    message.clone(),
                ));
                reporter.finish_error(message);
            }

            Ok(ApiResponse::success(version))
        }
        Err(e) => {
            error!("版本检测失败: {}", e);
            reporter.step(error_step(
                "start_program",
                "启动程序检测版本",
                StepKind::Normal,
                e.to_string(),
            ));
            reporter.finish_error(e.to_string());
            Err(e.to_string())
        }
    }
}

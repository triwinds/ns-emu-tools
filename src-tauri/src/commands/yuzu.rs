//! Yuzu 相关 Tauri 命令
//!
//! 暴露给前端的 Yuzu/Eden 管理命令

use crate::models::response::ApiResponse;
use crate::repositories::yuzu::get_yuzu_all_release_info;
use crate::services::installer::{
    error_step, install_steps, pending_step, running_step, success_step, InstallReporter, StepKind,
};
use crate::services::notifier::send_notify;
use crate::services::yuzu::*;
use tauri::Window;
use tracing::{error, info};

/// 获取所有 Yuzu/Eden 版本列表
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

/// 安装 Eden
#[tauri::command]
pub async fn install_yuzu_by_version(
    target_version: String,
    branch: String,
    window: Window,
) -> Result<ApiResponse<()>, String> {
    info!("安装 {} 版本: {}", get_emu_name(&branch), target_version);
    let reporter = InstallReporter::from_window(window.clone());
    reporter.start(install_steps(format!("下载 {}", get_emu_name(&branch))));

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
    let result = install_yuzu(&target_version, &branch, move |event| {
        reporter_clone.emit(event)
    })
    .await;

    match result {
        Ok(_) => {
            reporter.finish_success();
            let _ = send_notify(&window, &format!("{} 安装成功", get_emu_name(&branch)));
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

/// 检测 Yuzu 版本
#[tauri::command]
pub async fn detect_yuzu_version_command(
    window: Window,
) -> Result<ApiResponse<Option<String>>, String> {
    info!("检测 Yuzu 版本");
    let reporter = InstallReporter::from_window(window);
    reporter.start(vec![
        pending_step("check_exe", "检查模拟器程序"),
        pending_step("start_program", "启动程序检测版本"),
        pending_step("detect_version", "读取版本信息"),
    ]);
    reporter.step(running_step("check_exe", "检查模拟器程序"));

    let exe_path = get_yuzu_exe_path();
    if !exe_path.exists() {
        let error_msg = format!("未找到模拟器程序: {}", exe_path.display());
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

    match detect_yuzu_version().await {
        Ok(version) => {
            reporter.step(success_step("start_program", "启动程序检测版本"));
            reporter.step(running_step("detect_version", "读取版本信息"));

            if let Some(ref v) = version {
                reporter.step(success_step("detect_version", format!("检测到版本: {}", v)));
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

/// 打开 Yuzu 的密钥目录
#[tauri::command]
pub fn open_yuzu_keys_folder_command() -> Result<ApiResponse<()>, String> {
    info!("打开 Yuzu 的密钥目录");

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
    let reporter = InstallReporter::from_window(window.clone());

    // Windows: 检查并安装 aria2（如果需要）
    #[cfg(target_os = "windows")]
    {
        use crate::services::downloader::{check_and_install_aria2_with_ui, should_use_aria2};

        // 如果需要 aria2，显示进度对话框
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
    match install_firmware_to_yuzu(firmware_version.as_deref(), move |event| {
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

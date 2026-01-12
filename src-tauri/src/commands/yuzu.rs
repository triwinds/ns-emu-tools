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
    use crate::models::{ProgressEvent, ProgressStep, ProgressStatus};

    info!("安装 {} 版本: {}", get_emu_name(&branch), target_version);

    // 创建初始步骤（自动包含 aria2 检查步骤，如果需要）
    let mut steps = crate::services::downloader::create_installation_steps();

    // 其他步骤
    steps.extend(vec![
        ProgressStep {
            id: "fetch_version".to_string(),
            title: "获取版本信息".to_string(),
            status: ProgressStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        },
        ProgressStep {
            id: "download".to_string(),
            title: format!("下载 {}", get_emu_name(&branch)),
            status: ProgressStatus::Pending,
            step_type: "download".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
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
            download_source: None,
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
            download_source: None,
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
            download_source: None,
        },
    ]);

    // Emit initial event to open dialog and show steps
    let _ = window.emit("installation-event", ProgressEvent::Started { steps });

    // Windows: 检查并安装 aria2（如果需要）
    #[cfg(target_os = "windows")]
    {
        use crate::services::downloader::check_and_install_aria2_with_ui;

        if let Err(e) = check_and_install_aria2_with_ui(window.clone()).await {
            error!("aria2 安装失败: {}", e);
            return Err(format!("aria2 安装失败: {}", e));
        }
    }

    // 安装
    let window_clone = window.clone();
    let result = install_yuzu(&target_version, &branch, move |event| {
        // 发送事件到前端
        let _ = window_clone.emit("installation-event", event);
    })
    .await;

    match result {
        Ok(_) => {
            let _ = window.emit("installation-event", ProgressEvent::Finished { success: true, message: None });
            let _ = send_notify(&window, &format!("{} 安装成功", get_emu_name(&branch)));
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

/// 检测 Yuzu 版本
#[tauri::command]
pub async fn detect_yuzu_version_command(
    window: Window,
) -> Result<ApiResponse<Option<String>>, String> {
    use crate::models::{ProgressEvent, ProgressStep, ProgressStatus};

    info!("检测 Yuzu 版本");

    // 定义检测步骤
    let steps = vec![
        ProgressStep {
            id: "check_exe".to_string(),
            title: "检查模拟器程序".to_string(),
            status: ProgressStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        },
        ProgressStep {
            id: "start_program".to_string(),
            title: "启动程序检测版本".to_string(),
            status: ProgressStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        },
        ProgressStep {
            id: "detect_version".to_string(),
            title: "读取版本信息".to_string(),
            status: ProgressStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        },
    ];

    // 发送启动事件
    let _ = window.emit("installation-event", ProgressEvent::Started { steps: steps.clone() });

    // 步骤1: 检查程序
    let _ = window.emit("installation-event", ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "check_exe".to_string(),
            title: "检查模拟器程序".to_string(),
            status: ProgressStatus::Running,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        }
    });

    let exe_path = get_yuzu_exe_path();
    if !exe_path.exists() {
        let error_msg = format!("未找到模拟器程序: {}", exe_path.display());
        let _ = window.emit("installation-event", ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "check_exe".to_string(),
                title: "检查模拟器程序".to_string(),
                status: ProgressStatus::Error,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                error: Some(error_msg.clone()),
            download_source: None,
            }
        });
        let _ = window.emit("installation-event", ProgressEvent::Finished { success: false, message: Some(error_msg.clone()) });
        return Err(error_msg);
    }

    let _ = window.emit("installation-event", ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "check_exe".to_string(),
            title: "检查模拟器程序".to_string(),
            status: ProgressStatus::Success,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        }
    });

    // 步骤2: 启动程序
    let _ = window.emit("installation-event", ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "start_program".to_string(),
            title: "启动程序检测版本".to_string(),
            status: ProgressStatus::Running,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        }
    });

    match detect_yuzu_version().await {
        Ok(version) => {
            let _ = window.emit("installation-event", ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "start_program".to_string(),
                    title: "启动程序检测版本".to_string(),
                    status: ProgressStatus::Success,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: None,
            download_source: None,
                }
            });

            // 步骤3: 读取版本信息
            let _ = window.emit("installation-event", ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "detect_version".to_string(),
                    title: "读取版本信息".to_string(),
                    status: ProgressStatus::Running,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: None,
            download_source: None,
                }
            });

            if let Some(ref v) = version {
                let _ = window.emit("installation-event", ProgressEvent::StepUpdate {
                    step: ProgressStep {
                        id: "detect_version".to_string(),
                        title: format!("检测到版本: {}", v),
                        status: ProgressStatus::Success,
                        step_type: "normal".to_string(),
                        progress: 0.0,
                        download_speed: "".to_string(),
                        eta: "".to_string(),
                        error: None,
            download_source: None,
                    }
                });
                let _ = window.emit("installation-event", ProgressEvent::Finished { success: true, message: None });
            } else {
                let _ = window.emit("installation-event", ProgressEvent::StepUpdate {
                    step: ProgressStep {
                        id: "detect_version".to_string(),
                        title: "未能检测到版本".to_string(),
                        status: ProgressStatus::Error,
                        step_type: "normal".to_string(),
                        progress: 0.0,
                        download_speed: "".to_string(),
                        eta: "".to_string(),
                        error: Some("未能检测到版本".to_string()),
            download_source: None,
                    }
                });
                let _ = window.emit("installation-event", ProgressEvent::Finished { success: false, message: Some("未能检测到版本".to_string()) });
            }

            Ok(ApiResponse::success(version))
        }
        Err(e) => {
            error!("版本检测失败: {}", e);
            let _ = window.emit("installation-event", ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "start_program".to_string(),
                    title: "启动程序检测版本".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: Some(e.to_string()),
            download_source: None,
                }
            });
            let _ = window.emit("installation-event", ProgressEvent::Finished { success: false, message: Some(e.to_string()) });
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
    use crate::models::ProgressEvent;

    info!("安装固件到 Yuzu");

    let _ = send_notify(&window, "开始安装固件...");

    // Windows: 检查并安装 aria2（如果需要）
    #[cfg(target_os = "windows")]
    {
        use crate::services::downloader::{check_and_install_aria2_with_ui, should_use_aria2};

        // 如果需要 aria2，显示进度对话框
        if should_use_aria2() {
            let steps = crate::services::downloader::create_installation_steps();
            let _ = window.emit("installation-event", ProgressEvent::Started { steps });

            if let Err(e) = check_and_install_aria2_with_ui(window.clone()).await {
                error!("aria2 安装失败: {}", e);
                return Err(format!("aria2 安装失败: {}", e));
            }

            // aria2 检查完成，关闭对话框
            let _ = window.emit(
                "installation-event",
                ProgressEvent::Finished {
                    success: true,
                    message: None,
                },
            );
        }
    }

    let window_clone = window.clone();
    match install_firmware_to_yuzu(firmware_version.as_deref(), move |event| {
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

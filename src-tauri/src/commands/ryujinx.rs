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
    let mut steps = vec![];

    // Windows: 添加 aria2 检查步骤
    #[cfg(target_os = "windows")]
    steps.push(ProgressStep {
        id: "install_aria2".to_string(),
        title: "检查下载工具".to_string(),
        status: ProgressStatus::Pending,
        step_type: "download".to_string(),
        progress: 0.0,
        download_speed: "".to_string(),
        eta: "".to_string(),
        error: None,
        download_source: Some("GitHub".to_string()),
    });

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
            title: format!("下载 Ryujinx {}", branch),
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

    // Windows: 前置检查 aria2（带进度反馈）
    #[cfg(target_os = "windows")]
    {
        use crate::services::downloader::{ensure_aria2_installed_with_progress, format_bytes};

        let window_clone = window.clone();

        // 开始 aria2 检查步骤
        let _ = window.emit(
            "installation-event",
            ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "install_aria2".to_string(),
                    title: "检查下载工具".to_string(),
                    status: ProgressStatus::Running,
                    step_type: "download".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: None,
                    download_source: Some("GitHub".to_string()),
                },
            },
        );

        // 执行安装（带进度回调）
        match ensure_aria2_installed_with_progress(Some(Box::new(move |progress| {
            // 将 Aria2InstallProgress 转换为 ProgressStep 并发送
            let step = ProgressStep {
                id: "install_aria2".to_string(),
                title: match progress.stage.as_str() {
                    "checking" => "检查下载工具".to_string(),
                    "fetching" => "获取 aria2 版本信息".to_string(),
                    "downloading" => format!("下载 aria2 ({:.1}%)", progress.percentage),
                    "extracting" => "解压 aria2".to_string(),
                    _ => "准备下载工具".to_string(),
                },
                status: ProgressStatus::Running,
                step_type: "download".to_string(),
                progress: progress.percentage,
                download_speed: if progress.speed > 0 {
                    format!("{}/s", format_bytes(progress.speed))
                } else {
                    String::new()
                },
                eta: if progress.eta > 0 {
                    format_eta(progress.eta)
                } else {
                    String::new()
                },
                error: None,
                download_source: Some("GitHub".to_string()),
            };

            let _ = window_clone.emit("installation-event", ProgressEvent::StepUpdate { step });
        })))
        .await
        {
            Ok(_) => {
                // aria2 安装成功
                let _ = window.emit(
                    "installation-event",
                    ProgressEvent::StepUpdate {
                        step: ProgressStep {
                            id: "install_aria2".to_string(),
                            title: "下载工具就绪".to_string(),
                            status: ProgressStatus::Success,
                            step_type: "download".to_string(),
                            progress: 100.0,
                            download_speed: "".to_string(),
                            eta: "".to_string(),
                            error: None,
                            download_source: Some("GitHub".to_string()),
                        },
                    },
                );
            }
            Err(e) => {
                // aria2 安装失败
                error!("aria2 安装失败: {}", e);
                let _ = window.emit(
                    "installation-event",
                    ProgressEvent::StepUpdate {
                        step: ProgressStep {
                            id: "install_aria2".to_string(),
                            title: "下载工具安装失败".to_string(),
                            status: ProgressStatus::Error,
                            step_type: "download".to_string(),
                            progress: 0.0,
                            download_speed: "".to_string(),
                            eta: "".to_string(),
                            error: Some(e.to_string()),
                            download_source: Some("GitHub".to_string()),
                        },
                    },
                );
                let _ = window.emit(
                    "installation-event",
                    ProgressEvent::Finished {
                        success: false,
                        message: Some(format!("下载工具安装失败: {}", e)),
                    },
                );
                return Err(format!("aria2 安装失败: {}", e));
            }
        }
    }

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
    use crate::models::{ProgressEvent, ProgressStep, ProgressStatus};

    info!("安装固件到 Ryujinx");

    let _ = send_notify(&window, "开始安装固件...");

    // Windows: 前置检查 aria2（带进度反馈）
    #[cfg(target_os = "windows")]
    {
        use crate::services::downloader::{ensure_aria2_installed_with_progress, format_bytes};

        // 定义检查步骤
        let steps = vec![ProgressStep {
            id: "install_aria2".to_string(),
            title: "检查下载工具".to_string(),
            status: ProgressStatus::Pending,
            step_type: "download".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: Some("GitHub".to_string()),
        }];

        // 发送初始事件
        let _ = window.emit("installation-event", ProgressEvent::Started { steps });

        let window_clone = window.clone();

        // 开始 aria2 检查步骤
        let _ = window.emit(
            "installation-event",
            ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "install_aria2".to_string(),
                    title: "检查下载工具".to_string(),
                    status: ProgressStatus::Running,
                    step_type: "download".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: None,
                    download_source: Some("GitHub".to_string()),
                },
            },
        );

        // 执行安装（带进度回调）
        match ensure_aria2_installed_with_progress(Some(Box::new(move |progress| {
            // 将 Aria2InstallProgress 转换为 ProgressStep 并发送
            let step = ProgressStep {
                id: "install_aria2".to_string(),
                title: match progress.stage.as_str() {
                    "checking" => "检查下载工具".to_string(),
                    "fetching" => "获取 aria2 版本信息".to_string(),
                    "downloading" => format!("下载 aria2 ({:.1}%)", progress.percentage),
                    "extracting" => "解压 aria2".to_string(),
                    _ => "准备下载工具".to_string(),
                },
                status: ProgressStatus::Running,
                step_type: "download".to_string(),
                progress: progress.percentage,
                download_speed: if progress.speed > 0 {
                    format!("{}/s", format_bytes(progress.speed))
                } else {
                    String::new()
                },
                eta: if progress.eta > 0 {
                    format_eta(progress.eta)
                } else {
                    String::new()
                },
                error: None,
                download_source: Some("GitHub".to_string()),
            };

            let _ = window_clone.emit("installation-event", ProgressEvent::StepUpdate { step });
        })))
        .await
        {
            Ok(_) => {
                // aria2 安装成功
                let _ = window.emit(
                    "installation-event",
                    ProgressEvent::StepUpdate {
                        step: ProgressStep {
                            id: "install_aria2".to_string(),
                            title: "下载工具就绪".to_string(),
                            status: ProgressStatus::Success,
                            step_type: "download".to_string(),
                            progress: 100.0,
                            download_speed: "".to_string(),
                            eta: "".to_string(),
                            error: None,
                            download_source: Some("GitHub".to_string()),
                        },
                    },
                );
                let _ = window.emit(
                    "installation-event",
                    ProgressEvent::Finished {
                        success: true,
                        message: None,
                    },
                );
            }
            Err(e) => {
                // aria2 安装失败
                error!("aria2 安装失败: {}", e);
                let _ = window.emit(
                    "installation-event",
                    ProgressEvent::StepUpdate {
                        step: ProgressStep {
                            id: "install_aria2".to_string(),
                            title: "下载工具安装失败".to_string(),
                            status: ProgressStatus::Error,
                            step_type: "download".to_string(),
                            progress: 0.0,
                            download_speed: "".to_string(),
                            eta: "".to_string(),
                            error: Some(e.to_string()),
                            download_source: Some("GitHub".to_string()),
                        },
                    },
                );
                let _ = window.emit(
                    "installation-event",
                    ProgressEvent::Finished {
                        success: false,
                        message: Some(format!("下载工具安装失败: {}", e)),
                    },
                );
                return Err(format!("aria2 安装失败: {}", e));
            }
        }
    }

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
pub async fn detect_ryujinx_version_command(
    window: Window,
) -> Result<ApiResponse<Option<String>>, String> {
    use crate::models::{ProgressEvent, ProgressStep, ProgressStatus};
    use crate::services::ryujinx::detect_ryujinx_version;

    info!("检测 Ryujinx 版本");

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

    let config = crate::config::get_config();
    let ryujinx_path = std::path::PathBuf::from(&config.ryujinx.path);

    // 检查可执行文件（根据平台）
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

    if exe_path.is_none() || !exe_path.as_ref().unwrap().exists() {
        let error_msg = if let Some(path) = exe_path {
            format!("未找到模拟器程序: {}", path.display())
        } else {
            format!("未找到模拟器程序在: {}", ryujinx_path.display())
        };
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

    match detect_ryujinx_version().await {
        Ok((version, branch)) => {
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
                        title: format!("检测到版本: {} ({})", v, branch),
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

/// 格式化 ETA（剩余时间）
fn format_eta(seconds: u64) -> String {
    if seconds == 0 {
        return String::new();
    }

    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if hours > 0 {
        format!("{}h{}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m{}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}

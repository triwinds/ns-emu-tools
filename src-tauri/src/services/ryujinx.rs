//! Ryujinx 模拟器管理服务
//!
//! 提供 Ryujinx 模拟器（mainline/canary 分支）的安装、更新、版本检测等功能

use crate::config::{get_config, CONFIG};
use crate::error::{AppError, AppResult};
use crate::models::{ProgressEvent, ProgressStatus, ProgressStep};
use crate::repositories::ryujinx::{
    get_all_ryujinx_release_infos, get_ryujinx_release_info_by_version, load_ryujinx_change_log,
};
use crate::services::downloader::{get_download_manager, DownloadOptions};
#[cfg(target_os = "windows")]
use crate::services::msvc::check_and_install_msvc;
use crate::services::network::{get_download_source_name, get_final_url};
use crate::utils::archive::uncompress;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Ryujinx 可执行文件名（根据平台不同）
#[cfg(target_os = "windows")]
const RYUJINX_EXE_NAMES: &[&str] = &["Ryujinx.Ava.exe", "Ryujinx.exe"];

#[cfg(target_os = "macos")]
const RYUJINX_APP_NAME: &str = "Ryujinx.app";

#[cfg(target_os = "linux")]
const RYUJINX_EXE_NAMES: &[&str] = &["Ryujinx.Ava", "Ryujinx"];

/// 获取 Ryujinx 下载 URL
///
/// # 参数
/// * `target_version` - 目标版本
/// * `branch` - 分支 (mainline, canary)
///
/// # 返回
/// 下载 URL
async fn get_ryujinx_download_url(target_version: &str, branch: &str) -> AppResult<String> {
    debug!("获取 Ryujinx {} 版本 {} 的下载链接", branch, target_version);
    let release_info = get_ryujinx_release_info_by_version(target_version, branch).await?;

    if release_info.tag_name.is_empty() {
        warn!("未找到 Ryujinx {} 版本: {}", branch, target_version);
        return Err(AppError::Emulator(format!(
            "未找到 Ryujinx {} 版本: {}",
            branch, target_version
        )));
    }

    debug!("找到版本: {}, 资源数量: {}", release_info.tag_name, release_info.assets.len());

    // 根据平台选择资源后缀
    #[cfg(target_os = "windows")]
    let suffix = "-win_x64.zip";

    #[cfg(target_os = "macos")]
    let suffix = "-macos_universal.app.tar.gz";

    #[cfg(target_os = "linux")]
    let suffix = if cfg!(target_arch = "aarch64") {
        "-linux_arm64.tar.gz"
    } else {
        "-linux_x64.tar.gz"
    };

    // 查找对应平台的版本
    for asset in &release_info.assets {
        let name = asset.name.to_lowercase();
        debug!("检查资源: {} (size: {})", asset.name, asset.size);
        if name.starts_with("ryujinx-") && name.ends_with(suffix) {
            let url = get_final_url(&asset.download_url);
            info!("选择下载资源: {}, URL: {}", asset.name, url);
            return Ok(url);
        }
    }

    warn!("未找到合适的下载资源 for {} [{}]", branch, target_version);
    Err(AppError::Emulator(format!(
        "未找到 Ryujinx {} [{}] 版本的下载链接",
        branch, target_version
    )))
}

/// 获取 Ryujinx 可执行文件路径
///
/// 根据平台返回对应的可执行文件路径
fn get_ryujinx_exe_path_internal(ryujinx_path: &Path) -> Option<PathBuf> {
    debug!("查找 Ryujinx 可执行文件，路径: {}", ryujinx_path.display());

    #[cfg(target_os = "macos")]
    {
        // macOS: 查找 Ryujinx.app 并返回实际可执行文件路径
        let app_path = ryujinx_path.join(RYUJINX_APP_NAME);
        if app_path.exists() {
            let exe_path = app_path.join("Contents/MacOS/Ryujinx");
            debug!("找到 Ryujinx.app: {}", exe_path.display());
            return Some(exe_path);
        }
        warn!("未找到 Ryujinx.app 在: {}", ryujinx_path.display());
        return None;
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        // Windows/Linux: 按优先级查找可执行文件
        for exe_name in RYUJINX_EXE_NAMES {
            let exe_path = ryujinx_path.join(exe_name);
            if exe_path.exists() {
                debug!("找到 {}: {}", exe_name, exe_path.display());
                return Some(exe_path);
            }
        }
        warn!("未找到 Ryujinx 可执行文件在: {}", ryujinx_path.display());
        None
    }
}

/// 检测当前 Ryujinx 分支
///
/// 通过检查可执行文件名来判断
/// - Ryujinx.Ava.exe -> ava
/// - Ryujinx.exe -> mainline
pub fn detect_current_branch() -> String {
    debug!("检测 Ryujinx 当前分支");
    let config = get_config();
    let ryujinx_path = PathBuf::from(&config.ryujinx.path);

    if let Some(exe_path) = get_ryujinx_exe_path_internal(&ryujinx_path) {
        if exe_path.file_name().unwrap().to_string_lossy().contains("Ava") {
            debug!("通过文件名检测到分支: ava");
            return "ava".to_string();
        } else {
            debug!("通过文件名检测到分支: mainline");
            return "mainline".to_string();
        }
    }

    // 默认返回配置中的分支
    debug!("使用配置中的分支: {}", config.ryujinx.branch);
    config.ryujinx.branch.clone()
}

/// 安装 Ryujinx
///
/// # 参数
/// * `target_version` - 目标版本
/// * `branch` - 分支 (mainline, canary)
/// * `on_event` - 事件回调
pub async fn install_ryujinx_by_version<F>(
    target_version: &str,
    branch: &str,
    on_event: F,
) -> AppResult<()>
where
    F: Fn(ProgressEvent) + Send + Sync + 'static + Clone,
{
    info!(
        "开始安装 Ryujinx {} 版本: {}",
        branch, target_version
    );

    let (ryujinx_path, auto_delete) = {
        let config = get_config();
        (
            PathBuf::from(&config.ryujinx.path),
            config.setting.download.auto_delete_after_install,
        )
    };

    // 检查当前版本
    let current_branch = detect_current_branch();
    let config = get_config();
    if let Some(ref current_version) = config.ryujinx.version {
        if current_version == target_version
            && (branch == "ldn" || current_branch == branch)
        {
            info!("当前已是目标版本，跳过安装");

            // 更新步骤状态
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "fetch_version".to_string(),
                    title: format!("当前已是目标版本 {} ({}), 跳过安装", target_version, branch),
                    status: ProgressStatus::Success,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: None,
                    download_source: None,
                }
            });

            // 标记其他步骤为取消
            for step_id in &["download", "extract", "install", "check_env"] {
                on_event(ProgressEvent::StepUpdate {
                    step: ProgressStep {
                        id: step_id.to_string(),
                        title: match *step_id {
                            "download" => format!("下载 Ryujinx {}", branch),
                            "extract" => "解压文件".to_string(),
                            "install" => "安装文件".to_string(),
                            "check_env" => "检查运行环境".to_string(),
                            _ => "".to_string(),
                        },
                        status: ProgressStatus::Cancelled,
                        step_type: if *step_id == "download" { "download" } else { "normal" }.to_string(),
                        progress: 0.0,
                        download_speed: "".to_string(),
                        eta: "".to_string(),
                        error: None,
                        download_source: None,
                    }
                });
            }

            return Ok(());
        }
    }

    // 获取版本信息
    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "fetch_version".to_string(),
            title: "获取版本信息".to_string(),
            status: ProgressStatus::Running,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        }
    });

    let download_url = match get_ryujinx_download_url(target_version, branch).await {
        Ok(url) => url,
        Err(e) => {
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "fetch_version".to_string(),
                    title: "获取版本信息".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: Some(e.to_string()),
                    download_source: None,
                }
            });
            return Err(e);
        }
    };

    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "fetch_version".to_string(),
            title: "获取版本信息".to_string(),
            status: ProgressStatus::Success,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        }
    });

    info!("下载 URL: {}", download_url);

    // 获取下载源名称
    let download_source = get_download_source_name(&download_url);
    debug!("下载源: {}", download_source);

    // 下载
    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "download".to_string(),
            title: format!("下载 Ryujinx {}", branch),
            status: ProgressStatus::Running,
            step_type: "download".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: Some(download_source.clone()),
        }
    });

    // 使用统一下载接口
    let download_manager = match get_download_manager().await {
        Ok(manager) => manager,
        Err(e) => {
            warn!("获取下载管理器失败: {}", e);
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "download".to_string(),
                    title: format!("下载 Ryujinx {}", branch),
                    status: ProgressStatus::Error,
                    step_type: "download".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: Some(e.to_string()),
                    download_source: Some(download_source.clone()),
                }
            });
            return Err(e);
        }
    };
    let options = DownloadOptions {
        use_github_mirror: false, // Ryujinx 使用 GitLab，不需要镜像
        ..Default::default()
    };

    // 下载并等待完成
    let on_event_clone = on_event.clone();
    let download_source_clone = download_source.clone();
    let branch_clone = branch.to_string();
    let result = match download_manager.download_and_wait(&download_url, options, Box::new(move |progress| {
        on_event_clone(ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "download".to_string(),
                title: format!("下载 Ryujinx {}", branch_clone),
                status: ProgressStatus::Running,
                step_type: "download".to_string(),
                progress: progress.percentage,
                download_speed: progress.speed_string(),
                eta: progress.eta_string(),
                error: None,
                download_source: Some(download_source_clone.clone()),
            }
        });
    })).await {
        Ok(res) => res,
        Err(e) => {
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "download".to_string(),
                    title: format!("下载 Ryujinx {}", branch),
                    status: ProgressStatus::Error,
                    step_type: "download".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: Some(e.to_string()),
                    download_source: Some(download_source.clone()),
                }
            });
            return Err(e);
        }
    };

    let package_path = result.path;
    info!("下载完成: {}", package_path.display());

    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "download".to_string(),
            title: format!("下载 Ryujinx {}", branch),
            status: ProgressStatus::Success,
            step_type: "download".to_string(),
            progress: 100.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        }
    });

    // 解压
    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "extract".to_string(),
            title: "解压文件".to_string(),
            status: ProgressStatus::Running,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        }
    });

    // 解压到临时目录
    let tmp_dir = std::env::temp_dir().join("ryujinx-install");
    debug!("准备解压到临时目录: {}", tmp_dir.display());
    if tmp_dir.exists() {
        debug!("清理已存在的临时目录");
        if let Err(e) = std::fs::remove_dir_all(&tmp_dir) {
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "extract".to_string(),
                    title: "解压文件".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: Some(format!("清理临时目录失败: {}", e)),
                    download_source: None,
                }
            });
            return Err(e.into());
        }
    }
    if let Err(e) = std::fs::create_dir_all(&tmp_dir) {
        on_event(ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "extract".to_string(),
                title: "解压文件".to_string(),
                status: ProgressStatus::Error,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                error: Some(format!("创建临时目录失败: {}", e)),
                download_source: None,
            }
        });
        return Err(e.into());
    }

    info!("解压 Ryujinx 文件到: {}", tmp_dir.display());
    debug!("解压包路径: {}, 大小: {} bytes",
        package_path.display(),
        package_path.metadata().map(|m| m.len()).unwrap_or(0));
    if let Err(e) = uncompress(&package_path, &tmp_dir, false) {
        on_event(ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "extract".to_string(),
                title: "解压文件".to_string(),
                status: ProgressStatus::Error,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                error: Some(e.to_string()),
                download_source: None,
            }
        });
        return Err(e);
    }

    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "extract".to_string(),
            title: "解压文件".to_string(),
            status: ProgressStatus::Success,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        }
    });

    // 安装
    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "install".to_string(),
            title: "安装文件".to_string(),
            status: ProgressStatus::Running,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        }
    });

    // 清理旧文件并安装
    debug!("清理旧 Ryujinx 文件");
    if let Err(e) = clear_ryujinx_folder(&ryujinx_path) {
        on_event(ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "install".to_string(),
                title: "安装文件".to_string(),
                status: ProgressStatus::Error,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                error: Some(e.to_string()),
                download_source: None,
            }
        });
        return Err(e);
    }

    // 复制文件（根据平台不同处理方式）
    #[cfg(target_os = "macos")]
    {
        // macOS: 解压后直接是 Ryujinx.app，查找并复制
        let mut app_path = None;
        if let Ok(entries) = std::fs::read_dir(&tmp_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "app") {
                    app_path = Some(path);
                    break;
                }
            }
        }

        let source_app = app_path.ok_or_else(|| AppError::Emulator("未找到 Ryujinx.app".to_string()))?;
        let dest_app = ryujinx_path.join("Ryujinx.app");

        info!("复制 Ryujinx.app 从: {} 到: {}", source_app.display(), dest_app.display());

        // 删除旧的 .app（如果存在）
        if dest_app.exists() {
            std::fs::remove_dir_all(&dest_app)?;
        }

        if let Err(e) = copy_dir_all(&source_app, &dest_app) {
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "install".to_string(),
                    title: "安装文件".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: Some(e.to_string()),
                    download_source: None,
                }
            });
            return Err(e);
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Windows/Linux: 从 publish 目录复制
        let ryujinx_tmp_dir = tmp_dir.join("publish");
        info!("复制 Ryujinx 文件从: {} 到: {}",
            ryujinx_tmp_dir.display(), ryujinx_path.display());
        debug!("检查解压后的 publish 目录: {}", ryujinx_tmp_dir.display());

        if let Err(e) = copy_dir_all(&ryujinx_tmp_dir, &ryujinx_path) {
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "install".to_string(),
                    title: "安装文件".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: Some(e.to_string()),
                    download_source: None,
                }
            });
            return Err(e);
        }
    }

    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "install".to_string(),
            title: "安装文件".to_string(),
            status: ProgressStatus::Success,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        }
    });

    // 清理临时目录
    std::fs::remove_dir_all(&tmp_dir)?;

    // 检查运行环境
    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "check_env".to_string(),
            title: "检查运行环境".to_string(),
            status: ProgressStatus::Running,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        }
    });

    #[cfg(target_os = "windows")]
    {
        if let Err(e) = check_and_install_msvc().await {
            warn!("MSVC 运行库检查失败: {}", e);
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "check_env".to_string(),
                    title: "检查运行环境".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: Some(e.to_string()),
                    download_source: None,
                }
            });
            // 不阻止安装流程，继续执行
        } else {
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "check_env".to_string(),
                    title: "检查运行环境".to_string(),
                    status: ProgressStatus::Success,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: None,
                    download_source: None,
                }
            });
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // macOS/Linux: 不需要 MSVC 运行库，直接标记为成功
        on_event(ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "check_env".to_string(),
                title: "检查运行环境".to_string(),
                status: ProgressStatus::Success,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                error: None,
                download_source: None,
            }
        });
    }

    // 更新配置
    {
        let mut cfg = CONFIG.write();
        cfg.ryujinx.version = Some(target_version.to_string());
        cfg.ryujinx.branch = branch.to_string();
        cfg.save()?;
    }

    info!("Ryujinx {} [{}] 安装成功", branch, target_version);

    // 如果配置了自动删除，删除下载文件
    if auto_delete {
        let _ = std::fs::remove_file(&package_path);
    }

    Ok(())
}

/// 清理 Ryujinx 文件夹（保留用户数据）
///
/// 删除旧的程序文件，但保留用户数据和配置
///
/// # 安全说明
/// - Windows: 只删除 Ryujinx*.exe 可执行文件，不删除整个目录（避免误删其他程序）
/// - macOS: 只删除 Ryujinx.app，不删除整个目录（避免误删其他应用）
/// - Linux: 只删除 Ryujinx 可执行文件，不删除整个目录（避免误删其他程序）
fn clear_ryujinx_folder(ryujinx_path: &Path) -> AppResult<()> {
    if !ryujinx_path.exists() {
        std::fs::create_dir_all(ryujinx_path)?;
        return Ok(());
    }

    info!("清理 Ryujinx 目录: {}", ryujinx_path.display());

    #[cfg(target_os = "macos")]
    {
        // macOS: 只删除 Ryujinx.app，不删除整个目录
        // 这样可以避免误删用户放在同一目录下的其他应用程序
        let app_path = ryujinx_path.join(RYUJINX_APP_NAME);
        if app_path.exists() {
            debug!("删除旧的 Ryujinx.app: {}", app_path.display());
            std::fs::remove_dir_all(&app_path)?;
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Linux: 只删除 Ryujinx 可执行文件，不删除整个目录
        // 这样可以避免误删用户放在同一目录下的其他程序
        for exe_name in RYUJINX_EXE_NAMES {
            let exe_path = ryujinx_path.join(exe_name);
            if exe_path.exists() {
                debug!("删除可执行文件: {}", exe_path.display());
                std::fs::remove_file(&exe_path)?;
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Windows: 只删除 Ryujinx*.exe 可执行文件，不删除整个目录
        // 这样可以避免误删用户放在同一目录下的其他程序
        // 匹配 Ryujinx*.exe 模式
        if let Ok(entries) = std::fs::read_dir(ryujinx_path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() {
                    if let Some(file_name) = path.file_name() {
                        let name = file_name.to_string_lossy();
                        // 匹配 Ryujinx*.exe 模式（不区分大小写）
                        if name.to_lowercase().starts_with("ryujinx")
                            && name.to_lowercase().ends_with(".exe") {
                            debug!("删除可执行文件: {}", path.display());
                            std::fs::remove_file(&path)?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// 递归复制目录
fn copy_dir_all(src: &Path, dst: &Path) -> AppResult<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst.join(&file_name);

        if path.is_dir() {
            copy_dir_all(&path, &dst_path)?;
        } else {
            std::fs::copy(&path, &dst_path).map_err(|e| {
                AppError::Emulator(format!("Ryujinx 文件复制失败: {}", e))
            })?;
        }
    }

    Ok(())
}

/// 获取 Ryujinx 用户文件夹路径
pub fn get_ryujinx_user_folder() -> PathBuf {
    debug!("获取 Ryujinx 用户文件夹路径");
    let config = get_config();
    let ryujinx_path = PathBuf::from(&config.ryujinx.path);

    // 检查是否使用 portable 模式
    let portable_path = ryujinx_path.join("portable");
    if portable_path.exists() {
        debug!("使用 portable 模式: {}", portable_path.display());
        return portable_path;
    }

    #[cfg(target_os = "windows")]
    {
        // Windows: 使用 %APPDATA%/Ryujinx
        if let Ok(appdata) = std::env::var("APPDATA") {
            let appdata_path = PathBuf::from(appdata);
            let ryujinx_appdata = appdata_path.join("Ryujinx");
            if ryujinx_appdata.exists() {
                debug!("使用 AppData 目录: {}", ryujinx_appdata.display());
                return ryujinx_appdata;
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        // macOS: 使用 ~/Library/Application Support/Ryujinx
        if let Ok(home) = std::env::var("HOME") {
            let macos_path = PathBuf::from(home)
                .join("Library/Application Support/Ryujinx");
            if macos_path.exists() {
                debug!("使用 macOS Application Support 目录: {}", macos_path.display());
                return macos_path;
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Linux: 使用 ~/.config/Ryujinx
        if let Ok(home) = std::env::var("HOME") {
            let linux_path = PathBuf::from(home).join(".config/Ryujinx");
            if linux_path.exists() {
                debug!("使用 Linux config 目录: {}", linux_path.display());
                return linux_path;
            }
        }
    }

    // 默认创建 portable 目录
    debug!("创建默认 portable 目录: {}", portable_path.display());
    std::fs::create_dir_all(&portable_path).ok();
    portable_path
}

/// 安装固件到 Ryujinx
///
/// # 参数
/// * `firmware_version` - 固件版本，None 表示最新版本
/// * `on_event` - 事件回调
pub async fn install_firmware_to_ryujinx<F>(
    firmware_version: Option<&str>,
    on_event: F,
) -> AppResult<()>
where
    F: Fn(ProgressEvent) + Send + Sync + 'static + Clone,
{
    let config = get_config();

    // 检查是否已安装此版本
    if let Some(ref version) = firmware_version {
        if let Some(ref current_firmware) = config.ryujinx.firmware {
            if current_firmware == version {
                info!("固件已是版本 {}，跳过安装", version);

                // 发送 Started 事件
                let steps = vec![
                    ProgressStep {
                        id: "check_firmware".to_string(),
                        title: format!("当前固件已是版本 {}, 跳过安装", version),
                        status: ProgressStatus::Success,
                        step_type: "normal".to_string(),
                        progress: 0.0,
                        download_speed: "".to_string(),
                        eta: "".to_string(),
                        error: None,
                        download_source: None,
                    }
                ];
                on_event(ProgressEvent::Started { steps: steps.clone() });
                on_event(ProgressEvent::StepUpdate {
                    step: steps[0].clone(),
                });
                return Ok(());
            }
        }
    }

    // 固件路径
    let firmware_path = get_ryujinx_user_folder().join("bis/system/Contents/registered");
    let tmp_dir = firmware_path.parent().unwrap().join("tmp");

    // 确保临时目录存在
    std::fs::create_dir_all(&tmp_dir)?;

    info!("开始安装固件到 Ryujinx，临时路径: {}", tmp_dir.display());

    // 发送 Started 事件,包含所有步骤
    let steps = vec![
        ProgressStep {
            id: "fetch_firmware_info".to_string(),
            title: "获取固件信息".to_string(),
            status: ProgressStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        },
        ProgressStep {
            id: "download_firmware".to_string(),
            title: "下载固件".to_string(),
            status: ProgressStatus::Pending,
            step_type: "download".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        },
        ProgressStep {
            id: "extract_firmware".to_string(),
            title: "解压固件".to_string(),
            status: ProgressStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        },
        ProgressStep {
            id: "reorganize_firmware".to_string(),
            title: "重组织固件文件".to_string(),
            status: ProgressStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        },
    ];

    on_event(ProgressEvent::Started { steps });

    // 调用固件服务进行安装（先解压到临时目录）
    let version_to_install = firmware_version.unwrap_or_else(|| {
        // 如果没有指定版本，需要获取最新版本
        "latest"
    });

    let on_event_clone = on_event.clone();
    let new_version = crate::services::firmware::install_firmware(
        version_to_install,
        &tmp_dir,
        on_event_clone,
    ).await?;

    // 步骤5: 重组织固件文件（Ryujinx 的特殊格式）
    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "reorganize_firmware".to_string(),
            title: "重组织固件文件".to_string(),
            status: ProgressStatus::Running,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        }
    });

    if let Err(e) = crate::services::firmware::reorganize_firmware_for_ryujinx(&tmp_dir, &firmware_path).await {
        on_event(ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "reorganize_firmware".to_string(),
                title: "重组织固件文件".to_string(),
                status: ProgressStatus::Error,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                error: Some(e.to_string()),
                download_source: None,
            }
        });
        // 清理临时目录
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(e);
    }

    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "reorganize_firmware".to_string(),
            title: "重组织固件文件".to_string(),
            status: ProgressStatus::Success,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
            download_source: None,
        }
    });

    // 清理临时目录
    std::fs::remove_dir_all(&tmp_dir)?;

    // 更新配置
    {
        let mut cfg = CONFIG.write();
        cfg.ryujinx.firmware = Some(new_version.clone());
        cfg.save()?;
    }

    info!("固件 {} 安装成功到 Ryujinx", new_version);
    Ok(())
}

/// 启动 Ryujinx
pub fn start_ryujinx() -> AppResult<()> {
    let config = get_config();
    let ryujinx_path = PathBuf::from(&config.ryujinx.path);

    let exe_path = match get_ryujinx_exe_path_internal(&ryujinx_path) {
        Some(path) => path,
        None => {
            return Err(AppError::FileNotFound(
                format!("未找到 Ryujinx 程序: {}", ryujinx_path.display())
            ));
        }
    };

    info!("启动 Ryujinx: {}", exe_path.display());

    Command::new(&exe_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    Ok(())
}

/// 打开 Ryujinx keys 文件夹
pub fn open_ryujinx_keys_folder() -> AppResult<()> {
    let keys_path = get_ryujinx_user_folder().join("system");
    std::fs::create_dir_all(&keys_path)?;

    // 创建提示文件
    let hint_file = keys_path.join("把prod.keys和title.keys放当前目录.txt");
    std::fs::write(&hint_file, "")?;

    info!("打开 keys 目录: {}", keys_path.display());

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(keys_path.to_string_lossy().to_string())
            .spawn()?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(keys_path.to_string_lossy().to_string())
            .spawn()?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(keys_path.to_string_lossy().to_string())
            .spawn()?;
    }

    Ok(())
}

/// 更新 Ryujinx 安装路径
pub fn update_ryujinx_path(new_path: &str) -> AppResult<()> {
    use crate::models::storage::{add_ryujinx_history, get_storage, STORAGE};

    let new_path_buf = PathBuf::from(new_path);

    // 创建目录
    if !new_path_buf.exists() {
        info!("创建目录: {}", new_path_buf.display());
        std::fs::create_dir_all(&new_path_buf)?;
    }

    let config = get_config();
    let old_path = PathBuf::from(&config.ryujinx.path);

    // 检查是否相同
    if new_path_buf == old_path {
        info!("路径相同，跳过更新");
        return Ok(());
    }

    // 保存旧配置到历史记录
    add_ryujinx_history(config.ryujinx.clone(), true)?;
    info!("已保存旧 Ryujinx 配置到历史记录: {}", old_path.display());

    // 从历史记录中获取新路径的配置，如果不存在则使用默认配置
    let new_config = {
        let storage = get_storage();
        let new_path_str = new_path_buf
            .canonicalize()
            .unwrap_or(new_path_buf.clone())
            .to_string_lossy()
            .to_string();

        if let Some(historical_config) = storage.ryujinx_history.get(&new_path_str) {
            info!("从历史记录中恢复 Ryujinx 配置");
            let mut cfg = historical_config.clone();
            cfg.path = new_path_buf.clone();
            cfg
        } else {
            info!("历史记录中没有该路径，使用默认配置");
            let mut cfg = crate::config::RyujinxConfig::default();
            cfg.path = new_path_buf.clone();
            cfg
        }
    };

    // 更新配置
    {
        let mut cfg = CONFIG.write();
        cfg.ryujinx = new_config.clone();
        cfg.save()?;
    }

    // 如果新路径不在历史记录中，保存到历史记录
    {
        let storage = STORAGE.read();
        let new_path_str = new_path_buf
            .canonicalize()
            .unwrap_or(new_path_buf.clone())
            .to_string_lossy()
            .to_string();

        if !storage.ryujinx_history.contains_key(&new_path_str) {
            drop(storage); // 释放读锁
            add_ryujinx_history(new_config, true)?;
            info!("已保存新 Ryujinx 配置到历史记录");
        }
    }

    info!("Ryujinx 路径已更新: {}", new_path_buf.display());
    Ok(())
}

/// 获取 Ryujinx 变更日志
pub async fn get_ryujinx_change_logs(branch: &str) -> AppResult<String> {
    let changelog = load_ryujinx_change_log(branch).await?;
    Ok(changelog)
}

/// 获取所有 Ryujinx 版本
pub async fn get_all_ryujinx_versions(branch: &str) -> AppResult<Vec<String>> {
    let releases = get_all_ryujinx_release_infos(branch).await?;
    let versions: Vec<String> = releases.iter().map(|r| r.tag_name.clone()).collect();
    Ok(versions)
}

/// 检测 Ryujinx 版本（通过启动程序并读取窗口标题）
///
/// # 返回
/// (版本号, 分支)
pub async fn detect_ryujinx_version() -> AppResult<(Option<String>, String)> {
    info!("检测 Ryujinx 版本");

    let config = get_config();
    let ryujinx_path = PathBuf::from(&config.ryujinx.path);
    debug!("Ryujinx 路径: {}", ryujinx_path.display());

    let exe_path = match get_ryujinx_exe_path_internal(&ryujinx_path) {
        Some(path) => path,
        None => {
            warn!("未找到 Ryujinx 程序: {}", ryujinx_path.display());
            return Ok((None, "mainline".to_string()));
        }
    };

    // 先检测基础分支（通过文件名）
    #[allow(unused_mut)]
    let mut branch = detect_current_branch();

    // 启动程序
    info!("启动 Ryujinx: {}", exe_path.display());
    debug!("检测版本时启动 Ryujinx");
    let mut child = Command::new(&exe_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    debug!("Ryujinx 进程 ID: {}", child.id());

    // 等待窗口创建
    debug!("等待窗口创建...");
    tokio::time::sleep(Duration::from_secs(3)).await;

    #[allow(unused_mut)]
    let mut version: Option<String> = None;

    #[cfg(windows)]
    {
        use std::sync::{Arc, Mutex};
        use windows::Win32::Foundation::{HWND, LPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            EnumWindows, GetWindowTextW, IsWindowVisible,
        };

        let version_data = Arc::new(Mutex::new((None::<String>, branch.clone())));
        let version_data_clone = version_data.clone();

        unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> windows::Win32::Foundation::BOOL {
            if IsWindowVisible(hwnd).as_bool() {
                let mut text: [u16; 512] = [0; 512];
                let len = GetWindowTextW(hwnd, &mut text);

                if len > 0 {
                    let window_title = String::from_utf16_lossy(&text[..len as usize]);

                    let data_ptr = lparam.0 as *const Arc<Mutex<(Option<String>, String)>>;
                    let data = &*data_ptr;

                    // Ryujinx 窗口标题格式:
                    // - "Ryujinx 1.1.1234 - ..." (打开了游戏)
                    // - "Ryujinx 1.1.1234" (没打开游戏)
                    // - "Ryujinx Console 1.1.1234"
                    // - "Ryujinx Canary 1.3.236"
                    // - "Ryujinx Canary Console 1.3.236"
                    if window_title.starts_with("Ryujinx ") && !window_title.contains('-') {
                        let mut guard = data.lock().unwrap();

                        // 跳过 "Ryujinx " 前缀
                        let mut remaining = &window_title[8..];

                        // 检测分支类型
                        let mut detected_branch = guard.1.clone(); // 默认使用文件名检测的分支

                        // 检测 Canary 分支
                        if remaining.starts_with("Canary ") {
                            detected_branch = "canary".to_string();
                            remaining = &remaining[7..]; // 跳过 "Canary "
                        }

                        // 跳过 Console 前缀（如果有）
                        if remaining.starts_with("Console ") {
                            remaining = &remaining[8..]; // 跳过 "Console "
                        }

                        // 提取版本号（空格前或全部内容）
                        let version = if let Some(space_pos) = remaining.find(' ') {
                            &remaining[..space_pos]
                        } else {
                            remaining
                        };

                        // 检测 LDN 分支（版本号中包含 ldn）
                        if version.contains("ldn") {
                            if let Some(ldn_pos) = version.find("ldn") {
                                guard.0 = Some(version[ldn_pos + 3..].to_string());
                                guard.1 = "ldn".to_string();
                            }
                        } else {
                            // 设置版本号和分支
                            guard.0 = Some(version.to_string());
                            guard.1 = detected_branch;
                        }

                        return windows::Win32::Foundation::BOOL(0); // Stop enumeration
                    }
                }
            }
            windows::Win32::Foundation::BOOL(1) // Continue enumeration
        }

        // 多次尝试，等待窗口出现
        debug!("开始枚举窗口标题以检测版本");
        for i in 0..30 {
            unsafe {
                let _ = EnumWindows(
                    Some(enum_proc),
                    LPARAM(&version_data_clone as *const _ as isize),
                );
            }

            let guard = version_data.lock().unwrap();
            if guard.0.is_some() {
                version = guard.0.clone();
                branch = guard.1.clone();
                debug!("第 {} 次尝试找到窗口标题，检测到版本: {:?}, 分支: {:?}", i + 1, version, branch);
                break;
            }

            if i % 5 == 0 {
                debug!("第 {} 次尝试，尚未找到窗口标题", i + 1);
            }

            std::thread::sleep(Duration::from_millis(500));
        }
    }

    // 结束进程
    debug!("结束 Ryujinx 进程");
    let _ = child.kill();
    let _ = child.wait();

    // 更新配置
    if let Some(ref v) = version {
        info!("检测到 Ryujinx 版本: {}, 分支: {}", v, branch);
        debug!("更新配置文件");

        let mut cfg = CONFIG.write();
        cfg.ryujinx.version = Some(v.clone());
        cfg.ryujinx.branch = branch.clone();
        cfg.save()?;
        debug!("配置文件已保存");
    } else {
        warn!("未能检测到 Ryujinx 版本");
        debug!("可能的原因: 窗口标题不匹配或窗口创建延迟过长");
    }

    Ok((version, branch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_current_branch() {
        // 测试需要配置环境
        let _branch = detect_current_branch();
    }

    #[test]
    fn test_get_ryujinx_user_folder() {
        // 测试需要配置环境
        let _path = get_ryujinx_user_folder();
    }
}

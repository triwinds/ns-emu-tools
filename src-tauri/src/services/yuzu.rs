//! Yuzu 系列模拟器（Eden/Citron）管理服务
//!
//! 提供 Yuzu、Eden、Citron 模拟器的安装、更新、版本检测等功能

use crate::config::{get_config, CONFIG};
use crate::error::{AppError, AppResult};
use crate::models::{ProgressEvent, ProgressStatus, ProgressStep}; // Import models
use crate::repositories::yuzu::{get_latest_change_log, get_yuzu_release_info_by_version};
use crate::services::aria2::{get_aria2_manager, Aria2DownloadOptions};
use crate::services::msvc::check_and_install_msvc;
use crate::utils::archive::uncompress;
use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use tracing::{debug, info, warn};

/// 支持的模拟器可执行文件列表
const DETECT_EXE_LIST: &[&str] = &["yuzu.exe", "eden.exe", "citron.exe", "suzu.exe", "cemu.exe"];

/// 支持下载的分支
const DOWNLOAD_AVAILABLE_BRANCH: &[&str] = &["citron", "eden"];

/// 当前正在进行的下载任务 GID
static CURRENT_DOWNLOAD_GID: RwLock<Option<String>> = RwLock::new(None);

/// 获取模拟器名称
pub fn get_emu_name(branch: &str) -> &'static str {
    match branch {
        "eden" => "Eden",
        "citron" => "Citron",
        _ => "Yuzu",
    }
}

/// 下载 Yuzu/Eden/Citron
///
/// # 参数
/// * `target_version` - 目标版本
/// * `branch` - 分支 (eden, citron)
/// * `on_progress` - 进度回调
///
/// # 返回
/// 下载文件的路径
pub async fn download_yuzu<F>(
    target_version: &str,
    branch: &str,
    on_progress: F,
) -> AppResult<PathBuf>
where
    F: Fn(crate::services::aria2::Aria2DownloadProgress) + Send + 'static,
{
    // 检查分支是否支持
    if !DOWNLOAD_AVAILABLE_BRANCH.contains(&branch) {
        warn!("不支持的分支: {}, 支持的分支: {:?}", branch, DOWNLOAD_AVAILABLE_BRANCH);
        return Err(AppError::Emulator(format!(
            "只支持安装分支: {:?}",
            DOWNLOAD_AVAILABLE_BRANCH
        )));
    }

    info!(
        "开始下载 {} 版本: {}",
        get_emu_name(branch),
        target_version
    );

    // 获取版本信息
    debug!("获取 {} 版本 {} 的发布信息", get_emu_name(branch), target_version);
    let release_info = get_yuzu_release_info_by_version(target_version, branch).await?;

    if release_info.tag_name.is_empty() {
        warn!("未找到 {} 版本: {}", get_emu_name(branch), target_version);
        return Err(AppError::Emulator(format!(
            "未找到 {} 版本: {}",
            get_emu_name(branch),
            target_version
        )));
    }

    debug!("找到版本: {}, 资源数量: {}", release_info.tag_name, release_info.assets.len());

    // 查找下载 URL
    let mut download_url: Option<String> = None;

    for asset in &release_info.assets {
        let name = asset.name.to_lowercase();
        debug!("检查资源: {} (size: {})", asset.name, asset.size);

        if name.ends_with(".7z") {
            download_url = Some(asset.download_url.clone());
            debug!("选择 .7z 资源: {}", asset.name);
            break;
        } else if name.starts_with("windows-yuzu-ea-") && name.ends_with(".zip") {
            download_url = Some(asset.download_url.clone());
            debug!("选择 Yuzu EA .zip 资源: {}", asset.name);
            break;
        } else if name.starts_with("eden-windows-") && name.ends_with(".zip") {
            download_url = Some(asset.download_url.clone());
            debug!("选择 Eden .zip 资源: {}", asset.name);
            break;
        } else if name.contains("windows") {
            // for citron
            download_url = Some(asset.download_url.clone());
            debug!("选择 Windows 资源: {}", asset.name);
            break;
        }
    }

    let url = download_url.ok_or_else(|| {
        warn!("无法找到合适的下载资源");
        AppError::Emulator(format!("无法获取 {} 下载链接", get_emu_name(branch)))
    })?;

    info!("下载 {} 从: {}", get_emu_name(branch), url);

    // 使用 aria2 下载
    debug!("创建 aria2 下载任务");
    let aria2 = get_aria2_manager().await?;
    let options = Aria2DownloadOptions {
        use_github_mirror: true,
        ..Default::default()
    };

    // 添加下载任务并获取 GID
    let gid = aria2.download(&url, options).await?;

    // 保存当前下载的 GID（用于取消功能）
    *CURRENT_DOWNLOAD_GID.write() = Some(gid.clone());

    info!("下载任务已添加，GID: {}", gid);
    debug!("开始轮询下载进度");

    // 轮询下载进度
    let poll_interval = Duration::from_millis(500);
    let mut poll_count = 0;
    loop {
        tokio::time::sleep(poll_interval).await;

        let progress = match aria2.get_download_progress(&gid).await {
            Ok(p) => p,
            Err(e) => {
                warn!("获取下载进度失败 [GID: {}]: {}", gid, e);
                *CURRENT_DOWNLOAD_GID.write() = None;
                return Err(e);
            }
        };

        poll_count += 1;
        // 每 10 次轮询打印一次 debug 日志（避免日志过多）
        if poll_count % 10 == 0 {
            debug!(
                "下载进度 [GID: {}]: {:.1}%, 速度: {}, 状态: {:?}",
                gid, progress.percentage, progress.speed_string(), progress.status
            );
        }

        on_progress(progress.clone());

        match progress.status {
            crate::services::aria2::Aria2DownloadStatus::Complete => {
                info!("下载完成 [GID: {}]", gid);
                // 从 aria2 获取实际文件路径
                let status = aria2.get_download_status(&gid).await?;
                let path = status
                    .files
                    .first()
                    .map(|f| PathBuf::from(&f.path))
                    .ok_or_else(|| {
                        warn!("无法获取下载文件路径");
                        AppError::Aria2("无法获取下载文件路径".to_string())
                    })?;

                // 清除 GID
                *CURRENT_DOWNLOAD_GID.write() = None;

                info!("下载完成: {}", path.display());
                debug!("下载文件大小: {} bytes", progress.total);
                return Ok(path);
            }
            crate::services::aria2::Aria2DownloadStatus::Error => {
                warn!("下载失败 [GID: {}]", gid);
                *CURRENT_DOWNLOAD_GID.write() = None;

                // 获取详细错误信息
                let error_message = match aria2.get_download_status(&gid).await {
                    Ok(status) => {
                        let error_code = status.error_code.as_deref().unwrap_or("未知");
                        let error_msg = status.error_message.as_deref().unwrap_or("未知错误");
                        debug!("下载失败详情: 错误码={}, 错误信息={}", error_code, error_msg);
                        format!("下载失败 (错误码: {}): {}", error_code, error_msg)
                    }
                    Err(_) => "下载失败".to_string(),
                };

                return Err(AppError::Aria2(error_message));
            }
            crate::services::aria2::Aria2DownloadStatus::Removed => {
                info!("下载已取消 [GID: {}]", gid);
                *CURRENT_DOWNLOAD_GID.write() = None;
                return Err(AppError::Aria2("下载已取消".to_string()));
            }
            _ => continue,
        }
    }
}

/// 解压 Yuzu 安装包
///
/// # 参数
/// * `package_path` - 压缩包路径
/// * `target_dir` - 解压目标目录，如果为 None 则使用临时目录
///
/// # 返回
/// 解压后的目录路径
pub fn unzip_yuzu(package_path: &Path, target_dir: Option<&Path>) -> AppResult<PathBuf> {
    info!("解压 Yuzu 文件: {}", package_path.display());
    debug!("解压包大小: {} bytes", package_path.metadata().map(|m| m.len()).unwrap_or(0));

    let extract_dir = target_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir());

    debug!("解压目标目录: {}", extract_dir.display());

    // 解压文件
    uncompress(package_path, &extract_dir, false)?;

    info!("解压完成: {}", extract_dir.display());
    Ok(extract_dir)
}

/// 安装 Eden 模拟器
///
/// # 参数
/// * `target_version` - 目标版本
/// * `on_event` - 事件回调
pub async fn install_eden<F>(target_version: &str, on_event: F) -> AppResult<()>
where
    F: Fn(ProgressEvent) + Send + Sync + 'static + Clone,
{
    info!("开始安装 Eden 版本: {}", target_version);

    let (yuzu_path, auto_delete) = {
        let config = get_config();
        (
            PathBuf::from(&config.yuzu.yuzu_path),
            config.setting.download.auto_delete_after_install,
        )
    };

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
        }
    });
    let _release_info = match get_yuzu_release_info_by_version(target_version, "eden").await {
        Ok(info) => {
            if info.tag_name.is_empty() {
                let err_msg = format!("未找到 Eden 版本: {}", target_version);
                on_event(ProgressEvent::StepUpdate {
                    step: ProgressStep {
                        id: "fetch_version".to_string(),
                        title: "获取版本信息".to_string(),
                        status: ProgressStatus::Error,
                        step_type: "normal".to_string(),
                        progress: 0.0,
                        download_speed: "".to_string(),
                        eta: "".to_string(),
                        error: Some(err_msg.clone()),
                    }
                });
                return Err(AppError::Emulator(err_msg));
            }
            info
        }
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
        }
    });

    // 下载
    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "download".to_string(),
            title: format!("下载 {}", "Eden"),
            status: ProgressStatus::Running,
            step_type: "download".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });
    let on_event_clone = on_event.clone();
    let package_path = match download_yuzu(target_version, "eden", move |progress| {
         on_event_clone(ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "download".to_string(),
                title: format!("下载 {}", "Eden"),
                status: ProgressStatus::Running,
                step_type: "download".to_string(),
                progress: progress.percentage,
                download_speed: progress.speed_string(),
                eta: progress.eta_string(),
                error: None,
            }
         });
    }).await {
        Ok(path) => path,
        Err(e) => {
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "download".to_string(),
                    title: format!("下载 {}", "Eden"),
                    status: ProgressStatus::Error,
                    step_type: "download".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: Some(e.to_string()),
                }
            });
            return Err(e);
        }
    };
    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "download".to_string(),
            title: format!("下载 {}", "Eden"),
            status: ProgressStatus::Success,
            step_type: "download".to_string(),
            progress: 100.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
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
        }
    });
    let tmp_dir = std::env::temp_dir().join("eden-install");
    if tmp_dir.exists() {
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
            }
        });
        return Err(e.into());
    }

    if let Err(e) = unzip_yuzu(&package_path, Some(&tmp_dir)) {
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
        }
    });
    // 复制文件
    if let Err(e) = copy_back_yuzu_files(&tmp_dir, &yuzu_path) {
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
            }
        });
        return Err(e);
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
        }
    });

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
        }
    });
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
            }
        });
    }

    // 如果配置了自动删除，删除下载文件
    if auto_delete {
        let _ = std::fs::remove_file(&package_path);
    }

    Ok(())
}

/// 安装 Citron 模拟器
///
/// # 参数
/// * `target_version` - 目标版本
/// * `on_event` - 事件回调
pub async fn install_citron<F>(target_version: &str, on_event: F) -> AppResult<()>
where
    F: Fn(ProgressEvent) + Send + Sync + 'static + Clone,
{
    info!("开始安装 Citron 版本: {}", target_version);

    let (yuzu_path, auto_delete) = {
        let config = get_config();
        (
            PathBuf::from(&config.yuzu.yuzu_path),
            config.setting.download.auto_delete_after_install,
        )
    };

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
        }
    });
    let _release_info = match get_yuzu_release_info_by_version(target_version, "citron").await {
        Ok(info) => {
            if info.tag_name.is_empty() {
                let err_msg = format!("未找到 Citron 版本: {}", target_version);
                on_event(ProgressEvent::StepUpdate {
                    step: ProgressStep {
                        id: "fetch_version".to_string(),
                        title: "获取版本信息".to_string(),
                        status: ProgressStatus::Error,
                        step_type: "normal".to_string(),
                        progress: 0.0,
                        download_speed: "".to_string(),
                        eta: "".to_string(),
                        error: Some(err_msg.clone()),
                    }
                });
                return Err(AppError::Emulator(err_msg));
            }
            info
        }
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
        }
    });

    // 下载
    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "download".to_string(),
            title: format!("下载 {}", "Citron"),
            status: ProgressStatus::Running,
            step_type: "download".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });
    let on_event_clone = on_event.clone();
    let package_path = match download_yuzu(target_version, "citron", move |progress| {
         on_event_clone(ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "download".to_string(),
                title: format!("下载 {}", "Citron"),
                status: ProgressStatus::Running,
                step_type: "download".to_string(),
                progress: progress.percentage,
                download_speed: progress.speed_string(),
                eta: progress.eta_string(),
                error: None,
            }
         });
    }).await {
        Ok(path) => path,
        Err(e) => {
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "download".to_string(),
                    title: format!("下载 {}", "Citron"),
                    status: ProgressStatus::Error,
                    step_type: "download".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: Some(e.to_string()),
                }
            });
            return Err(e);
        }
    };
    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "download".to_string(),
            title: format!("下载 {}", "Citron"),
            status: ProgressStatus::Success,
            step_type: "download".to_string(),
            progress: 100.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
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
        }
    });
    let tmp_dir = std::env::temp_dir().join("citron-install");
    if tmp_dir.exists() {
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
            }
        });
        return Err(e.into());
    }

    if let Err(e) = unzip_yuzu(&package_path, Some(&tmp_dir)) {
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
        }
    });

    // Citron 解压后有一个顶层目录，需要进入
    let mut release_dir = tmp_dir.clone();
    match std::fs::read_dir(&tmp_dir) {
        Ok(mut entries) => {
            if let Some(first_entry) = entries.next() {
                match first_entry {
                    Ok(entry) => {
                        if entry.path().is_dir() {
                            release_dir = entry.path();
                        }
                    }
                    Err(e) => {
                        on_event(ProgressEvent::StepUpdate {
                            step: ProgressStep {
                                id: "extract".to_string(),
                                title: "解压文件".to_string(),
                                status: ProgressStatus::Error,
                                step_type: "normal".to_string(),
                                progress: 0.0,
                                download_speed: "".to_string(),
                                eta: "".to_string(),
                                error: Some(format!("读取解压目录失败: {}", e)),
                            }
                        });
                        return Err(e.into());
                    }
                }
            }
        }
        Err(e) => {
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "extract".to_string(),
                    title: "解压文件".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: Some(format!("读取解压目录失败: {}", e)),
                }
            });
            return Err(e.into());
        }
    }

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
        }
    });
    // 复制文件
    if let Err(e) = copy_back_yuzu_files(&release_dir, &yuzu_path) {
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
            }
        });
        return Err(e);
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
        }
    });

    // 清理临时目录
    if let Err(e) = std::fs::remove_dir_all(&tmp_dir) {
        warn!("清理临时目录失败: {}", e);
        // 不阻止安装流程
    }

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
        }
    });
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
            }
        });
    }

    // 如果配置了自动删除，删除下载文件
    if auto_delete {
        let _ = std::fs::remove_file(&package_path);
    }

    Ok(())
}

/// 复制 Yuzu 文件到目标目录
///
/// # 参数
/// * `tmp_dir` - 临时解压目录
/// * `yuzu_path` - Yuzu 安装目录
fn copy_back_yuzu_files(tmp_dir: &Path, yuzu_path: &Path) -> AppResult<()> {
    info!("复制 Yuzu 文件到: {}", yuzu_path.display());

    // 删除无用的源码包
    for entry in std::fs::read_dir(tmp_dir)? {
        let entry = entry?;
        let filename = entry.file_name();
        let filename_str = filename.to_string_lossy();
        if filename_str.starts_with("yuzu-windows-msvc-source-")
            && filename_str.ends_with(".tar.xz")
        {
            let _ = std::fs::remove_file(entry.path());
            debug!("删除无用文件: {}", filename_str);
        }
    }

    // 确保目标目录存在
    std::fs::create_dir_all(yuzu_path)?;

    // 复制文件
    copy_dir_all(tmp_dir, yuzu_path)?;

    // 等待一下确保文件系统同步
    std::thread::sleep(Duration::from_millis(500));

    // 清理临时目录
    std::fs::remove_dir_all(tmp_dir)?;

    info!("文件复制完成");
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
            std::fs::copy(&path, &dst_path)?;
        }
    }

    Ok(())
}

/// 删除所有旧的模拟器可执行文件
pub fn remove_all_executable_file() -> AppResult<()> {
    info!("删除旧模拟器可执行文件");

    let config = get_config();
    let yuzu_path = PathBuf::from(&config.yuzu.yuzu_path);

    for exe_name in DETECT_EXE_LIST {
        let exe_path = yuzu_path.join(exe_name);
        if exe_path.exists() {
            info!("删除: {}", exe_path.display());
            std::fs::remove_file(&exe_path)?;
        }
    }

    Ok(())
}

/// 安装 Yuzu/Eden/Citron
///
/// # 参数
/// * `target_version` - 目标版本
/// * `branch` - 分支 (eden, citron)
/// * `on_event` - 事件回调
pub async fn install_yuzu<F>(target_version: &str, branch: &str, on_event: F) -> AppResult<()>
where
    F: Fn(ProgressEvent) + Send + Sync + 'static + Clone,
{
    info!(
        "安装 {} 版本: {}, 分支: {}",
        get_emu_name(branch),
        target_version,
        branch
    );

    // 检查当前版本 (在异步调用之前获取所有需要的配置值)
    let (current_version, yuzu_path, rename_to_cemu, _auto_delete) = {
        let config = get_config();
        (
            config.yuzu.yuzu_version.clone(),
            PathBuf::from(&config.yuzu.yuzu_path),
            config.setting.other.rename_yuzu_to_cemu,
            config.setting.download.auto_delete_after_install,
        )
    };

    if let Some(ref cv) = current_version {
        let current_branch = {
            let config = get_config();
            config.yuzu.branch.clone()
        };
        if cv == target_version && current_branch == branch {
            warn!("当前已是目标版本，跳过安装");
            // 更新第一个步骤，告知用户跳过原因
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "fetch_version".to_string(),
                    title: format!("当前已是目标版本 {} ({}),跳过安装", target_version, get_emu_name(branch)),
                    status: ProgressStatus::Success,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: None,
                }
            });
            // 将其他步骤标记为取消
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "download".to_string(),
                    title: format!("下载 {}", get_emu_name(branch)),
                    status: ProgressStatus::Cancelled,
                    step_type: "download".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: None,
                }
            });
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "extract".to_string(),
                    title: "解压文件".to_string(),
                    status: ProgressStatus::Cancelled,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: None,
                }
            });
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "install".to_string(),
                    title: "安装文件".to_string(),
                    status: ProgressStatus::Cancelled,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: None,
                }
            });
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "check_env".to_string(),
                    title: "检查运行环境".to_string(),
                    status: ProgressStatus::Cancelled,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: None,
                }
            });
            return Ok(());
        }
    }

    // 删除旧的可执行文件
    remove_all_executable_file()?;

    // 根据分支安装
    match branch {
        "eden" => install_eden(target_version, on_event).await?,
        "citron" => install_citron(target_version, on_event).await?,
        _ => {
            return Err(AppError::Emulator(
                "只支持安装 eden 和 citron 分支".to_string(),
            ))
        }
    }

    // 确保安装目录存在
    std::fs::create_dir_all(&yuzu_path)?;

    // 如果需要重命名为 cemu
    if rename_to_cemu {
        let exe_path = get_yuzu_exe_path();
        if exe_path.exists() {
            let cemu_path = yuzu_path.join("cemu.exe");
            std::fs::rename(&exe_path, &cemu_path)?;
            info!("重命名 {} 为 cemu.exe", exe_path.display());
        }
    }

    // 更新配置
    {
        let mut cfg = CONFIG.write();
        cfg.yuzu.yuzu_version = Some(target_version.to_string());
        cfg.yuzu.branch = branch.to_string();
        cfg.save()?;
    }

    info!("{} [{}] 安装成功", get_emu_name(branch), target_version);
    Ok(())
}

/// 获取 Yuzu 可执行文件路径
pub fn get_yuzu_exe_path() -> PathBuf {
    let config = get_config();
    let yuzu_path = PathBuf::from(&config.yuzu.yuzu_path);

    // 如果配置了重命名为 cemu，或者 yuzu.exe 不存在但 cemu.exe 存在
    if (config.setting.other.rename_yuzu_to_cemu || !yuzu_path.join("yuzu.exe").exists())
        && yuzu_path.join("cemu.exe").exists()
    {
        return yuzu_path.join("cemu.exe");
    }

    // 按优先级查找可执行文件
    for exe_name in DETECT_EXE_LIST {
        let exe_path = yuzu_path.join(exe_name);
        if exe_path.exists() {
            return exe_path;
        }
    }

    // 默认返回 yuzu.exe
    yuzu_path.join("yuzu.exe")
}

/// 检测 Yuzu 版本（通过启动程序并读取窗口标题）
///
/// 注意：此功能需要窗口枚举功能，在 Windows 上需要使用 Windows API
pub async fn detect_yuzu_version() -> AppResult<Option<String>> {
    info!("检测 Yuzu 版本");

    let exe_path = get_yuzu_exe_path();
    debug!("Yuzu 可执行文件路径: {}", exe_path.display());

    if !exe_path.exists() {
        warn!("未找到 Yuzu 程序: {}", exe_path.display());
        return Ok(None);
    }

    // TODO: 检查是否已经在运行
    // let instances = find_all_instances(&exe_path);
    // if !instances.is_empty() {
    //     return Err(AppError::Process("Yuzu 正在运行，请先关闭".to_string()));
    // }

    // 启动程序
    info!("启动 Yuzu: {}", exe_path.display());
    debug!("使用参数启动以检测版本");
    let mut child = Command::new(&exe_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    debug!("Yuzu 进程 ID: {}", child.id());
    debug!("等待窗口创建...");

    // 等待窗口创建
    tokio::time::sleep(Duration::from_secs(2)).await;

    // TODO: 枚举窗口标题检测版本
    // Windows 上需要使用 EnumWindows API
    // 示例窗口标题:
    // - "yuzu Early Access 1234"
    // - "Eden | v1.0.0"
    // - "citron | v1.0.0"

    let mut version: Option<String> = None;
    let mut branch: Option<String> = None;

    #[cfg(windows)]
    {
        use std::sync::{Arc, Mutex};
        use windows::Win32::Foundation::{HWND, LPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            EnumWindows, GetWindowTextW, IsWindowVisible,
        };

        let version_data = Arc::new(Mutex::new((None, None)));
        let version_data_clone = version_data.clone();

        unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> windows::Win32::Foundation::BOOL {
            if IsWindowVisible(hwnd).as_bool() {
                let mut text: [u16; 512] = [0; 512];
                let len = GetWindowTextW(hwnd, &mut text);

                if len > 0 {
                    let window_title = String::from_utf16_lossy(&text[..len as usize]);

                    let data_ptr = lparam.0 as *const Arc<Mutex<(Option<String>, Option<String>)>>;
                    let data = &*data_ptr;

                    if window_title.starts_with("yuzu ") {
                        let mut guard = data.lock().unwrap();
                        if window_title.starts_with("yuzu Early Access ") {
                            guard.0 = Some(window_title[18..].to_string());
                            guard.1 = Some("ea".to_string());
                        } else {
                            guard.0 = Some(window_title[5..].to_string());
                            guard.1 = Some("mainline".to_string());
                        }
                        return windows::Win32::Foundation::BOOL(0); // Stop enumeration
                    } else if window_title.starts_with("Eden | ") {
                        let mut guard = data.lock().unwrap();
                        // 提取版本号，去掉可能存在的 MSVC 版本信息
                        // 例如: "Eden | v0.0.4-rc3 | MSVC 19.44.35219.0" -> "v0.0.4-rc3"
                        let version_part = &window_title[7..];
                        let version = if let Some(pipe_pos) = version_part.find(" | ") {
                            version_part[..pipe_pos].to_string()
                        } else {
                            version_part.to_string()
                        };
                        guard.0 = Some(version);
                        guard.1 = Some("eden".to_string());
                        return windows::Win32::Foundation::BOOL(0);
                    } else if window_title.starts_with("citron | ") {
                        let mut guard = data.lock().unwrap();
                        // 提取版本号，去掉可能存在的 MSVC 版本信息
                        let version_part = &window_title[9..];
                        let version = if let Some(pipe_pos) = version_part.find(" | ") {
                            version_part[..pipe_pos].to_string()
                        } else {
                            version_part.to_string()
                        };
                        guard.0 = Some(version);
                        guard.1 = Some("citron".to_string());
                        return windows::Win32::Foundation::BOOL(0);
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
    debug!("结束 Yuzu 进程");
    let _ = child.kill();
    let _ = child.wait();

    // 更新配置
    if let Some(ref v) = version {
        info!("检测到版本: {}, 分支: {:?}", v, branch);
        debug!("更新配置文件");

        let mut cfg = CONFIG.write();
        cfg.yuzu.yuzu_version = Some(v.clone());
        if let Some(b) = branch {
            cfg.yuzu.branch = b;
        }
        cfg.save()?;
        debug!("配置文件已保存");
    } else {
        warn!("未能检测到 Yuzu 版本");
        debug!("可能的原因: 窗口标题不匹配或窗口创建延迟过长");
    }

    Ok(version)
}

/// 启动 Yuzu
pub fn start_yuzu() -> AppResult<()> {
    let exe_path = get_yuzu_exe_path();

    debug!("检查 Yuzu 可执行文件: {}", exe_path.display());

    if !exe_path.exists() {
        warn!("Yuzu 可执行文件不存在: {}", exe_path.display());
        return Err(AppError::FileNotFound(exe_path.display().to_string()));
    }

    info!("启动 Yuzu: {}", exe_path.display());

    Command::new(&exe_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    Ok(())
}

/// 获取 Yuzu 用户数据目录
pub fn get_yuzu_user_path() -> PathBuf {
    let config = get_config();
    let yuzu_path = PathBuf::from(&config.yuzu.yuzu_path);

    // 优先使用本地 user 目录
    let local_user = yuzu_path.join("user");
    if local_user.exists() {
        return local_user;
    }

    // 检查 AppData 目录
    if let Ok(appdata) = std::env::var("APPDATA") {
        let appdata_path = PathBuf::from(appdata);

        // 按优先级检查
        for name in &["yuzu", "eden", "citron"] {
            let path = appdata_path.join(name);
            if path.exists() {
                return path;
            }
        }
    }

    // 默认返回本地 user 目录
    local_user
}

/// 打开 Yuzu keys 文件夹
pub fn open_yuzu_keys_folder() -> AppResult<()> {
    let keys_path = get_yuzu_user_path().join("keys");
    std::fs::create_dir_all(&keys_path)?;

    // 创建提示文件
    let hint_file = keys_path.join("把prod.keys放当前目录.txt");
    std::fs::write(&hint_file, "")?;

    info!("打开 keys 目录: {}", keys_path.display());

    #[cfg(windows)]
    {
        Command::new("explorer")
            .arg(keys_path.to_string_lossy().to_string())
            .spawn()?;
    }

    #[cfg(not(windows))]
    {
        Command::new("xdg-open")
            .arg(keys_path.to_string_lossy().to_string())
            .spawn()?;
    }

    Ok(())
}

/// 读取 Yuzu qt-config.ini 中的 Data Storage 配置
fn get_yuzu_data_storage_config(user_path: &Path) -> Option<std::collections::HashMap<String, String>> {
    let config_path = user_path.join("config").join("qt-config.ini");

    if !config_path.exists() {
        return None;
    }

    // 手动解析 INI 文件
    match std::fs::read_to_string(&config_path) {
        Ok(content) => {
            let mut result = std::collections::HashMap::new();
            let mut in_data_storage = false;

            for line in content.lines() {
                let line = line.trim();

                // 检查是否进入 [Data%20Storage] section
                if line == "[Data%20Storage]" {
                    in_data_storage = true;
                    continue;
                }

                // 如果遇到新的 section，退出
                if line.starts_with('[') && line.ends_with(']') {
                    in_data_storage = false;
                    continue;
                }

                // 如果在 Data%20Storage section 中，解析键值对
                if in_data_storage {
                    if let Some(pos) = line.find('=') {
                        let key = line[..pos].trim();
                        let value = line[pos + 1..].trim();
                        result.insert(key.to_string(), value.to_string());
                    }
                }
            }

            if result.is_empty() {
                None
            } else {
                Some(result)
            }
        }
        Err(e) => {
            warn!("读取 qt-config.ini 失败: {}", e);
            None
        }
    }
}

/// 解码 Yuzu 配置文件中的路径
///
/// Yuzu 配置文件中的路径可能包含 Unicode 转义序列，如 \x65b0 这样的格式
/// 需要将其转换为正常的 Unicode 字符
fn decode_yuzu_path(raw_path: &str) -> String {
    // 使用正则表达式将 \x 替换为 \u
    let re = regex::Regex::new(r"\\x([0-9a-fA-F]{4})").unwrap();
    let converted = re.replace_all(raw_path, r"\u$1");

    // 解码 unicode-escape
    // Rust 中处理 unicode-escape 需要手动解析
    // 简化处理：直接尝试解析 \uXXXX 格式
    let mut result = String::new();
    let mut chars = converted.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(&next_ch) = chars.peek() {
                if next_ch == 'u' {
                    chars.next(); // 跳过 'u'

                    // 读取 4 位十六进制数字
                    let hex: String = chars.by_ref().take(4).collect();

                    if hex.len() == 4 {
                        if let Ok(code) = u32::from_str_radix(&hex, 16) {
                            if let Some(unicode_char) = char::from_u32(code) {
                                result.push(unicode_char);
                                continue;
                            }
                        }
                    }

                    // 如果解析失败，保留原样
                    result.push('\\');
                    result.push('u');
                    result.push_str(&hex);
                } else {
                    result.push(ch);
                }
            } else {
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// 获取 Yuzu NAND 路径
pub fn get_yuzu_nand_path() -> PathBuf {
    let user_path = get_yuzu_user_path();
    let mut nand_path = user_path.join("nand");

    // 尝试从 qt-config.ini 读取自定义路径
    if let Some(config) = get_yuzu_data_storage_config(&user_path) {
        if let Some(path_str) = config.get("nand_directory") {
            let decoded_path = decode_yuzu_path(path_str);
            nand_path = PathBuf::from(decoded_path);
            info!("从配置文件读取 NAND 路径: {}", nand_path.display());
        }
    }

    nand_path
}

/// 获取 Yuzu load 路径（用于 mods/cheats）
pub fn get_yuzu_load_path() -> PathBuf {
    let user_path = get_yuzu_user_path();
    let mut load_path = user_path.join("load");

    // 尝试从 qt-config.ini 读取自定义路径
    if let Some(config) = get_yuzu_data_storage_config(&user_path) {
        if let Some(path_str) = config.get("load_directory") {
            // 检查是否包含 Unicode 转义序列
            let decoded_path = if path_str.contains("\\u") || path_str.contains("\\x") {
                decode_yuzu_path(path_str)
            } else {
                path_str.to_string()
            };
            load_path = PathBuf::from(decoded_path);
            info!("从配置文件读取 Load 路径: {}", load_path.display());
        }
    }

    load_path
}

/// 更新 Yuzu 安装路径
pub fn update_yuzu_path(new_yuzu_path: &str) -> AppResult<()> {
    use crate::models::storage::{add_yuzu_history, get_storage, STORAGE};

    let new_path = PathBuf::from(new_yuzu_path);

    // 创建目录
    if !new_path.exists() {
        info!("创建目录: {}", new_path.display());
        std::fs::create_dir_all(&new_path)?;
    }

    let config = get_config();
    let old_path = PathBuf::from(&config.yuzu.yuzu_path);

    // 检查是否相同
    if new_path == old_path {
        info!("路径相同，跳过更新");
        return Ok(());
    }

    // 保存旧配置到历史记录
    add_yuzu_history(config.yuzu.clone(), true)?;
    info!("已保存旧 Yuzu 配置到历史记录: {}", old_path.display());

    // 从历史记录中获取新路径的配置，如果不存在则使用当前配置
    let new_config = {
        let storage = get_storage();
        let new_path_str = new_path
            .canonicalize()
            .unwrap_or(new_path.clone())
            .to_string_lossy()
            .to_string();

        if let Some(historical_config) = storage.yuzu_history.get(&new_path_str) {
            info!("从历史记录中恢复 Yuzu 配置");
            let mut cfg = historical_config.clone();
            cfg.yuzu_path = new_path.clone();
            cfg
        } else {
            info!("历史记录中没有该路径，使用默认配置");
            let mut cfg = crate::config::YuzuConfig::default();
            cfg.yuzu_path = new_path.clone();
            cfg
        }
    };

    // 更新配置
    {
        let mut cfg = CONFIG.write();
        cfg.yuzu = new_config.clone();
        cfg.save()?;
    }

    // 如果新路径不在历史记录中，保存到历史记录
    {
        let storage = STORAGE.read();
        let new_path_str = new_path
            .canonicalize()
            .unwrap_or(new_path.clone())
            .to_string_lossy()
            .to_string();

        if !storage.yuzu_history.contains_key(&new_path_str) {
            drop(storage); // 释放读锁
            add_yuzu_history(new_config, true)?;
            info!("已保存新 Yuzu 配置到历史记录");
        }
    }

    info!("Yuzu 路径已更新: {}", new_path.display());
    Ok(())
}

/// 获取变更日志
pub async fn get_yuzu_change_logs() -> AppResult<String> {
    // 直接获取配置的克隆
    let branch = get_config().yuzu.branch.clone();

    let changelog = get_latest_change_log(&branch).await?;
    Ok(changelog)
}

/// 安装固件到 Yuzu
///
/// # 参数
/// * `firmware_version` - 固件版本，None 表示最新版本
/// * `on_event` - 事件回调
pub async fn install_firmware_to_yuzu<F>(
    firmware_version: Option<&str>,
    on_event: F,
) -> AppResult<()>
where
    F: Fn(ProgressEvent) + Send + Sync + 'static + Clone,
{
    let config = get_config();

    // 检查是否已安装此版本
    if let Some(ref version) = firmware_version {
        if let Some(ref current_firmware) = config.yuzu.yuzu_firmware {
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

    // 获取固件路径
    let firmware_path = crate::services::firmware::get_yuzu_firmware_path();

    info!("开始安装固件到 Yuzu，路径: {}", firmware_path.display());

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
        },
    ];

    on_event(ProgressEvent::Started { steps });

    // 调用固件服务进行安装
    let version_to_install = firmware_version.unwrap_or_else(|| {
        // 如果没有指定版本，需要获取最新版本
        // 这里可以从 firmware_infos 中获取第一个版本
        "latest"
    });

    let new_version = crate::services::firmware::install_firmware(
        version_to_install,
        &firmware_path,
        on_event.clone(),
    ).await?;

    // 更新配置
    {
        let mut cfg = CONFIG.write();
        cfg.yuzu.yuzu_firmware = Some(new_version.clone());
        cfg.save()?;
    }

    info!("固件 {} 安装成功到 Yuzu", new_version);
    Ok(())
}

/// 取消当前的 Yuzu 下载
pub async fn cancel_yuzu_download() -> AppResult<()> {
    let gid = {
        let gid_lock = CURRENT_DOWNLOAD_GID.read();
        gid_lock.clone()
    };

    if let Some(gid) = gid {
        info!("取消下载任务: {}", gid);
        let aria2 = get_aria2_manager().await?;
        aria2.cancel(&gid).await?;

        // 清除 GID
        *CURRENT_DOWNLOAD_GID.write() = None;

        info!("下载已取消");
        Ok(())
    } else {
        Err(AppError::Aria2("没有正在进行的下载任务".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_emu_name() {
        assert_eq!(get_emu_name("eden"), "Eden");
        assert_eq!(get_emu_name("citron"), "Citron");
        assert_eq!(get_emu_name("unknown"), "Yuzu");
    }

    #[test]
    fn test_get_yuzu_exe_path() {
        // 测试需要配置环境
        // 这里只是确保函数不会 panic
        let _path = get_yuzu_exe_path();
    }
}

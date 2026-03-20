//! Yuzu 系列模拟器管理服务
//!
//! 提供 Eden 安装、以及历史 Yuzu/Citron 安装目录的版本检测等功能

use crate::config::{get_config, CONFIG};
use crate::error::{AppError, AppResult};
use crate::models::{ProgressEvent, ProgressStatus, ProgressStep}; // Import models
use crate::repositories::yuzu::{get_latest_change_log, get_yuzu_release_info_by_version};
use crate::services::downloader::{get_download_manager, DownloadOptions, DownloadProgress};
#[cfg(not(target_os = "macos"))]
use crate::services::msvc::check_and_install_msvc;
use crate::services::network::get_download_source_name;
use crate::utils::archive::uncompress;
use crate::utils::spawn_blocking_io;
#[cfg(target_os = "macos")]
use crate::utils::{finalize_macos_app_install, get_macos_bundle_executable_path};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
#[cfg(not(target_os = "macos"))]
use std::time::Duration;
use tracing::{debug, info, warn};

/// 支持的模拟器可执行文件/应用列表
#[cfg(target_os = "macos")]
const DETECT_EXE_LIST: &[&str] = &["Eden.app", "Citron.app", "yuzu.app"];

#[cfg(not(target_os = "macos"))]
const DETECT_EXE_LIST: &[&str] = &["yuzu.exe", "eden.exe", "citron.exe", "suzu.exe", "cemu.exe"];

/// 支持下载的分支
const DOWNLOAD_AVAILABLE_BRANCH: &[&str] = &["eden"];

/// 获取模拟器名称
pub fn get_emu_name(branch: &str) -> &'static str {
    match branch {
        "eden" => "Eden",
        "citron" => "Citron",
        _ => "Yuzu",
    }
}

fn unsupported_install_branch_error(branch: &str) -> AppError {
    if branch == "citron" {
        AppError::Unsupported("Citron 分支已不再支持在线版本、下载和安装".to_string())
    } else {
        AppError::InvalidArgument(format!("不支持的分支: {}，当前仅支持 eden", branch))
    }
}

fn normalize_yuzu_branch(branch: &str) -> Option<&'static str> {
    match branch {
        "eden" => Some("eden"),
        "citron" => Some("citron"),
        "mainline" | "ea" | "yuzu" => Some("yuzu"),
        _ => None,
    }
}

fn preferred_yuzu_user_dir_names(branch: &str) -> Vec<&'static str> {
    let mut names = Vec::new();

    if let Some(primary) = normalize_yuzu_branch(branch) {
        names.push(primary);
    }

    for fallback in ["eden", "yuzu", "citron"] {
        if !names.contains(&fallback) {
            names.push(fallback);
        }
    }

    names
}

#[cfg(not(target_os = "windows"))]
fn default_yuzu_user_dir_name(branch: &str) -> &'static str {
    normalize_yuzu_branch(branch).unwrap_or("eden")
}

fn find_existing_yuzu_user_dir(base_dir: &Path, branch: &str) -> Option<PathBuf> {
    preferred_yuzu_user_dir_names(branch)
        .into_iter()
        .map(|name| base_dir.join(name))
        .find(|path| path.exists())
}

#[cfg(target_os = "macos")]
fn get_macos_bundle_spec(branch: &str) -> Option<(&'static str, &'static str)> {
    match normalize_yuzu_branch(branch) {
        Some("eden") => Some(("Eden.app", "Eden")),
        Some("citron") => Some(("Citron.app", "Citron")),
        Some("yuzu") => Some(("yuzu.app", "yuzu")),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn macos_bundle_search_order(preferred_branch: &str) -> Vec<&'static str> {
    let mut order = Vec::new();

    if let Some(branch) = normalize_yuzu_branch(preferred_branch) {
        order.push(branch);
    }

    for fallback in ["eden", "yuzu", "citron"] {
        if !order.contains(&fallback) {
            order.push(fallback);
        }
    }

    order
}

#[cfg(target_os = "macos")]
fn resolve_existing_macos_bundle(yuzu_path: &Path, preferred_branch: &str) -> Option<PathBuf> {
    for branch in macos_bundle_search_order(preferred_branch) {
        let Some((app_name, _)) = get_macos_bundle_spec(branch) else {
            continue;
        };

        let app_path = yuzu_path.join(app_name);
        if app_path.exists() {
            return Some(app_path);
        }
    }

    None
}

#[cfg(target_os = "macos")]
fn get_default_macos_bundle_path(yuzu_path: &Path, preferred_branch: &str) -> PathBuf {
    let (app_name, _) = get_macos_bundle_spec(preferred_branch).unwrap_or(("Eden.app", "Eden"));
    yuzu_path.join(app_name)
}

#[cfg(target_os = "macos")]
fn get_default_macos_executable_name(preferred_branch: &str) -> &'static str {
    get_macos_bundle_spec(preferred_branch)
        .map(|(_, executable_name)| executable_name)
        .unwrap_or("Eden")
}

#[cfg(target_os = "macos")]
fn infer_branch_from_macos_app_name(app_name: &str) -> Option<String> {
    match app_name {
        "Eden.app" => Some("eden".to_string()),
        "Citron.app" => Some("citron".to_string()),
        "yuzu.app" => Some("mainline".to_string()),
        _ => None,
    }
}

/// 选择 macOS 下载资源
fn select_macos_asset(
    release_info: &crate::models::release::ReleaseInfo,
    branch: &str,
) -> Option<String> {
    if branch != "eden" {
        return None;
    }

    for asset in &release_info.assets {
        let name = &asset.name;

        // 排除其他平台的文件
        let name_lower = name.to_lowercase();
        if name_lower.contains("windows")
            || name_lower.contains("linux")
            || name_lower.contains("android")
        {
            continue;
        }

        // Eden: 匹配 macOS + .tar.gz
        if name.contains("macOS") && name_lower.ends_with(".tar.gz") {
            debug!("选择 Eden macOS 资源: {}", name);
            return Some(asset.download_url.clone());
        }
    }
    None
}

/// 下载 Eden
///
/// # 参数
/// * `target_version` - 目标版本
/// * `branch` - 分支 (eden)
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
    F: Fn(DownloadProgress) + Send + 'static,
{
    // 检查分支是否支持
    if !DOWNLOAD_AVAILABLE_BRANCH.contains(&branch) {
        warn!(
            "不支持的分支: {}, 支持的分支: {:?}",
            branch, DOWNLOAD_AVAILABLE_BRANCH
        );
        return Err(unsupported_install_branch_error(branch));
    }

    info!("开始下载 {} 版本: {}", get_emu_name(branch), target_version);

    // 获取版本信息
    debug!(
        "获取 {} 版本 {} 的发布信息",
        get_emu_name(branch),
        target_version
    );
    let release_info = get_yuzu_release_info_by_version(target_version, branch).await?;

    if release_info.tag_name.is_empty() {
        warn!("未找到 {} 版本: {}", get_emu_name(branch), target_version);
        return Err(AppError::Emulator(format!(
            "未找到 {} 版本: {}",
            get_emu_name(branch),
            target_version
        )));
    }

    debug!(
        "找到版本: {}, 资源数量: {}",
        release_info.tag_name,
        release_info.assets.len()
    );

    // 查找下载 URL - 根据平台选择
    let download_url: Option<String> = if cfg!(target_os = "macos") {
        select_macos_asset(&release_info, branch)
    } else {
        // Windows 筛选逻辑
        let mut url = None;
        for asset in &release_info.assets {
            let name = asset.name.to_lowercase();
            debug!("检查资源: {} (size: {})", asset.name, asset.size);

            if name.ends_with(".7z") {
                url = Some(asset.download_url.clone());
                debug!("选择 .7z 资源: {}", asset.name);
                break;
            } else if name.starts_with("windows-yuzu-ea-") && name.ends_with(".zip") {
                url = Some(asset.download_url.clone());
                debug!("选择 Yuzu EA .zip 资源: {}", asset.name);
                break;
            } else if name.starts_with("eden-windows-") && name.ends_with(".zip") {
                url = Some(asset.download_url.clone());
                debug!("选择 Eden .zip 资源: {}", asset.name);
                break;
            }
        }
        url
    };

    let url = download_url.ok_or_else(|| {
        warn!("无法找到合适的下载资源");
        AppError::Emulator(format!("无法获取 {} 下载链接", get_emu_name(branch)))
    })?;

    info!("开始下载 {}，来源：{}", get_emu_name(branch), url);

    // 使用统一下载接口
    debug!("创建下载任务");
    let download_manager = get_download_manager().await?;
    let options = DownloadOptions {
        use_github_mirror: url.contains("github.com"),
        ..Default::default()
    };

    // 下载并等待完成
    let result = download_manager
        .download_and_wait(&url, options, Box::new(on_progress))
        .await?;

    info!("下载完成: {}", result.path.display());
    debug!("下载文件大小：{} 字节", result.size);

    Ok(result.path)
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
    debug!(
        "解压包大小: {} bytes",
        package_path.metadata().map(|m| m.len()).unwrap_or(0)
    );

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
            downloaded_size: None,
            total_size: None,
            error: None,
            download_source: None,
        },
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
                        downloaded_size: None,
                        total_size: None,
                        error: Some(err_msg.clone()),
                        download_source: None,
                    },
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
                    downloaded_size: None,
                    total_size: None,
                    error: Some(e.to_string()),
                    download_source: None,
                },
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
            downloaded_size: None,
            total_size: None,
            error: None,
            download_source: None,
        },
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
            downloaded_size: None,
            total_size: None,
            error: None,
            download_source: None,
        },
    });
    let download_source = get_download_source_name("https://git.eden-emu.dev");
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
                downloaded_size: Some(progress.downloaded_string()),
                total_size: Some(progress.total_string_or_unknown()),
                error: None,
                download_source: Some(download_source.clone()),
            },
        });
    })
    .await
    {
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
                    downloaded_size: None,
                    total_size: None,
                    error: Some(e.to_string()),
                    download_source: None,
                },
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
            downloaded_size: None,
            total_size: None,
            error: None,
            download_source: None,
        },
    });

    // 解压/安装 - 根据平台区分处理
    #[cfg(target_os = "macos")]
    {
        // macOS: tar.gz -> 提取 .app
        on_event(ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "install".to_string(),
                title: "安装文件".to_string(),
                status: ProgressStatus::Running,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                downloaded_size: None,
                total_size: None,
                error: None,
                download_source: None,
            },
        });

        let app_name = get_macos_bundle_spec("eden")
            .map(|(app_name, _)| app_name)
            .unwrap_or("Eden.app");
        let package_path_for_extract = package_path.clone();
        let yuzu_path_for_extract = yuzu_path.clone();
        match spawn_blocking_io("install_eden_app_bundle", move || {
            crate::utils::archive::extract_and_install_app_from_tar_gz(
                &package_path_for_extract,
                &yuzu_path_for_extract,
                app_name,
            )
        })
        .await
        {
            Ok(installed_app) => {
                let fallback_executable_name = get_macos_bundle_spec("eden")
                    .map(|(_, executable_name)| executable_name)
                    .unwrap_or("Eden");
                if let Err(error) =
                    finalize_macos_app_install(&installed_app, Some(fallback_executable_name))
                {
                    on_event(ProgressEvent::StepUpdate {
                        step: ProgressStep {
                            id: "install".to_string(),
                            title: "安装文件".to_string(),
                            status: ProgressStatus::Error,
                            step_type: "normal".to_string(),
                            progress: 0.0,
                            download_speed: "".to_string(),
                            eta: "".to_string(),
                            downloaded_size: None,
                            total_size: None,
                            error: Some(error.to_string()),
                            download_source: None,
                        },
                    });
                    return Err(error);
                }

                info!("Eden.app 已安装到: {}", installed_app.display());
                on_event(ProgressEvent::StepUpdate {
                    step: ProgressStep {
                        id: "install".to_string(),
                        title: "安装文件".to_string(),
                        status: ProgressStatus::Success,
                        step_type: "normal".to_string(),
                        progress: 0.0,
                        download_speed: "".to_string(),
                        eta: "".to_string(),
                        downloaded_size: None,
                        total_size: None,
                        error: None,
                        download_source: None,
                    },
                });
            }
            Err(e) => {
                on_event(ProgressEvent::StepUpdate {
                    step: ProgressStep {
                        id: "install".to_string(),
                        title: "安装文件".to_string(),
                        status: ProgressStatus::Error,
                        step_type: "normal".to_string(),
                        progress: 0.0,
                        download_speed: "".to_string(),
                        eta: "".to_string(),
                        downloaded_size: None,
                        total_size: None,
                        error: Some(e.to_string()),
                        download_source: None,
                    },
                });
                return Err(e);
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Windows: 现有的解压和复制逻辑
        on_event(ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "extract".to_string(),
                title: "解压文件".to_string(),
                status: ProgressStatus::Running,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                downloaded_size: None,
                total_size: None,
                error: None,
                download_source: None,
            },
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
                        downloaded_size: None,
                        total_size: None,
                        error: Some(format!("清理临时目录失败: {}", e)),
                        download_source: None,
                    },
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
                    downloaded_size: None,
                    total_size: None,
                    error: Some(format!("创建临时目录失败: {}", e)),
                    download_source: None,
                },
            });
            return Err(e.into());
        }

        let package_path_for_extract = package_path.clone();
        let tmp_dir_for_extract = tmp_dir.clone();
        if let Err(e) = spawn_blocking_io("extract_eden_package", move || {
            unzip_yuzu(&package_path_for_extract, Some(&tmp_dir_for_extract)).map(|_| ())
        })
        .await
        {
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "extract".to_string(),
                    title: "解压文件".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    downloaded_size: None,
                    total_size: None,
                    error: Some(e.to_string()),
                    download_source: None,
                },
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
                downloaded_size: None,
                total_size: None,
                error: None,
                download_source: None,
            },
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
                downloaded_size: None,
                total_size: None,
                error: None,
                download_source: None,
            },
        });
        // 复制文件
        let tmp_dir_for_install = tmp_dir.clone();
        let yuzu_path_for_install = yuzu_path.clone();
        if let Err(e) = spawn_blocking_io("install_eden_files", move || {
            copy_back_yuzu_files(&tmp_dir_for_install, &yuzu_path_for_install)
        })
        .await
        {
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "install".to_string(),
                    title: "安装文件".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    downloaded_size: None,
                    total_size: None,
                    error: Some(e.to_string()),
                    download_source: None,
                },
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
                downloaded_size: None,
                total_size: None,
                error: None,
                download_source: None,
            },
        });
    }

    // 检查运行环境 - macOS 跳过 MSVC 检查
    #[cfg(target_os = "macos")]
    {
        on_event(ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "check_env".to_string(),
                title: "检查运行环境".to_string(),
                status: ProgressStatus::Success,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                downloaded_size: None,
                total_size: None,
                error: None,
                download_source: None,
            },
        });
        // macOS 无需 MSVC，直接成功
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Windows: 执行 MSVC 检查
        on_event(ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "check_env".to_string(),
                title: "检查运行环境".to_string(),
                status: ProgressStatus::Running,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                downloaded_size: None,
                total_size: None,
                error: None,
                download_source: None,
            },
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
                    downloaded_size: None,
                    total_size: None,
                    error: Some(e.to_string()),
                    download_source: None,
                },
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
                    downloaded_size: None,
                    total_size: None,
                    error: None,
                    download_source: None,
                },
            });
        }
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
#[cfg(not(target_os = "macos"))]
fn copy_back_yuzu_files(tmp_dir: &Path, yuzu_path: &Path) -> AppResult<()> {
    info!("正在将 Yuzu 文件复制到 {}", yuzu_path.display());

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

    // 清理临时目录
    std::fs::remove_dir_all(tmp_dir)?;

    info!("文件复制完成");
    Ok(())
}

/// 递归复制目录
#[cfg(not(target_os = "macos"))]
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
/// 删除旧的模拟器（仅删除当前分支对应的）
pub fn remove_target_app(branch: &str) -> AppResult<()> {
    info!("正在删除旧的 {} 应用", branch);

    let config = get_config();
    let yuzu_path = PathBuf::from(&config.yuzu.yuzu_path);

    #[cfg(target_os = "macos")]
    {
        let Some((app_name, _)) = get_macos_bundle_spec(branch) else {
            return Ok(());
        };
        let app_path = yuzu_path.join(app_name);
        if app_path.exists() {
            info!("删除旧应用: {}", app_path.display());
            std::fs::remove_dir_all(&app_path)?;
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let exe_name = match branch {
            "eden" => "eden.exe",
            "citron" => "citron.exe",
            _ => return Ok(()),
        };
        let exe_path = yuzu_path.join(exe_name);
        if exe_path.exists() {
            info!("删除旧可执行文件: {}", exe_path.display());
            std::fs::remove_file(&exe_path)?;
        }
    }

    Ok(())
}

/// 删除所有模拟器可执行文件（已废弃，使用 remove_target_app 代替）
#[deprecated(note = "使用 remove_target_app 代替，以避免误删其他分支的应用")]
pub fn remove_all_executable_file() -> AppResult<()> {
    info!("删除旧模拟器可执行文件");

    let config = get_config();
    let yuzu_path = PathBuf::from(&config.yuzu.yuzu_path);

    #[cfg(target_os = "macos")]
    {
        for app_name in DETECT_EXE_LIST {
            let app_path = yuzu_path.join(app_name);
            if app_path.exists() {
                info!("删除: {}", app_path.display());
                std::fs::remove_dir_all(&app_path)?;
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        for exe_name in DETECT_EXE_LIST {
            let exe_path = yuzu_path.join(exe_name);
            if exe_path.exists() {
                info!("删除: {}", exe_path.display());
                std::fs::remove_file(&exe_path)?;
            }
        }
    }

    Ok(())
}

/// 安装 Eden
///
/// # 参数
/// * `target_version` - 目标版本
/// * `branch` - 分支 (eden)
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
    let (current_version, yuzu_path, _auto_delete) = {
        let config = get_config();
        (
            config.yuzu.yuzu_version.clone(),
            PathBuf::from(&config.yuzu.yuzu_path),
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
                    title: format!(
                        "当前已是目标版本 {} ({}),跳过安装",
                        target_version,
                        get_emu_name(branch)
                    ),
                    status: ProgressStatus::Success,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    downloaded_size: None,
                    total_size: None,
                    error: None,
                    download_source: None,
                },
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
                    downloaded_size: None,
                    total_size: None,
                    error: None,
                    download_source: None,
                },
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
                    downloaded_size: None,
                    total_size: None,
                    error: None,
                    download_source: None,
                },
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
                    downloaded_size: None,
                    total_size: None,
                    error: None,
                    download_source: None,
                },
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
                    downloaded_size: None,
                    total_size: None,
                    error: None,
                    download_source: None,
                },
            });
            return Ok(());
        }
    }

    // 删除旧的可执行文件/应用（只删除当前分支的）
    spawn_blocking_io("remove_target_yuzu_app", {
        let branch = branch.to_string();
        move || remove_target_app(&branch)
    })
    .await?;

    // 根据分支安装
    match branch {
        "eden" => install_eden(target_version, on_event).await?,
        _ => return Err(unsupported_install_branch_error(branch)),
    }

    // 确保安装目录存在
    let yuzu_path_for_prepare = yuzu_path.clone();
    spawn_blocking_io("prepare_yuzu_install_dir", move || {
        std::fs::create_dir_all(&yuzu_path_for_prepare)?;
        Ok(())
    })
    .await?;

    #[cfg(not(target_os = "macos"))]
    {
        let rename_to_cemu = get_config().setting.other.rename_yuzu_to_cemu;

        // 如果需要重命名为 cemu
        if rename_to_cemu {
            let exe_path = get_yuzu_exe_path();
            if exe_path.exists() {
                let cemu_path = yuzu_path.join("cemu.exe");
                let exe_path_for_log = exe_path.display().to_string();
                spawn_blocking_io("rename_yuzu_to_cemu", move || {
                    std::fs::rename(&exe_path, &cemu_path)?;
                    Ok(())
                })
                .await?;
                info!("正在将 {} 重命名为 cemu.exe", exe_path_for_log);
            }
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

    #[cfg(target_os = "macos")]
    {
        let preferred_branch = config.yuzu.branch.clone();

        if let Some(app_path) = resolve_existing_macos_bundle(&yuzu_path, &preferred_branch) {
            let fallback_name = match app_path.file_name().and_then(|name| name.to_str()) {
                Some("Eden.app") => "Eden",
                Some("Citron.app") => "Citron",
                Some("yuzu.app") => "yuzu",
                _ => get_default_macos_executable_name(&preferred_branch),
            };

            if let Ok(exe_path) = get_macos_bundle_executable_path(&app_path, Some(fallback_name)) {
                return exe_path;
            }
        }

        let default_app_path = get_default_macos_bundle_path(&yuzu_path, &preferred_branch);
        default_app_path
            .join("Contents/MacOS")
            .join(get_default_macos_executable_name(&preferred_branch))
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Windows: 查找 .exe 文件
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
}

fn extract_ascii_strings(bytes: &[u8]) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = Vec::new();

    for &byte in bytes {
        if byte.is_ascii_graphic() || byte == b' ' {
            current.push(byte);
            continue;
        }

        if current.len() >= 4 {
            result.push(String::from_utf8_lossy(&current).into_owned());
        }
        current.clear();
    }

    if current.len() >= 4 {
        result.push(String::from_utf8_lossy(&current).into_owned());
    }

    result
}

fn normalize_version_token(token: &str) -> Option<String> {
    let normalized = token
        .trim()
        .trim_matches(|c: char| !c.is_ascii_alphanumeric() && !matches!(c, '.' | '-' | '_'));

    if normalized.is_empty() || normalized.len() > 64 {
        return None;
    }

    if !normalized.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }

    if !normalized
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return None;
    }

    Some(normalized.to_string())
}

fn is_likely_release_version(token: &str) -> bool {
    let starts_with_prefixed_semver = token
        .strip_prefix('v')
        .and_then(|rest| rest.chars().next())
        .is_some_and(|c| c.is_ascii_digit());

    if starts_with_prefixed_semver || token.contains('.') {
        return true;
    }

    token.contains('-')
        && token.chars().any(|c| c.is_ascii_alphabetic())
        && token.chars().any(|c| c.is_ascii_digit())
}

fn extract_version_after_prefix(text: &str, prefix: &str) -> Option<String> {
    let rest = text.strip_prefix(prefix)?;
    let token = rest.split_whitespace().next()?;
    let version = normalize_version_token(token)?;

    if is_likely_release_version(&version) {
        Some(version)
    } else {
        None
    }
}

fn infer_branch_from_marker(text: &str) -> Option<&'static str> {
    let marker = text.trim();

    if marker.eq_ignore_ascii_case("eden")
        || marker.starts_with("Eden ")
        || marker.starts_with("Eden | ")
    {
        return Some("eden");
    }

    if marker.eq_ignore_ascii_case("citron")
        || marker.starts_with("Citron ")
        || marker.starts_with("citron ")
        || marker.starts_with("Citron | ")
        || marker.starts_with("citron | ")
    {
        return Some("citron");
    }

    if marker.starts_with("yuzu Early Access ") {
        return Some("ea");
    }

    if marker.eq_ignore_ascii_case("yuzu") || marker.starts_with("yuzu ") {
        return Some("mainline");
    }

    None
}

fn detect_yuzu_version_from_ascii_strings(strings: &[String]) -> Option<(String, Option<String>)> {
    for text in strings {
        for prefix in ["Eden | ", "Eden "] {
            if let Some(version) = extract_version_after_prefix(text, prefix) {
                return Some((version, Some("eden".to_string())));
            }
        }

        for prefix in ["Citron | ", "citron | ", "Citron ", "citron "] {
            if let Some(version) = extract_version_after_prefix(text, prefix) {
                return Some((version, Some("citron".to_string())));
            }
        }

        if let Some(rest) = text.strip_prefix("yuzu Early Access ") {
            let Some(version) = rest.split_whitespace().next() else {
                continue;
            };
            if version.chars().all(|c| c.is_ascii_digit()) {
                return Some((version.to_string(), Some("ea".to_string())));
            }
        }

        if let Some(rest) = text.strip_prefix("yuzu ") {
            let Some(version) = rest.split_whitespace().next() else {
                continue;
            };
            if version.chars().all(|c| c.is_ascii_digit()) {
                return Some((version.to_string(), Some("mainline".to_string())));
            }
        }
    }

    for (index, text) in strings.iter().enumerate() {
        let Some(version) = normalize_version_token(text) else {
            continue;
        };

        if !is_likely_release_version(&version) {
            continue;
        }

        let start = index.saturating_sub(3);
        let end = (index + 4).min(strings.len());
        let branch = strings[start..end]
            .iter()
            .find_map(|marker| infer_branch_from_marker(marker))
            .map(ToString::to_string);

        if branch.is_some() {
            return Some((version, branch));
        }
    }

    None
}

fn detect_yuzu_version_from_binary(exe_path: &Path) -> Option<(String, Option<String>)> {
    let binary = match std::fs::read(exe_path) {
        Ok(binary) => binary,
        Err(error) => {
            warn!(
                "读取模拟器二进制失败，无法直接提取版本: {} ({})",
                exe_path.display(),
                error
            );
            return None;
        }
    };

    let ascii_strings = extract_ascii_strings(&binary);
    let detected = detect_yuzu_version_from_ascii_strings(&ascii_strings);

    if let Some((version, branch)) = &detected {
        info!(
            "通过二进制内嵌字符串检测到版本: {}, 分支: {:?}",
            version, branch
        );
    } else {
        debug!("未能从二进制中提取版本: {}", exe_path.display());
    }

    detected
}

#[cfg(target_os = "macos")]
fn extract_version_from_plist_text(text: &str) -> Option<String> {
    if let Some(version) = normalize_version_token(text) {
        if is_likely_release_version(&version) {
            return Some(version);
        }
    }

    for token in text.split(|c: char| c.is_whitespace() || matches!(c, '|' | '(' | ')' | ',')) {
        let Some(version) = normalize_version_token(token) else {
            continue;
        };

        if is_likely_release_version(&version) {
            return Some(version);
        }
    }

    None
}

#[cfg(target_os = "macos")]
fn detect_yuzu_version_from_bundle_metadata(exe_path: &Path) -> Option<(String, Option<String>)> {
    let app_path = exe_path.ancestors().find(|ancestor| {
        ancestor
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
    })?;

    let plist_path = app_path.join("Contents/Info.plist");
    let contents = match std::fs::read(&plist_path) {
        Ok(contents) => contents,
        Err(error) => {
            debug!(
                "读取 Info.plist 失败，无法作为版本检测回退: {} ({})",
                plist_path.display(),
                error
            );
            return None;
        }
    };

    let plist: plist::Dictionary = match plist::from_bytes(&contents) {
        Ok(plist) => plist,
        Err(error) => {
            warn!("解析 Info.plist 失败: {} ({})", plist_path.display(), error);
            return None;
        }
    };

    let version = [
        "CFBundleShortVersionString",
        "CFBundleLongVersionString",
        "CFBundleVersion",
    ]
    .iter()
    .find_map(|key| match plist.get(*key) {
        Some(plist::Value::String(value)) => extract_version_from_plist_text(value),
        _ => None,
    })?;

    let branch = ["CFBundleName", "CFBundleDisplayName", "CFBundleExecutable"]
        .iter()
        .find_map(|key| match plist.get(*key) {
            Some(plist::Value::String(value)) => {
                infer_branch_from_marker(value).map(ToString::to_string)
            }
            _ => None,
        })
        .or_else(|| {
            app_path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(infer_branch_from_macos_app_name)
        });

    info!(
        "通过 Info.plist 回退检测到版本: {}, 分支: {:?}",
        version, branch
    );

    Some((version, branch))
}

fn save_detected_yuzu_version(version: &str, branch: Option<&str>) -> AppResult<()> {
    debug!("更新配置文件");

    let mut cfg = CONFIG.write();
    cfg.yuzu.yuzu_version = Some(version.to_string());
    if let Some(branch) = branch {
        cfg.yuzu.branch = branch.to_string();
    }
    cfg.save()?;

    debug!("配置文件已保存");
    Ok(())
}

/// 开始检测 Yuzu 版本
///
/// 优先直接解析二进制内嵌版本串；如果失败，则在 Windows 上回退到窗口标题检测。
pub async fn detect_yuzu_version() -> AppResult<Option<String>> {
    info!("开始检测 Yuzu 版本");

    let exe_path = get_yuzu_exe_path();
    debug!("Yuzu 可执行文件路径: {}", exe_path.display());

    if !exe_path.exists() {
        warn!("未找到 Yuzu 程序: {}", exe_path.display());
        return Ok(None);
    }

    if let Some((version, branch)) = detect_yuzu_version_from_binary(&exe_path) {
        info!("通过二进制检测到版本: {}, 分支: {:?}", version, branch);
        save_detected_yuzu_version(&version, branch.as_deref())?;
        return Ok(Some(version));
    }

    #[cfg(target_os = "macos")]
    {
        if let Some((version, branch)) = detect_yuzu_version_from_bundle_metadata(&exe_path) {
            save_detected_yuzu_version(&version, branch.as_deref())?;
            return Ok(Some(version));
        }
    }

    #[cfg(not(windows))]
    {
        warn!("未能从二进制中检测到版本，当前平台不支持窗口标题回退检测");
        return Ok(None);
    }

    // TODO: 检查是否已经在运行
    // let instances = find_all_instances(&exe_path);
    // if !instances.is_empty() {
    //     return Err(AppError::Process("Yuzu 正在运行，请先关闭".to_string()));
    // }

    #[cfg(windows)]
    {
        // 启动程序
        info!("启动 Yuzu: {}", exe_path.display());
        debug!("使用窗口标题回退检测版本");
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

        use std::sync::{Arc, Mutex};
        use windows::Win32::Foundation::{HWND, LPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            EnumWindows, GetWindowTextW, IsWindowVisible,
        };

        let version_data = Arc::new(Mutex::new((None, None)));
        let version_data_clone = version_data.clone();

        unsafe extern "system" fn enum_proc(
            hwnd: HWND,
            lparam: LPARAM,
        ) -> windows::Win32::Foundation::BOOL {
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

            let detected = {
                let guard = version_data.lock().unwrap();
                match (guard.0.clone(), guard.1.clone()) {
                    (Some(version), branch) => Some((version, branch)),
                    (None, _) => None,
                }
            };
            if let Some((detected_version, detected_branch)) = detected {
                version = Some(detected_version);
                branch = detected_branch;
                debug!(
                    "第 {} 次尝试找到窗口标题，检测到版本: {:?}, 分支: {:?}",
                    i + 1,
                    version,
                    branch
                );
                break;
            }

            if i % 5 == 0 {
                debug!("第 {} 次尝试，尚未找到窗口标题", i + 1);
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        // 结束进程
        debug!("结束 Yuzu 进程");
        let _ = child.kill();
        let _ = child.wait();

        // 更新配置
        if let Some(ref v) = version {
            info!("通过窗口标题检测到版本: {}, 分支: {:?}", v, branch);
            save_detected_yuzu_version(v, branch.as_deref())?;
        } else {
            warn!("未能检测到 Yuzu 版本");
            debug!("可能原因：窗口标题不匹配，或窗口创建延迟过长");
        }

        Ok(version)
    }
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
    let preferred_branch = config.yuzu.branch.clone();

    #[cfg(target_os = "macos")]
    {
        // 优先使用本地 user 目录（便携版）
        let local_user = yuzu_path.join("user");
        if local_user.exists() {
            return local_user;
        }

        if let Ok(home) = std::env::var("HOME") {
            let home_path = PathBuf::from(home);

            // 先检查 ~/Library/Application Support（macOS 标准路径）
            let app_support = home_path.join("Library/Application Support");
            if let Some(path) = find_existing_yuzu_user_dir(&app_support, &preferred_branch) {
                return path;
            }

            // 再回退到 ~/.local/share，兼容历史目录布局
            let local_share = home_path.join(".local/share");
            if let Some(path) = find_existing_yuzu_user_dir(&local_share, &preferred_branch) {
                return path;
            }

            // 默认返回基于当前分支的 macOS 标准路径
            return app_support.join(default_yuzu_user_dir_name(&preferred_branch));
        }

        // Fallback 到本地 user 目录
        return local_user;
    }

    #[cfg(target_os = "windows")]
    {
        // 优先使用本地 user 目录
        let local_user = yuzu_path.join("user");
        if local_user.exists() {
            return local_user;
        }

        // 检查 AppData 目录
        if let Ok(appdata) = std::env::var("APPDATA") {
            let appdata_path = PathBuf::from(appdata);

            if let Some(path) = find_existing_yuzu_user_dir(&appdata_path, &preferred_branch) {
                return path;
            }
        }

        // 默认返回本地 user 目录
        return local_user;
    }

    #[cfg(target_os = "linux")]
    {
        // 优先使用本地 user 目录
        let local_user = yuzu_path.join("user");
        if local_user.exists() {
            return local_user;
        }

        // 检查 ~/.local/share 目录
        if let Ok(home) = std::env::var("HOME") {
            let local_share = PathBuf::from(home).join(".local/share");

            if let Some(path) = find_existing_yuzu_user_dir(&local_share, &preferred_branch) {
                return path;
            }

            // 默认返回基于当前分支的路径
            return local_share.join(default_yuzu_user_dir_name(&preferred_branch));
        }

        // 默认返回本地 user 目录
        return local_user;
    }
}

/// 打开 Yuzu keys 文件夹
pub fn open_yuzu_keys_folder() -> AppResult<()> {
    let keys_path = get_yuzu_user_path().join("keys");
    std::fs::create_dir_all(&keys_path)?;

    // 创建提示文件
    let hint_file = keys_path.join("把prod.keys放当前目录.txt");
    std::fs::write(&hint_file, "")?;

    info!("正在打开密钥目录：{}", keys_path.display());

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

/// 读取 Yuzu qt-config.ini 中的 Data Storage 配置
fn get_yuzu_data_storage_config(
    user_path: &Path,
) -> Option<std::collections::HashMap<String, String>> {
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
            info!("从配置文件读取到 NAND 路径：{}", nand_path.display());
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
            info!("从配置文件读取到 Load 路径：{}", load_path.display());
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

    info!("Yuzu 路径已更新为：{}", new_path.display());
    Ok(())
}

/// 获取变更日志
pub async fn get_yuzu_change_logs() -> AppResult<String> {
    // 直接获取配置的克隆
    let branch = get_config().yuzu.branch.clone();
    if branch != "eden" {
        return Err(unsupported_install_branch_error(&branch));
    }

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
                let steps = vec![ProgressStep {
                    id: "check_firmware".to_string(),
                    title: format!("当前固件已是版本 {}, 跳过安装", version),
                    status: ProgressStatus::Success,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    downloaded_size: None,
                    total_size: None,
                    error: None,
                    download_source: None,
                }];
                on_event(ProgressEvent::Started {
                    steps: steps.clone(),
                });
                on_event(ProgressEvent::StepUpdate {
                    step: steps[0].clone(),
                });
                return Ok(());
            }
        }
    }

    // 获取固件路径
    let firmware_path = crate::services::firmware::get_yuzu_firmware_path();

    info!("开始向 Yuzu 安装固件，路径：{}", firmware_path.display());

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
            downloaded_size: None,
            total_size: None,
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
            downloaded_size: None,
            total_size: None,
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
            downloaded_size: None,
            total_size: None,
            error: None,
            download_source: None,
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
    )
    .await?;

    // 更新配置
    {
        let mut cfg = CONFIG.write();
        cfg.yuzu.yuzu_firmware = Some(new_version.clone());
        cfg.save()?;
    }

    info!("固件 {} 已成功安装到 Yuzu", new_version);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::release::{ReleaseAsset, ReleaseInfo};
    use tempfile::tempdir;

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

    #[test]
    fn test_detect_yuzu_version_from_ascii_strings_for_eden() {
        let strings = vec![
            "8e373eb714".to_string(),
            "v0.2.0-rc1".to_string(),
            "Eden".to_string(),
            "2026-02-15T03:33:38Z".to_string(),
            "Eden v0.2.0-rc1 ".to_string(),
            "MSVC 19.44.35222.0".to_string(),
        ];

        let detected = detect_yuzu_version_from_ascii_strings(&strings);
        assert_eq!(
            detected,
            Some(("v0.2.0-rc1".to_string(), Some("eden".to_string())))
        );
    }

    #[test]
    fn test_detect_yuzu_version_from_ascii_strings_with_context_fallback() {
        let strings = vec![
            "Random text".to_string(),
            "Citron".to_string(),
            "stable-01c042048".to_string(),
            "MSVC 19.44.35222.0".to_string(),
        ];

        let detected = detect_yuzu_version_from_ascii_strings(&strings);
        assert_eq!(
            detected,
            Some(("stable-01c042048".to_string(), Some("citron".to_string())))
        );
    }

    #[test]
    fn test_detect_yuzu_version_from_ascii_strings_for_yuzu_ea() {
        let strings = vec![
            "Something else".to_string(),
            "yuzu Early Access 4176".to_string(),
        ];

        let detected = detect_yuzu_version_from_ascii_strings(&strings);
        assert_eq!(detected, Some(("4176".to_string(), Some("ea".to_string()))));
    }

    #[test]
    fn test_select_macos_asset_eden() {
        let release = ReleaseInfo {
            name: "v0.0.4-rc3".to_string(),
            tag_name: "v0.0.4-rc3".to_string(),
            description: "".to_string(),
            published_at: None,
            prerelease: false,
            html_url: None,
            assets: vec![
                ReleaseAsset {
                    name: "Eden-Windows-v0.0.4-rc3-amd64-msvc-standard.zip".to_string(),
                    download_url: "https://example.com/windows.zip".to_string(),
                    size: 0,
                    content_type: None,
                },
                ReleaseAsset {
                    name: "Eden-macOS-v0.0.4-rc3.tar.gz".to_string(),
                    download_url: "https://example.com/macos.tar.gz".to_string(),
                    size: 0,
                    content_type: None,
                },
                ReleaseAsset {
                    name: "Eden-Linux-v0.0.4-rc3-x86_64.AppImage".to_string(),
                    download_url: "https://example.com/linux.AppImage".to_string(),
                    size: 0,
                    content_type: None,
                },
            ],
        };

        let url = select_macos_asset(&release, "eden");
        assert_eq!(url, Some("https://example.com/macos.tar.gz".to_string()));
    }

    #[test]
    fn test_find_existing_yuzu_user_dir_prefers_requested_branch() {
        let dir = tempdir().unwrap();
        let base = dir.path();
        std::fs::create_dir_all(base.join("eden")).unwrap();
        std::fs::create_dir_all(base.join("citron")).unwrap();
        std::fs::create_dir_all(base.join("yuzu")).unwrap();

        assert_eq!(
            find_existing_yuzu_user_dir(base, "eden"),
            Some(base.join("eden"))
        );
        assert_eq!(
            find_existing_yuzu_user_dir(base, "citron"),
            Some(base.join("citron"))
        );
        assert_eq!(
            find_existing_yuzu_user_dir(base, "mainline"),
            Some(base.join("yuzu"))
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_resolve_existing_macos_bundle_prefers_current_branch() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Eden.app")).unwrap();
        std::fs::create_dir_all(dir.path().join("yuzu.app")).unwrap();

        let app_path = resolve_existing_macos_bundle(dir.path(), "eden").unwrap();
        assert_eq!(app_path.file_name().unwrap().to_string_lossy(), "Eden.app");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_detect_yuzu_version_from_bundle_metadata_for_eden() {
        let dir = tempdir().unwrap();
        let app_path = dir.path().join("Eden.app");
        let contents_dir = app_path.join("Contents");
        let macos_dir = contents_dir.join("MacOS");
        std::fs::create_dir_all(&macos_dir).unwrap();
        std::fs::write(
            contents_dir.join("Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>Eden</string>
  <key>CFBundleShortVersionString</key>
  <string>v0.2.0-rc1</string>
  <key>CFBundleName</key>
  <string>Eden</string>
</dict>
</plist>"#,
        )
        .unwrap();
        let exe_path = macos_dir.join("Eden");
        std::fs::write(&exe_path, b"").unwrap();

        let detected = detect_yuzu_version_from_bundle_metadata(&exe_path);
        assert_eq!(
            detected,
            Some(("v0.2.0-rc1".to_string(), Some("eden".to_string())))
        );
    }

    #[tokio::test]
    async fn test_download_yuzu_rejects_citron_branch() {
        let error = download_yuzu("stable", "citron", |_| {}).await.unwrap_err();
        assert!(matches!(error, AppError::Unsupported(_)));
        assert!(error.to_string().contains("Citron"));
    }

    #[test]
    fn test_select_macos_asset_rejects_unsupported_branch() {
        let release = ReleaseInfo {
            name: "stable".to_string(),
            tag_name: "stable-01c042048".to_string(),
            description: "".to_string(),
            published_at: None,
            prerelease: false,
            html_url: None,
            assets: vec![
                ReleaseAsset {
                    name: "Citron-windows-stable-01c042048-x64.zip".to_string(),
                    download_url: "https://example.com/windows.zip".to_string(),
                    size: 0,
                    content_type: None,
                },
                ReleaseAsset {
                    name: "Citron-macOS-stable-01c042048.dmg".to_string(),
                    download_url: "https://example.com/macos.dmg".to_string(),
                    size: 0,
                    content_type: None,
                },
                ReleaseAsset {
                    name: "citron_stable-01c042048-linux-x86_64.AppImage".to_string(),
                    download_url: "https://example.com/linux.AppImage".to_string(),
                    size: 0,
                    content_type: None,
                },
            ],
        };

        let url = select_macos_asset(&release, "citron");
        assert_eq!(url, None);
    }

    #[test]
    fn test_select_macos_asset_no_macos_build() {
        let release = ReleaseInfo {
            name: "old-version".to_string(),
            tag_name: "v0.0.1".to_string(),
            description: "".to_string(),
            published_at: None,
            prerelease: false,
            html_url: None,
            assets: vec![
                ReleaseAsset {
                    name: "Eden-Windows-v0.0.1.zip".to_string(),
                    download_url: "https://example.com/windows.zip".to_string(),
                    size: 0,
                    content_type: None,
                },
                ReleaseAsset {
                    name: "Eden-Linux-v0.0.1.AppImage".to_string(),
                    download_url: "https://example.com/linux.AppImage".to_string(),
                    size: 0,
                    content_type: None,
                },
            ],
        };

        let url = select_macos_asset(&release, "eden");
        assert_eq!(url, None);
    }

    #[test]
    fn test_select_macos_asset_excludes_other_platforms() {
        let release = ReleaseInfo {
            name: "test".to_string(),
            tag_name: "test".to_string(),
            description: "".to_string(),
            published_at: None,
            prerelease: false,
            html_url: None,
            assets: vec![
                ReleaseAsset {
                    name: "Eden-Windows-test.zip".to_string(),
                    download_url: "https://example.com/windows.zip".to_string(),
                    size: 0,
                    content_type: None,
                },
                ReleaseAsset {
                    name: "Eden-Linux-test.AppImage".to_string(),
                    download_url: "https://example.com/linux.AppImage".to_string(),
                    size: 0,
                    content_type: None,
                },
                ReleaseAsset {
                    name: "Eden-Android-test.apk".to_string(),
                    download_url: "https://example.com/android.apk".to_string(),
                    size: 0,
                    content_type: None,
                },
                ReleaseAsset {
                    name: "Eden-macOS-test.tar.gz".to_string(),
                    download_url: "https://example.com/macos.tar.gz".to_string(),
                    size: 0,
                    content_type: None,
                },
            ],
        };

        let url = select_macos_asset(&release, "eden");
        // 应该只选择 macOS 版本，排除其他平台
        assert_eq!(url, Some("https://example.com/macos.tar.gz".to_string()));
    }
}

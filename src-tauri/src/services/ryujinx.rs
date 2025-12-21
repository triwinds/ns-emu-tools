//! Ryujinx 模拟器管理服务
//!
//! 提供 Ryujinx 模拟器（mainline/canary 分支）的安装、更新、版本检测等功能

use crate::config::{get_config, CONFIG};
use crate::error::{AppError, AppResult};
use crate::models::{InstallationEvent, InstallationStatus, InstallationStep};
use crate::repositories::ryujinx::{
    get_all_ryujinx_release_infos, get_ryujinx_release_info_by_version, load_ryujinx_change_log,
};
use crate::services::aria2::{get_aria2_manager, Aria2DownloadOptions};
use crate::services::msvc::check_and_install_msvc;
use crate::services::network::get_final_url;
use crate::utils::archive::uncompress;
use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Ryujinx 可执行文件名
const RYUJINX_EXE: &str = "Ryujinx.exe";

/// 当前正在进行的下载任务 GID
static CURRENT_DOWNLOAD_GID: RwLock<Option<String>> = RwLock::new(None);

/// 获取 Ryujinx 下载 URL
///
/// # 参数
/// * `target_version` - 目标版本
/// * `branch` - 分支 (mainline, canary)
///
/// # 返回
/// 下载 URL
async fn get_ryujinx_download_url(target_version: &str, branch: &str) -> AppResult<String> {
    let release_info = get_ryujinx_release_info_by_version(target_version, branch).await?;

    if release_info.tag_name.is_empty() {
        return Err(AppError::Emulator(format!(
            "未找到 Ryujinx {} 版本: {}",
            branch, target_version
        )));
    }

    // 查找 Windows x64 版本
    for asset in &release_info.assets {
        let name = asset.name.to_lowercase();
        if name.starts_with("ryujinx-") && name.ends_with("-win_x64.zip") {
            let url = get_final_url(&asset.download_url);
            return Ok(url);
        }
    }

    Err(AppError::Emulator(format!(
        "未找到 Ryujinx {} [{}] 版本的下载链接",
        branch, target_version
    )))
}

/// 检测当前 Ryujinx 分支
///
/// 通过检查可执行文件名来判断是 mainline 还是 canary
pub fn detect_current_branch() -> String {
    let config = get_config();
    let ryujinx_path = PathBuf::from(&config.ryujinx.path);

    // 检查可执行文件
    let exe_path = ryujinx_path.join(RYUJINX_EXE);
    if !exe_path.exists() {
        return "mainline".to_string();
    }

    // 尝试从文件版本信息检测（Windows）
    #[cfg(windows)]
    {
        // TODO: 读取 PE 文件版本信息判断是否为 canary
    }

    // 默认返回配置中的分支
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
    F: Fn(InstallationEvent) + Send + Sync + 'static + Clone,
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
            on_event(InstallationEvent::StepUpdate {
                step: InstallationStep {
                    id: "fetch_version".to_string(),
                    title: format!("当前已是目标版本 {} ({}), 跳过安装", target_version, branch),
                    status: InstallationStatus::Success,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: None,
                }
            });

            // 标记其他步骤为取消
            for step_id in &["download", "extract", "install", "check_env"] {
                on_event(InstallationEvent::StepUpdate {
                    step: InstallationStep {
                        id: step_id.to_string(),
                        title: match *step_id {
                            "download" => format!("下载 Ryujinx {}", branch),
                            "extract" => "解压文件".to_string(),
                            "install" => "安装文件".to_string(),
                            "check_env" => "检查运行环境".to_string(),
                            _ => "".to_string(),
                        },
                        status: InstallationStatus::Cancelled,
                        step_type: if *step_id == "download" { "download" } else { "normal" }.to_string(),
                        progress: 0.0,
                        download_speed: "".to_string(),
                        eta: "".to_string(),
                        error: None,
                    }
                });
            }

            return Ok(());
        }
    }

    // 获取版本信息
    on_event(InstallationEvent::StepUpdate {
        step: InstallationStep {
            id: "fetch_version".to_string(),
            title: "获取版本信息".to_string(),
            status: InstallationStatus::Running,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });

    let download_url = match get_ryujinx_download_url(target_version, branch).await {
        Ok(url) => url,
        Err(e) => {
            on_event(InstallationEvent::StepUpdate {
                step: InstallationStep {
                    id: "fetch_version".to_string(),
                    title: "获取版本信息".to_string(),
                    status: InstallationStatus::Error,
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

    on_event(InstallationEvent::StepUpdate {
        step: InstallationStep {
            id: "fetch_version".to_string(),
            title: "获取版本信息".to_string(),
            status: InstallationStatus::Success,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });

    info!("下载 URL: {}", download_url);

    // 下载
    on_event(InstallationEvent::StepUpdate {
        step: InstallationStep {
            id: "download".to_string(),
            title: format!("下载 Ryujinx {}", branch),
            status: InstallationStatus::Running,
            step_type: "download".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });

    // 使用 aria2 下载
    let aria2 = get_aria2_manager().await?;
    let options = Aria2DownloadOptions {
        use_github_mirror: false, // Ryujinx 使用 GitLab，不需要镜像
        ..Default::default()
    };

    // 添加下载任务并获取 GID
    let gid = aria2.download(&download_url, options).await?;

    // 保存当前下载的 GID（用于取消功能）
    *CURRENT_DOWNLOAD_GID.write() = Some(gid.clone());

    info!("下载任务已添加，GID: {}", gid);

    // 轮询下载进度
    let on_event_clone = on_event.clone();
    let poll_interval = Duration::from_millis(500);
    let package_path = loop {
        tokio::time::sleep(poll_interval).await;

        let progress = match aria2.get_download_progress(&gid).await {
            Ok(p) => p,
            Err(e) => {
                *CURRENT_DOWNLOAD_GID.write() = None;
                on_event_clone(InstallationEvent::StepUpdate {
                    step: InstallationStep {
                        id: "download".to_string(),
                        title: format!("下载 Ryujinx {}", branch),
                        status: InstallationStatus::Error,
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

        // 发送进度更新
        on_event_clone(InstallationEvent::StepUpdate {
            step: InstallationStep {
                id: "download".to_string(),
                title: format!("下载 Ryujinx {}", branch),
                status: InstallationStatus::Running,
                step_type: "download".to_string(),
                progress: progress.percentage,
                download_speed: progress.speed_string(),
                eta: progress.eta_string(),
                error: None,
            }
        });

        match progress.status {
            crate::services::aria2::Aria2DownloadStatus::Complete => {
                // 从 aria2 获取实际文件路径
                let status = aria2.get_download_status(&gid).await?;
                let path = status
                    .files
                    .first()
                    .map(|f| PathBuf::from(&f.path))
                    .ok_or_else(|| AppError::Aria2("无法获取下载文件路径".to_string()))?;

                // 清除 GID
                *CURRENT_DOWNLOAD_GID.write() = None;

                info!("下载完成: {}", path.display());
                break path;
            }
            crate::services::aria2::Aria2DownloadStatus::Error => {
                *CURRENT_DOWNLOAD_GID.write() = None;
                on_event_clone(InstallationEvent::StepUpdate {
                    step: InstallationStep {
                        id: "download".to_string(),
                        title: format!("下载 Ryujinx {}", branch),
                        status: InstallationStatus::Error,
                        step_type: "download".to_string(),
                        progress: 0.0,
                        download_speed: "".to_string(),
                        eta: "".to_string(),
                        error: Some("下载失败".to_string()),
                    }
                });
                return Err(AppError::Aria2("下载失败".to_string()));
            }
            crate::services::aria2::Aria2DownloadStatus::Removed => {
                *CURRENT_DOWNLOAD_GID.write() = None;
                on_event_clone(InstallationEvent::StepUpdate {
                    step: InstallationStep {
                        id: "download".to_string(),
                        title: format!("下载 Ryujinx {}", branch),
                        status: InstallationStatus::Cancelled,
                        step_type: "download".to_string(),
                        progress: 0.0,
                        download_speed: "".to_string(),
                        eta: "".to_string(),
                        error: None,
                    }
                });
                return Err(AppError::Aria2("下载已取消".to_string()));
            }
            _ => continue,
        }
    };

    on_event(InstallationEvent::StepUpdate {
        step: InstallationStep {
            id: "download".to_string(),
            title: format!("下载 Ryujinx {}", branch),
            status: InstallationStatus::Success,
            step_type: "download".to_string(),
            progress: 100.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });

    // 解压
    on_event(InstallationEvent::StepUpdate {
        step: InstallationStep {
            id: "extract".to_string(),
            title: "解压文件".to_string(),
            status: InstallationStatus::Running,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });

    // 解压到临时目录
    let tmp_dir = std::env::temp_dir().join("ryujinx-install");
    if tmp_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&tmp_dir) {
            on_event(InstallationEvent::StepUpdate {
                step: InstallationStep {
                    id: "extract".to_string(),
                    title: "解压文件".to_string(),
                    status: InstallationStatus::Error,
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
        on_event(InstallationEvent::StepUpdate {
            step: InstallationStep {
                id: "extract".to_string(),
                title: "解压文件".to_string(),
                status: InstallationStatus::Error,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                error: Some(format!("创建临时目录失败: {}", e)),
            }
        });
        return Err(e.into());
    }

    info!("解压 Ryujinx 文件到: {}", tmp_dir.display());
    if let Err(e) = uncompress(&package_path, &tmp_dir, false) {
        on_event(InstallationEvent::StepUpdate {
            step: InstallationStep {
                id: "extract".to_string(),
                title: "解压文件".to_string(),
                status: InstallationStatus::Error,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                error: Some(e.to_string()),
            }
        });
        return Err(e);
    }

    on_event(InstallationEvent::StepUpdate {
        step: InstallationStep {
            id: "extract".to_string(),
            title: "解压文件".to_string(),
            status: InstallationStatus::Success,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });

    // 安装
    on_event(InstallationEvent::StepUpdate {
        step: InstallationStep {
            id: "install".to_string(),
            title: "安装文件".to_string(),
            status: InstallationStatus::Running,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });

    // 清理旧文件并安装
    if let Err(e) = clear_ryujinx_folder(&ryujinx_path) {
        on_event(InstallationEvent::StepUpdate {
            step: InstallationStep {
                id: "install".to_string(),
                title: "安装文件".to_string(),
                status: InstallationStatus::Error,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                error: Some(e.to_string()),
            }
        });
        return Err(e);
    }

    // 复制文件
    let ryujinx_tmp_dir = tmp_dir.join("publish");
    info!("复制 Ryujinx 文件到: {}", ryujinx_path.display());

    if let Err(e) = copy_dir_all(&ryujinx_tmp_dir, &ryujinx_path) {
        on_event(InstallationEvent::StepUpdate {
            step: InstallationStep {
                id: "install".to_string(),
                title: "安装文件".to_string(),
                status: InstallationStatus::Error,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                error: Some(e.to_string()),
            }
        });
        return Err(e);
    }

    on_event(InstallationEvent::StepUpdate {
        step: InstallationStep {
            id: "install".to_string(),
            title: "安装文件".to_string(),
            status: InstallationStatus::Success,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });

    // 清理临时目录
    std::fs::remove_dir_all(&tmp_dir)?;

    // 检查运行环境
    on_event(InstallationEvent::StepUpdate {
        step: InstallationStep {
            id: "check_env".to_string(),
            title: "检查运行环境".to_string(),
            status: InstallationStatus::Running,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });

    if let Err(e) = check_and_install_msvc().await {
        warn!("MSVC 运行库检查失败: {}", e);
        on_event(InstallationEvent::StepUpdate {
            step: InstallationStep {
                id: "check_env".to_string(),
                title: "检查运行环境".to_string(),
                status: InstallationStatus::Error,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                error: Some(e.to_string()),
            }
        });
        // 不阻止安装流程，继续执行
    } else {
        on_event(InstallationEvent::StepUpdate {
            step: InstallationStep {
                id: "check_env".to_string(),
                title: "检查运行环境".to_string(),
                status: InstallationStatus::Success,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                error: None,
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
fn clear_ryujinx_folder(ryujinx_path: &Path) -> AppResult<()> {
    if !ryujinx_path.exists() {
        std::fs::create_dir_all(ryujinx_path)?;
        return Ok(());
    }

    info!("清理 Ryujinx 目录: {}", ryujinx_path.display());

    // 保留的目录列表
    let preserve_dirs = vec!["bis", "sdcard", "portable"];

    // 遍历目录
    for entry in std::fs::read_dir(ryujinx_path)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();

        // 检查是否需要保留
        let should_preserve = preserve_dirs.iter().any(|&p| name_str == p);

        if !should_preserve {
            if path.is_dir() {
                debug!("删除目录: {}", path.display());
                std::fs::remove_dir_all(&path)?;
            } else {
                debug!("删除文件: {}", path.display());
                std::fs::remove_file(&path)?;
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
    let config = get_config();
    let ryujinx_path = PathBuf::from(&config.ryujinx.path);

    // 检查是否使用 portable 模式
    let portable_path = ryujinx_path.join("portable");
    if portable_path.exists() {
        return portable_path;
    }

    // 使用 AppData 目录
    if let Ok(appdata) = std::env::var("APPDATA") {
        let appdata_path = PathBuf::from(appdata);
        let ryujinx_appdata = appdata_path.join("Ryujinx");
        if ryujinx_appdata.exists() {
            return ryujinx_appdata;
        }
    }

    // 默认创建 portable 目录
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
    F: Fn(InstallationEvent) + Send + Sync + 'static + Clone,
{
    let config = get_config();

    // 检查是否已安装此版本
    if let Some(ref version) = firmware_version {
        if let Some(ref current_firmware) = config.ryujinx.firmware {
            if current_firmware == version {
                info!("固件已是版本 {}，跳过安装", version);

                // 发送 Started 事件
                let steps = vec![
                    InstallationStep {
                        id: "check_firmware".to_string(),
                        title: format!("当前固件已是版本 {}, 跳过安装", version),
                        status: InstallationStatus::Success,
                        step_type: "normal".to_string(),
                        progress: 0.0,
                        download_speed: "".to_string(),
                        eta: "".to_string(),
                        error: None,
                    }
                ];
                on_event(InstallationEvent::Started { steps: steps.clone() });
                on_event(InstallationEvent::StepUpdate {
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
        InstallationStep {
            id: "fetch_firmware_info".to_string(),
            title: "获取固件信息".to_string(),
            status: InstallationStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        },
        InstallationStep {
            id: "download_firmware".to_string(),
            title: "下载固件".to_string(),
            status: InstallationStatus::Pending,
            step_type: "download".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        },
        InstallationStep {
            id: "extract_firmware".to_string(),
            title: "解压固件".to_string(),
            status: InstallationStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        },
        InstallationStep {
            id: "reorganize_firmware".to_string(),
            title: "重组织固件文件".to_string(),
            status: InstallationStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        },
    ];

    on_event(InstallationEvent::Started { steps });

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
    on_event(InstallationEvent::StepUpdate {
        step: InstallationStep {
            id: "reorganize_firmware".to_string(),
            title: "重组织固件文件".to_string(),
            status: InstallationStatus::Running,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });

    if let Err(e) = crate::services::firmware::reorganize_firmware_for_ryujinx(&tmp_dir, &firmware_path).await {
        on_event(InstallationEvent::StepUpdate {
            step: InstallationStep {
                id: "reorganize_firmware".to_string(),
                title: "重组织固件文件".to_string(),
                status: InstallationStatus::Error,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                error: Some(e.to_string()),
            }
        });
        // 清理临时目录
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(e);
    }

    on_event(InstallationEvent::StepUpdate {
        step: InstallationStep {
            id: "reorganize_firmware".to_string(),
            title: "重组织固件文件".to_string(),
            status: InstallationStatus::Success,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
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
    let exe_path = ryujinx_path.join(RYUJINX_EXE);

    if !exe_path.exists() {
        return Err(AppError::FileNotFound(exe_path.display().to_string()));
    }

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

/// 更新 Ryujinx 安装路径
pub fn update_ryujinx_path(new_path: &str) -> AppResult<()> {
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

    // TODO: 保存到历史记录

    // 更新配置
    {
        let mut cfg = CONFIG.write();
        cfg.ryujinx.path = new_path_buf;
        cfg.save()?;
    }

    info!("Ryujinx 路径已更新");
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

/// 取消当前的 Ryujinx 下载
pub async fn cancel_ryujinx_download() -> AppResult<()> {
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

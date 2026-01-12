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
        // 解压失败时，删除可能损坏的下载文件
        warn!("解压失败，删除可能损坏的文件: {}", package_path.display());
        let _ = std::fs::remove_file(&package_path);

        on_event(ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "extract".to_string(),
                title: "解压文件".to_string(),
                status: ProgressStatus::Error,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                error: Some(format!("{}\n\n已自动删除损坏的文件，请重新尝试下载。", e)),
                download_source: None,
            }
        });
        return Err(AppError::Extract(format!("{}\n\n已自动删除损坏的文件，请重新尝试下载。", e)));
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

        // macOS 特定处理：设置权限和移除 quarantine 属性
        debug!("设置 macOS .app bundle 权限");

        // 1. 移除 quarantine 属性
        let xattr_result = Command::new("xattr")
            .args(["-r", "-d", "com.apple.quarantine"])
            .arg(&dest_app)
            .output();

        match xattr_result {
            Ok(output) => {
                if output.status.success() {
                    debug!("成功移除 quarantine 属性");
                } else {
                    // 如果文件本来就没有 quarantine 属性，xattr 会返回错误，这是正常的
                    debug!("xattr 命令执行完成（文件可能没有 quarantine 属性）");
                }
            }
            Err(e) => {
                warn!("移除 quarantine 属性失败: {}", e);
                // 不中断安装流程
            }
        }

        // 2. 设置 .app bundle 权限为 755
        let chmod_app_result = Command::new("chmod")
            .args(["755"])
            .arg(&dest_app)
            .output();

        if let Err(e) = chmod_app_result {
            warn!("设置 .app 权限失败: {}", e);
            // 不中断安装流程
        } else {
            debug!("成功设置 .app bundle 权限为 755");
        }

        // 3. 设置可执行文件权限
        let exe_path = dest_app.join("Contents/MacOS/Ryujinx");
        if exe_path.exists() {
            let chmod_exe_result = Command::new("chmod")
                .args(["+x"])
                .arg(&exe_path)
                .output();

            if let Err(e) = chmod_exe_result {
                warn!("设置可执行文件权限失败: {}", e);
                // 不中断安装流程
            } else {
                debug!("成功设置可执行文件权限");
            }
        }

        info!("macOS .app 权限设置完成");
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

        // Linux 特定处理：设置可执行权限
        #[cfg(target_os = "linux")]
        {
            debug!("设置 Linux 可执行文件权限");

            // 设置 Ryujinx 可执行文件权限
            for exe_name in RYUJINX_EXE_NAMES {
                let exe_path = ryujinx_path.join(exe_name);
                if exe_path.exists() {
                    let chmod_result = Command::new("chmod")
                        .args(["+x"])
                        .arg(&exe_path)
                        .output();

                    match chmod_result {
                        Ok(_) => {
                            debug!("成功设置 {} 可执行权限", exe_name);
                        }
                        Err(e) => {
                            warn!("设置 {} 可执行权限失败: {}", exe_name, e);
                            // 不中断安装流程
                        }
                    }
                }
            }

            info!("Linux 可执行文件权限设置完成");
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

/// macOS: 通过解析 Info.plist 检测版本和分支
///
/// # 参数
/// * `app_path` - Ryujinx.app 路径
///
/// # 返回
/// (版本号, 分支)
///
/// # 检测策略
/// 按优先级使用以下方法检测分支类型：
/// 1. Copyright 包含 "Ryubing" → Canary 版本（最可靠）
/// 2. CFBundleIconName 键存在 → Canary 版本（较可靠）
/// 3. 版本号格式 (1.3.xxx) → 可能是 Canary（辅助判断）
/// 4. 版本号包含 "ldn" → LDN 分支
#[cfg(target_os = "macos")]
fn detect_version_from_binary(app_path: &Path) -> AppResult<(Option<String>, String)> {
    use std::io::Read;

    // 1. 从 Info.plist 读取版本号
    let plist_path = app_path.join("Contents/Info.plist");
    if !plist_path.exists() {
        debug!("Info.plist 不存在: {}", plist_path.display());
        return Ok((None, "mainline".to_string()));
    }

    debug!("读取 Info.plist: {}", plist_path.display());
    let mut file = std::fs::File::open(&plist_path)?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;

    let plist: plist::Dictionary = plist::from_bytes(&contents)
        .map_err(|e| AppError::Emulator(format!("解析 Info.plist 失败: {}", e)))?;

    // 读取版本信息（优先使用 CFBundleLongVersionString，因为它更准确）
    let version = if let Some(plist::Value::String(version)) = plist.get("CFBundleLongVersionString") {
        // 提取版本号部分 (例如 "1.3.3-e2143d4" -> "1.3.3")
        let version_part = version.split('-').next().unwrap_or(version).to_string();
        // 移除可能的尾部引号
        let version_clean = version_part.trim_end_matches('"').to_string();
        debug!("从 CFBundleLongVersionString 读取版本: {} -> {}", version, version_clean);
        Some(version_clean)
    } else if let Some(plist::Value::String(version)) = plist.get("CFBundleVersion") {
        debug!("从 CFBundleVersion 读取版本: {}", version);
        Some(version.clone())
    } else {
        None
    };

    if let Some(ref v) = version {
        info!("从 Info.plist 检测到版本: {}", v);
    } else {
        warn!("Info.plist 中未找到版本信息");
    }

    // 2. 检测分支 - 使用多种方法按优先级检测
    let branch = detect_branch_from_plist(&plist, version.as_deref());

    info!("从 Info.plist 检测到版本: {:?}, 分支: {}", version, branch);

    Ok((version, branch))
}

/// 从 plist 数据中检测分支类型
///
/// # 参数
/// * `plist` - Info.plist 的字典数据
/// * `version` - 版本号（可选）
///
/// # 返回
/// 分支类型字符串
#[cfg(target_os = "macos")]
fn detect_branch_from_plist(plist: &plist::Dictionary, version: Option<&str>) -> String {
    // 方法1: 检查 Copyright 信息（最可靠）
    // Canary 版本的 Copyright 包含 "Ryubing and Contributors"
    if let Some(plist::Value::String(copyright)) = plist.get("NSHumanReadableCopyright") {
        if copyright.contains("Ryubing") {
            debug!("通过 Copyright 检测到 Canary 分支: {}", copyright);
            return "canary".to_string();
        }
    }

    // 方法2: 检查 CFBundleIconName 键（较可靠）
    // Canary 版本有此键，Mainline 版本没有
    if plist.contains_key("CFBundleIconName") {
        debug!("通过 CFBundleIconName 键检测到 Canary 分支");
        return "canary".to_string();
    }

    // 方法3: 检测 LDN 分支（版本号中包含 ldn）
    if let Some(v) = version {
        if v.to_lowercase().contains("ldn") {
            debug!("从版本号检测到 LDN 分支: {}", v);
            return "ldn".to_string();
        }
    }

    // 方法4: 通过版本号格式检测分支（辅助判断）
    // Canary: 补丁号 >= 100（如 1.3.243）
    // Mainline: 补丁号 < 100（如 1.3.3）
    if let Some(v) = version {
        let parts: Vec<&str> = v.split('.').collect();
        if parts.len() >= 3 {
            if let Ok(patch_version) = parts[2].parse::<u32>() {
                if patch_version >= 100 {
                    debug!("通过版本号格式检测到 Canary 分支: {}，补丁号={}", v, patch_version);
                    return "canary".to_string();
                } else {
                    debug!("通过版本号格式检测到 Mainline 分支: {}，补丁号={}", v, patch_version);
                    // 继续，可能是 mainline
                }
            }
        }
    }

    // 默认为 Mainline
    debug!("未检测到特殊分支标识，默认为 mainline");
    "mainline".to_string()
}

/// Windows: 通过解析 PE 文件版本信息检测版本和分支
///
/// # 参数
/// * `exe_path` - Ryujinx 可执行文件路径
///
/// # 返回
/// (版本号, 分支)
///
/// # 检测策略
/// 按优先级使用以下方法检测分支类型：
/// 1. LegalCopyright 包含 "Ryubing" → Canary 版本（最可靠）
/// 2. ProductName 包含 "Canary" → Canary 版本（较可靠）
/// 3. ProductName 包含 "ldn" → LDN 分支
/// 4. 文件名包含 "Ava" → Ava 分支
/// 5. 版本号包含 "ldn" → LDN 分支
/// 6. 默认 → Mainline
#[cfg(target_os = "windows")]
fn detect_version_from_binary(exe_path: &Path) -> AppResult<(Option<String>, String)> {
    use pelite::pe64::{Pe, PeFile};
    use pelite::resources::version_info::Language;

    debug!("解析 PE 文件: {}", exe_path.display());

    // 读取 PE 文件
    let file_data = std::fs::read(exe_path)
        .map_err(|e| AppError::Emulator(format!("读取文件失败: {}", e)))?;

    // 解析 PE 文件
    let pe = PeFile::from_bytes(&file_data)
        .map_err(|e| AppError::Emulator(format!("解析 PE 文件失败: {}", e)))?;

    // 检测分支（通过文件名）
    let mut branch = "mainline".to_string();
    if let Some(file_name) = exe_path.file_name() {
        if file_name.to_string_lossy().contains("Ava") {
            branch = "ava".to_string();
        }
    }

    // 获取版本信息资源
    let resources = pe.resources()
        .map_err(|e| AppError::Emulator(format!("获取资源失败: {}", e)))?;

    let version_info = resources.version_info()
        .map_err(|e| AppError::Emulator(format!("获取版本信息失败: {}", e)))?;

    // 读取 StringFileInfo
    let mut version: Option<String> = None;
    let mut copyright_text: Option<String> = None;
    let mut product_name_text: Option<String> = None;
    let mut file_desc_text: Option<String> = None;

    // 尝试多个语言 ID (优先使用 0x0000 = 中性语言，因为 Ryujinx 使用这个)
    let language_ids = [
        (0x0000, "Neutral"),    // 中性语言（Ryujinx 使用这个）
        (0x0409, "en-US"),      // 英语-美国
        (0x0009, "English"),    // 英语
    ];

    for (lang_id, lang_name) in language_ids.iter() {
        let lang = Language { lang_id: *lang_id, charset_id: 1200 }; // 1200 = Unicode
        debug!("尝试读取语言 ID: 0x{:04X} ({})", lang_id, lang_name);

        let mut found_any = false;
        version_info.strings(lang, |key, value| {
            found_any = true;
            debug!("  {}: {}", key, value);
            match key {
                "ProductVersion" => {
                    if version.is_none() {
                        version = Some(value.to_string());
                        debug!("从 ProductVersion 读取: {}", value);
                    }
                }
                "FileVersion" => {
                    if version.is_none() {
                        version = Some(value.to_string());
                        debug!("从 FileVersion 读取: {}", value);
                    }
                }
                "LegalCopyright" => {
                    if copyright_text.is_none() {
                        copyright_text = Some(value.to_string());
                    }
                }
                "ProductName" => {
                    if product_name_text.is_none() {
                        product_name_text = Some(value.to_string());
                        debug!("ProductName: {}", value);
                    }
                }
                "FileDescription" => {
                    if file_desc_text.is_none() {
                        file_desc_text = Some(value.to_string());
                        debug!("FileDescription: {}", value);
                    }
                }
                _ => {}
            }
        });

        // 如果找到了版本信息，就不再尝试其他语言
        if found_any {
            debug!("在语言 ID 0x{:04X} ({}) 中找到版本信息", lang_id, lang_name);
            break;
        }
    }

    // 从 LegalCopyright 检测分支（最高优先级）
    if let Some(ref copyright) = copyright_text {
        if copyright.contains("Ryubing") {
            debug!("通过 LegalCopyright 检测到 Canary 分支: {}", copyright);
            branch = "canary".to_string();
        }
    }

    // 从 ProductName 检测分支
    if let Some(ref product_name) = product_name_text {
        if product_name.contains("Canary") {
            debug!("通过 ProductName 检测到 Canary 分支");
            branch = "canary".to_string();
        } else if product_name.to_lowercase().contains("ldn") {
            debug!("通过 ProductName 检测到 LDN 分支");
            branch = "ldn".to_string();
        }
    }

    // 从 FileDescription 检测分支（备选）
    if let Some(ref file_desc) = file_desc_text {
        if file_desc.contains("Canary") {
            branch = "canary".to_string();
        } else if file_desc.to_lowercase().contains("ldn") {
            branch = "ldn".to_string();
        }
    }

    // 清理版本号
    if let Some(ref mut v) = version {
        // 移除 + 号后缀 (例如 "1.3.3+e2143d4" -> "1.3.3")
        if let Some(plus_pos) = v.find('+') {
            *v = v[..plus_pos].to_string();
        }
        // 移除 - 号后缀
        if let Some(dash_pos) = v.find('-') {
            *v = v[..dash_pos].to_string();
        }

        // 移除尾部的 .0 (例如 "1.3.3.0" -> "1.3.3")
        while v.ends_with(".0") && v.matches('.').count() > 2 {
            *v = v[..v.len() - 2].to_string();
        }

        // 检测版本号中的分支标识
        if v.to_lowercase().contains("ldn") {
            if let Some(ldn_idx) = v.to_lowercase().find("ldn") {
                *v = v[ldn_idx + 3..].to_string();
                branch = "ldn".to_string();
            }
        }

        // 通过版本号格式检测分支（如果还没有从其他字段检测到分支）
        // Canary: 1.3.xxx（xxx 是三位数，如 1.3.243）
        // Mainline: 1.3.x（x 是 1-2 位数，如 1.3.3）
        if branch == "mainline" || branch == "ava" {
            let parts: Vec<&str> = v.split('.').collect();
            if parts.len() >= 3 {
                if let Ok(patch_version) = parts[2].parse::<u32>() {
                    // Canary 版本号的第三位通常是三位数（>= 100）
                    if patch_version >= 100 {
                        debug!("通过版本号格式检测到 Canary 分支: {}，补丁号={}", v, patch_version);
                        branch = "canary".to_string();
                    } else {
                        debug!("通过版本号格式检测到 Mainline 分支: {}，补丁号={}", v, patch_version);
                        // 保持 mainline
                    }
                }
            }
        }

        debug!("检测到版本: {}, 分支: {}", v, branch);
    } else {
        debug!("未找到版本信息，分支: {}", branch);
    }

    Ok((version, branch))
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

    // macOS: 直接从 Info.plist 和二进制文件读取版本信息
    #[cfg(target_os = "macos")]
    {
        let app_path = ryujinx_path.join(RYUJINX_APP_NAME);
        if app_path.exists() {
            match detect_version_from_binary(&app_path) {
                Ok((Some(version), branch)) => {
                    info!("检测到 Ryujinx 版本: {}, 分支: {}", version, branch);

                    // 更新配置
                    let mut cfg = CONFIG.write();
                    cfg.ryujinx.version = Some(version.clone());
                    cfg.ryujinx.branch = branch.clone();
                    cfg.save()?;

                    return Ok((Some(version), branch));
                }
                Ok((None, branch)) => {
                    warn!("未找到版本信息，使用检测到的分支: {}", branch);
                    return Ok((None, branch));
                }
                Err(e) => {
                    warn!("从 Info.plist 读取版本失败: {}", e);
                    return Err(e);
                }
            }
        } else {
            return Err(AppError::FileNotFound(
                format!("未找到 Ryujinx.app: {}", app_path.display())
            ));
        }
    }

    // Windows: 直接从 PE 文件读取版本信息（快速，无需启动程序）
    #[cfg(target_os = "windows")]
    {
        let exe_path = match get_ryujinx_exe_path_internal(&ryujinx_path) {
            Some(path) => path,
            None => {
                warn!("未找到 Ryujinx 程序: {}", ryujinx_path.display());
                return Ok((None, "mainline".to_string()));
            }
        };

        match detect_version_from_binary(&exe_path) {
            Ok((Some(version), branch)) => {
                info!("检测到 Ryujinx 版本: {}, 分支: {}", version, branch);

                // 更新配置
                let mut cfg = CONFIG.write();
                cfg.ryujinx.version = Some(version.clone());
                cfg.ryujinx.branch = branch.clone();
                cfg.save()?;

                return Ok((Some(version), branch));
            }
            Ok((None, branch)) => {
                warn!("未找到版本信息，使用检测到的分支: {}", branch);

                // 更新配置
                let mut cfg = CONFIG.write();
                cfg.ryujinx.branch = branch.clone();
                cfg.save()?;

                return Ok((None, branch));
            }
            Err(e) => {
                warn!("从 PE 文件读取版本失败: {}", e);
                return Err(e);
            }
        }
    }

    // Linux: 通过启动程序并读取窗口标题进行检测
    // TODO: 考虑为 Linux 实现类似的 ELF 二进制检测
    #[cfg(target_os = "linux")]
    {
        let exe_path = match get_ryujinx_exe_path_internal(&ryujinx_path) {
            Some(path) => path,
            None => {
                warn!("未找到 Ryujinx 程序: {}", ryujinx_path.display());
                return Ok((None, "mainline".to_string()));
            }
        };

        // 检测基础分支（通过文件名）
        let branch = detect_current_branch();

        info!("Linux 平台暂不支持自动版本检测");
        warn!("请手动设置版本，或考虑实现 ELF 版本检测");

        // 更新配置
        let mut cfg = CONFIG.write();
        cfg.ryujinx.branch = branch.clone();
        cfg.save()?;

        Ok((None, branch))
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

    #[test]
    #[cfg(target_os = "windows")]
    fn test_detect_version_from_binary() {
        // 从配置中读取 Ryujinx 路径
        let config = get_config();
        let ryujinx_path = PathBuf::from(&config.ryujinx.path);

        // 查找可执行文件
        let exe_path = match get_ryujinx_exe_path_internal(&ryujinx_path) {
            Some(path) => path,
            None => {
                println!("跳过测试: 未找到 Ryujinx 可执行文件在: {}", ryujinx_path.display());
                return;
            }
        };

        println!("测试可执行文件: {}", exe_path.display());
        println!("文件大小: {} bytes", exe_path.metadata().unwrap().len());

        // 调用检测函数
        match detect_version_from_binary(&exe_path) {
            Ok((version, branch)) => {
                println!("\n检测结果:");
                println!("  版本: {:?}", version);
                println!("  分支: {}", branch);

                // 检查结果是否合理
                if let Some(ref v) = version {
                    assert!(!v.is_empty(), "版本号不应为空");
                    println!("\n✓ 检测成功");
                } else {
                    println!("\n✗ 警告: 未检测到版本号");
                }
            }
            Err(e) => {
                println!("\n✗ 检测失败: {}", e);
                panic!("版本检测失败: {}", e);
            }
        }
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_pe_version_resource_raw() {
        use pelite::pe64::{Pe, PeFile};
        use pelite::resources::version_info::Language;

        // 从配置中读取 Ryujinx 路径
        let config = get_config();
        let ryujinx_path = PathBuf::from(&config.ryujinx.path);

        // 查找可执行文件
        let exe_path = match get_ryujinx_exe_path_internal(&ryujinx_path) {
            Some(path) => path,
            None => {
                println!("跳过测试: 未找到 Ryujinx 可执行文件");
                return;
            }
        };

        println!("测试文件: {}", exe_path.display());

        // 读取文件
        let file_data = std::fs::read(&exe_path).expect("读取文件失败");
        let pe = PeFile::from_bytes(&file_data).expect("解析 PE 文件失败");

        // 获取资源
        let resources = pe.resources().expect("获取资源失败");
        let version_info = resources.version_info().expect("获取版本信息失败");

        println!("\n尝试读取版本信息（Language ID: 0x0409, Charset: 1200）:");

        // 尝试默认语言
        let lang = Language { lang_id: 0x0409, charset_id: 1200 };
        let mut found_any = false;

        version_info.strings(lang, |key, value| {
            found_any = true;
            println!("  {}: {}", key, value);
        });

        if !found_any {
            println!("  (未找到任何字符串)");

            // 尝试其他语言
            println!("\n尝试遍历所有可用语言:");
            // 注意: pelite 的 version_info API 可能需要手动枚举语言
            // 这里可以尝试一些常见的语言 ID
            for lang_id in [0x0000, 0x0009, 0x0409, 0x0809] {
                for charset_id in [0, 1200, 1252] {
                    let test_lang = Language { lang_id, charset_id };
                    println!("\n  尝试 Language ID: 0x{:04X}, Charset: {}", lang_id, charset_id);
                    let mut found_in_lang = false;
                    version_info.strings(test_lang, |key, value| {
                        found_in_lang = true;
                        println!("    {}: {}", key, value);
                    });
                    if !found_in_lang {
                        println!("    (空)");
                    }
                }
            }
        }
    }
}

//! 应用更新服务
//!
//! 提供 portable 模式的自动更新功能

use crate::error::{AppError, AppResult};
use crate::models::progress::{ProgressEvent, ProgressStatus, ProgressStep};
use crate::repositories::app_info;
use crate::services::network::{create_client, get_github_download_url};
use crate::utils::archive;
use futures_util::StreamExt;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{Emitter, Window};
use tracing::info;

/// 更新结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateResult {
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: String,
}

/// 下载更新文件
///
/// # Arguments
/// * `window` - Tauri 窗口，用于发送进度事件
/// * `include_prerelease` - 是否包含预发布版本
/// * `download_url` - 可选的下载链接，如果提供则直接使用，否则检查更新获取
pub async fn download_update(
    window: &Window,
    include_prerelease: bool,
    download_url: Option<String>,
) -> AppResult<PathBuf> {
    info!(
        "开始下载更新 (包含预发布: {}, download_url: {:?})",
        include_prerelease, download_url
    );

    // 定义步骤
    let steps = vec![
        ProgressStep {
            id: "check_update".to_string(),
            title: "检查更新".to_string(),
            status: ProgressStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: String::new(),
            eta: String::new(),
            error: None,
        },
        ProgressStep {
            id: "download".to_string(),
            title: "下载更新文件".to_string(),
            status: ProgressStatus::Pending,
            step_type: "download".to_string(),
            progress: 0.0,
            download_speed: String::new(),
            eta: String::new(),
            error: None,
        },
    ];

    // 发送开始事件
    let _ = window.emit(
        "installation-event",
        ProgressEvent::Started {
            steps: steps.clone(),
        },
    );

    // 步骤 1: 检查更新
    let _ = window.emit(
        "installation-event",
        ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "check_update".to_string(),
                title: "检查更新".to_string(),
                status: ProgressStatus::Running,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: String::new(),
                eta: String::new(),
                error: None,
            },
        },
    );

    // 如果提供了 download_url，直接使用；否则检查更新获取
    let download_url = if let Some(url) = download_url {
        info!("使用前端提供的下载链接: {}", url);
        url
    } else {
        info!("未提供下载链接，将调用 check_update 获取");
        let update_check = app_info::check_update(include_prerelease).await?;

        info!("check_update 返回结果:");
        info!("  has_update: {}", update_check.has_update);
        info!("  current_version: {}", update_check.current_version);
        info!("  latest_version: {}", update_check.latest_version);
        info!("  download_url: {:?}", update_check.download_url);

        if !update_check.has_update {
            info!("当前已是最新版本，终止下载");
            let _ = window.emit(
                "installation-event",
                ProgressEvent::StepUpdate {
                    step: ProgressStep {
                        id: "check_update".to_string(),
                        title: "检查更新".to_string(),
                        status: ProgressStatus::Error,
                        step_type: "normal".to_string(),
                        progress: 0.0,
                        download_speed: String::new(),
                        eta: String::new(),
                        error: Some("当前已是最新版本".to_string()),
                    },
                },
            );
            let _ = window.emit(
                "installation-event",
                ProgressEvent::Finished {
                    success: false,
                    message: Some("当前已是最新版本".to_string()),
                },
            );
            return Err(AppError::InvalidArgument("当前已是最新版本".to_string()));
        }

        let url = update_check
            .download_url
            .ok_or_else(|| {
                info!("错误: download_url 为 None，无法下载更新");
                AppError::InvalidArgument("未找到下载链接".to_string())
            })?;

        info!("从 check_update 获取到下载链接: {}", url);
        url
    };

    let _ = window.emit(
        "installation-event",
        ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "check_update".to_string(),
                title: "检查更新".to_string(),
                status: ProgressStatus::Success,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: String::new(),
                eta: String::new(),
                error: None,
            },
        },
    );

    // 步骤 2: 下载更新文件
    info!("下载 URL: {}", download_url);
    let mirror_url = get_github_download_url(&download_url);
    info!("镜像 URL: {}", mirror_url);

    let _ = window.emit(
        "installation-event",
        ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "download".to_string(),
                title: "下载更新文件".to_string(),
                status: ProgressStatus::Running,
                step_type: "download".to_string(),
                progress: 0.0,
                download_speed: String::new(),
                eta: String::new(),
                error: None,
            },
        },
    );

    // 获取当前可执行文件所在目录
    let current_exe = std::env::current_exe()?;
    let current_dir = current_exe.parent().ok_or_else(|| {
        AppError::Unknown("无法获取当前程序目录".to_string())
    })?;

    // 创建下载目录（与Python版本保持一致：./download/upgrade_files）
    let download_dir = current_dir.join("download").join("upgrade_files");
    fs::create_dir_all(&download_dir)?;
    info!("下载目录: {}", download_dir.display());

    // 提取文件名
    let file_name = download_url
        .rsplit('/')
        .next()
        .unwrap_or("update.zip")
        .to_string();
    let download_path = download_dir.join(&file_name);

    // 下载文件
    let client = create_client()?;
    let resp = client.get(&mirror_url).send().await.map_err(|e| {
        AppError::Unknown(format!("下载更新文件失败: {}", e))
    })?;

    if !resp.status().is_success() {
        return Err(AppError::Unknown(format!(
            "下载更新文件失败: HTTP {}",
            resp.status()
        )));
    }

    let total_size = resp.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();
    let mut file = fs::File::create(&download_path)?;
    let start_time = std::time::Instant::now();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| AppError::Unknown(format!("下载数据失败: {}", e)))?;
        std::io::copy(&mut chunk.as_ref(), &mut file)?;
        downloaded += chunk.len() as u64;

        if total_size > 0 {
            let progress = (downloaded as f64 / total_size as f64) * 100.0;
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                downloaded as f64 / elapsed
            } else {
                0.0
            };
            let remaining = if speed > 0.0 {
                ((total_size - downloaded) as f64 / speed) as u64
            } else {
                0
            };

            let speed_str = format_speed(speed);
            let eta_str = format_time(remaining);

            let _ = window.emit(
                "installation-event",
                ProgressEvent::StepUpdate {
                    step: ProgressStep {
                        id: "download".to_string(),
                        title: "下载更新文件".to_string(),
                        status: ProgressStatus::Running,
                        step_type: "download".to_string(),
                        progress,
                        download_speed: speed_str,
                        eta: eta_str,
                        error: None,
                    },
                },
            );
        }
    }

    let _ = window.emit(
        "installation-event",
        ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "download".to_string(),
                title: "下载更新文件".to_string(),
                status: ProgressStatus::Success,
                step_type: "download".to_string(),
                progress: 100.0,
                download_speed: String::new(),
                eta: String::new(),
                error: None,
            },
        },
    );

    let _ = window.emit(
        "installation-event",
        ProgressEvent::Finished {
            success: true,
            message: Some("下载完成".to_string()),
        },
    );

    info!("更新文件已下载到: {}", download_path.display());
    Ok(download_path)
}

/// 安装更新
///
/// # Arguments
/// * `update_file` - 更新文件路径
pub async fn install_update(update_file: &Path) -> AppResult<()> {
    info!("开始安装更新: {}", update_file.display());

    if !update_file.exists() {
        return Err(AppError::InvalidArgument(format!(
            "更新文件不存在: {}",
            update_file.display()
        )));
    }

    // 获取当前可执行文件路径
    let current_exe = std::env::current_exe()?;
    let current_dir = current_exe.parent().ok_or_else(|| {
        AppError::Unknown("无法获取当前程序目录".to_string())
    })?;

    info!("当前程序路径: {}", current_exe.display());
    info!("当前程序目录: {}", current_dir.display());

    // 检查文件扩展名，决定是否需要解压
    let extension = update_file.extension().and_then(|s| s.to_str());

    let upgrade_files_folder = if extension == Some("7z") || extension == Some("zip") {
        // 创建解压目录（与Python版本保持一致：在download文件夹中）
        let extract_dir = current_dir.join("download").join("upgrade_files_extracted");
        if extract_dir.exists() {
            fs::remove_dir_all(&extract_dir)?;
        }
        fs::create_dir_all(&extract_dir)?;

        // 解压更新文件
        info!("解压更新文件到: {}", extract_dir.display());

        if extension == Some("zip") {
            archive::extract_zip(update_file, &extract_dir)?;
        } else {
            archive::extract_7z(update_file, &extract_dir)?;
        }

        // 查找 NsEmuTools 文件夹（解压后的目录结构）
        let nsemutools_dir = extract_dir.join("NsEmuTools");
        if nsemutools_dir.exists() {
            info!("使用解压后的 NsEmuTools 目录: {}", nsemutools_dir.display());
            nsemutools_dir
        } else {
            info!("使用解压目录: {}", extract_dir.display());
            extract_dir
        }
    } else if extension == Some("exe") {
        // 如果是 .exe 文件，直接使用下载目录
        let download_dir = update_file.parent().ok_or_else(|| {
            AppError::Unknown("无法获取更新文件父目录".to_string())
        })?;
        info!("更新文件是 .exe，直接使用下载目录: {}", download_dir.display());
        download_dir.to_path_buf()
    } else {
        return Err(AppError::InvalidArgument(format!(
            "不支持的更新文件格式: {}，仅支持 .exe, .zip, .7z",
            update_file.display()
        )));
    };

    // 查找新的可执行文件
    let new_exe = find_executable(&upgrade_files_folder)?;
    info!("找到新的可执行文件: {}", new_exe.display());

    // 创建更新脚本
    #[cfg(windows)]
    create_windows_update_script(&current_exe, &new_exe, current_dir)?;

    #[cfg(not(windows))]
    return Err(AppError::Unsupported("当前仅支持 Windows 平台".to_string()));

    info!("更新脚本已创建，程序将退出并自动更新");
    Ok(())
}

/// 查找可执行文件
fn find_executable(dir: &Path) -> AppResult<PathBuf> {
    for entry in walkdir::WalkDir::new(dir).max_depth(3) {
        let entry = entry.map_err(|e| AppError::Unknown(format!("遍历目录失败: {}", e)))?;
        let path = entry.path();

        #[cfg(windows)]
        {
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("exe") {
                let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                // 匹配 NsEmuTools.exe 或 NsEmuTools-console.exe
                if file_name.to_lowercase().contains("nsemutools") {
                    return Ok(path.to_path_buf());
                }
            }
        }

        #[cfg(not(windows))]
        {
            if path.is_file() {
                let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if file_name.to_lowercase().contains("nsemutools") {
                    return Ok(path.to_path_buf());
                }
            }
        }
    }

    Err(AppError::Unknown("未找到可执行文件".to_string()))
}

/// 创建 Windows 更新脚本
#[cfg(windows)]
fn create_windows_update_script(
    current_exe: &Path,
    new_exe: &Path,
    target_dir: &Path,
) -> AppResult<()> {
    let script_path = target_dir.join("update.bat");

    // 获取解压后的文件夹路径（与Python版本一致）
    let extract_folder = new_exe.parent().ok_or_else(|| {
        AppError::Unknown("无法获取更新文件父目录".to_string())
    })?;

    let extract_folder_str = extract_folder.to_string_lossy();
    let target_exe = target_dir.join(current_exe.file_name().unwrap());
    let target_exe_str = target_exe.to_string_lossy();

    // 创建与Python版本类似的批处理脚本
    let script_content = format!(
        r#"@echo off
chcp>nul 2>nul 65001
echo 开始准备更新

echo 尝试优雅关闭程序（允许自动清理）...
taskkill /IM NsEmuTools* >nul 2>nul
echo 等待程序正常退出（3秒）...
timeout /t 3 /nobreak

echo 检查是否还有残留进程...
tasklist /FI "IMAGENAME eq NsEmuTools*" 2>nul | find /I "NsEmuTools" >nul
if %ERRORLEVEL% equ 0 (
  echo 程序未能正常退出，强制终止...
  taskkill /F /IM NsEmuTools* >nul 2>nul
  echo 等待进程完全退出...
  timeout /t 3 /nobreak
) else (
  echo 程序已正常退出
)

if exist "{0}.bak" (
  echo 清理旧的备份文件
  del /f /q "{0}.bak"
)

if exist "{0}" (
  echo 备份原文件至 "{0}.bak"
  move /Y "{0}" "{0}.bak"
)

if exist "_internal" (
  echo 备份 _internal 目录
  if exist "_internal_bak" (
    rmdir /s /q "_internal_bak"
  )
  move /Y "_internal" "_internal_bak"
  timeout /t 1 /nobreak
)

if not exist "{1}" (
  echo 无法找到更新文件 "{1}"
  pause
) else (
  echo 复制文件中
  robocopy "{1}" . /MOVE /E /NFL /NDL /NC
  if exist "download\upgrade_files" (
    timeout /t 1 /nobreak
    echo 清理下载的更新文件
    rmdir /s /q "download\upgrade_files"
  )
  if exist "download\upgrade_files_extracted" (
    timeout /t 1 /nobreak
    echo 清理解压的临时文件
    rmdir /s /q "download\upgrade_files_extracted"
  )
  if exist "_internal_bak" (
    echo 清理旧的 _internal_bak 备份
    rmdir /s /q "_internal_bak" 2>nul
  )
  echo 清理完成，准备启动新版本...
  timeout /t 2 /nobreak
  echo 启动程序
  start /b "NsEmuTools" "{0}"
)
del "%~f0" & exit
"#,
        target_exe_str,
        extract_folder_str,
    );

    // 将 LF 换行符替换为 CRLF（Windows 批处理文件需要）
    let script_content_crlf = script_content.replace('\n', "\r\n");

    // 使用 UTF-8 编码（无BOM）保存脚本
    let content_bytes = script_content_crlf.as_bytes();

    // 创建 .bat 文件
    fs::write(&script_path, content_bytes)?;
    info!("更新脚本已创建: {}", script_path.display());

    // 启动更新脚本
    if let Some(script_path_str) = script_path.to_str() {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", script_path_str])
            .spawn()?;
    } else {
        return Err(AppError::InvalidArgument(
            "脚本路径包含无效的 UTF-8 字符".to_string()
        ));
    }

    Ok(())
}

/// 格式化速度
fn format_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec >= 1024.0 * 1024.0 {
        format!("{:.2} MB/s", bytes_per_sec / (1024.0 * 1024.0))
    } else if bytes_per_sec >= 1024.0 {
        format!("{:.2} KB/s", bytes_per_sec / 1024.0)
    } else {
        format!("{:.0} B/s", bytes_per_sec)
    }
}

/// 格式化时间
fn format_time(seconds: u64) -> String {
    if seconds >= 3600 {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    } else if seconds >= 60 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{}s", seconds)
    }
}

/// 根据 tag 更新自身（一体化更新流程，类似 Python 版本）
///
/// # Arguments
/// * `window` - Tauri 窗口，用于发送进度事件
/// * `tag` - 要更新到的版本标签（例如 "v1.2.3"）
pub async fn update_self_by_tag(window: &Window, tag: &str) -> AppResult<PathBuf> {
    info!("开始根据 tag 更新自身: {}", tag);

    // 定义步骤
    let steps = vec![
        ProgressStep {
            id: "get_release_info".to_string(),
            title: "获取版本信息".to_string(),
            status: ProgressStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: String::new(),
            eta: String::new(),
            error: None,
        },
        ProgressStep {
            id: "find_asset".to_string(),
            title: "查找安装包".to_string(),
            status: ProgressStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: String::new(),
            eta: String::new(),
            error: None,
        },
        ProgressStep {
            id: "download".to_string(),
            title: "下载更新文件".to_string(),
            status: ProgressStatus::Pending,
            step_type: "download".to_string(),
            progress: 0.0,
            download_speed: String::new(),
            eta: String::new(),
            error: None,
        },
    ];

    // 发送开始事件
    let _ = window.emit(
        "installation-event",
        ProgressEvent::Started {
            steps: steps.clone(),
        },
    );

    // 步骤 1: 获取指定版本的 release 信息
    let _ = window.emit(
        "installation-event",
        ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "get_release_info".to_string(),
                title: "获取版本信息".to_string(),
                status: ProgressStatus::Running,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: String::new(),
                eta: String::new(),
                error: None,
            },
        },
    );

    let release_info = match app_info::get_release_info_by_tag(tag).await {
        Ok(info) => {
            info!("获取到版本 {} 的信息: {}", tag, info.name);
            info
        }
        Err(e) => {
            let _ = window.emit(
                "installation-event",
                ProgressEvent::StepUpdate {
                    step: ProgressStep {
                        id: "get_release_info".to_string(),
                        title: "获取版本信息".to_string(),
                        status: ProgressStatus::Error,
                        step_type: "normal".to_string(),
                        progress: 0.0,
                        download_speed: String::new(),
                        eta: String::new(),
                        error: Some(e.to_string()),
                    },
                },
            );
            let _ = window.emit(
                "installation-event",
                ProgressEvent::Finished {
                    success: false,
                    message: Some(e.to_string()),
                },
            );
            return Err(e);
        }
    };

    let _ = window.emit(
        "installation-event",
        ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "get_release_info".to_string(),
                title: "获取版本信息".to_string(),
                status: ProgressStatus::Success,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: String::new(),
                eta: String::new(),
                error: None,
            },
        },
    );

    // 步骤 2: 查找 Windows 对应的 asset
    let _ = window.emit(
        "installation-event",
        ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "find_asset".to_string(),
                title: "查找安装包".to_string(),
                status: ProgressStatus::Running,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: String::new(),
                eta: String::new(),
                error: None,
            },
        },
    );

    let asset = match release_info.find_windows_asset() {
        Some(asset) => {
            info!("找到 Windows 资源: {}", asset.name);
            asset
        }
        None => {
            let err_msg = format!("版本 {} 没有找到 Windows 安装包", tag);
            let _ = window.emit(
                "installation-event",
                ProgressEvent::StepUpdate {
                    step: ProgressStep {
                        id: "find_asset".to_string(),
                        title: "查找安装包".to_string(),
                        status: ProgressStatus::Error,
                        step_type: "normal".to_string(),
                        progress: 0.0,
                        download_speed: String::new(),
                        eta: String::new(),
                        error: Some(err_msg.clone()),
                    },
                },
            );
            let _ = window.emit(
                "installation-event",
                ProgressEvent::Finished {
                    success: false,
                    message: Some(err_msg.clone()),
                },
            );
            return Err(AppError::InvalidArgument(err_msg));
        }
    };

    let download_url = asset.download_url.clone();
    let file_name = asset.name.clone();

    let _ = window.emit(
        "installation-event",
        ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "find_asset".to_string(),
                title: "查找安装包".to_string(),
                status: ProgressStatus::Success,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: String::new(),
                eta: String::new(),
                error: None,
            },
        },
    );

    // 步骤 3: 下载更新文件
    info!("开始下载 {}, 版本: [{}]", file_name, tag);

    let mirror_url = get_github_download_url(&download_url);
    info!("镜像 URL: {}", mirror_url);

    let _ = window.emit(
        "installation-event",
        ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "download".to_string(),
                title: "下载更新文件".to_string(),
                status: ProgressStatus::Running,
                step_type: "download".to_string(),
                progress: 0.0,
                download_speed: String::new(),
                eta: String::new(),
                error: None,
            },
        },
    );

    // 获取当前可执行文件所在目录
    let current_exe = std::env::current_exe()?;
    let current_dir = current_exe.parent().ok_or_else(|| {
        AppError::Unknown("无法获取当前程序目录".to_string())
    })?;

    // 创建下载目录（与Python版本保持一致：./download/upgrade_files）
    let download_dir = current_dir.join("download").join("upgrade_files");
    fs::create_dir_all(&download_dir)?;
    info!("下载目录: {}", download_dir.display());

    let download_path = download_dir.join(&file_name);

    // 下载文件
    let client = create_client()?;
    let resp = client.get(&mirror_url).send().await.map_err(|e| {
        let err = AppError::Unknown(format!("下载更新文件失败: {}", e));
        let _ = window.emit(
            "installation-event",
            ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "download".to_string(),
                    title: "下载更新文件".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "download".to_string(),
                    progress: 0.0,
                    download_speed: String::new(),
                    eta: String::new(),
                    error: Some(err.to_string()),
                },
            },
        );
        let _ = window.emit(
            "installation-event",
            ProgressEvent::Finished {
                success: false,
                message: Some(err.to_string()),
            },
        );
        err
    })?;

    if !resp.status().is_success() {
        let err_msg = format!("下载更新文件失败: HTTP {}", resp.status());
        let _ = window.emit(
            "installation-event",
            ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "download".to_string(),
                    title: "下载更新文件".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "download".to_string(),
                    progress: 0.0,
                    download_speed: String::new(),
                    eta: String::new(),
                    error: Some(err_msg.clone()),
                },
            },
        );
        let _ = window.emit(
            "installation-event",
            ProgressEvent::Finished {
                success: false,
                message: Some(err_msg.clone()),
            },
        );
        return Err(AppError::Unknown(err_msg));
    }

    let total_size = resp.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();
    let mut file = fs::File::create(&download_path)?;
    let start_time = std::time::Instant::now();

    info!("开始下载文件，总大小: {} bytes", total_size);

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| AppError::Unknown(format!("下载数据失败: {}", e)))?;
        std::io::copy(&mut chunk.as_ref(), &mut file)?;
        downloaded += chunk.len() as u64;

        if total_size > 0 {
            let progress = (downloaded as f64 / total_size as f64) * 100.0;
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                downloaded as f64 / elapsed
            } else {
                0.0
            };
            let remaining = if speed > 0.0 {
                ((total_size - downloaded) as f64 / speed) as u64
            } else {
                0
            };

            let speed_str = format_speed(speed);
            let eta_str = format_time(remaining);

            let _ = window.emit(
                "installation-event",
                ProgressEvent::StepUpdate {
                    step: ProgressStep {
                        id: "download".to_string(),
                        title: "下载更新文件".to_string(),
                        status: ProgressStatus::Running,
                        step_type: "download".to_string(),
                        progress,
                        download_speed: speed_str,
                        eta: eta_str,
                        error: None,
                    },
                },
            );
        }
    }

    let _ = window.emit(
        "installation-event",
        ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "download".to_string(),
                title: "下载更新文件".to_string(),
                status: ProgressStatus::Success,
                step_type: "download".to_string(),
                progress: 100.0,
                download_speed: String::new(),
                eta: String::new(),
                error: None,
            },
        },
    );

    let _ = window.emit(
        "installation-event",
        ProgressEvent::Finished {
            success: true,
            message: Some("下载完成".to_string()),
        },
    );

    info!("{} 版本 [{}] 已下载至 {}", file_name, tag, download_path.display());
    Ok(download_path)
}

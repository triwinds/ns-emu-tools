//! 应用更新服务
//!
//! 提供 portable 模式的自动更新功能

#[cfg(target_os = "macos")]
use crate::config::app_data_dir;
#[cfg(not(target_os = "macos"))]
use crate::config::effective_config_dir;
use crate::error::{AppError, AppResult};
use crate::models::progress::{ProgressEvent, ProgressStatus, ProgressStep};
use crate::repositories::app_info;
use crate::services::downloader::{get_download_manager, DownloadOptions};
use crate::services::installer::{
    cancelled_step, download_progress_step, error_step, is_cancelled_error_message,
    running_download_step, success_download_step, StepKind, INSTALLATION_EVENT,
};
use crate::services::network::resolve_github_download_target;
use crate::utils::archive;
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

fn update_runtime_dir_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        app_data_dir().join("self-update")
    }

    #[cfg(not(target_os = "macos"))]
    {
        effective_config_dir()
    }
}

fn update_runtime_dir() -> AppResult<PathBuf> {
    let dir = update_runtime_dir_path();
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn update_download_dir_path() -> PathBuf {
    update_runtime_dir_path()
        .join("download")
        .join("upgrade_files")
}

fn update_download_dir() -> AppResult<PathBuf> {
    let _ = update_runtime_dir()?;
    let dir = update_download_dir_path();
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn update_extract_dir_path() -> PathBuf {
    update_runtime_dir_path()
        .join("download")
        .join("upgrade_files_extracted")
}

fn update_extract_dir() -> AppResult<PathBuf> {
    let _ = update_runtime_dir()?;
    Ok(update_extract_dir_path())
}

fn emit_installation_event(window: &Window, event: ProgressEvent) {
    let _ = window.emit(INSTALLATION_EVENT, event);
}

fn emit_installation_step(window: &Window, step: ProgressStep) {
    emit_installation_event(window, ProgressEvent::StepUpdate { step });
}

fn emit_installation_finished(window: &Window, success: bool, message: impl Into<Option<String>>) {
    emit_installation_event(
        window,
        ProgressEvent::Finished {
            success,
            message: message.into(),
        },
    );
}

fn with_download_source(mut step: ProgressStep, download_source: &Option<String>) -> ProgressStep {
    if let Some(download_source) = download_source {
        step = step.with_download_source(download_source.clone());
    }

    step
}

async fn download_update_package(
    window: &Window,
    download_url: &str,
    file_name: &str,
    download_source: Option<String>,
) -> AppResult<PathBuf> {
    const STEP_ID: &str = "download";
    const STEP_TITLE: &str = "下载更新文件";

    emit_installation_step(
        window,
        with_download_source(running_download_step(STEP_ID, STEP_TITLE), &download_source),
    );

    let download_dir = update_download_dir()?;
    info!("下载目录: {}", download_dir.display());

    let download_manager = match get_download_manager().await {
        Ok(manager) => manager,
        Err(error) => {
            let message = error.to_string();
            emit_installation_step(
                window,
                with_download_source(
                    error_step(STEP_ID, STEP_TITLE, StepKind::Download, message.clone()),
                    &download_source,
                ),
            );
            emit_installation_finished(window, false, Some(message));
            return Err(error);
        }
    };

    let options = DownloadOptions {
        save_dir: Some(download_dir),
        filename: Some(file_name.to_string()),
        overwrite: true,
        use_github_mirror: false,
        ..Default::default()
    };

    let progress_window = window.clone();
    let progress_download_source = download_source.clone();
    let result = download_manager
        .download_and_wait(
            download_url,
            options,
            Box::new(move |progress| {
                emit_installation_step(
                    &progress_window,
                    download_progress_step(
                        STEP_ID,
                        STEP_TITLE,
                        &progress,
                        progress_download_source.clone(),
                    ),
                );
            }),
        )
        .await;

    match result {
        Ok(result) => {
            emit_installation_step(
                window,
                with_download_source(success_download_step(STEP_ID, STEP_TITLE), &download_source),
            );
            Ok(result.path)
        }
        Err(error) => {
            let message = error.to_string();
            let step = if is_cancelled_error_message(&message) {
                cancelled_step(STEP_ID, STEP_TITLE, StepKind::Download)
            } else {
                error_step(STEP_ID, STEP_TITLE, StepKind::Download, message.clone())
            };

            emit_installation_step(window, with_download_source(step, &download_source));
            emit_installation_finished(window, false, Some(message));
            Err(error)
        }
    }
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
            downloaded_size: None,
            total_size: None,
            error: None,
            download_source: None,
        },
        ProgressStep {
            id: "download".to_string(),
            title: "下载更新文件".to_string(),
            status: ProgressStatus::Pending,
            step_type: "download".to_string(),
            progress: 0.0,
            download_speed: String::new(),
            eta: String::new(),
            downloaded_size: None,
            total_size: None,
            error: None,
            download_source: None,
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
                downloaded_size: None,
                total_size: None,
                error: None,
                download_source: None,
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
                        downloaded_size: None,
                        total_size: None,
                        error: Some("当前已是最新版本".to_string()),
                        download_source: None,
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

        let url = update_check.download_url.ok_or_else(|| {
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
                downloaded_size: None,
                total_size: None,
                error: None,
                download_source: None,
            },
        },
    );

    // 步骤 2: 下载更新文件
    info!("下载链接：{}", download_url);
    let download_target = resolve_github_download_target(&download_url);
    let mirror_url = download_target.url;
    info!("镜像链接：{}", mirror_url);

    // 获取下载源名称
    let download_source = download_target.source_name;
    info!("下载源: {}", download_source);

    // 提取文件名
    let file_name = download_url
        .rsplit('/')
        .next()
        .unwrap_or("update.zip")
        .to_string();

    let download_path =
        download_update_package(window, &mirror_url, &file_name, Some(download_source)).await?;

    emit_installation_finished(window, true, Some("下载完成".to_string()));

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

    #[cfg(windows)]
    {
        install_update_windows(update_file).await
    }

    #[cfg(target_os = "macos")]
    {
        install_update_macos(update_file).await
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    Err(AppError::Unsupported("当前平台不支持自动更新".to_string()))
}

/// Windows 安装更新
#[cfg(windows)]
async fn install_update_windows(update_file: &Path) -> AppResult<()> {
    // 获取当前可执行文件路径
    let current_exe = std::env::current_exe()?;
    let current_dir = current_exe
        .parent()
        .ok_or_else(|| AppError::Unknown("无法获取当前程序目录".to_string()))?;

    info!("当前程序路径: {}", current_exe.display());
    info!("当前程序目录: {}", current_dir.display());

    // 检查文件扩展名，决定是否需要解压
    let extension = update_file.extension().and_then(|s| s.to_str());

    let upgrade_files_folder = if extension == Some("7z") || extension == Some("zip") {
        // 创建解压目录（与 config.json 保持一致：在 download 文件夹中）
        let extract_dir = update_extract_dir()?;
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
        let download_dir = update_file
            .parent()
            .ok_or_else(|| AppError::Unknown("无法获取更新文件父目录".to_string()))?;
        info!(
            "更新文件是 .exe，直接使用下载目录: {}",
            download_dir.display()
        );
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
    create_windows_update_script(&current_exe, &new_exe, current_dir)?;
    info!("更新脚本已创建，程序将退出并自动更新");
    Ok(())
}

/// macOS 安装更新
#[cfg(target_os = "macos")]
async fn install_update_macos(update_file: &Path) -> AppResult<()> {
    use crate::utils::platform::{find_app_bundle_in_dir, find_current_macos_app_bundle};

    // 1. 识别当前 .app bundle 路径
    let current_app = find_current_macos_app_bundle()?;
    let target_dir = current_app
        .parent()
        .ok_or_else(|| AppError::Unknown("无法获取当前 .app 所在目录".to_string()))?;

    info!("当前 .app bundle: {}", current_app.display());
    info!("目标安装目录: {}", target_dir.display());

    // 检查目标目录写权限
    let test_file = target_dir.join(".ns-emu-tools-write-test");
    match fs::write(&test_file, b"test") {
        Ok(_) => {
            let _ = fs::remove_file(&test_file);
        }
        Err(_) => {
            return Err(AppError::Permission(format!(
                "目标目录无写权限: {}，请将应用移动到有权限的目录后重试",
                target_dir.display()
            )));
        }
    }

    // 2. 解压更新文件到 staging 目录
    let extract_dir = update_extract_dir()?;
    if extract_dir.exists() {
        fs::remove_dir_all(&extract_dir)?;
    }
    fs::create_dir_all(&extract_dir)?;

    let filename = update_file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();

    if filename.ends_with(".zip") {
        info!("解压 macOS zip 到: {}", extract_dir.display());
        archive::extract_zip(update_file, &extract_dir)?;
    } else if filename.ends_with(".tar.gz") || filename.ends_with(".tgz") {
        info!("解压 macOS tar.gz 到: {}", extract_dir.display());
        archive::extract_tar_gz(update_file, &extract_dir)?;
    } else {
        return Err(AppError::InvalidArgument(format!(
            "不支持的 macOS 更新文件格式: {}，仅支持 .zip, .tar.gz",
            update_file.display()
        )));
    }

    // 3. 在 staging 中查找新的 .app bundle
    let new_app = find_app_bundle_in_dir(&extract_dir)?;
    info!("找到新版本 .app bundle: {}", new_app.display());

    // 4. 生成更新 shell 脚本
    let new_app_name = current_app
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("NS Emu Tools.app");
    let target_app = target_dir.join(new_app_name);

    let script_dir = update_runtime_dir()?;
    let script_path = script_dir.join("update.sh");
    create_macos_update_script(&script_path, &current_app, &new_app, &target_app)?;

    // 5. 启动脚本并退出
    std::process::Command::new("bash")
        .arg(&script_path)
        .spawn()
        .map_err(|e| AppError::Process(format!("启动更新脚本失败: {}", e)))?;

    info!("macOS 更新脚本已启动，程序将退出");
    Ok(())
}

/// 查找可执行文件
#[cfg(windows)]
fn find_executable(dir: &Path) -> AppResult<PathBuf> {
    for entry in walkdir::WalkDir::new(dir).max_depth(3) {
        let entry = entry.map_err(|e| AppError::Unknown(format!("遍历目录失败: {}", e)))?;
        let path = entry.path();

        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("exe") {
            let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if file_name.to_lowercase().contains("nsemutools") {
                return Ok(path.to_path_buf());
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

    // 获取更新脚本路径
    let extract_folder = new_exe
        .parent()
        .ok_or_else(|| AppError::Unknown("无法获取更新文件父目录".to_string()))?;

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
        target_exe_str, extract_folder_str,
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
            "脚本路径包含无效的 UTF-8 字符".to_string(),
        ));
    }

    Ok(())
}

/// 创建 macOS 更新 shell 脚本
///
/// 脚本在主进程退出后执行：备份旧 .app → 复制新 .app → 权限处理 → 清理 → 重启
#[cfg(target_os = "macos")]
fn create_macos_update_script(
    script_path: &Path,
    current_app: &Path,
    new_app: &Path,
    target_app: &Path,
) -> AppResult<()> {
    let current_app_str = current_app.to_string_lossy();
    let new_app_str = new_app.to_string_lossy();
    let target_app_str = target_app.to_string_lossy();
    let pid = std::process::id();

    let download_dir = update_download_dir_path();
    let extract_dir = update_extract_dir_path();
    let download_dir_str = download_dir.to_string_lossy();
    let extract_dir_str = extract_dir.to_string_lossy();

    let script_content = format!(
        r#"#!/bin/bash
set -euo pipefail

echo "NS Emu Tools macOS 更新脚本"
echo "等待主进程 (PID {pid}) 退出..."

# 等待主进程退出（最多 30 秒）
for i in $(seq 1 30); do
    if ! kill -0 {pid} 2>/dev/null; then
        echo "主进程已退出"
        break
    fi
    if [ "$i" -eq 30 ]; then
        echo "错误: 主进程未在 30 秒内退出，放弃更新"
        exit 1
    fi
    sleep 1
done

# 备份旧版本
BACKUP_PATH="{current_app_str}.bak"
if [ -d "$BACKUP_PATH" ]; then
    echo "清理旧备份..."
    rm -rf "$BACKUP_PATH"
fi

if [ -d "{current_app_str}" ]; then
    echo "备份当前版本至 $BACKUP_PATH"
    mv "{current_app_str}" "$BACKUP_PATH"
fi

# 复制新版本（ditto 默认保留资源分支和扩展属性）
echo "安装新版本..."
if ! ditto "{new_app_str}" "{target_app_str}"; then
    echo "错误: 复制新版本失败，尝试恢复旧版本..."
    if [ -d "$BACKUP_PATH" ]; then
        mv "$BACKUP_PATH" "{current_app_str}"
    fi
    exit 1
fi

# 移除 quarantine 属性
echo "移除隔离属性..."
xattr -r -d com.apple.quarantine "{target_app_str}" 2>/dev/null || true

# 设置权限
echo "设置权限..."
chmod 755 "{target_app_str}"
# 查找并设置 Contents/MacOS 下的可执行文件
find "{target_app_str}/Contents/MacOS" -type f -exec chmod +x {{}} \;

# 清理 staging 和下载目录
echo "清理临时文件..."
rm -rf "{extract_dir_str}" 2>/dev/null || true
rm -rf "{download_dir_str}" 2>/dev/null || true

# 清理旧备份
echo "清理备份..."
rm -rf "$BACKUP_PATH" 2>/dev/null || true

# 重启应用
echo "启动新版本..."
open "{target_app_str}"

# 自删除
rm -f "$0"
echo "更新完成"
"#,
        pid = pid,
        current_app_str = current_app_str,
        new_app_str = new_app_str,
        target_app_str = target_app_str,
        extract_dir_str = extract_dir_str,
        download_dir_str = download_dir_str,
    );

    fs::write(script_path, script_content.as_bytes())?;

    // 设置执行权限
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(script_path, fs::Permissions::from_mode(0o755))?;
    }

    info!("macOS 更新脚本已创建: {}", script_path.display());
    Ok(())
}

/// 根据 tag 更新自身（一体化更新流程，类似 Python 版本）
///
/// # Arguments
/// * `window` - Tauri 窗口，用于发送进度事件
/// * `tag` - 要更新到的版本标签（例如 "v1.2.3"）
pub async fn update_self_by_tag(window: &Window, tag: &str) -> AppResult<PathBuf> {
    info!("开始根据标签更新程序：{}", tag);

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
            downloaded_size: None,
            total_size: None,
            error: None,
            download_source: None,
        },
        ProgressStep {
            id: "find_asset".to_string(),
            title: "查找安装包".to_string(),
            status: ProgressStatus::Pending,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: String::new(),
            eta: String::new(),
            downloaded_size: None,
            total_size: None,
            error: None,
            download_source: None,
        },
        ProgressStep {
            id: "download".to_string(),
            title: "下载更新文件".to_string(),
            status: ProgressStatus::Pending,
            step_type: "download".to_string(),
            progress: 0.0,
            download_speed: String::new(),
            eta: String::new(),
            downloaded_size: None,
            total_size: None,
            error: None,
            download_source: None,
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
                downloaded_size: None,
                total_size: None,
                error: None,
                download_source: None,
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
                        downloaded_size: None,
                        total_size: None,
                        error: Some(e.to_string()),
                        download_source: None,
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
                downloaded_size: None,
                total_size: None,
                error: None,
                download_source: None,
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
                downloaded_size: None,
                total_size: None,
                error: None,
                download_source: None,
            },
        },
    );

    let asset = match release_info.best_self_update_asset() {
        Some(asset) => {
            info!("找到当前平台更新资源: {}", asset.name);
            asset
        }
        None => {
            let err_msg = format!("版本 {} 没有找到当前平台的安装包", tag);
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
                        downloaded_size: None,
                        total_size: None,
                        error: Some(err_msg.clone()),
                        download_source: None,
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
                downloaded_size: None,
                total_size: None,
                error: None,
                download_source: None,
            },
        },
    );

    // 步骤 3: 下载更新文件
    info!("开始下载 {}，版本：{}", file_name, tag);

    let download_target = resolve_github_download_target(&download_url);
    let mirror_url = download_target.url;
    let download_source = download_target.source_name;
    info!("镜像链接：{}", mirror_url);

    info!("下载源: {}", download_source);

    let download_path =
        download_update_package(window, &mirror_url, &file_name, Some(download_source)).await?;

    emit_installation_finished(window, true, Some("下载完成".to_string()));

    info!(
        "{} 版本 [{}] 已下载至 {}",
        file_name,
        tag,
        download_path.display()
    );
    Ok(download_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_download_dir_uses_platform_runtime_directory() {
        let download_dir = update_download_dir_path();

        #[cfg(target_os = "macos")]
        let expected_base = crate::config::app_data_dir().join("self-update");

        #[cfg(not(target_os = "macos"))]
        let expected_base = crate::config::effective_config_dir();

        assert_eq!(
            download_dir,
            expected_base.join("download").join("upgrade_files")
        );
    }

    #[test]
    fn test_update_extract_dir_uses_platform_runtime_directory() {
        let extract_dir = update_extract_dir_path();

        #[cfg(target_os = "macos")]
        let expected_base = crate::config::app_data_dir().join("self-update");

        #[cfg(not(target_os = "macos"))]
        let expected_base = crate::config::effective_config_dir();

        assert_eq!(
            extract_dir,
            expected_base
                .join("download")
                .join("upgrade_files_extracted")
        );
    }
}

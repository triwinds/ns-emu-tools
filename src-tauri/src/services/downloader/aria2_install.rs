//! Aria2 安装前置检查和下载
//!
//! 将 aria2 安装逻辑从 Aria2Manager 中分离，支持进度回调
//! 使用 RustDownloader 实现下载功能

use crate::error::{AppError, AppResult};
use crate::services::downloader::{DownloadManager, DownloadOptions, RustDownloader};
use std::path::PathBuf;

/// Aria2 安装进度信息
#[derive(Debug, Clone)]
pub struct Aria2InstallProgress {
    pub stage: String,    // "checking", "fetching", "downloading", "extracting", "complete"
    pub percentage: f64,
    pub downloaded: u64,
    pub total: u64,
    pub speed: u64,  // bytes/s
    pub eta: u64,    // seconds
    pub message: String,
}

/// Aria2 安装进度回调类型
pub type Aria2InstallCallback = Box<dyn Fn(Aria2InstallProgress) + Send + Sync + 'static>;

/// 确保 aria2 已安装（带进度回调）
pub async fn ensure_aria2_installed_with_progress(
    on_progress: Option<Aria2InstallCallback>,
) -> AppResult<PathBuf> {
    use super::aria2::try_find_aria2_path;

    // 检查已安装
    if let Ok(path) = try_find_aria2_path() {
        if let Some(cb) = on_progress {
            cb(Aria2InstallProgress {
                stage: "checking".to_string(),
                percentage: 100.0,
                downloaded: 0,
                total: 0,
                speed: 0,
                eta: 0,
                message: "aria2 已安装".to_string(),
            });
        }
        return Ok(path);
    }

    // 非 Windows 平台提示手动安装
    #[cfg(not(target_os = "windows"))]
    {
        let install_instructions = if cfg!(target_os = "macos") {
            "请使用 Homebrew 安装 aria2:\n  brew install aria2\n\n或访问 https://aria2.github.io/ 下载安装"
        } else {
            "请使用包管理器安装 aria2:\n  Ubuntu/Debian: sudo apt install aria2\n  Fedora: sudo dnf install aria2\n  Arch: sudo pacman -S aria2\n\n或访问 https://aria2.github.io/ 下载安装"
        };

        return Err(AppError::Aria2(format!(
            "未找到 aria2 可执行文件。{}",
            install_instructions
        )));
    }

    // Windows: 自动下载安装
    #[cfg(target_os = "windows")]
    {
        download_and_install_aria2(on_progress).await
    }
}

/// Windows: 下载并安装 aria2
#[cfg(target_os = "windows")]
async fn download_and_install_aria2(
    on_progress: Option<Aria2InstallCallback>,
) -> AppResult<PathBuf> {
    use super::aria2::{extract_aria2, get_aria2_install_dir, get_latest_aria2_release};
    use std::sync::Arc;
    use tracing::info;

    // Wrap in Arc to allow sharing across multiple uses
    let on_progress = on_progress.map(Arc::new);

    // 1. 获取最新版本
    if let Some(ref cb) = on_progress {
        cb(Aria2InstallProgress {
            stage: "fetching".to_string(),
            percentage: 0.0,
            downloaded: 0,
            total: 0,
            speed: 0,
            eta: 0,
            message: "获取 aria2 最新版本...".to_string(),
        });
    }

    let release = get_latest_aria2_release().await?;

    // 查找 Windows 64 位版本
    let asset = release
        .assets
        .iter()
        .find(|a| {
            let name = a.name.to_lowercase();
            name.contains("win")
                && name.contains("64bit")
                && (name.ends_with(".zip") || name.ends_with(".7z"))
        })
        .ok_or_else(|| AppError::Aria2("未找到适合的 aria2 Windows 版本".to_string()))?;

    info!("选择下载: {} ({}字节)", asset.name, asset.size);

    // 2. 使用 RustDownloader 下载
    let temp_dir = std::env::temp_dir();
    let install_dir = get_aria2_install_dir()?;

    // 创建 RustDownloader 实例
    let downloader = RustDownloader::new();
    downloader.start().await?;

    let options = DownloadOptions {
        save_dir: Some(temp_dir.clone()),
        filename: Some(asset.name.clone()),
        overwrite: true,
        use_github_mirror: true, // 使用镜像源
        ..Default::default()
    };

    // 捕获 release.tag_name 用于闭包
    let tag_name = release.tag_name.clone();

    // Clone on_progress for use in download callback
    let on_progress_clone = on_progress.clone();

    // 下载（带进度回调）
    let result = downloader
        .download_and_wait(
            &asset.browser_download_url,
            options,
            Box::new(move |download_progress| {
                if let Some(ref cb) = on_progress_clone {
                    cb(Aria2InstallProgress {
                        stage: "downloading".to_string(),
                        percentage: download_progress.percentage,
                        downloaded: download_progress.downloaded,
                        total: download_progress.total,
                        speed: download_progress.speed,
                        eta: download_progress.eta,
                        message: format!("正在下载 aria2 {}...", tag_name),
                    });
                }
            }),
        )
        .await?;

    info!("aria2 下载完成: {}", result.path.display());

    // 3. 解压
    if let Some(ref cb) = on_progress {
        cb(Aria2InstallProgress {
            stage: "extracting".to_string(),
            percentage: 100.0,
            downloaded: result.size,
            total: result.size,
            speed: 0,
            eta: 0,
            message: "正在解压 aria2...".to_string(),
        });
    }

    let aria2c_path = extract_aria2(&result.path, &install_dir)?;

    // 4. 清理临时文件
    let _ = std::fs::remove_file(&result.path);

    if let Some(ref cb) = on_progress {
        cb(Aria2InstallProgress {
            stage: "complete".to_string(),
            percentage: 100.0,
            downloaded: result.size,
            total: result.size,
            speed: 0,
            eta: 0,
            message: "aria2 安装完成".to_string(),
        });
    }

    Ok(aria2c_path)
}

/// 格式化字节大小（用于显示）
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    format!("{:.1}{}", size, UNITS[unit_idx])
}

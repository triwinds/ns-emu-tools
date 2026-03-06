//! 统一下载模块
//!
//! 提供统一的下载接口，支持 aria2 和纯 Rust 实现
//!
//! # 使用方式
//!
//! ```rust,ignore
//! use crate::services::downloader::{get_download_manager, DownloadOptions};
//!
//! let manager = get_download_manager().await?;
//! let result = manager
//!     .download_and_wait(url, DownloadOptions::default(), |progress| {
//!         // 进度回调
//!     })
//!     .await?;
//! ```

pub mod aria2;
pub mod aria2_backend;
pub mod aria2_install;
pub mod chunk_manager;
pub mod client;
pub mod filename;
pub mod manager;
pub mod retry_strategy;
pub mod rust_downloader;
pub mod state_store;
pub mod types;

#[cfg(test)]
mod tests;

use crate::error::{AppError, AppResult};
use once_cell::sync::OnceCell;
use std::sync::Arc;
use tracing::{debug, info, warn};

pub use aria2_backend::Aria2Backend;
pub use aria2_install::{
    ensure_aria2_installed_with_progress, format_bytes, Aria2InstallCallback, Aria2InstallProgress,
};
pub use manager::{DownloadManager, ProgressCallback};
pub use rust_downloader::RustDownloader;
pub use types::{DownloadOptions, DownloadProgress, DownloadResult, DownloadStatus};

/// 全局下载管理器实例
static DOWNLOAD_MANAGER: OnceCell<Arc<dyn DownloadManager>> = OnceCell::new();

/// 下载后端类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadBackend {
    /// 自动选择：优先 aria2，不可用时回退 RustDownloader
    Auto,
    /// 强制使用 aria2
    Aria2,
    /// 强制使用纯 Rust 实现
    Rust,
}

impl From<&str> for DownloadBackend {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "aria2" => Self::Aria2,
            "rust" => Self::Rust,
            _ => Self::Auto,
        }
    }
}

impl Default for DownloadBackend {
    fn default() -> Self {
        Self::Auto
    }
}

/// 初始化下载管理器
///
/// # 参数
/// - `backend`: 下载后端类型
///
/// # 说明
/// - `Auto`: 优先 aria2；若 aria2 启动失败/不可用则回退 RustDownloader
/// - `Aria2`: 强制使用 aria2
/// - `Rust`: 强制使用 RustDownloader
pub async fn init_download_manager(backend: DownloadBackend) -> AppResult<Arc<dyn DownloadManager>> {
    if let Some(manager) = DOWNLOAD_MANAGER.get() {
        return Ok(manager.clone());
    }

    info!("初始化下载管理器，后端类型: {:?}", backend);

    let manager: Arc<dyn DownloadManager> = match backend {
        DownloadBackend::Aria2 => {
            debug!("强制使用 aria2 后端");
            Arc::new(Aria2Backend::from_global().await?)
        }
        DownloadBackend::Rust => {
            info!("使用 RustDownloader 后端");
            let downloader = RustDownloader::new();
            downloader.start().await?;
            Arc::new(downloader)
        }
        DownloadBackend::Auto => {
            debug!("自动选择下载后端");
            // 优先尝试 aria2
            match Aria2Backend::from_global().await {
                Ok(aria2) => {
                    info!("使用 aria2 后端");
                    Arc::new(aria2)
                }
                Err(e) => {
                    warn!("aria2 不可用: {}，回退到 RustDownloader", e);
                    let downloader = RustDownloader::new();
                    downloader.start().await?;
                    Arc::new(downloader)
                }
            }
        }
    };

    DOWNLOAD_MANAGER
        .set(manager.clone())
        .map_err(|_| AppError::Download("下载管理器已经初始化".to_string()))?;

    Ok(manager)
}

/// 获取下载管理器
///
/// 如果未初始化，将自动使用 Auto 模式初始化
pub async fn get_download_manager() -> AppResult<Arc<dyn DownloadManager>> {
    if let Some(manager) = DOWNLOAD_MANAGER.get() {
        return Ok(manager.clone());
    }

    // 从配置读取后端类型
    let backend = {
        let config = crate::config::get_config();
        DownloadBackend::from(config.setting.download.backend.as_str())
    };

    init_download_manager(backend).await
}

/// 重置下载管理器(仅用于测试)
#[cfg(test)]
pub fn reset_download_manager() {
    // OnceCell 不支持重置，测试时需要特殊处理
}

/// 格式化 ETA 时间
pub fn format_eta(seconds: u64) -> String {
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

/// 判断是否需要 aria2（Windows 平台且配置为 auto 或 aria2）
#[cfg(target_os = "windows")]
pub fn should_use_aria2() -> bool {
    let config = crate::config::get_config();
    let backend = config.setting.download.backend.as_str();
    backend == "auto" || backend == "aria2"
}

#[cfg(not(target_os = "windows"))]
pub fn should_use_aria2() -> bool {
    false
}

/// 创建安装初始步骤列表（如果需要 aria2 则自动添加 aria2 检查步骤）
#[cfg(target_os = "windows")]
pub fn create_installation_steps() -> Vec<crate::models::ProgressStep> {
    use crate::models::{ProgressStep, ProgressStatus};

    let mut steps = vec![];

    if should_use_aria2() {
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
    }

    steps
}

#[cfg(not(target_os = "windows"))]
pub fn create_installation_steps() -> Vec<crate::models::ProgressStep> {
    vec![]
}

/// Windows 平台下检查并安装 aria2，带 UI 进度反馈
#[cfg(target_os = "windows")]
pub async fn check_and_install_aria2_with_ui(window: tauri::Window) -> AppResult<()> {
    use crate::models::{ProgressEvent, ProgressStep, ProgressStatus};
    use tauri::Emitter;

    // 如果不需要 aria2，直接返回
    if !should_use_aria2() {
        return Ok(());
    }

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
            Ok(())
        }
        Err(e) => {
            // aria2 安装失败
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
            Err(e)
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub async fn check_and_install_aria2_with_ui(_window: tauri::Window) -> AppResult<()> {
    Ok(())
}

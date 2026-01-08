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

/// 重置下载管理器（仅用于测试）
#[cfg(test)]
pub fn reset_download_manager() {
    // OnceCell 不支持重置，测试时需要特殊处理
}

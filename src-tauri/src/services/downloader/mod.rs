//! 统一下载模块
//!
//! 提供统一的下载接口，支持 rust 和 aria2 fallback
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
pub mod bytehaul_backend;
pub mod manager;
pub mod types;

#[cfg(test)]
mod tests;

use crate::error::AppResult;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

pub use aria2_backend::Aria2Backend;
pub use aria2_install::{
    ensure_aria2_installed_with_progress, format_bytes, Aria2InstallCallback, Aria2InstallProgress,
};
pub use bytehaul_backend::BytehaulBackend;
pub use manager::{DownloadManager, ProgressCallback};
pub use types::{DownloadOptions, DownloadProgress, DownloadResult, DownloadStatus};

/// 全局下载管理器实例
static DOWNLOAD_MANAGER: Lazy<Mutex<Option<ActiveDownloadManager>>> =
    Lazy::new(|| Mutex::new(None));

/// 临时下载管理器注册表。
///
/// 某些下载流程（如 aria2 安装前置下载）会直接创建临时下载器，
/// 不经过全局 `DOWNLOAD_MANAGER`。为了让统一取消入口也能中断这些流程，
/// 需要在下载期间临时注册到这里。
static TRANSIENT_DOWNLOAD_MANAGERS: Lazy<RwLock<HashMap<u64, Arc<dyn DownloadManager>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// 临时下载管理器 ID 生成器。
static TRANSIENT_DOWNLOAD_MANAGER_ID: AtomicU64 = AtomicU64::new(1);

/// 临时下载管理器注册句柄。
///
/// 句柄被丢弃时会自动从注册表中移除对应下载器。
pub struct TransientDownloadManagerGuard {
    id: u64,
}

impl Drop for TransientDownloadManagerGuard {
    fn drop(&mut self) {
        TRANSIENT_DOWNLOAD_MANAGERS.write().remove(&self.id);
    }
}

/// 下载后端类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadBackend {
    /// 自动选择：优先 rust，不可用时回退 aria2
    Auto,
    /// 强制使用 aria2
    Aria2,
    /// 兼容旧配置的 bytehaul 后端别名，内部映射为 rust
    Bytehaul,
    /// 强制使用 rust
    Rust,
}

impl From<&str> for DownloadBackend {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "aria2" => Self::Aria2,
            "bytehaul" => Self::Bytehaul,
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

struct ActiveDownloadManager {
    requested_backend: DownloadBackend,
    manager: Arc<dyn DownloadManager>,
}

#[cfg(test)]
fn auto_backend_candidates() -> [DownloadBackend; 2] {
    [DownloadBackend::Rust, DownloadBackend::Aria2]
}

#[cfg(any(test, target_os = "windows"))]
fn uses_aria2_preflight(backend: DownloadBackend) -> bool {
    matches!(backend, DownloadBackend::Aria2)
}

fn canonical_backend(backend: DownloadBackend) -> DownloadBackend {
    match backend {
        DownloadBackend::Bytehaul => DownloadBackend::Rust,
        other => other,
    }
}

async fn create_started_bytehaul_manager() -> AppResult<Arc<dyn DownloadManager>> {
    let downloader = BytehaulBackend::from_config()?;
    downloader.start().await?;
    Ok(Arc::new(downloader))
}

async fn create_download_manager(backend: DownloadBackend) -> AppResult<Arc<dyn DownloadManager>> {
    if backend == DownloadBackend::Bytehaul {
        warn!("download.backend = bytehaul 已废弃，自动映射为 rust");
    }

    let manager: Arc<dyn DownloadManager> = match canonical_backend(backend) {
        DownloadBackend::Aria2 => {
            debug!("强制使用 aria2 后端");
            Arc::new(Aria2Backend::from_global().await?)
        }
        DownloadBackend::Rust => {
            info!("使用 rust 后端");
            create_started_bytehaul_manager().await?
        }
        DownloadBackend::Auto => {
            debug!("自动选择下载后端");
            match create_started_bytehaul_manager().await {
                Ok(manager) => {
                    info!("Auto 模式使用 rust 后端");
                    manager
                }
                Err(bytehaul_error) => {
                    warn!("rust 后端不可用: {}，回退到 aria2", bytehaul_error);
                    match Aria2Backend::from_global().await {
                        Ok(aria2) => {
                            info!("Auto 模式回退到 aria2 后端");
                            Arc::new(aria2)
                        }
                        Err(aria2_error) => {
                            return Err(crate::error::AppError::Download(format!(
                                "rust 与 aria2 均不可用: rust={}, aria2={}",
                                bytehaul_error, aria2_error
                            )));
                        }
                    }
                }
            }
        }
        DownloadBackend::Bytehaul => {
            unreachable!("bytehaul backend should be canonicalized to rust")
        }
    };

    Ok(manager)
}

/// 初始化下载管理器
///
/// # 参数
/// - `backend`: 下载后端类型
///
/// # 说明
/// - `Auto`: 优先 rust；若 rust 不可用则回退 aria2
/// - `Aria2`: 强制使用 aria2
/// - `Bytehaul`: 兼容旧配置，内部映射为 rust
/// - `Rust`: 强制使用 rust
pub async fn init_download_manager(
    backend: DownloadBackend,
) -> AppResult<Arc<dyn DownloadManager>> {
    let effective_backend = canonical_backend(backend);
    let mut active = DOWNLOAD_MANAGER.lock().await;
    if let Some(current) = active.as_ref() {
        if current.requested_backend == effective_backend {
            return Ok(current.manager.clone());
        }

        info!(
            "检测到下载后端变更，重建下载管理器: {:?} -> {:?}",
            current.requested_backend, effective_backend
        );
    } else {
        info!("初始化下载管理器，后端类型: {:?}", effective_backend);
    }

    if let Some(previous) = active.take() {
        if let Err(err) = previous.manager.stop().await {
            warn!("停止旧下载管理器失败: {}", err);
        }
    }

    let manager = create_download_manager(backend).await?;
    *active = Some(ActiveDownloadManager {
        requested_backend: effective_backend,
        manager: manager.clone(),
    });
    Ok(manager)
}

/// 获取下载管理器
///
/// 如果未初始化，将自动使用 Auto 模式初始化
pub async fn get_download_manager() -> AppResult<Arc<dyn DownloadManager>> {
    let backend = {
        let config = crate::config::get_config();
        DownloadBackend::from(config.setting.download.backend.as_str())
    };

    init_download_manager(backend).await
}

/// 注册一个临时下载管理器，使统一取消入口可以中断它。
pub fn register_transient_download_manager<T>(manager: Arc<T>) -> TransientDownloadManagerGuard
where
    T: DownloadManager + 'static,
{
    let id = TRANSIENT_DOWNLOAD_MANAGER_ID.fetch_add(1, Ordering::SeqCst);
    let manager: Arc<dyn DownloadManager> = manager;
    TRANSIENT_DOWNLOAD_MANAGERS.write().insert(id, manager);

    TransientDownloadManagerGuard { id }
}

/// 取消所有当前活跃的下载。
///
/// 会同时尝试取消：
/// - 已初始化的全局下载管理器中的任务
/// - 注册中的临时下载管理器任务（例如 aria2 安装下载）
pub async fn cancel_active_downloads(remove_files: bool) -> AppResult<Option<String>> {
    let transient_managers: Vec<Arc<dyn DownloadManager>> = TRANSIENT_DOWNLOAD_MANAGERS
        .read()
        .values()
        .cloned()
        .collect();

    let global_manager = {
        let active = DOWNLOAD_MANAGER.lock().await;
        active.as_ref().map(|manager| manager.manager.clone())
    };
    let mut first_path = None;

    for manager in transient_managers {
        let file_path = manager.cancel_all(remove_files).await?;
        if first_path.is_none() {
            first_path = file_path;
        }
    }

    if let Some(manager) = global_manager {
        let file_path = manager.cancel_all(remove_files).await?;
        if first_path.is_none() {
            first_path = file_path;
        }
    }

    Ok(first_path)
}

/// 重置下载管理器(仅用于测试)
#[cfg(test)]
pub fn reset_download_manager() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build test runtime");
    runtime.block_on(async {
        let mut active = DOWNLOAD_MANAGER.lock().await;
        if let Some(manager) = active.take() {
            let _ = manager.manager.stop().await;
        }
    });
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
    uses_aria2_preflight(DownloadBackend::from(
        config.setting.download.backend.as_str(),
    ))
}

#[cfg(not(target_os = "windows"))]
pub fn should_use_aria2() -> bool {
    false
}

/// 创建安装初始步骤列表（如果需要 aria2 则自动添加 aria2 检查步骤）
#[cfg(target_os = "windows")]
pub fn create_installation_steps() -> Vec<crate::models::ProgressStep> {
    use crate::models::{ProgressStatus, ProgressStep};

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
            downloaded_size: None,
            total_size: None,
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
    use crate::models::{ProgressEvent, ProgressStatus, ProgressStep};
    use crate::services::installer::{cancelled_step, is_cancelled_error_message, StepKind};
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
                downloaded_size: None,
                total_size: None,
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
            downloaded_size: Some(format_bytes(progress.downloaded)),
            total_size: Some(if progress.total > 0 {
                format_bytes(progress.total)
            } else {
                "未知".to_string()
            }),
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
                        downloaded_size: None,
                        total_size: None,
                        error: None,
                        download_source: Some("GitHub".to_string()),
                    },
                },
            );
            Ok(())
        }
        Err(e) => {
            let error_message = e.to_string();

            if is_cancelled_error_message(&error_message) {
                let _ = window.emit(
                    "installation-event",
                    ProgressEvent::StepUpdate {
                        step: cancelled_step(
                            "install_aria2",
                            "下载工具安装已取消",
                            StepKind::Download,
                        )
                        .with_download_source("GitHub"),
                    },
                );
                let _ = window.emit(
                    "installation-event",
                    ProgressEvent::Finished {
                        success: false,
                        message: Some("下载工具安装已取消".to_string()),
                    },
                );
                return Err(e);
            }

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
                        downloaded_size: None,
                        total_size: None,
                        error: Some(error_message.clone()),
                        download_source: Some("GitHub".to_string()),
                    },
                },
            );
            let _ = window.emit(
                "installation-event",
                ProgressEvent::Finished {
                    success: false,
                    message: Some(format!("下载工具安装失败: {}", error_message)),
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

//! Aria2 后端适配层
//!
//! 将现有的 Aria2Manager 适配为 DownloadManager trait

use crate::error::AppResult;
use crate::services::downloader::aria2::{
    get_aria2_manager, Aria2DownloadOptions, Aria2DownloadProgress, Aria2DownloadStatus,
    Aria2Manager,
};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::info;

use super::manager::{DownloadManager, ProgressCallback};
use super::types::{DownloadOptions, DownloadProgress, DownloadResult, DownloadStatus};

/// Aria2 后端
///
/// 将 Aria2Manager 适配为 DownloadManager trait
pub struct Aria2Backend {
    manager: Arc<Aria2Manager>,
}

impl Aria2Backend {
    /// 创建新的 Aria2 后端
    pub fn new(manager: Arc<Aria2Manager>) -> Self {
        Self { manager }
    }

    /// 从全局 Aria2Manager 创建后端
    pub async fn from_global() -> AppResult<Self> {
        let manager = get_aria2_manager().await?;
        Ok(Self::new(manager))
    }
}

#[async_trait]
impl DownloadManager for Aria2Backend {
    async fn start(&self) -> AppResult<()> {
        self.manager.start().await
    }

    async fn stop(&self) -> AppResult<()> {
        self.manager.stop().await
    }

    async fn download(&self, url: &str, options: DownloadOptions) -> AppResult<String> {
        let aria2_options = convert_options(options);
        self.manager.download(url, aria2_options).await
    }

    async fn download_and_wait(
        &self,
        url: &str,
        options: DownloadOptions,
        on_progress: ProgressCallback,
    ) -> AppResult<DownloadResult> {
        let aria2_options = convert_options(options);

        // 包装进度回调，转换类型
        let result = self
            .manager
            .download_and_wait(url, aria2_options, move |aria2_progress| {
                let progress = convert_progress(aria2_progress);
                on_progress(progress);
            })
            .await?;

        Ok(DownloadResult {
            path: result.path,
            filename: result.filename,
            size: result.size,
            gid: result.gid,
        })
    }

    async fn pause(&self, task_id: &str) -> AppResult<()> {
        self.manager.pause(task_id).await
    }

    async fn resume(&self, task_id: &str) -> AppResult<()> {
        self.manager.resume(task_id).await
    }

    async fn cancel(&self, task_id: &str) -> AppResult<()> {
        self.manager.cancel(task_id).await
    }

    async fn cancel_all(&self, remove_files: bool) -> AppResult<Option<String>> {
        let file_paths = self.manager.cancel_all().await?;
        let first_path = file_paths.first().cloned();

        if remove_files && !file_paths.is_empty() {
            match Aria2Manager::remove_download_files(&file_paths) {
                Ok(count) => {
                    info!("已删除 {} 个下载文件及其 aria2 控制文件", count);
                }
                Err(e) => {
                    tracing::warn!("删除下载文件时出错: {}", e);
                }
            }
        }

        Ok(first_path)
    }

    async fn get_download_progress(&self, task_id: &str) -> AppResult<DownloadProgress> {
        let aria2_progress = self.manager.get_download_progress(task_id).await?;
        Ok(convert_progress(aria2_progress))
    }

    fn is_started(&self) -> bool {
        self.manager.is_started()
    }
}

/// 转换下载选项
fn convert_options(options: DownloadOptions) -> Aria2DownloadOptions {
    Aria2DownloadOptions {
        save_dir: options.save_dir,
        filename: options.filename,
        overwrite: options.overwrite,
        use_github_mirror: options.use_github_mirror,
        split: options.split,
        max_connection_per_server: options.max_connection_per_server,
        min_split_size: options.min_split_size,
        user_agent: options.user_agent,
        headers: options.headers,
    }
}

/// 转换下载进度
fn convert_progress(aria2_progress: Aria2DownloadProgress) -> DownloadProgress {
    DownloadProgress {
        gid: aria2_progress.gid,
        downloaded: aria2_progress.downloaded,
        total: aria2_progress.total,
        speed: aria2_progress.speed,
        percentage: aria2_progress.percentage,
        eta: aria2_progress.eta,
        filename: aria2_progress.filename,
        status: convert_status(aria2_progress.status),
    }
}

/// 转换下载状态
fn convert_status(aria2_status: Aria2DownloadStatus) -> DownloadStatus {
    match aria2_status {
        Aria2DownloadStatus::Waiting => DownloadStatus::Waiting,
        Aria2DownloadStatus::Active => DownloadStatus::Active,
        Aria2DownloadStatus::Paused => DownloadStatus::Paused,
        Aria2DownloadStatus::Complete => DownloadStatus::Complete,
        Aria2DownloadStatus::Error => DownloadStatus::Error,
        Aria2DownloadStatus::Removed => DownloadStatus::Removed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_status() {
        assert_eq!(
            convert_status(Aria2DownloadStatus::Active),
            DownloadStatus::Active
        );
        assert_eq!(
            convert_status(Aria2DownloadStatus::Complete),
            DownloadStatus::Complete
        );
        assert_eq!(
            convert_status(Aria2DownloadStatus::Error),
            DownloadStatus::Error
        );
    }
}

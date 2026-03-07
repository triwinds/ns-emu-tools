//! 下载管理器 trait 定义
//!
//! 定义统一的下载管理器接口，aria2 和 RustDownloader 都实现此 trait

use crate::error::AppResult;
use async_trait::async_trait;

use super::types::{DownloadOptions, DownloadProgress, DownloadResult};

/// 进度回调类型
pub type ProgressCallback = Box<dyn Fn(DownloadProgress) + Send + 'static>;

/// 下载管理器 trait
///
/// 统一的下载接口，支持 aria2 和纯 Rust 实现
#[async_trait]
pub trait DownloadManager: Send + Sync {
    /// 启动下载管理器
    async fn start(&self) -> AppResult<()>;

    /// 停止下载管理器
    async fn stop(&self) -> AppResult<()>;

    /// 添加下载任务
    ///
    /// # 参数
    /// - `url`: 下载 URL
    /// - `options`: 下载选项
    ///
    /// # 返回
    /// 任务 ID (GID)
    async fn download(&self, url: &str, options: DownloadOptions) -> AppResult<String>;

    /// 下载并等待完成
    ///
    /// # 参数
    /// - `url`: 下载 URL
    /// - `options`: 下载选项
    /// - `on_progress`: 进度回调
    ///
    /// # 返回
    /// 下载结果
    async fn download_and_wait(
        &self,
        url: &str,
        options: DownloadOptions,
        on_progress: ProgressCallback,
    ) -> AppResult<DownloadResult>;

    /// 暂停下载
    async fn pause(&self, task_id: &str) -> AppResult<()>;

    /// 恢复下载
    async fn resume(&self, task_id: &str) -> AppResult<()>;

    /// 取消下载
    async fn cancel(&self, task_id: &str) -> AppResult<()>;

    /// 取消所有下载
    ///
    /// # 参数
    /// - `remove_files`: 是否删除已下载的文件
    ///   - `true`: 删除 `.part` 临时文件 + `.download` 状态文件
    ///   - `false`: 仅停止下载任务，保留文件以便后续恢复
    ///
    /// # 返回
    /// 被取消的文件路径（如果有）
    async fn cancel_all(&self, remove_files: bool) -> AppResult<Option<String>>;

    /// 获取下载进度
    async fn get_download_progress(&self, task_id: &str) -> AppResult<DownloadProgress>;

    /// 是否已启动
    fn is_started(&self) -> bool;
}

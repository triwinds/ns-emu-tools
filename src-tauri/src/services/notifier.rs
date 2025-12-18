//! 消息通知服务
//!
//! 用于向前端发送事件和消息

use crate::models::response::{DownloadProgress, InstallProgress, NotifyMessage};
use tauri::{AppHandle, Emitter};
use tracing::debug;

/// 事件名称常量
pub mod events {
    /// 安装进度事件
    pub const INSTALL_PROGRESS: &str = "install-progress";
    /// 下载进度事件
    pub const DOWNLOAD_PROGRESS: &str = "download-progress";
    /// 消息通知事件
    pub const NOTIFY_MESSAGE: &str = "notify-message";
    /// 日志消息事件
    pub const LOG_MESSAGE: &str = "log-message";
}

/// 消息通知器
pub struct Notifier {
    app: AppHandle,
}

impl Notifier {
    /// 创建新的通知器
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    /// 发送安装进度
    pub fn emit_install_progress(&self, progress: InstallProgress) {
        debug!("发送安装进度: {:?}", progress);
        let _ = self.app.emit(events::INSTALL_PROGRESS, progress);
    }

    /// 发送下载进度
    pub fn emit_download_progress(&self, progress: DownloadProgress) {
        let _ = self.app.emit(events::DOWNLOAD_PROGRESS, progress);
    }

    /// 发送消息通知
    pub fn emit_message(&self, message: NotifyMessage) {
        debug!("发送消息: {:?}", message);
        let _ = self.app.emit(events::NOTIFY_MESSAGE, message);
    }

    /// 发送信息消息
    pub fn info(&self, content: impl Into<String>) {
        self.emit_message(NotifyMessage::info(content));
    }

    /// 发送成功消息
    pub fn success(&self, content: impl Into<String>) {
        self.emit_message(NotifyMessage::success(content));
    }

    /// 发送警告消息
    pub fn warning(&self, content: impl Into<String>) {
        self.emit_message(NotifyMessage::warning(content));
    }

    /// 发送错误消息
    pub fn error(&self, content: impl Into<String>) {
        self.emit_message(NotifyMessage::error(content));
    }

    /// 发送日志消息
    pub fn log(&self, message: impl Into<String>) {
        let _ = self.app.emit(events::LOG_MESSAGE, message.into());
    }
}

/// 全局通知函数（需要 AppHandle）
pub fn emit_to_all<S: serde::Serialize + Clone>(
    app: &AppHandle,
    event: &str,
    payload: S,
) {
    let _ = app.emit(event, payload);
}

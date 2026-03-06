//! 服务模块
//!
//! 业务逻辑层，处理核心功能

pub mod cheats;
pub mod cheats_parser;
pub mod crypto;
pub mod doh;
pub mod downloader;
pub mod firmware;
pub mod keys;
pub mod msvc;
pub mod nca;
pub mod network;
pub mod ryujinx;
pub mod save_manager;
pub mod updater;
pub mod yuzu;

#[cfg(not(test))]
pub mod notifier;

// 在测试模式下提供 notifier 的 stub 实现
#[cfg(test)]
pub mod notifier {
    //! 测试模式下的 notifier stub

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

    /// 发送通知（测试模式下为空实现）
    pub fn send_notify(_window: &tauri::Window, _message: &str) -> Result<(), tauri::Error> {
        // 在测试模式下不做任何事情
        Ok(())
    }
}

pub use cheats::*;
pub use doh::*;
pub use downloader::aria2::*;
pub use downloader::{get_download_manager, DownloadManager, DownloadOptions, DownloadProgress, DownloadResult, DownloadStatus, ProgressCallback};
pub use firmware::*;
pub use nca::*;
pub use network::*;
pub use ryujinx::*;
pub use updater::*;
pub use yuzu::*;

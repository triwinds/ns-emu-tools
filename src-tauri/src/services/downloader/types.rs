//! 下载模块统一数据类型
//!
//! 定义与 Aria2 兼容的数据结构，用于统一下载接口

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// 下载选项
#[derive(Debug, Clone)]
pub struct DownloadOptions {
    /// 保存目录
    pub save_dir: Option<PathBuf>,
    /// 自定义文件名
    pub filename: Option<String>,
    /// 是否覆盖已存在的文件
    pub overwrite: bool,
    /// 使用 GitHub 镜像
    pub use_github_mirror: bool,
    /// 分块数量
    pub split: u32,
    /// 每服务器最大连接数
    pub max_connection_per_server: u32,
    /// 最小分块大小
    pub min_split_size: String,
    /// 自定义 User-Agent
    pub user_agent: Option<String>,
    /// 额外的 HTTP 头
    pub headers: HashMap<String, String>,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            save_dir: None,
            filename: None,
            overwrite: false,
            use_github_mirror: true,
            split: 4,
            max_connection_per_server: 4,
            min_split_size: "4M".to_string(),
            user_agent: None,
            headers: HashMap::new(),
        }
    }
}

impl DownloadOptions {
    /// 创建高速下载选项（用于代理环境）
    pub fn high_speed() -> Self {
        Self {
            split: 16,
            max_connection_per_server: 16,
            min_split_size: "4M".to_string(),
            ..Default::default()
        }
    }

    /// 创建 CDN 友好的下载选项
    pub fn cdn_friendly() -> Self {
        Self {
            split: 4,
            max_connection_per_server: 4,
            min_split_size: "12M".to_string(),
            ..Default::default()
        }
    }
}

/// 下载进度信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    /// 下载 GID（任务 ID）
    pub gid: String,
    /// 已下载字节数
    pub downloaded: u64,
    /// 总字节数（0 表示未知长度）
    pub total: u64,
    /// 下载速度（字节/秒）
    pub speed: u64,
    /// 进度百分比（-1 表示无法计算）
    pub percentage: f64,
    /// 预计剩余时间（秒，u64::MAX 表示未知）
    pub eta: u64,
    /// 文件名
    pub filename: String,
    /// 下载状态
    pub status: DownloadStatus,
}

impl DownloadProgress {
    /// 创建新的下载进度
    pub fn new(gid: &str, filename: &str) -> Self {
        Self {
            gid: gid.to_string(),
            downloaded: 0,
            total: 0,
            speed: 0,
            percentage: 0.0,
            eta: 0,
            filename: filename.to_string(),
            status: DownloadStatus::Waiting,
        }
    }

    /// 创建未知长度下载的进度
    pub fn from_unknown_length(downloaded: u64, speed: u64, filename: &str, gid: &str) -> Self {
        Self {
            gid: gid.to_string(),
            downloaded,
            total: 0,
            speed,
            percentage: -1.0,
            eta: u64::MAX,
            filename: filename.to_string(),
            status: DownloadStatus::Active,
        }
    }

    /// 格式化已下载大小
    pub fn downloaded_string(&self) -> String {
        format_bytes(self.downloaded)
    }

    /// 格式化总大小
    pub fn total_string(&self) -> String {
        format_bytes(self.total)
    }

    /// 格式化速度
    pub fn speed_string(&self) -> String {
        format!("{}/s", format_bytes(self.speed))
    }

    /// 格式化 ETA
    pub fn eta_string(&self) -> String {
        if self.eta == 0 {
            return "0s".to_string();
        }

        if self.eta == u64::MAX {
            return "--:--".to_string();
        }

        let hours = self.eta / 3600;
        let minutes = (self.eta % 3600) / 60;
        let seconds = self.eta % 60;

        if hours > 0 {
            format!("{}h{}m{}s", hours, minutes, seconds)
        } else if minutes > 0 {
            format!("{}m{}s", minutes, seconds)
        } else {
            format!("{}s", seconds)
        }
    }
}

/// 下载状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    /// 等待中
    Waiting,
    /// 下载中
    Active,
    /// 已暂停
    Paused,
    /// 已完成
    Complete,
    /// 出错
    Error,
    /// 已移除
    Removed,
}

impl From<&str> for DownloadStatus {
    fn from(s: &str) -> Self {
        match s {
            "waiting" => Self::Waiting,
            "active" => Self::Active,
            "paused" => Self::Paused,
            "complete" => Self::Complete,
            "error" => Self::Error,
            "removed" => Self::Removed,
            _ => Self::Waiting,
        }
    }
}

/// 下载结果
#[derive(Debug, Clone)]
pub struct DownloadResult {
    /// 保存路径
    pub path: PathBuf,
    /// 文件名
    pub filename: String,
    /// 文件大小
    pub size: u64,
    /// GID
    pub gid: String,
}

/// 格式化文件大小
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    format!("{:.1}{}", size, UNITS[unit_index])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0.0B");
        assert_eq!(format_bytes(1023), "1023.0B");
        assert_eq!(format_bytes(1024), "1.0KiB");
        assert_eq!(format_bytes(1024 * 1024), "1.0MiB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0GiB");
    }

    #[test]
    fn test_download_status_from_str() {
        assert_eq!(DownloadStatus::from("active"), DownloadStatus::Active);
        assert_eq!(DownloadStatus::from("complete"), DownloadStatus::Complete);
        assert_eq!(DownloadStatus::from("error"), DownloadStatus::Error);
        assert_eq!(DownloadStatus::from("unknown"), DownloadStatus::Waiting);
    }

    #[test]
    fn test_download_progress() {
        let mut progress = DownloadProgress::new("abc123", "test.zip");
        progress.downloaded = 512 * 1024;
        progress.total = 1024 * 1024;
        progress.speed = 100 * 1024;
        progress.percentage = 50.0;
        progress.eta = 5;

        assert_eq!(progress.downloaded_string(), "512.0KiB");
        assert_eq!(progress.total_string(), "1.0MiB");
        assert_eq!(progress.speed_string(), "100.0KiB/s");
        assert_eq!(progress.eta_string(), "5s");
    }

    #[test]
    fn test_unknown_length_progress() {
        let progress = DownloadProgress::from_unknown_length(1024, 512, "test.bin", "gid123");
        assert_eq!(progress.percentage, -1.0);
        assert_eq!(progress.eta, u64::MAX);
        assert_eq!(progress.eta_string(), "--:--");
    }
}

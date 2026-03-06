//! 通用工具函数

use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tracing::{debug, info};

/// 检查路径是否存在
pub fn path_exists(path: &str) -> bool {
    Path::new(path).exists()
}

/// 检查是否为目录
pub fn is_directory(path: &str) -> bool {
    Path::new(path).is_dir()
}

/// 检查是否为文件
pub fn is_file(path: &str) -> bool {
    Path::new(path).is_file()
}

/// 获取文件扩展名
pub fn get_extension(path: &str) -> Option<String> {
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase())
}

/// 格式化文件大小
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// 格式化速度
pub fn format_speed(bytes_per_sec: u64) -> String {
    format!("{}/s", format_size(bytes_per_sec))
}

/// 格式化持续时间
pub fn format_duration(seconds: u64) -> String {
    if seconds >= 3600 {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        format!("{}h {}m", hours, minutes)
    } else if seconds >= 60 {
        let minutes = seconds / 60;
        let secs = seconds % 60;
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", seconds)
    }
}

/// 规范化路径并去除 Windows 长路径前缀
///
/// Windows 的 `canonicalize()` 会返回带有 `\\?\` 前缀的路径，
/// 这个函数会去除该前缀以便正常显示
pub fn normalize_path(path: &Path) -> PathBuf {
    let normalized = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());

    // 去除 Windows 长路径前缀 \\?\
    let path_str = normalized.to_string_lossy();
    if path_str.starts_with(r"\\?\") {
        PathBuf::from(&path_str[4..])
    } else {
        normalized
    }
}

/// 检查文件 MD5
pub async fn check_file_md5(file_path: &Path, target_md5: &str) -> AppResult<bool> {
    if target_md5.is_empty() {
        return Ok(true);
    }

    if !file_path.exists() {
        return Err(AppError::FileNotFound(file_path.display().to_string()));
    }

    info!("计算文件 MD5: {}", file_path.display());

    let mut file = File::open(file_path).await?;
    let mut hasher = md5::Context::new();
    let mut buffer = vec![0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer).await?;
        if bytes_read == 0 {
            break;
        }
        hasher.consume(&buffer[..bytes_read]);
    }

    let digest = hasher.compute();
    let file_md5 = format!("{:x}", digest);

    debug!("文件 MD5: {}, 目标 MD5: {}", file_md5, target_md5);

    Ok(file_md5.to_lowercase() == target_md5.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1536), "1.50 KB");
        assert_eq!(format_size(1048576), "1.00 MB");
        assert_eq!(format_size(1073741824), "1.00 GB");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(3661), "1h 1m");
    }

    #[test]
    fn test_get_extension() {
        assert_eq!(get_extension("test.zip"), Some("zip".to_string()));
        assert_eq!(get_extension("test.TAR.GZ"), Some("gz".to_string()));
        assert_eq!(get_extension("noext"), None);
    }
}

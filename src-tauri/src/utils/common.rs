//! 通用工具函数

use std::path::Path;

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

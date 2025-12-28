//! 下载模块测试
//!
//! Phase 1 验证点测试：
//! 1. 编译通过
//! 2. 类型转换正确
//! 3. 配置项可以正确读取

#[cfg(test)]
mod tests {
    use super::super::types::*;
    use super::super::manager::ProgressCallback;

    #[test]
    fn test_download_options_default() {
        let options = DownloadOptions::default();

        assert_eq!(options.overwrite, false);
        assert_eq!(options.use_github_mirror, true);
        assert_eq!(options.split, 4);
        assert_eq!(options.max_connection_per_server, 4);
        assert_eq!(options.min_split_size, "4M");
        assert!(options.save_dir.is_none());
        assert!(options.filename.is_none());
        assert!(options.user_agent.is_none());
        assert!(options.headers.is_empty());
    }

    #[test]
    fn test_download_options_high_speed() {
        let options = DownloadOptions::high_speed();

        assert_eq!(options.split, 16);
        assert_eq!(options.max_connection_per_server, 16);
        assert_eq!(options.min_split_size, "4M");
    }

    #[test]
    fn test_download_options_cdn_friendly() {
        let options = DownloadOptions::cdn_friendly();

        assert_eq!(options.split, 4);
        assert_eq!(options.max_connection_per_server, 4);
        assert_eq!(options.min_split_size, "12M");
    }

    #[test]
    fn test_download_status_from_str() {
        assert_eq!(DownloadStatus::from("waiting"), DownloadStatus::Waiting);
        assert_eq!(DownloadStatus::from("active"), DownloadStatus::Active);
        assert_eq!(DownloadStatus::from("paused"), DownloadStatus::Paused);
        assert_eq!(DownloadStatus::from("complete"), DownloadStatus::Complete);
        assert_eq!(DownloadStatus::from("error"), DownloadStatus::Error);
        assert_eq!(DownloadStatus::from("removed"), DownloadStatus::Removed);
        assert_eq!(DownloadStatus::from("unknown"), DownloadStatus::Waiting);
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0.0B");
        assert_eq!(format_bytes(512), "512.0B");
        assert_eq!(format_bytes(1023), "1023.0B");
        assert_eq!(format_bytes(1024), "1.0KiB");
        assert_eq!(format_bytes(1536), "1.5KiB");
        assert_eq!(format_bytes(1024 * 1024), "1.0MiB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0GiB");
        assert_eq!(format_bytes(1024u64 * 1024 * 1024 * 1024), "1.0TiB");
    }

    #[test]
    fn test_download_progress_new() {
        let progress = DownloadProgress::new("test-gid", "test.zip");

        assert_eq!(progress.gid, "test-gid");
        assert_eq!(progress.filename, "test.zip");
        assert_eq!(progress.downloaded, 0);
        assert_eq!(progress.total, 0);
        assert_eq!(progress.speed, 0);
        assert_eq!(progress.percentage, 0.0);
        assert_eq!(progress.eta, 0);
        assert_eq!(progress.status, DownloadStatus::Waiting);
    }

    #[test]
    fn test_download_progress_from_unknown_length() {
        let progress = DownloadProgress::from_unknown_length(1024, 512, "test.bin", "gid123");

        assert_eq!(progress.gid, "gid123");
        assert_eq!(progress.filename, "test.bin");
        assert_eq!(progress.downloaded, 1024);
        assert_eq!(progress.total, 0);
        assert_eq!(progress.speed, 512);
        assert_eq!(progress.percentage, -1.0);
        assert_eq!(progress.eta, u64::MAX);
        assert_eq!(progress.status, DownloadStatus::Active);
    }

    #[test]
    fn test_download_progress_formatting() {
        let mut progress = DownloadProgress::new("gid", "file.zip");
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
    fn test_download_progress_eta_formatting() {
        let mut progress = DownloadProgress::new("gid", "file.zip");

        // 测试 0 秒
        progress.eta = 0;
        assert_eq!(progress.eta_string(), "0s");

        // 测试未知时间
        progress.eta = u64::MAX;
        assert_eq!(progress.eta_string(), "--:--");

        // 测试秒
        progress.eta = 45;
        assert_eq!(progress.eta_string(), "45s");

        // 测试分钟
        progress.eta = 90;
        assert_eq!(progress.eta_string(), "1m30s");

        // 测试小时
        progress.eta = 3665;
        assert_eq!(progress.eta_string(), "1h1m5s");
    }

    #[test]
    fn test_progress_callback_type() {
        // 测试 ProgressCallback 类型可以正确创建
        let _callback: ProgressCallback = Box::new(|progress| {
            assert!(progress.percentage >= 0.0);
        });
    }

    #[test]
    fn test_download_backend_from_str() {
        use super::super::DownloadBackend;

        assert_eq!(DownloadBackend::from("aria2"), DownloadBackend::Aria2);
        assert_eq!(DownloadBackend::from("rust"), DownloadBackend::Rust);
        assert_eq!(DownloadBackend::from("auto"), DownloadBackend::Auto);
        assert_eq!(DownloadBackend::from("unknown"), DownloadBackend::Auto);
        assert_eq!(DownloadBackend::from(""), DownloadBackend::Auto);
    }

    #[test]
    fn test_download_backend_default() {
        use super::super::DownloadBackend;

        assert_eq!(DownloadBackend::default(), DownloadBackend::Auto);
    }
}

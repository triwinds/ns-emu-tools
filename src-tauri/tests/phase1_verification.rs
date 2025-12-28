//! Phase 1 验证测试
//!
//! 独立测试文件，验证 download 模块的基础功能

use ns_emu_tools_lib::services::download::types::*;
use ns_emu_tools_lib::services::download::DownloadBackend;

#[test]
fn test_phase1_download_options() {
    // 验证点 1: 类型定义正确
    let options = DownloadOptions::default();
    assert_eq!(options.use_github_mirror, true);
    assert_eq!(options.split, 4);

    let high_speed = DownloadOptions::high_speed();
    assert_eq!(high_speed.split, 16);

    let cdn = DownloadOptions::cdn_friendly();
    assert_eq!(cdn.min_split_size, "12M");
}

#[test]
fn test_phase1_download_status() {
    // 验证点 2: 状态转换正确
    assert_eq!(DownloadStatus::from("active"), DownloadStatus::Active);
    assert_eq!(DownloadStatus::from("complete"), DownloadStatus::Complete);
    assert_eq!(DownloadStatus::from("error"), DownloadStatus::Error);
}

#[test]
fn test_phase1_format_bytes() {
    // 验证点 3: 工具函数正确
    assert_eq!(format_bytes(0), "0.0B");
    assert_eq!(format_bytes(1024), "1.0KiB");
    assert_eq!(format_bytes(1024 * 1024), "1.0MiB");
}

#[test]
fn test_phase1_download_progress() {
    // 验证点 4: 进度信息结构正确
    let progress = DownloadProgress::new("gid123", "test.zip");
    assert_eq!(progress.gid, "gid123");
    assert_eq!(progress.filename, "test.zip");
    assert_eq!(progress.status, DownloadStatus::Waiting);

    // 测试未知长度下载
    let unknown = DownloadProgress::from_unknown_length(1024, 512, "file.bin", "gid456");
    assert_eq!(unknown.percentage, -1.0);
    assert_eq!(unknown.eta, u64::MAX);
    assert_eq!(unknown.eta_string(), "--:--");
}

#[test]
fn test_phase1_download_backend() {
    // 验证点 5: 后端选择正确
    assert_eq!(DownloadBackend::from("aria2"), DownloadBackend::Aria2);
    assert_eq!(DownloadBackend::from("rust"), DownloadBackend::Rust);
    assert_eq!(DownloadBackend::from("auto"), DownloadBackend::Auto);
    assert_eq!(DownloadBackend::default(), DownloadBackend::Auto);
}

#[test]
fn test_phase1_eta_formatting() {
    // 验证点 6: ETA 格式化正确
    let mut progress = DownloadProgress::new("gid", "file.zip");

    progress.eta = 0;
    assert_eq!(progress.eta_string(), "0s");

    progress.eta = 45;
    assert_eq!(progress.eta_string(), "45s");

    progress.eta = 90;
    assert_eq!(progress.eta_string(), "1m30s");

    progress.eta = 3665;
    assert_eq!(progress.eta_string(), "1h1m5s");

    progress.eta = u64::MAX;
    assert_eq!(progress.eta_string(), "--:--");
}

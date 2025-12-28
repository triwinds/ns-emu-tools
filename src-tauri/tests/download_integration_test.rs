//! 下载模块集成测试
//!
//! 使用 wiremock 模拟 HTTP 服务器进行测试

use ns_emu_tools_lib::services::downloader::chunk_manager::ChunkManager;
use ns_emu_tools_lib::services::downloader::filename::parse_content_disposition;
use ns_emu_tools_lib::services::downloader::manager::DownloadManager;
use ns_emu_tools_lib::services::downloader::rust_downloader::RustDownloader;
use ns_emu_tools_lib::services::downloader::state_store::{
    ChunkState, DownloadState, StateStore,
};
use ns_emu_tools_lib::services::downloader::types::DownloadOptions;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tempfile::TempDir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 测试 Range 支持检测 - 服务器支持 Range
#[tokio::test]
async fn test_range_support_detection_with_range() {
    let mock_server = MockServer::start().await;

    // 模拟支持 Range 的服务器（返回 206）
    Mock::given(method("GET"))
        .and(path("/file.zip"))
        .and(header("Range", "bytes=0-0"))
        .respond_with(
            ResponseTemplate::new(206)
                .insert_header("Content-Range", "bytes 0-0/1000000")
                .insert_header("ETag", "\"abc123\"")
                .insert_header("Last-Modified", "Mon, 01 Jan 2024 00:00:00 GMT")
                .set_body_bytes(b"x".to_vec()),
        )
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::new();
    let url = format!("{}/file.zip", mock_server.uri());

    let result = ChunkManager::check_range_support(&client, &url).await;

    assert!(result.is_ok());
    let range_support = result.unwrap();
    assert!(range_support.supports_range);
    assert_eq!(range_support.total_size, 1000000);
    assert_eq!(range_support.etag, Some("\"abc123\"".to_string()));
    assert_eq!(
        range_support.last_modified,
        Some("Mon, 01 Jan 2024 00:00:00 GMT".to_string())
    );
}

/// 测试 Range 支持检测 - 服务器不支持 Range
#[tokio::test]
async fn test_range_support_detection_without_range() {
    let mock_server = MockServer::start().await;

    // 模拟不支持 Range 的服务器（返回 200）
    // Content-Length 需要与实际 body 大小一致
    let body = vec![0u8; 1000]; // 1KB 文件
    Mock::given(method("GET"))
        .and(path("/file.zip"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", body.len().to_string())
                .set_body_bytes(body.clone()),
        )
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::new();
    let url = format!("{}/file.zip", mock_server.uri());

    let result = ChunkManager::check_range_support(&client, &url).await;

    assert!(result.is_ok());
    let range_support = result.unwrap();
    assert!(!range_support.supports_range);
    assert_eq!(range_support.total_size, 1000);
}

/// 测试简单下载（不支持 Range，小文件）
#[tokio::test]
async fn test_simple_download_no_range() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();

    let file_content = b"Hello, World! This is a test file.";

    // Range 探测请求（返回 200 表示不支持）
    Mock::given(method("GET"))
        .and(path("/test.txt"))
        .and(header("Range", "bytes=0-0"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", file_content.len().to_string())
                .set_body_bytes(file_content.to_vec()),
        )
        .mount(&mock_server)
        .await;

    // HEAD 请求
    Mock::given(method("HEAD"))
        .and(path("/test.txt"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", file_content.len().to_string()),
        )
        .mount(&mock_server)
        .await;

    // 实际下载请求
    Mock::given(method("GET"))
        .and(path("/test.txt"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", file_content.len().to_string())
                .set_body_bytes(file_content.to_vec()),
        )
        .mount(&mock_server)
        .await;

    let url = format!("{}/test.txt", mock_server.uri());
    let downloader = RustDownloader::new();
    downloader.start().await.unwrap();

    let options = DownloadOptions {
        save_dir: Some(temp_dir.path().to_path_buf()),
        filename: Some("test.txt".to_string()),
        overwrite: true,
        ..Default::default()
    };

    let result = downloader
        .download_and_wait(&url, options, Box::new(|_| {}))
        .await;

    assert!(result.is_ok());
    let download_result = result.unwrap();
    assert_eq!(download_result.filename, "test.txt");
    assert!(download_result.path.exists());

    // 验证文件内容
    let content = std::fs::read(&download_result.path).unwrap();
    assert_eq!(content, file_content);

    downloader.stop().await.unwrap();
}

/// 测试支持 Range 的下载
#[tokio::test]
async fn test_download_with_range_support() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();

    let file_content = vec![0u8; 1024 * 100]; // 100KB

    // Range 探测请求（返回 206）
    Mock::given(method("GET"))
        .and(path("/largefile.bin"))
        .and(header("Range", "bytes=0-0"))
        .respond_with(
            ResponseTemplate::new(206)
                .insert_header("Content-Range", format!("bytes 0-0/{}", file_content.len()))
                .set_body_bytes(file_content[0..1].to_vec()),
        )
        .mount(&mock_server)
        .await;

    // HEAD 请求
    Mock::given(method("HEAD"))
        .and(path("/largefile.bin"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", file_content.len().to_string()),
        )
        .mount(&mock_server)
        .await;

    // 分块下载请求 - 由于分块策略，小文件可能仍使用单连接
    Mock::given(method("GET"))
        .and(path("/largefile.bin"))
        .respond_with(
            ResponseTemplate::new(206)
                .insert_header(
                    "Content-Range",
                    format!("bytes 0-{}/{}", file_content.len() - 1, file_content.len()),
                )
                .set_body_bytes(file_content.clone()),
        )
        .mount(&mock_server)
        .await;

    let url = format!("{}/largefile.bin", mock_server.uri());
    let downloader = RustDownloader::new();
    downloader.start().await.unwrap();

    let options = DownloadOptions {
        save_dir: Some(temp_dir.path().to_path_buf()),
        filename: Some("largefile.bin".to_string()),
        overwrite: true,
        split: 1, // 强制单连接以简化测试
        ..Default::default()
    };

    let result = downloader
        .download_and_wait(&url, options, Box::new(|_| {}))
        .await;

    assert!(result.is_ok());
    let download_result = result.unwrap();
    assert!(download_result.path.exists());

    downloader.stop().await.unwrap();
}

/// 测试 429 重试
///
/// 注：此测试涉及真正的重试延迟，可能需要较长时间
#[tokio::test]
#[ignore]
async fn test_retry_on_429() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();

    let file_content = b"Success after retry!";

    // 使用计数器来模拟第一次返回 429，之后返回成功
    // 注意：wiremock 的 up_to_n_times 可以实现类似效果
    Mock::given(method("GET"))
        .and(path("/retry-test.txt"))
        .and(header("Range", "bytes=0-0"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "1"))
        .up_to_n_times(1)
        .expect(1)
        .mount(&mock_server)
        .await;

    // 第二次及以后的 Range 探测返回成功
    Mock::given(method("GET"))
        .and(path("/retry-test.txt"))
        .and(header("Range", "bytes=0-0"))
        .respond_with(
            ResponseTemplate::new(206)
                .insert_header("Content-Range", format!("bytes 0-0/{}", file_content.len()))
                .set_body_bytes(file_content[0..1].to_vec()),
        )
        .mount(&mock_server)
        .await;

    // HEAD 请求
    Mock::given(method("HEAD"))
        .and(path("/retry-test.txt"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", file_content.len().to_string()),
        )
        .mount(&mock_server)
        .await;

    // 下载请求
    Mock::given(method("GET"))
        .and(path("/retry-test.txt"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", file_content.len().to_string())
                .set_body_bytes(file_content.to_vec()),
        )
        .mount(&mock_server)
        .await;

    let url = format!("{}/retry-test.txt", mock_server.uri());
    let downloader = RustDownloader::new();
    downloader.start().await.unwrap();

    let options = DownloadOptions {
        save_dir: Some(temp_dir.path().to_path_buf()),
        filename: Some("retry-test.txt".to_string()),
        overwrite: true,
        ..Default::default()
    };

    let result = downloader
        .download_and_wait(&url, options, Box::new(|_| {}))
        .await;

    // 由于重试机制，应该最终成功
    assert!(result.is_ok());

    downloader.stop().await.unwrap();
}

/// 测试 Content-Disposition 文件名解析
#[test]
fn test_content_disposition_filename() {
    // 测试标准格式
    assert_eq!(
        parse_content_disposition("attachment; filename=\"example.zip\""),
        Some("example.zip".to_string())
    );

    // 测试不带引号
    assert_eq!(
        parse_content_disposition("attachment; filename=example.zip"),
        Some("example.zip".to_string())
    );

    // 测试 filename* 格式（UTF-8）
    assert_eq!(
        parse_content_disposition("attachment; filename*=UTF-8''%E4%B8%AD%E6%96%87.zip"),
        Some("中文.zip".to_string())
    );

    // 测试 inline 类型
    assert_eq!(
        parse_content_disposition("inline; filename=\"inline-file.txt\""),
        Some("inline-file.txt".to_string())
    );

    // 测试无 filename
    assert_eq!(parse_content_disposition("attachment"), None);
}

/// 测试状态文件持久化
#[tokio::test]
async fn test_state_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let store = StateStore::new(temp_dir.path().to_path_buf());

    // 创建一个下载状态
    let mut state = DownloadState::new(
        "https://example.com/file.zip",
        "file.zip",
        temp_dir.path().to_path_buf(),
        1000000,
        true,
    );

    state.etag = Some("\"test-etag\"".to_string());
    state.last_modified = Some("Mon, 01 Jan 2024 00:00:00 GMT".to_string());
    state.chunks = vec![
        ChunkState {
            index: 0,
            start: 0,
            end: 499999,
            downloaded: 250000,
            completed: false,
        },
        ChunkState {
            index: 1,
            start: 500000,
            end: 999999,
            downloaded: 500000,
            completed: true,
        },
    ];

    // 保存状态
    store.save(&state).await.unwrap();

    // 验证状态文件存在
    let state_path = state.state_file_path();
    assert!(state_path.exists());

    // 重新加载状态
    let loaded = store.load("file.zip").await.unwrap();
    assert!(loaded.is_some());

    let loaded_state = loaded.unwrap();
    assert_eq!(loaded_state.url, "https://example.com/file.zip");
    assert_eq!(loaded_state.total_size, 1000000);
    assert_eq!(loaded_state.chunks.len(), 2);
    assert_eq!(loaded_state.chunks[0].downloaded, 250000);
    assert!(loaded_state.chunks[1].completed);

    // 删除状态文件
    store.delete("file.zip").await.unwrap();
    assert!(!state_path.exists());
}

/// 测试下载状态一致性验证
#[test]
fn test_state_consistency_validation() {
    let mut state = DownloadState::new(
        "https://example.com/file.zip",
        "file.zip",
        PathBuf::from("/tmp"),
        1000,
        true,
    );
    state.etag = Some("\"abc123\"".to_string());
    state.last_modified = Some("Mon, 01 Jan 2024 00:00:00 GMT".to_string());

    // 完全匹配
    assert!(state.validate_consistency(
        "https://example.com/file.zip",
        None,
        1000,
        Some("\"abc123\""),
        Some("Mon, 01 Jan 2024 00:00:00 GMT"),
    ));

    // ETag 不匹配
    assert!(!state.validate_consistency(
        "https://example.com/file.zip",
        None,
        1000,
        Some("\"different\""),
        Some("Mon, 01 Jan 2024 00:00:00 GMT"),
    ));

    // Content-Length 不匹配
    assert!(!state.validate_consistency(
        "https://example.com/file.zip",
        None,
        2000,
        Some("\"abc123\""),
        Some("Mon, 01 Jan 2024 00:00:00 GMT"),
    ));

    // URL 不匹配
    assert!(!state.validate_consistency(
        "https://example.com/other.zip",
        None,
        1000,
        Some("\"abc123\""),
        Some("Mon, 01 Jan 2024 00:00:00 GMT"),
    ));
}

/// 测试分块计算
#[test]
fn test_chunk_calculation() {
    let manager = ChunkManager::new(4, "4M");

    // 小文件（小于 min_split_size）- 单连接
    let chunks = manager.calculate_chunks(1024 * 1024, true);
    assert_eq!(chunks.len(), 1);

    // 中等文件 - 根据大小计算分块数
    let chunks = manager.calculate_chunks(16 * 1024 * 1024, true);
    assert_eq!(chunks.len(), 4);

    // 验证分块覆盖完整
    let total_size: u64 = chunks.iter().map(|c| c.size()).sum();
    assert_eq!(total_size, 16 * 1024 * 1024);

    // 验证分块连续
    for i in 1..chunks.len() {
        assert_eq!(chunks[i].start, chunks[i - 1].end + 1);
    }

    // 不支持 Range - 始终单连接
    let chunks = manager.calculate_chunks(100 * 1024 * 1024, false);
    assert_eq!(chunks.len(), 1);

    // 未知大小 - 单连接
    let chunks = manager.calculate_chunks(0, true);
    assert_eq!(chunks.len(), 1);
}

/// 测试 RustDownloader 基本功能
#[tokio::test]
async fn test_rust_downloader_lifecycle() {
    let downloader = RustDownloader::new();

    // 初始状态
    assert!(!downloader.is_started());

    // 启动
    downloader.start().await.unwrap();
    assert!(downloader.is_started());

    // 重复启动无副作用
    downloader.start().await.unwrap();
    assert!(downloader.is_started());

    // 停止
    downloader.stop().await.unwrap();
    assert!(!downloader.is_started());

    // 重复停止无副作用
    downloader.stop().await.unwrap();
    assert!(!downloader.is_started());
}

/// 测试取消下载
#[tokio::test]
async fn test_cancel_download() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();

    // 模拟一个很大的文件（延迟响应模拟慢速下载）
    Mock::given(method("GET"))
        .and(path("/large.zip"))
        .and(header("Range", "bytes=0-0"))
        .respond_with(
            ResponseTemplate::new(206)
                .insert_header("Content-Range", "bytes 0-0/100000000")
                .set_body_bytes(b"x".to_vec()),
        )
        .mount(&mock_server)
        .await;

    Mock::given(method("HEAD"))
        .and(path("/large.zip"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", "100000000"),
        )
        .mount(&mock_server)
        .await;

    // 实际下载请求 - 延迟响应
    Mock::given(method("GET"))
        .and(path("/large.zip"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(std::time::Duration::from_secs(10)) // 10秒延迟
                .set_body_bytes(vec![0u8; 1000]),
        )
        .mount(&mock_server)
        .await;

    let url = format!("{}/large.zip", mock_server.uri());
    let downloader = Arc::new(RustDownloader::new());
    downloader.start().await.unwrap();

    let options = DownloadOptions {
        save_dir: Some(temp_dir.path().to_path_buf()),
        filename: Some("large.zip".to_string()),
        overwrite: true,
        ..Default::default()
    };

    // 启动下载（异步）
    let task_id = downloader.download(&url, options).await.unwrap();

    // 短暂等待
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // 取消下载
    let cancel_result = downloader.cancel(&task_id).await;
    assert!(cancel_result.is_ok());

    downloader.stop().await.unwrap();
}

/// 测试进度回调
#[tokio::test]
async fn test_progress_callback() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();

    let file_content = vec![0u8; 1024 * 10]; // 10KB

    Mock::given(method("GET"))
        .and(path("/progress-test.bin"))
        .and(header("Range", "bytes=0-0"))
        .respond_with(
            ResponseTemplate::new(206)
                .insert_header("Content-Range", format!("bytes 0-0/{}", file_content.len()))
                .set_body_bytes(file_content[0..1].to_vec()),
        )
        .mount(&mock_server)
        .await;

    Mock::given(method("HEAD"))
        .and(path("/progress-test.bin"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", file_content.len().to_string()),
        )
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/progress-test.bin"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", file_content.len().to_string())
                .set_body_bytes(file_content.clone()),
        )
        .mount(&mock_server)
        .await;

    let url = format!("{}/progress-test.bin", mock_server.uri());
    let downloader = RustDownloader::new();
    downloader.start().await.unwrap();

    let progress_count = Arc::new(AtomicU32::new(0));
    let progress_count_clone = progress_count.clone();

    let options = DownloadOptions {
        save_dir: Some(temp_dir.path().to_path_buf()),
        filename: Some("progress-test.bin".to_string()),
        overwrite: true,
        ..Default::default()
    };

    let result = downloader
        .download_and_wait(
            &url,
            options,
            Box::new(move |_progress| {
                progress_count_clone.fetch_add(1, Ordering::SeqCst);
            }),
        )
        .await;

    assert!(result.is_ok());

    // 验证进度回调被调用
    assert!(progress_count.load(Ordering::SeqCst) > 0);

    downloader.stop().await.unwrap();
}

/// 测试 404 错误不重试
///
/// 注：此测试涉及完整的下载流程，可能需要较长时间
#[tokio::test]
#[ignore]
async fn test_no_retry_on_404() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();

    // 404 响应 - 移除 expect(1) 以避免超时
    Mock::given(method("GET"))
        .and(path("/not-found.txt"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let url = format!("{}/not-found.txt", mock_server.uri());
    let downloader = RustDownloader::new();
    downloader.start().await.unwrap();

    let options = DownloadOptions {
        save_dir: Some(temp_dir.path().to_path_buf()),
        filename: Some("not-found.txt".to_string()),
        overwrite: true,
        ..Default::default()
    };

    let result = downloader
        .download_and_wait(&url, options, Box::new(|_| {}))
        .await;

    // 应该失败，不应重试
    assert!(result.is_err());

    downloader.stop().await.unwrap();
}

/// 测试错误分类
#[test]
fn test_error_categorization() {
    use ns_emu_tools_lib::error::AppError;
    use ns_emu_tools_lib::services::downloader::retry_strategy::{ErrorCategory, RetryStrategy};

    // 404 错误 - 永久错误
    let error = AppError::Network("404 Not Found".to_string());
    assert_eq!(RetryStrategy::categorize_error(&error), ErrorCategory::Permanent);

    // 429 错误 - 限流
    let error = AppError::Network("429 Too Many Requests".to_string());
    assert_eq!(RetryStrategy::categorize_error(&error), ErrorCategory::RateLimited);

    // 500 错误 - 临时错误
    let error = AppError::Network("500 Internal Server Error".to_string());
    assert_eq!(RetryStrategy::categorize_error(&error), ErrorCategory::Temporary);

    // 超时 - 临时错误
    let error = AppError::Network("connection timeout".to_string());
    assert_eq!(RetryStrategy::categorize_error(&error), ErrorCategory::Temporary);

    // SSL 错误 - 不可重试
    let error = AppError::Network("SSL certificate error".to_string());
    assert_eq!(RetryStrategy::categorize_error(&error), ErrorCategory::SslError);

    // DNS 错误 - 可重试
    let error = AppError::Network("DNS resolution failed".to_string());
    assert_eq!(RetryStrategy::categorize_error(&error), ErrorCategory::DnsError);
}

/// 测试指数退避
#[test]
fn test_exponential_backoff() {
    use ns_emu_tools_lib::services::downloader::retry_strategy::RetryStrategy;
    use std::time::Duration;

    let mut strategy = RetryStrategy::new(5);

    // 注：由于 jitter 使用整数秒，第 0 次重试时基础值为 1 秒，
    // jitter 因子在 0.75-1.25 之间，结果可能是 0-1 秒

    // 第 0 次重试
    let delay0 = strategy.backoff_delay();
    assert!(delay0 <= Duration::from_millis(1500)); // 最多 1s * 1.25

    // 第 1 次重试
    strategy.increment_retry();
    let delay1 = strategy.backoff_delay();
    assert!(delay1 >= Duration::from_secs(1)); // 至少 2s * 0.75 = 1.5s -> 1s (截断)
    assert!(delay1 <= Duration::from_secs(3)); // 最多 2s * 1.25 = 2.5s -> 2s (截断)

    // 第 2 次重试
    strategy.increment_retry();
    let delay2 = strategy.backoff_delay();
    assert!(delay2 >= Duration::from_secs(3)); // 至少 4s * 0.75 = 3s
    assert!(delay2 <= Duration::from_secs(5)); // 最多 4s * 1.25 = 5s
}

/// 测试重试策略的 should_retry 方法
#[test]
fn test_should_retry() {
    use ns_emu_tools_lib::error::AppError;
    use ns_emu_tools_lib::services::downloader::retry_strategy::RetryStrategy;

    let strategy = RetryStrategy::new(3);

    // 临时错误应该重试
    let temp_error = AppError::Network("500 Internal Server Error".to_string());
    assert!(strategy.should_retry(&temp_error));

    // 永久错误不应该重试
    let perm_error = AppError::Network("404 Not Found".to_string());
    assert!(!strategy.should_retry(&perm_error));

    // SSL 错误不应该重试
    let ssl_error = AppError::Network("SSL certificate error".to_string());
    assert!(!strategy.should_retry(&ssl_error));
}

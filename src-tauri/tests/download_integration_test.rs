//! 下载模块集成测试
//!
//! 覆盖当前默认的 bytehaul 下载路径与统一取消入口的基础行为。

use bytehaul::Downloader;
use ns_emu_tools_lib::services::downloader::{
    BytehaulBackend, DownloadManager, DownloadOptions, DownloadStatus,
};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn create_backend() -> BytehaulBackend {
    BytehaulBackend::new(Arc::new(
        Downloader::builder().enable_ipv6(false).build().unwrap(),
    ))
}

#[tokio::test]
async fn test_bytehaul_download_and_progress_callback() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let file_content = b"Hello from bytehaul".to_vec();

    Mock::given(method("GET"))
        .and(path("/artifact.bin"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", file_content.len().to_string())
                .set_body_bytes(file_content.clone()),
        )
        .mount(&mock_server)
        .await;

    let backend = create_backend();
    backend.start().await.unwrap();

    let progress_count = Arc::new(AtomicU32::new(0));
    let progress_counter = progress_count.clone();
    let url = format!("{}/artifact.bin", mock_server.uri());
    let result = backend
        .download_and_wait(
            &url,
            DownloadOptions {
                save_dir: Some(temp_dir.path().to_path_buf()),
                filename: Some("artifact.bin".to_string()),
                overwrite: true,
                ..Default::default()
            },
            Box::new(move |progress| {
                assert!(matches!(
                    progress.status,
                    DownloadStatus::Active | DownloadStatus::Complete | DownloadStatus::Waiting
                ));
                progress_counter.fetch_add(1, Ordering::SeqCst);
            }),
        )
        .await
        .unwrap();

    assert_eq!(result.filename, "artifact.bin");
    assert_eq!(std::fs::read(result.path).unwrap(), file_content);
    assert!(progress_count.load(Ordering::SeqCst) > 0);

    backend.stop().await.unwrap();
}

#[tokio::test]
async fn test_bytehaul_cancel_all_smoke() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();

    Mock::given(method("GET"))
        .and(path("/slow.bin"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", "1024")
                .set_delay(std::time::Duration::from_secs(5))
                .set_body_bytes(vec![42u8; 1024]),
        )
        .mount(&mock_server)
        .await;

    let backend = create_backend();
    backend.start().await.unwrap();

    let output_path = temp_dir.path().join("slow.bin");
    let task_id = backend
        .download(
            &format!("{}/slow.bin", mock_server.uri()),
            DownloadOptions {
                save_dir: Some(temp_dir.path().to_path_buf()),
                filename: Some("slow.bin".to_string()),
                overwrite: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let cancelled_path = backend.cancel_all(false).await.unwrap();

    assert_eq!(cancelled_path, Some(output_path.display().to_string()));
    let progress = backend.get_download_progress(&task_id).await.unwrap();
    assert!(matches!(
        progress.status,
        DownloadStatus::Waiting | DownloadStatus::Active | DownloadStatus::Removed
    ));

    backend.stop().await.unwrap();
}

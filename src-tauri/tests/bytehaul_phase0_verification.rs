//! Phase 0 验证测试
//!
//! 通过最小集成测试验证 bytehaul 在当前仓库里的基础可用性：
//! - 显式输出路径
//! - 自定义请求头 / User-Agent
//! - 非法配置能在网络请求前被拒绝

use std::collections::HashMap;

use bytehaul::{DownloadError, DownloadSpec, Downloader};
use tempfile::tempdir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_bytehaul_phase0_download_with_headers_and_output_path() {
    let server = MockServer::start().await;
    let body = b"bytehaul-phase0".to_vec();

    Mock::given(method("GET"))
        .and(path("/artifact.bin"))
        .and(header("X-Test-Header", "phase0"))
        .and(header("User-Agent", "NsEmuTools-Phase0"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", body.len().to_string())
                .set_body_bytes(body.clone()),
        )
        .mount(&server)
        .await;

    let temp_dir = tempdir().unwrap();
    let output_path = temp_dir.path().join("artifact.bin");

    let mut headers = HashMap::new();
    headers.insert("X-Test-Header".to_string(), "phase0".to_string());
    headers.insert("User-Agent".to_string(), "NsEmuTools-Phase0".to_string());

    let downloader = Downloader::builder().enable_ipv6(false).build().unwrap();
    let spec = DownloadSpec::new(format!("{}/artifact.bin", server.uri()))
        .output_path(output_path.clone())
        .headers(headers);

    let handle = downloader.download(spec);
    handle.wait().await.unwrap();

    assert_eq!(std::fs::read(&output_path).unwrap(), body);
    assert!(!std::path::PathBuf::from(format!("{}.bytehaul", output_path.display())).exists());
}

#[tokio::test]
async fn test_bytehaul_phase0_invalid_spec_is_rejected_before_network_work() {
    let downloader = Downloader::builder().build().unwrap();
    let output_path = tempdir().unwrap().path().join("invalid.bin");
    let spec = DownloadSpec::new("https://example.com/file.bin")
        .output_path(output_path)
        .max_connections(0);

    let handle = downloader.download(spec);
    let error = handle.wait().await.unwrap_err();

    assert!(
        matches!(error, DownloadError::InvalidConfig(message) if message.contains("max_connections"))
    );
}

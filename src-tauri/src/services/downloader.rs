//! 下载器模块
//!
//! 提供文件下载功能，支持进度回调、断点续传等特性

use crate::error::{AppError, AppResult};
use crate::services::network::{create_client, get_github_download_url, CHROME_UA};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info};

/// 下载目录
pub fn get_download_dir() -> PathBuf {
    let path = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("download");

    if !path.exists() {
        std::fs::create_dir_all(&path).ok();
    }

    path
}

/// 下载进度信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    /// 已下载字节数
    pub downloaded: u64,
    /// 总字节数
    pub total: u64,
    /// 下载速度（字节/秒）
    pub speed: u64,
    /// 进度百分比
    pub percentage: f64,
    /// 预计剩余时间（秒）
    pub eta: u64,
    /// 文件名
    pub filename: String,
}

impl DownloadProgress {
    pub fn new(filename: &str) -> Self {
        Self {
            downloaded: 0,
            total: 0,
            speed: 0,
            percentage: 0.0,
            eta: 0,
            filename: filename.to_string(),
        }
    }

    pub fn update(&mut self, downloaded: u64, total: u64, speed: u64) {
        self.downloaded = downloaded;
        self.total = total;
        self.speed = speed;
        self.percentage = if total > 0 {
            (downloaded as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        self.eta = if speed > 0 && total > downloaded {
            (total - downloaded) / speed
        } else {
            0
        };
    }

    /// 格式化已下载大小
    pub fn downloaded_string(&self) -> String {
        format_size(self.downloaded)
    }

    /// 格式化总大小
    pub fn total_string(&self) -> String {
        format_size(self.total)
    }

    /// 格式化速度
    pub fn speed_string(&self) -> String {
        format!("{}/s", format_size(self.speed))
    }

    /// 格式化 ETA
    pub fn eta_string(&self) -> String {
        if self.eta == 0 {
            return "0s".to_string();
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

/// 格式化文件大小
pub fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    format!("{:.1}{}", size, UNITS[unit_index])
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
}

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
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            save_dir: None,
            filename: None,
            overwrite: false,
            use_github_mirror: true,
        }
    }
}

/// 下载器
pub struct Downloader {
    client: Client,
    cancelled: Arc<AtomicBool>,
}

impl Downloader {
    /// 创建新的下载器实例
    pub fn new() -> AppResult<Self> {
        let client = create_client()?;
        Ok(Self {
            client,
            cancelled: Arc::new(AtomicBool::new(false)),
        })
    }

    /// 使用指定客户端创建下载器
    pub fn with_client(client: Client) -> Self {
        Self {
            client,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 取消下载
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// 检查是否已取消
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// 重置取消状态
    pub fn reset(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
    }

    /// 下载文件
    pub async fn download<F>(
        &self,
        url: &str,
        options: DownloadOptions,
        on_progress: F,
    ) -> AppResult<DownloadResult>
    where
        F: Fn(DownloadProgress) + Send + 'static,
    {
        self.reset();

        // 处理 URL（应用镜像）
        let final_url = if options.use_github_mirror && url.contains("github.com") {
            get_github_download_url(url)
        } else {
            url.to_string()
        };

        info!("开始下载: {}", final_url);

        // 选择 User-Agent
        let user_agent = if final_url.contains("e6ex.com") {
            crate::config::user_agent()
        } else {
            CHROME_UA.to_string()
        };

        // 发送请求
        let response = self
            .client
            .get(&final_url)
            .header("User-Agent", user_agent)
            .send()
            .await
            .map_err(|e| AppError::Download(format!("请求失败: {}", e)))?;

        // 检查响应状态
        if !response.status().is_success() {
            return Err(AppError::Download(format!(
                "HTTP 错误: {}",
                response.status()
            )));
        }

        // 获取文件大小
        let total_size = response.content_length().unwrap_or(0);

        // 确定文件名
        let filename = options.filename.clone().unwrap_or_else(|| {
            // 尝试从 Content-Disposition 获取
            if let Some(cd) = response.headers().get("content-disposition") {
                if let Ok(cd_str) = cd.to_str() {
                    if let Some(name) = extract_filename_from_content_disposition(cd_str) {
                        return name;
                    }
                }
            }
            // 从 URL 获取
            extract_filename_from_url(&final_url)
        });

        // 确定保存目录
        let save_dir = options.save_dir.unwrap_or_else(get_download_dir);
        let save_path = save_dir.join(&filename);

        // 检查文件是否已存在
        if save_path.exists() && !options.overwrite {
            info!("文件已存在，跳过下载: {}", save_path.display());
            return Ok(DownloadResult {
                path: save_path.clone(),
                filename,
                size: save_path.metadata().map(|m| m.len()).unwrap_or(0),
            });
        }

        // 创建保存目录
        if let Some(parent) = save_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // 创建临时文件
        let temp_path = save_path.with_extension("tmp");
        let mut file = File::create(&temp_path).await?;

        // 初始化进度
        let mut progress = DownloadProgress::new(&filename);
        progress.total = total_size;

        // 下载追踪变量
        let downloaded = Arc::new(AtomicU64::new(0));
        let start_time = Instant::now();
        let mut last_progress_time = Instant::now();
        let mut last_downloaded: u64 = 0;

        // 获取响应流
        let mut stream = response.bytes_stream();

        // 下载循环
        while let Some(chunk_result) = stream.next().await {
            // 检查是否取消
            if self.is_cancelled() {
                // 清理临时文件
                drop(file);
                let _ = tokio::fs::remove_file(&temp_path).await;
                return Err(AppError::Download("下载已取消".to_string()));
            }

            let chunk = chunk_result.map_err(|e| AppError::Download(format!("读取数据失败: {}", e)))?;

            // 写入文件
            file.write_all(&chunk).await?;

            // 更新下载量
            let current = downloaded.fetch_add(chunk.len() as u64, Ordering::SeqCst)
                + chunk.len() as u64;

            // 计算速度（每 200ms 更新一次）
            let elapsed = last_progress_time.elapsed();
            if elapsed >= Duration::from_millis(200) {
                let bytes_diff = current - last_downloaded;
                let speed = (bytes_diff as f64 / elapsed.as_secs_f64()) as u64;

                progress.update(current, total_size, speed);
                on_progress(progress.clone());

                last_downloaded = current;
                last_progress_time = Instant::now();
            }
        }

        // 确保所有数据写入磁盘
        file.flush().await?;
        drop(file);

        // 重命名临时文件
        tokio::fs::rename(&temp_path, &save_path).await?;

        let final_size = downloaded.load(Ordering::SeqCst);
        let total_time = start_time.elapsed();

        info!(
            "下载完成: {} ({}, 耗时 {:.1}s)",
            filename,
            format_size(final_size),
            total_time.as_secs_f64()
        );

        // 发送最终进度
        progress.update(final_size, total_size, 0);
        progress.percentage = 100.0;
        on_progress(progress);

        Ok(DownloadResult {
            path: save_path,
            filename,
            size: final_size,
        })
    }

    /// 简单下载（无进度回调）
    pub async fn download_simple(
        &self,
        url: &str,
        save_dir: Option<PathBuf>,
    ) -> AppResult<DownloadResult> {
        let options = DownloadOptions {
            save_dir,
            ..Default::default()
        };
        self.download(url, options, |_| {}).await
    }
}

/// 从 Content-Disposition 头提取文件名
fn extract_filename_from_content_disposition(header: &str) -> Option<String> {
    // 尝试匹配 filename*=UTF-8''...
    if let Some(start) = header.find("filename*=") {
        let rest = &header[start + 10..];
        if let Some(encoded) = rest.split(';').next() {
            let encoded = encoded.trim().trim_matches('"');
            // 解码 UTF-8 编码的文件名
            if let Some(pos) = encoded.find("''") {
                let decoded = &encoded[pos + 2..];
                if let Ok(name) = urlencoding::decode(decoded) {
                    return Some(name.into_owned());
                }
            }
        }
    }

    // 尝试匹配 filename="..."
    if let Some(start) = header.find("filename=") {
        let rest = &header[start + 9..];
        let filename = rest.split(';').next().unwrap_or(rest);
        let filename = filename.trim().trim_matches('"').trim_matches('\'');
        if !filename.is_empty() {
            return Some(filename.to_string());
        }
    }

    None
}

/// 从 URL 提取文件名
fn extract_filename_from_url(url: &str) -> String {
    url.split('/')
        .last()
        .and_then(|s| s.split('?').next())
        .unwrap_or("download")
        .to_string()
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

    use tokio::io::AsyncReadExt;

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
        assert_eq!(format_size(0), "0.0B");
        assert_eq!(format_size(1023), "1023.0B");
        assert_eq!(format_size(1024), "1.0KiB");
        assert_eq!(format_size(1024 * 1024), "1.0MiB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0GiB");
    }

    #[test]
    fn test_extract_filename_from_url() {
        assert_eq!(
            extract_filename_from_url("https://example.com/file.zip"),
            "file.zip"
        );
        assert_eq!(
            extract_filename_from_url("https://example.com/path/to/file.tar.gz?token=abc"),
            "file.tar.gz"
        );
    }

    #[test]
    fn test_extract_filename_from_content_disposition() {
        assert_eq!(
            extract_filename_from_content_disposition("attachment; filename=\"test.zip\""),
            Some("test.zip".to_string())
        );
        assert_eq!(
            extract_filename_from_content_disposition("attachment; filename=test.zip"),
            Some("test.zip".to_string())
        );
    }

    #[test]
    fn test_download_progress() {
        let mut progress = DownloadProgress::new("test.zip");
        progress.update(512 * 1024, 1024 * 1024, 100 * 1024);

        assert_eq!(progress.percentage, 50.0);
        assert_eq!(progress.downloaded_string(), "512.0KiB");
        assert_eq!(progress.total_string(), "1.0MiB");
        assert_eq!(progress.speed_string(), "100.0KiB/s");
    }
}

//! 下载器模块
//!
//! 提供文件下载功能，支持进度回调、断点续传、多线程分块下载等特性

use crate::error::{AppError, AppResult};
use crate::services::network::{create_client, get_github_download_url, CHROME_UA};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

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
    /// 分块下载的线程数（默认为 4）
    pub num_threads: usize,
    /// 是否启用断点续传
    pub resume: bool,
    /// 每个分块的最小大小（默认 1MB）
    pub min_chunk_size: u64,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            save_dir: None,
            filename: None,
            overwrite: false,
            use_github_mirror: true,
            num_threads: 4,
            resume: true,
            min_chunk_size: 1024 * 1024, // 1MB
        }
    }
}

/// 分块信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkInfo {
    /// 分块索引
    pub index: usize,
    /// 起始字节
    pub start: u64,
    /// 结束字节（包含）
    pub end: u64,
    /// 已下载字节数
    pub downloaded: u64,
}

impl ChunkInfo {
    pub fn new(index: usize, start: u64, end: u64) -> Self {
        Self {
            index,
            start,
            end,
            downloaded: 0,
        }
    }

    /// 当前下载位置
    pub fn current_pos(&self) -> u64 {
        self.start + self.downloaded
    }

    /// 分块总大小
    pub fn size(&self) -> u64 {
        self.end - self.start + 1
    }

    /// 是否已完成
    pub fn is_complete(&self) -> bool {
        self.downloaded >= self.size()
    }

    /// 剩余字节数
    pub fn remaining(&self) -> u64 {
        self.size().saturating_sub(self.downloaded)
    }
}

/// 下载状态（用于断点续传）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadState {
    /// 原始 URL
    pub url: String,
    /// 文件总大小
    pub total_size: u64,
    /// 各分块状态
    pub chunks: Vec<ChunkInfo>,
    /// 文件名
    pub filename: String,
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

    /// 多线程分块下载（支持断点续传）
    pub async fn download_chunked<F>(
        &self,
        url: &str,
        options: DownloadOptions,
        on_progress: F,
    ) -> AppResult<DownloadResult>
    where
        F: Fn(DownloadProgress) + Send + Sync + 'static,
    {
        self.reset();

        // 处理 URL（应用镜像）
        let final_url = if options.use_github_mirror && url.contains("github.com") {
            get_github_download_url(url)
        } else {
            url.to_string()
        };

        info!("开始多线程下载: {}", final_url);

        // 选择 User-Agent
        let user_agent = if final_url.contains("e6ex.com") {
            crate::config::user_agent()
        } else {
            CHROME_UA.to_string()
        };

        // 发送 Range 请求来获取文件信息并验证是否支持分块下载
        // 使用 Range: bytes=0-0 请求第一个字节，如果返回 206 则支持 Range
        let range_test_response = self
            .client
            .get(&final_url)
            .header("User-Agent", &user_agent)
            .header("Range", "bytes=0-0")
            .send()
            .await
            .map_err(|e| AppError::Download(format!("Range 测试请求失败: {}", e)))?;

        let status = range_test_response.status();
        if !status.is_success() && status.as_u16() != 206 {
            return Err(AppError::Download(format!(
                "HTTP 错误: {}",
                status
            )));
        }

        // 206 Partial Content 表示支持 Range
        let supports_range = status.as_u16() == 206;

        // 从 Content-Range 头获取总大小: "bytes 0-0/12345"
        let total_size = if supports_range {
            range_test_response
                .headers()
                .get("content-range")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.split('/').last())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0)
        } else {
            // 如果不支持 Range，使用 Content-Length
            range_test_response.content_length().unwrap_or(0)
        };

        // 确定文件名
        let filename = options.filename.clone().unwrap_or_else(|| {
            if let Some(cd) = range_test_response.headers().get("content-disposition") {
                if let Ok(cd_str) = cd.to_str() {
                    if let Some(name) = extract_filename_from_content_disposition(cd_str) {
                        return name;
                    }
                }
            }
            extract_filename_from_url(&final_url)
        });

        // 确定保存路径
        let save_dir = options.save_dir.clone().unwrap_or_else(get_download_dir);
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

        // 如果不支持 Range 或文件太小，使用单线程下载
        if !supports_range || total_size == 0 || total_size < options.min_chunk_size * 2 {
            warn!("服务器不支持 Range 请求或文件太小，使用单线程下载");
            return self.download(url, options, on_progress).await;
        }

        info!(
            "文件大小: {}, 支持断点续传: {}, 线程数: {}",
            format_size(total_size),
            supports_range,
            options.num_threads
        );

        // 状态文件路径
        let state_path = save_path.with_extension("download_state");
        let temp_path = save_path.with_extension("download_tmp");

        // 尝试恢复下载状态
        let mut state = if options.resume {
            load_download_state(&state_path).await
        } else {
            None
        };

        // 验证恢复的状态是否有效
        if let Some(ref s) = state {
            if s.url != final_url || s.total_size != total_size {
                info!("下载状态不匹配，重新开始下载");
                state = None;
            }
        }

        // 创建或使用已有的分块信息
        let chunks = if let Some(ref mut s) = state {
            info!("恢复下载，已完成: {}", format_size(s.chunks.iter().map(|c| c.downloaded).sum()));
            s.chunks.clone()
        } else {
            create_chunks(total_size, options.num_threads, options.min_chunk_size)
        };

        // 确保临时文件存在并设置正确大小
        if !temp_path.exists() || state.is_none() {
            let file = File::create(&temp_path).await?;
            file.set_len(total_size).await?;
        }

        // 创建共享状态
        let chunks = Arc::new(Mutex::new(chunks));
        let total_downloaded = Arc::new(AtomicU64::new(
            chunks.lock().await.iter().map(|c| c.downloaded).sum(),
        ));
        let cancelled = self.cancelled.clone();
        let on_progress = Arc::new(on_progress);

        // 进度更新任务
        let progress_chunks = chunks.clone();
        let progress_downloaded = total_downloaded.clone();
        let progress_cancelled = cancelled.clone();
        let progress_callback = on_progress.clone();
        let progress_filename = filename.clone();
        let progress_state_path = state_path.clone();
        let progress_url = final_url.clone();

        let progress_handle = tokio::spawn(async move {
            let mut last_downloaded = progress_downloaded.load(Ordering::SeqCst);
            let mut last_time = Instant::now();

            loop {
                tokio::time::sleep(Duration::from_millis(200)).await;

                if progress_cancelled.load(Ordering::SeqCst) {
                    break;
                }

                let current = progress_downloaded.load(Ordering::SeqCst);
                let elapsed = last_time.elapsed();
                let speed = if elapsed.as_secs_f64() > 0.0 {
                    ((current - last_downloaded) as f64 / elapsed.as_secs_f64()) as u64
                } else {
                    0
                };

                let mut progress = DownloadProgress::new(&progress_filename);
                progress.update(current, total_size, speed);
                progress_callback(progress);

                // 保存下载状态
                let chunks_snapshot = progress_chunks.lock().await.clone();
                let state = DownloadState {
                    url: progress_url.clone(),
                    total_size,
                    chunks: chunks_snapshot,
                    filename: progress_filename.clone(),
                };
                let _ = save_download_state(&progress_state_path, &state).await;

                last_downloaded = current;
                last_time = Instant::now();

                if current >= total_size {
                    break;
                }
            }
        });

        // 启动下载任务
        let mut handles = Vec::new();
        let num_threads = options.num_threads;

        for thread_id in 0..num_threads {
            let client = self.client.clone();
            let url = final_url.clone();
            let user_agent = user_agent.clone();
            let chunks = chunks.clone();
            let total_downloaded = total_downloaded.clone();
            let cancelled = cancelled.clone();
            let temp_path = temp_path.clone();

            let handle = tokio::spawn(async move {
                download_worker(
                    thread_id,
                    client,
                    &url,
                    &user_agent,
                    chunks,
                    total_downloaded,
                    cancelled,
                    &temp_path,
                )
                .await
            });

            handles.push(handle);
        }

        // 等待所有下载任务完成
        let mut has_error = false;
        let mut error_msg = String::new();

        for handle in handles {
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    has_error = true;
                    error_msg = e.to_string();
                }
                Err(e) => {
                    has_error = true;
                    error_msg = format!("任务 panic: {}", e);
                }
            }
        }

        // 停止进度更新任务
        progress_handle.abort();

        // 检查是否取消
        if self.is_cancelled() {
            // 保存状态以便后续恢复
            let chunks_snapshot = chunks.lock().await.clone();
            let state = DownloadState {
                url: final_url,
                total_size,
                chunks: chunks_snapshot,
                filename: filename.clone(),
            };
            let _ = save_download_state(&state_path, &state).await;
            return Err(AppError::Download("下载已取消".to_string()));
        }

        if has_error {
            // 保存状态以便后续恢复
            let chunks_snapshot = chunks.lock().await.clone();
            let state = DownloadState {
                url: final_url,
                total_size,
                chunks: chunks_snapshot,
                filename: filename.clone(),
            };
            let _ = save_download_state(&state_path, &state).await;
            return Err(AppError::Download(error_msg));
        }

        // 验证下载完整性
        let final_downloaded = total_downloaded.load(Ordering::SeqCst);
        if final_downloaded < total_size {
            let chunks_snapshot = chunks.lock().await.clone();
            let state = DownloadState {
                url: final_url,
                total_size,
                chunks: chunks_snapshot,
                filename: filename.clone(),
            };
            let _ = save_download_state(&state_path, &state).await;
            return Err(AppError::Download(format!(
                "下载不完整: {} / {}",
                format_size(final_downloaded),
                format_size(total_size)
            )));
        }

        // 重命名临时文件
        tokio::fs::rename(&temp_path, &save_path).await?;

        // 删除状态文件
        let _ = tokio::fs::remove_file(&state_path).await;

        info!(
            "多线程下载完成: {} ({})",
            filename,
            format_size(total_size)
        );

        // 发送最终进度
        let mut progress = DownloadProgress::new(&filename);
        progress.update(total_size, total_size, 0);
        progress.percentage = 100.0;
        on_progress(progress);

        Ok(DownloadResult {
            path: save_path,
            filename,
            size: total_size,
        })
    }
}

/// 创建下载分块
fn create_chunks(total_size: u64, num_threads: usize, min_chunk_size: u64) -> Vec<ChunkInfo> {
    let chunk_size = (total_size / num_threads as u64).max(min_chunk_size);
    let actual_threads = ((total_size + chunk_size - 1) / chunk_size) as usize;

    let mut chunks = Vec::with_capacity(actual_threads);
    let mut start = 0u64;

    for i in 0..actual_threads {
        let end = if i == actual_threads - 1 {
            total_size - 1
        } else {
            (start + chunk_size - 1).min(total_size - 1)
        };

        chunks.push(ChunkInfo::new(i, start, end));
        start = end + 1;
    }

    chunks
}

/// 下载工作线程
async fn download_worker(
    thread_id: usize,
    client: Client,
    url: &str,
    user_agent: &str,
    chunks: Arc<Mutex<Vec<ChunkInfo>>>,
    total_downloaded: Arc<AtomicU64>,
    cancelled: Arc<AtomicBool>,
    temp_path: &Path,
) -> AppResult<()> {
    loop {
        // 检查取消
        if cancelled.load(Ordering::SeqCst) {
            return Ok(());
        }

        // 获取并标记未完成的分块（原子操作）
        let chunk_to_download = {
            let mut chunks_guard = chunks.lock().await;
            // 查找未完成且未被其他线程处理的分块
            let chunk_opt = chunks_guard
                .iter_mut()
                .find(|c| !c.is_complete());

            match chunk_opt {
                Some(chunk) => {
                    // 标记为已开始下载（设置 downloaded 为一个特殊值或使用其他标记）
                    // 这里我们复制分块信息并立即标记已完成，防止其他线程重复下载
                    let chunk_info = chunk.clone();
                    // 预先标记为完成，防止其他线程获取
                    chunk.downloaded = chunk.size();
                    Some(chunk_info)
                }
                None => None,
            }
        };

        let Some(chunk) = chunk_to_download else {
            debug!("线程 {} 没有更多分块需要下载", thread_id);
            return Ok(());
        };

        debug!(
            "线程 {} 开始下载分块 {}: {}-{} (已下载: {})",
            thread_id,
            chunk.index,
            chunk.start,
            chunk.end,
            chunk.downloaded
        );

        // 计算实际请求范围
        let range_start = chunk.current_pos();
        let range_end = chunk.end;

        if range_start > range_end {
            continue;
        }

        // 发送 Range 请求，禁用自动解压以获取原始数据
        let response = match client
            .get(url)
            .header("User-Agent", user_agent)
            .header("Range", format!("bytes={}-{}", range_start, range_end))
            .header("Accept-Encoding", "identity") // 禁用压缩，获取原始数据
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                // 下载失败，恢复分块状态
                let mut chunks_guard = chunks.lock().await;
                if let Some(c) = chunks_guard.iter_mut().find(|c| c.index == chunk.index) {
                    c.downloaded = chunk.downloaded;
                }
                return Err(AppError::Download(format!("分块 {} 请求失败: {}", chunk.index, e)));
            }
        };

        if !response.status().is_success() && response.status().as_u16() != 206 {
            // 下载失败，恢复分块状态
            let mut chunks_guard = chunks.lock().await;
            if let Some(c) = chunks_guard.iter_mut().find(|c| c.index == chunk.index) {
                c.downloaded = chunk.downloaded;
            }
            return Err(AppError::Download(format!(
                "分块 {} HTTP 错误: {}",
                chunk.index,
                response.status()
            )));
        }

        // 打开临时文件并定位
        let mut file = OpenOptions::new()
            .write(true)
            .open(temp_path)
            .await
            .map_err(|e| AppError::Download(format!("打开临时文件失败: {}", e)))?;

        file.seek(std::io::SeekFrom::Start(range_start)).await?;

        // 下载数据
        let mut stream = response.bytes_stream();
        let mut actual_downloaded = chunk.downloaded;

        while let Some(chunk_result) = stream.next().await {
            if cancelled.load(Ordering::SeqCst) {
                return Ok(());
            }

            let data =
                chunk_result.map_err(|e| AppError::Download(format!("读取数据失败: {}", e)))?;

            file.write_all(&data).await?;

            let bytes_written = data.len() as u64;
            actual_downloaded += bytes_written;
            total_downloaded.fetch_add(bytes_written, Ordering::SeqCst);
        }

        file.flush().await?;

        // 更新实际下载量
        {
            let mut chunks_guard = chunks.lock().await;
            if let Some(c) = chunks_guard.iter_mut().find(|c| c.index == chunk.index) {
                c.downloaded = actual_downloaded;
            }
        }

        debug!(
            "线程 {} 完成分块 {}: {} 字节",
            thread_id,
            chunk.index,
            actual_downloaded
        );
    }
}

/// 加载下载状态
async fn load_download_state(path: &Path) -> Option<DownloadState> {
    match tokio::fs::read_to_string(path).await {
        Ok(content) => match serde_json::from_str(&content) {
            Ok(state) => Some(state),
            Err(e) => {
                warn!("解析下载状态失败: {}", e);
                None
            }
        },
        Err(_) => None,
    }
}

/// 保存下载状态
async fn save_download_state(path: &Path, state: &DownloadState) -> AppResult<()> {
    let content = serde_json::to_string_pretty(state)?;
    tokio::fs::write(path, content).await?;
    Ok(())
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

    #[test]
    fn test_chunk_info() {
        let chunk = ChunkInfo::new(0, 0, 999);
        assert_eq!(chunk.size(), 1000);
        assert_eq!(chunk.current_pos(), 0);
        assert!(!chunk.is_complete());
        assert_eq!(chunk.remaining(), 1000);

        let mut chunk = ChunkInfo::new(1, 1000, 1999);
        chunk.downloaded = 500;
        assert_eq!(chunk.current_pos(), 1500);
        assert!(!chunk.is_complete());
        assert_eq!(chunk.remaining(), 500);

        chunk.downloaded = 1000;
        assert!(chunk.is_complete());
        assert_eq!(chunk.remaining(), 0);
    }

    #[test]
    fn test_create_chunks() {
        // 10MB 文件，4 线程
        let chunks = create_chunks(10 * 1024 * 1024, 4, 1024 * 1024);
        assert_eq!(chunks.len(), 4);

        // 验证分块覆盖整个文件
        let total: u64 = chunks.iter().map(|c| c.size()).sum();
        assert_eq!(total, 10 * 1024 * 1024);

        // 验证分块连续
        for i in 1..chunks.len() {
            assert_eq!(chunks[i].start, chunks[i - 1].end + 1);
        }

        // 小文件，线程数应该自动调整
        let chunks = create_chunks(2 * 1024 * 1024, 4, 1024 * 1024);
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn test_download_state_serialization() {
        let state = DownloadState {
            url: "https://example.com/file.zip".to_string(),
            total_size: 1024 * 1024,
            chunks: vec![
                ChunkInfo::new(0, 0, 511 * 1024),
                ChunkInfo::new(1, 512 * 1024, 1024 * 1024 - 1),
            ],
            filename: "file.zip".to_string(),
        };

        let json = serde_json::to_string(&state).unwrap();
        let restored: DownloadState = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.url, state.url);
        assert_eq!(restored.total_size, state.total_size);
        assert_eq!(restored.chunks.len(), 2);
        assert_eq!(restored.filename, state.filename);
    }

    /// 真实网络下载测试（需要网络连接）
    /// 运行: cargo test test_chunked_download_real -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn test_chunked_download_real() {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc;

        // 使用 Downloader::new() 测试，看是否使用了代理
        let downloader = Downloader::new().expect("创建下载器失败");

        let temp_dir = std::env::temp_dir().join("downloader_test");
        let _ = std::fs::create_dir_all(&temp_dir);

        let progress_count = Arc::new(AtomicU64::new(0));
        let progress_count_clone = progress_count.clone();

        let options = DownloadOptions {
            save_dir: Some(temp_dir.clone()),
            filename: Some("test_10mb.bin".to_string()),
            overwrite: true,
            use_github_mirror: false,
            num_threads: 4,
            resume: true,
            min_chunk_size: 1024 * 1024, // 1MB
        };

        // 使用可靠的测试服务器
        let result = downloader
            .download_chunked(
                "http://speedtest.tele2.net/10MB.zip",
                options,
                move |progress| {
                    progress_count_clone.fetch_add(1, Ordering::SeqCst);
                    if progress.percentage as u64 % 20 == 0 {
                        println!(
                            "下载进度: {:.1}% ({}/{}) 速度: {}",
                            progress.percentage,
                            progress.downloaded_string(),
                            progress.total_string(),
                            progress.speed_string()
                        );
                    }
                },
            )
            .await;

        match result {
            Ok(download_result) => {
                println!("下载完成: {:?}", download_result.path);
                assert!(download_result.path.exists());
                assert_eq!(download_result.size, 10 * 1024 * 1024); // 10MB
                assert!(progress_count.load(Ordering::SeqCst) > 0);

                // 清理
                let _ = std::fs::remove_file(&download_result.path);
            }
            Err(e) => {
                println!("下载失败: {}", e);
                // 在 CI 环境中可能网络不可用，不 panic
            }
        }

        // 清理临时目录
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

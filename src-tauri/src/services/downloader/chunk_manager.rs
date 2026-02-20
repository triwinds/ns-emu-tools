//! 分块下载管理
//!
//! 支持 Range 探测、分块计算和并发下载

use crate::error::{AppError, AppResult};
use crate::services::downloader::state_store::ChunkState;
use futures_util::StreamExt;
use reqwest::header::{ACCEPT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, RANGE};
use reqwest::Client;
use std::io::SeekFrom;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, trace, warn};

const WRITE_BUFFER_SIZE: usize = 64 * 1024; // 64KB
const FLUSH_INTERVAL: Duration = Duration::from_secs(1);

/// Range 支持检测结果
#[derive(Debug, Clone)]
pub struct RangeSupport {
    /// 是否支持 Range 请求
    pub supports_range: bool,
    /// 文件总大小（0 表示未知）
    pub total_size: u64,
    /// ETag（用于验证文件一致性）
    pub etag: Option<String>,
    /// Last-Modified（用于验证文件一致性）
    pub last_modified: Option<String>,
}

/// 分块进度更新
#[derive(Debug, Clone)]
pub struct ChunkProgress {
    /// 分块索引
    pub index: usize,
    /// 已下载字节数
    pub downloaded: u64,
    /// 是否完成
    pub completed: bool,
}

/// 分块管理器
pub struct ChunkManager {
    /// 分块数量
    split: u32,
    /// 最小分块大小（字节）
    min_split_size: u64,
}

impl ChunkManager {
    /// 创建新的分块管理器
    pub fn new(split: u32, min_split_size: &str) -> Self {
        let min_size = parse_size(min_split_size).unwrap_or(4 * 1024 * 1024); // 默认 4MB
        Self {
            split,
            min_split_size: min_size,
        }
    }

    /// 检测服务器是否支持 Range 请求
    ///
    /// 使用 GET + Range: bytes=0-0 探测
    pub async fn check_range_support(client: &Client, url: &str) -> AppResult<RangeSupport> {
        debug!("检测 Range 支持: {}", url);

        // 发送 Range 请求探测
        let response = client
            .get(url)
            .header(RANGE, "bytes=0-0")
            .header(ACCEPT_ENCODING, "identity") // 避免压缩影响字节范围
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Range 探测请求失败: {}", e)))?;

        let status = response.status();
        let headers = response.headers().clone();

        // 解析 ETag 和 Last-Modified
        let etag = headers
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let last_modified = headers
            .get("last-modified")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // 206 Partial Content 表示支持 Range
        if status == reqwest::StatusCode::PARTIAL_CONTENT {
            // 从 Content-Range 解析总大小
            // 格式: bytes 0-0/12345
            if let Some(content_range) = headers.get(CONTENT_RANGE) {
                if let Ok(range_str) = content_range.to_str() {
                    if let Some(total) = parse_content_range_total(range_str) {
                        info!("服务器支持 Range，文件大小: {} 字节", total);
                        return Ok(RangeSupport {
                            supports_range: true,
                            total_size: total,
                            etag,
                            last_modified,
                        });
                    }
                }
            }

            // 有 206 但无法解析大小
            info!("服务器支持 Range，但无法确定文件大小");
            return Ok(RangeSupport {
                supports_range: true,
                total_size: 0,
                etag,
                last_modified,
            });
        }

        // 200 OK 表示不支持 Range，尝试从 Content-Length 获取大小
        if status == reqwest::StatusCode::OK {
            let total_size = headers
                .get(CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);

            info!(
                "服务器不支持 Range，文件大小: {} 字节",
                if total_size > 0 {
                    total_size.to_string()
                } else {
                    "未知".to_string()
                }
            );

            return Ok(RangeSupport {
                supports_range: false,
                total_size,
                etag,
                last_modified,
            });
        }

        // 其他状态码
        warn!("Range 探测返回意外状态码: {}", status);
        Err(AppError::Network(format!(
            "Range 探测失败，HTTP 状态码: {}",
            status
        )))
    }

    /// 计算分块策略
    pub fn calculate_chunks(&self, total_size: u64, supports_range: bool) -> Vec<ChunkState> {
        // 不支持 Range 或文件太小，使用单连接
        if !supports_range || total_size == 0 || total_size < self.min_split_size {
            debug!(
                "使用单连接下载: supports_range={}, total_size={}, min_split_size={}",
                supports_range, total_size, self.min_split_size
            );
            return vec![ChunkState::new(
                0,
                0,
                if total_size > 0 { total_size - 1 } else { 0 },
            )];
        }

        // 计算实际分块数量
        let chunk_count = std::cmp::min(
            self.split as u64,
            total_size / self.min_split_size,
        )
        .max(1) as usize;

        let chunk_size = total_size / chunk_count as u64;
        let mut chunks = Vec::with_capacity(chunk_count);

        for i in 0..chunk_count {
            let start = i as u64 * chunk_size;
            let end = if i == chunk_count - 1 {
                total_size - 1 // 最后一个分块包含剩余所有字节
            } else {
                (i as u64 + 1) * chunk_size - 1
            };

            chunks.push(ChunkState::new(i, start, end));
        }

        debug!("分块策略: {} 个分块，每块约 {} 字节", chunk_count, chunk_size);
        chunks
    }

    /// 下载单个分块
    ///
    /// # 参数
    /// - `client`: HTTP 客户端
    /// - `url`: 下载 URL
    /// - `chunk`: 分块状态
    /// - `file_path`: 目标文件路径
    /// - `progress_tx`: 进度发送通道
    /// - `cancel_token`: 取消令牌
    pub async fn download_chunk(
        client: &Client,
        url: &str,
        chunk: &ChunkState,
        file_path: &Path,
        progress_tx: mpsc::UnboundedSender<ChunkProgress>,
        cancel_token: CancellationToken,
    ) -> AppResult<()> {
        let current_pos = chunk.current_position();
        let end = chunk.end;

        // 如果已完成，直接返回
        if chunk.completed || current_pos > end {
            let _ = progress_tx.send(ChunkProgress {
                index: chunk.index,
                downloaded: chunk.downloaded,
                completed: true,
            });
            return Ok(());
        }

        debug!(
            "下载分块 {}: bytes={}-{} (已下载: {})",
            chunk.index, current_pos, end, chunk.downloaded
        );

        debug!(
            "发送分块请求: index={}, range=bytes={}-{}, url={}",
            chunk.index,
            current_pos,
            end,
            url
        );

        // 发送 Range 请求
        let response = client
            .get(url)
            .header(RANGE, format!("bytes={}-{}", current_pos, end))
            .header(ACCEPT_ENCODING, "identity")
            .send()
            .await
            .map_err(|e| AppError::Network(format!("分块 {} 请求失败: {}", chunk.index, e)))?;

        let status = response.status();
        debug!("分块响应: index={}, status={}", chunk.index, status);
        if status != reqwest::StatusCode::PARTIAL_CONTENT && status != reqwest::StatusCode::OK {
            return Err(AppError::Network(format!(
                "分块 {} 下载失败，HTTP 状态码: {}",
                chunk.index, status
            )));
        }

        // 打开文件并定位到写入位置
        debug!(
            "打开临时文件准备写入: index={}, path={}, offset={}",
            chunk.index,
            file_path.display(),
            current_pos
        );
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(file_path)
            .await
            .map_err(|e| AppError::Io(e))?;

        file.seek(SeekFrom::Start(current_pos)).await?;

        // 流式下载
        let mut stream = response.bytes_stream();
        let mut downloaded = chunk.downloaded;
        let mut buffer = Vec::with_capacity(WRITE_BUFFER_SIZE);
        let mut last_flush = Instant::now();

        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    debug!("分块 {} 下载被取消", chunk.index);
                    // 刷新缓冲区
                    if !buffer.is_empty() {
                        file.write_all(&buffer).await?;
                    }
                    file.sync_all().await?;
                    return Err(AppError::Download("下载被取消".to_string()));
                }
                chunk_result = stream.next() => {
                    match chunk_result {
                        Some(Ok(bytes)) => {
                            buffer.extend_from_slice(&bytes);
                            downloaded += bytes.len() as u64;

                            // 缓冲区满或间隔到达时写入，确保慢速连接也能持续写盘/上报进度
                            if buffer.len() >= WRITE_BUFFER_SIZE || last_flush.elapsed() >= FLUSH_INTERVAL {
                                file.write_all(&buffer).await?;
                                buffer.clear();
                                last_flush = Instant::now();

                                trace!(
                                    "分块写入/进度: index={}, downloaded={}",
                                    chunk.index,
                                    downloaded
                                );

                                let _ = progress_tx.send(ChunkProgress {
                                    index: chunk.index,
                                    downloaded,
                                    completed: false,
                                });
                            }
                        }
                        Some(Err(e)) => {
                            // 写入已下载的数据
                            if !buffer.is_empty() {
                                file.write_all(&buffer).await?;
                            }
                            file.sync_all().await?;
                            return Err(AppError::Network(format!("分块 {} 下载错误: {}", chunk.index, e)));
                        }
                        None => {
                            // 下载完成，写入剩余数据
                            if !buffer.is_empty() {
                                file.write_all(&buffer).await?;
                            }
                            file.sync_all().await?;

                            // 发送完成进度
                            let _ = progress_tx.send(ChunkProgress {
                                index: chunk.index,
                                downloaded,
                                completed: true,
                            });

                            debug!("分块 {} 下载完成，共 {} 字节", chunk.index, downloaded);
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    /// 下载单连接（不使用 Range）
    pub async fn download_single(
        client: &Client,
        url: &str,
        file_path: &Path,
        progress_tx: mpsc::UnboundedSender<ChunkProgress>,
        cancel_token: CancellationToken,
        resume_from: u64,
    ) -> AppResult<u64> {
        debug!("单连接下载: url={}, resume_from={}", url, resume_from);

        // 构建请求
        let mut request = client.get(url).header(ACCEPT_ENCODING, "identity");

        // 如果有断点，添加 Range 头
        if resume_from > 0 {
            debug!("从 {} 字节处恢复下载", resume_from);
            request = request.header(RANGE, format!("bytes={}-", resume_from));
        }

        let response = request
            .send()
            .await
            .map_err(|e| AppError::Network(format!("下载请求失败: {}", e)))?;

        let status = response.status();
        debug!("单连接响应: status={}", status);
        if !status.is_success() {
            return Err(AppError::Network(format!(
                "下载失败，HTTP 状态码: {}",
                status
            )));
        }

        // 打开文件
        let mut file = if resume_from > 0 {
            debug!("单连接打开文件续写: path={}, offset={}", file_path.display(), resume_from);
            let mut f = OpenOptions::new()
                .write(true)
                .create(true)
                .open(file_path)
                .await?;
            f.seek(SeekFrom::Start(resume_from)).await?;
            f
        } else {
            debug!("单连接创建新文件: path={}", file_path.display());
            File::create(file_path).await?
        };

        // 流式下载
        let mut stream = response.bytes_stream();
        let mut downloaded = resume_from;
        let mut buffer = Vec::with_capacity(WRITE_BUFFER_SIZE);
        let mut last_flush = Instant::now();

        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    if !buffer.is_empty() {
                        file.write_all(&buffer).await?;
                    }
                    file.sync_all().await?;
                    return Err(AppError::Download("下载被取消".to_string()));
                }
                chunk_result = stream.next() => {
                    match chunk_result {
                        Some(Ok(bytes)) => {
                            buffer.extend_from_slice(&bytes);
                            downloaded += bytes.len() as u64;

                            if buffer.len() >= WRITE_BUFFER_SIZE || last_flush.elapsed() >= FLUSH_INTERVAL {
                                file.write_all(&buffer).await?;
                                buffer.clear();
                                last_flush = Instant::now();

                                trace!("单连接写入/进度: downloaded={}", downloaded);

                                let _ = progress_tx.send(ChunkProgress {
                                    index: 0,
                                    downloaded,
                                    completed: false,
                                });
                            }
                        }
                        Some(Err(e)) => {
                            if !buffer.is_empty() {
                                file.write_all(&buffer).await?;
                            }
                            file.sync_all().await?;
                            return Err(AppError::Network(format!("下载错误: {}", e)));
                        }
                        None => {
                            if !buffer.is_empty() {
                                file.write_all(&buffer).await?;
                            }
                            file.sync_all().await?;

                            let _ = progress_tx.send(ChunkProgress {
                                index: 0,
                                downloaded,
                                completed: true,
                            });

                            return Ok(downloaded);
                        }
                    }
                }
            }
        }
    }
}

/// 解析 Content-Range 头中的总大小
///
/// 格式: bytes 0-0/12345 或 bytes */12345
fn parse_content_range_total(range_str: &str) -> Option<u64> {
    // 查找 /
    let slash_pos = range_str.rfind('/')?;
    let total_str = &range_str[slash_pos + 1..];

    if total_str == "*" {
        return None; // 未知大小
    }

    total_str.trim().parse::<u64>().ok()
}

/// 解析大小字符串（如 "4M", "1G"）
fn parse_size(size_str: &str) -> Option<u64> {
    let size_str = size_str.trim().to_uppercase();

    if size_str.is_empty() {
        return None;
    }

    let (num_str, multiplier) = if size_str.ends_with('K') {
        (&size_str[..size_str.len() - 1], 1024u64)
    } else if size_str.ends_with('M') {
        (&size_str[..size_str.len() - 1], 1024 * 1024)
    } else if size_str.ends_with('G') {
        (&size_str[..size_str.len() - 1], 1024 * 1024 * 1024)
    } else if size_str.ends_with("KB") {
        (&size_str[..size_str.len() - 2], 1024)
    } else if size_str.ends_with("MB") {
        (&size_str[..size_str.len() - 2], 1024 * 1024)
    } else if size_str.ends_with("GB") {
        (&size_str[..size_str.len() - 2], 1024 * 1024 * 1024)
    } else {
        (size_str.as_str(), 1)
    };

    num_str.trim().parse::<u64>().ok().map(|n| n * multiplier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("4M"), Some(4 * 1024 * 1024));
        assert_eq!(parse_size("4MB"), Some(4 * 1024 * 1024));
        assert_eq!(parse_size("1G"), Some(1024 * 1024 * 1024));
        assert_eq!(parse_size("1GB"), Some(1024 * 1024 * 1024));
        assert_eq!(parse_size("512K"), Some(512 * 1024));
        assert_eq!(parse_size("512KB"), Some(512 * 1024));
        assert_eq!(parse_size("1024"), Some(1024));
        assert_eq!(parse_size(""), None);
    }

    #[test]
    fn test_parse_size_edge_cases() {
        // 大小写不敏感
        assert_eq!(parse_size("4m"), Some(4 * 1024 * 1024));
        assert_eq!(parse_size("4Mb"), Some(4 * 1024 * 1024));

        // 带空格
        assert_eq!(parse_size("  4M  "), Some(4 * 1024 * 1024));

        // 边界值
        assert_eq!(parse_size("0"), Some(0));
        assert_eq!(parse_size("1"), Some(1));

        // 无效输入
        assert_eq!(parse_size("abc"), None);
        assert_eq!(parse_size("M"), None);
    }

    #[test]
    fn test_parse_content_range_total() {
        assert_eq!(parse_content_range_total("bytes 0-0/12345"), Some(12345));
        assert_eq!(parse_content_range_total("bytes 0-999/1000"), Some(1000));
        assert_eq!(parse_content_range_total("bytes */12345"), Some(12345));
        assert_eq!(parse_content_range_total("bytes 0-0/*"), None);
    }

    #[test]
    fn test_parse_content_range_edge_cases() {
        // 带空格
        assert_eq!(parse_content_range_total("bytes 0-0/ 12345 "), Some(12345));

        // 大文件
        assert_eq!(
            parse_content_range_total("bytes 0-0/9999999999999"),
            Some(9999999999999)
        );

        // 无效格式
        assert_eq!(parse_content_range_total("invalid"), None);
        assert_eq!(parse_content_range_total("bytes 0-0"), None);
    }

    #[test]
    fn test_calculate_chunks_small_file() {
        let manager = ChunkManager::new(4, "4M");
        let chunks = manager.calculate_chunks(1024 * 1024, true); // 1MB

        assert_eq!(chunks.len(), 1); // 小于 min_split_size，单连接
        assert_eq!(chunks[0].start, 0);
        assert_eq!(chunks[0].end, 1024 * 1024 - 1);
    }

    #[test]
    fn test_calculate_chunks_large_file() {
        let manager = ChunkManager::new(4, "4M");
        let chunks = manager.calculate_chunks(100 * 1024 * 1024, true); // 100MB

        assert_eq!(chunks.len(), 4);

        // 验证分块覆盖整个文件
        let total_size: u64 = chunks.iter().map(|c| c.size()).sum();
        assert_eq!(total_size, 100 * 1024 * 1024);

        // 验证分块连续
        for i in 1..chunks.len() {
            assert_eq!(chunks[i].start, chunks[i - 1].end + 1);
        }
    }

    #[test]
    fn test_calculate_chunks_no_range_support() {
        let manager = ChunkManager::new(4, "4M");
        let chunks = manager.calculate_chunks(100 * 1024 * 1024, false);

        assert_eq!(chunks.len(), 1); // 不支持 Range，单连接
    }

    #[test]
    fn test_calculate_chunks_unknown_size() {
        let manager = ChunkManager::new(4, "4M");
        let chunks = manager.calculate_chunks(0, true);

        assert_eq!(chunks.len(), 1); // 未知大小，单连接
    }

    #[test]
    fn test_chunk_manager_new() {
        let manager = ChunkManager::new(8, "8M");

        assert_eq!(manager.split, 8);
        assert_eq!(manager.min_split_size, 8 * 1024 * 1024);
    }

    #[test]
    fn test_calculate_chunks_exact_boundary() {
        // 测试刚好等于 min_split_size 的情况
        let manager = ChunkManager::new(4, "4M");
        let chunks = manager.calculate_chunks(4 * 1024 * 1024, true); // 正好 4MB

        // 刚好等于 min_split_size，应该是单连接
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_calculate_chunks_just_above_boundary() {
        // 测试刚刚超过 min_split_size 的情况
        // 分块策略会考虑每块的最小大小，刚好超过 min_split_size 时可能仍是单块
        let manager = ChunkManager::new(4, "4M");
        let chunks = manager.calculate_chunks(4 * 1024 * 1024 + 1, true);

        // 验证分块覆盖整个文件
        let total_size: u64 = chunks.iter().map(|c| c.size()).sum();
        assert_eq!(total_size, 4 * 1024 * 1024 + 1);

        // 验证至少有 1 个分块
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_calculate_chunks_uneven_division() {
        // 测试不能整除的情况
        let manager = ChunkManager::new(3, "1M");
        let chunks = manager.calculate_chunks(10 * 1024 * 1024, true); // 10MB, 3 chunks

        assert_eq!(chunks.len(), 3);

        // 验证总大小正确
        let total_size: u64 = chunks.iter().map(|c| c.size()).sum();
        assert_eq!(total_size, 10 * 1024 * 1024);

        // 验证分块连续
        for i in 1..chunks.len() {
            assert_eq!(chunks[i].start, chunks[i - 1].end + 1);
        }
    }

    #[test]
    fn test_calculate_chunks_single_split() {
        // 测试 split = 1 的情况
        let manager = ChunkManager::new(1, "1M");
        let chunks = manager.calculate_chunks(100 * 1024 * 1024, true);

        // split = 1 应该始终是单连接
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_calculate_chunks_large_split() {
        // 测试 split 数量大于合理分块数的情况
        let manager = ChunkManager::new(100, "1M");
        let chunks = manager.calculate_chunks(10 * 1024 * 1024, true);

        // 分块数应该受限于实际需要
        assert!(chunks.len() <= 100);
        assert!(chunks.len() > 1);
    }

    #[test]
    fn test_chunk_progress_default() {
        let progress = ChunkProgress {
            index: 5,
            downloaded: 1000,
            completed: false,
        };

        assert_eq!(progress.index, 5);
        assert_eq!(progress.downloaded, 1000);
        assert!(!progress.completed);
    }

    #[test]
    fn test_range_support_default() {
        let support = RangeSupport {
            supports_range: true,
            total_size: 1000000,
            etag: Some("abc".to_string()),
            last_modified: None,
        };

        assert!(support.supports_range);
        assert_eq!(support.total_size, 1000000);
        assert_eq!(support.etag, Some("abc".to_string()));
        assert!(support.last_modified.is_none());
    }
}

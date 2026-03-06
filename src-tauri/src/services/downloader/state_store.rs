//! 下载状态持久化
//!
//! 使用 JSON 状态文件（`.download`）持久化下载状态，支持断点续传

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, warn};

/// 下载状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadState {
    /// 原始下载 URL
    pub url: String,
    /// 解析后的最终 URL（重定向后）
    pub resolved_url: Option<String>,
    /// 文件总大小（0 表示未知长度）
    pub total_size: u64,
    /// 是否支持 Range 请求
    pub supports_range: bool,
    /// ETag 头（用于验证文件是否变化）
    pub etag: Option<String>,
    /// Last-Modified 头（用于验证文件是否变化）
    pub last_modified: Option<String>,
    /// 分块信息
    pub chunks: Vec<ChunkState>,
    /// 最后更新时间戳（Unix 时间戳）
    pub last_updated: i64,
    /// 下载进程 PID（用于检测僵尸状态）
    pub pid: Option<u32>,
    /// 文件名
    pub filename: String,
    /// 保存目录
    pub save_dir: PathBuf,
}

/// 分块状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChunkState {
    /// 分块索引
    pub index: usize,
    /// 起始字节位置
    pub start: u64,
    /// 结束字节位置（包含）
    pub end: u64,
    /// 已下载字节数
    pub downloaded: u64,
    /// 是否已完成
    pub completed: bool,
}

impl ChunkState {
    /// 创建新的分块状态
    pub fn new(index: usize, start: u64, end: u64) -> Self {
        Self {
            index,
            start,
            end,
            downloaded: 0,
            completed: false,
        }
    }

    /// 获取分块大小
    pub fn size(&self) -> u64 {
        self.end - self.start + 1
    }

    /// 获取当前写入位置
    pub fn current_position(&self) -> u64 {
        self.start + self.downloaded
    }

    /// 获取剩余字节数
    pub fn remaining(&self) -> u64 {
        self.size().saturating_sub(self.downloaded)
    }
}

impl DownloadState {
    /// 创建新的下载状态
    pub fn new(
        url: &str,
        filename: &str,
        save_dir: PathBuf,
        total_size: u64,
        supports_range: bool,
    ) -> Self {
        Self {
            url: url.to_string(),
            resolved_url: None,
            total_size,
            supports_range,
            etag: None,
            last_modified: None,
            chunks: Vec::new(),
            last_updated: chrono::Utc::now().timestamp(),
            pid: Some(std::process::id()),
            filename: filename.to_string(),
            save_dir,
        }
    }

    /// 获取状态文件路径
    pub fn state_file_path(&self) -> PathBuf {
        self.save_dir.join(get_state_filename(&self.filename))
    }

    /// 获取临时文件路径
    pub fn temp_file_path(&self) -> PathBuf {
        self.save_dir.join(get_temp_filename(&self.filename))
    }

    /// 获取最终文件路径
    pub fn final_file_path(&self) -> PathBuf {
        self.save_dir.join(&self.filename)
    }

    /// 获取已下载的总字节数
    pub fn downloaded_bytes(&self) -> u64 {
        self.chunks.iter().map(|c| c.downloaded).sum()
    }

    /// 检查是否所有分块都已完成
    pub fn is_complete(&self) -> bool {
        !self.chunks.is_empty() && self.chunks.iter().all(|c| c.completed)
    }

    /// 更新时间戳
    pub fn touch(&mut self) {
        self.last_updated = chrono::Utc::now().timestamp();
    }

    /// 验证远端一致性
    ///
    /// 检查 URL、ETag、Last-Modified 和 Content-Length 是否匹配
    pub fn validate_consistency(
        &self,
        url: &str,
        resolved_url: Option<&str>,
        total_size: u64,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> bool {
        // URL 必须匹配（原始 URL 或解析后的 URL）
        let url_matches = self.url == url
            || self.resolved_url.as_deref() == Some(url)
            || resolved_url.map_or(false, |r| self.url == r || self.resolved_url.as_deref() == Some(r));

        if !url_matches {
            debug!("URL 不匹配: 状态文件 URL={}, 请求 URL={}", self.url, url);
            return false;
        }

        // 如果有 ETag，必须匹配
        if let (Some(state_etag), Some(new_etag)) = (&self.etag, etag) {
            if state_etag != new_etag {
                debug!("ETag 不匹配: 状态文件={}, 服务器={}", state_etag, new_etag);
                return false;
            }
        }

        // 如果有 Last-Modified，必须匹配
        if let (Some(state_lm), Some(new_lm)) = (&self.last_modified, last_modified) {
            if state_lm != new_lm {
                debug!(
                    "Last-Modified 不匹配: 状态文件={}, 服务器={}",
                    state_lm, new_lm
                );
                return false;
            }
        }

        // 如果有 Content-Length，必须匹配
        if self.total_size > 0 && total_size > 0 && self.total_size != total_size {
            debug!(
                "Content-Length 不匹配: 状态文件={}, 服务器={}",
                self.total_size, total_size
            );
            return false;
        }

        true
    }
}

/// 状态存储管理器
pub struct StateStore {
    /// 保存目录
    save_dir: PathBuf,
}

impl StateStore {
    /// 创建新的状态存储管理器
    pub fn new(save_dir: PathBuf) -> Self {
        Self { save_dir }
    }

    /// 加载下载状态
    ///
    /// 如果状态文件不存在或无效，返回 None
    pub async fn load(&self, filename: &str) -> AppResult<Option<DownloadState>> {
        let state_path = self.save_dir.join(get_state_filename(filename));

        if !state_path.exists() {
            return Ok(None);
        }

        let content = match fs::read_to_string(&state_path).await {
            Ok(c) => c,
            Err(e) => {
                warn!("读取状态文件失败: {}", e);
                return Ok(None);
            }
        };

        let state: DownloadState = match serde_json::from_str(&content) {
            Ok(s) => s,
            Err(e) => {
                warn!("解析状态文件失败: {}", e);
                // 删除损坏的状态文件
                let _ = fs::remove_file(&state_path).await;
                return Ok(None);
            }
        };

        // 检查 PID 是否仍在运行（避免多实例冲突）
        if let Some(pid) = state.pid {
            if is_process_running(pid) && pid != std::process::id() {
                warn!("下载任务正在被另一个进程处理 (PID: {})", pid);
                return Err(AppError::Download(format!(
                    "下载任务正在被另一个进程处理 (PID: {})",
                    pid
                )));
            }
        }

        debug!("加载状态文件: {:?}", state_path);
        Ok(Some(state))
    }

    /// 保存下载状态（原子写入）
    ///
    /// 使用临时文件 + rename 确保原子性
    pub async fn save(&self, state: &DownloadState) -> AppResult<()> {
        let state_path = state.state_file_path();
        let temp_path = state_path.with_extension("download.tmp");

        // 序列化状态
        let content = serde_json::to_string_pretty(state)
            .map_err(|e| AppError::Download(format!("序列化状态失败: {}", e)))?;

        // 写入临时文件
        let mut file = fs::File::create(&temp_path).await?;
        file.write_all(content.as_bytes()).await?;
        file.sync_all().await?;

        // 原子重命名
        fs::rename(&temp_path, &state_path).await?;

        debug!("保存状态文件: {:?}", state_path);
        Ok(())
    }

    /// 删除状态文件
    pub async fn delete(&self, filename: &str) -> AppResult<()> {
        let state_path = self.save_dir.join(get_state_filename(filename));

        if state_path.exists() {
            fs::remove_file(&state_path).await?;
            debug!("删除状态文件: {:?}", state_path);
        }

        Ok(())
    }

    /// 删除临时文件
    pub async fn delete_temp_file(&self, filename: &str) -> AppResult<()> {
        let temp_path = self.save_dir.join(get_temp_filename(filename));

        if temp_path.exists() {
            fs::remove_file(&temp_path).await?;
            debug!("删除临时文件: {:?}", temp_path);
        }

        Ok(())
    }

    /// 清理所有相关文件（状态文件 + 临时文件）
    pub async fn cleanup(&self, filename: &str) -> AppResult<()> {
        self.delete(filename).await?;
        self.delete_temp_file(filename).await?;
        Ok(())
    }
}

/// 获取临时文件名
pub fn get_temp_filename(filename: &str) -> String {
    format!("{}.part", filename)
}

/// 获取状态文件名
pub fn get_state_filename(filename: &str) -> String {
    format!("{}.download", filename)
}

/// 检查进程是否正在运行
fn is_process_running(pid: u32) -> bool {
    use sysinfo::{Pid, System};

    let mut system = System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true);
    system.process(Pid::from_u32(pid)).is_some()
}

/// 获取可用磁盘空间
#[cfg(unix)]
pub fn get_available_space(path: &Path) -> AppResult<u64> {
    // 使用 statvfs 获取磁盘空间信息
    let path_str = path.to_string_lossy();
    let output = std::process::Command::new("df")
        .arg("-k")
        .arg(path_str.as_ref())
        .output()
        .map_err(|e| AppError::Io(e))?;

    if !output.status.success() {
        return Err(AppError::Download("无法获取磁盘空间信息".to_string()));
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = output_str.lines().collect();

    if lines.len() < 2 {
        return Err(AppError::Download("无法解析磁盘空间信息".to_string()));
    }

    // 解析 df 输出的第二行
    let parts: Vec<&str> = lines[1].split_whitespace().collect();
    if parts.len() < 4 {
        return Err(AppError::Download("无法解析磁盘空间信息".to_string()));
    }

    // 第 4 列是可用空间（KB）
    let available_kb: u64 = parts[3]
        .parse()
        .map_err(|_| AppError::Download("无法解析可用空间".to_string()))?;

    Ok(available_kb * 1024)
}

/// 获取可用磁盘空间
#[cfg(windows)]
pub fn get_available_space(path: &Path) -> AppResult<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::core::PCWSTR;

    let path_wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut free_bytes_available: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut total_free_bytes: u64 = 0;

    unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR::from_raw(path_wide.as_ptr()),
            Some(&mut free_bytes_available as *mut u64),
            Some(&mut total_bytes as *mut u64),
            Some(&mut total_free_bytes as *mut u64),
        )
        .map_err(|e| AppError::Download(format!("获取磁盘空间失败: {}", e)))?;
    }

    Ok(free_bytes_available)
}

/// 检查磁盘空间是否足够
pub fn check_disk_space(save_dir: &Path, required_size: u64) -> AppResult<()> {
    if required_size == 0 {
        // 未知大小，跳过检查
        return Ok(());
    }

    let available = get_available_space(save_dir)?;

    // 预留 10% 或 100MB 的缓冲空间
    let buffer = std::cmp::max(required_size / 10, 100 * 1024 * 1024);

    if available < required_size + buffer {
        return Err(AppError::Download(format!(
            "磁盘空间不足: 需要 {}MB，可用 {}MB",
            (required_size + buffer) / 1024 / 1024,
            available / 1024 / 1024
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_chunk_state() {
        let chunk = ChunkState::new(0, 0, 999);

        assert_eq!(chunk.size(), 1000);
        assert_eq!(chunk.current_position(), 0);
        assert_eq!(chunk.remaining(), 1000);
        assert!(!chunk.completed);
    }

    #[test]
    fn test_chunk_state_partial() {
        let mut chunk = ChunkState::new(0, 0, 999);
        chunk.downloaded = 500;

        assert_eq!(chunk.size(), 1000);
        assert_eq!(chunk.current_position(), 500);
        assert_eq!(chunk.remaining(), 500);
    }

    #[test]
    fn test_download_state_new() {
        let state = DownloadState::new(
            "https://example.com/file.zip",
            "file.zip",
            PathBuf::from("/tmp"),
            1000,
            true,
        );

        assert_eq!(state.url, "https://example.com/file.zip");
        assert_eq!(state.filename, "file.zip");
        assert_eq!(state.total_size, 1000);
        assert!(state.supports_range);
        assert!(state.pid.is_some());
    }

    #[test]
    fn test_download_state_paths() {
        let state = DownloadState::new(
            "https://example.com/file.zip",
            "file.zip",
            PathBuf::from("/tmp"),
            1000,
            true,
        );

        assert_eq!(state.state_file_path(), PathBuf::from("/tmp/file.zip.download"));
        assert_eq!(state.temp_file_path(), PathBuf::from("/tmp/file.zip.part"));
        assert_eq!(state.final_file_path(), PathBuf::from("/tmp/file.zip"));
    }

    #[test]
    fn test_download_state_downloaded_bytes() {
        let mut state = DownloadState::new(
            "https://example.com/file.zip",
            "file.zip",
            PathBuf::from("/tmp"),
            1000,
            true,
        );

        state.chunks = vec![
            ChunkState {
                index: 0,
                start: 0,
                end: 499,
                downloaded: 300,
                completed: false,
            },
            ChunkState {
                index: 1,
                start: 500,
                end: 999,
                downloaded: 200,
                completed: false,
            },
        ];

        assert_eq!(state.downloaded_bytes(), 500);
    }

    #[test]
    fn test_download_state_is_complete() {
        let mut state = DownloadState::new(
            "https://example.com/file.zip",
            "file.zip",
            PathBuf::from("/tmp"),
            1000,
            true,
        );

        // 空分块列表
        assert!(!state.is_complete());

        // 未完成的分块
        state.chunks = vec![ChunkState {
            index: 0,
            start: 0,
            end: 999,
            downloaded: 500,
            completed: false,
        }];
        assert!(!state.is_complete());

        // 完成的分块
        state.chunks[0].completed = true;
        assert!(state.is_complete());
    }

    #[test]
    fn test_validate_consistency() {
        let mut state = DownloadState::new(
            "https://example.com/file.zip",
            "file.zip",
            PathBuf::from("/tmp"),
            1000,
            true,
        );
        state.etag = Some("abc123".to_string());
        state.last_modified = Some("Mon, 01 Jan 2024 00:00:00 GMT".to_string());

        // 完全匹配
        assert!(state.validate_consistency(
            "https://example.com/file.zip",
            None,
            1000,
            Some("abc123"),
            Some("Mon, 01 Jan 2024 00:00:00 GMT"),
        ));

        // URL 不匹配
        assert!(!state.validate_consistency(
            "https://example.com/other.zip",
            None,
            1000,
            Some("abc123"),
            Some("Mon, 01 Jan 2024 00:00:00 GMT"),
        ));

        // ETag 不匹配
        assert!(!state.validate_consistency(
            "https://example.com/file.zip",
            None,
            1000,
            Some("different"),
            Some("Mon, 01 Jan 2024 00:00:00 GMT"),
        ));

        // Content-Length 不匹配
        assert!(!state.validate_consistency(
            "https://example.com/file.zip",
            None,
            2000,
            Some("abc123"),
            Some("Mon, 01 Jan 2024 00:00:00 GMT"),
        ));
    }

    #[test]
    fn test_get_temp_filename() {
        assert_eq!(get_temp_filename("file.zip"), "file.zip.part");
        assert_eq!(get_temp_filename("test"), "test.part");
    }

    #[test]
    fn test_get_state_filename() {
        assert_eq!(get_state_filename("file.zip"), "file.zip.download");
        assert_eq!(get_state_filename("test"), "test.download");
    }

    #[tokio::test]
    async fn test_state_store_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let store = StateStore::new(temp_dir.path().to_path_buf());

        let mut state = DownloadState::new(
            "https://example.com/file.zip",
            "file.zip",
            temp_dir.path().to_path_buf(),
            1000,
            true,
        );
        state.chunks = vec![ChunkState::new(0, 0, 999)];

        // 保存状态
        store.save(&state).await.unwrap();

        // 加载状态
        let loaded = store.load("file.zip").await.unwrap().unwrap();

        assert_eq!(loaded.url, state.url);
        assert_eq!(loaded.filename, state.filename);
        assert_eq!(loaded.total_size, state.total_size);
        assert_eq!(loaded.chunks.len(), 1);
    }

    #[tokio::test]
    async fn test_state_store_delete() {
        let temp_dir = TempDir::new().unwrap();
        let store = StateStore::new(temp_dir.path().to_path_buf());

        let state = DownloadState::new(
            "https://example.com/file.zip",
            "file.zip",
            temp_dir.path().to_path_buf(),
            1000,
            true,
        );

        // 保存状态
        store.save(&state).await.unwrap();
        assert!(state.state_file_path().exists());

        // 删除状态
        store.delete("file.zip").await.unwrap();
        assert!(!state.state_file_path().exists());
    }

    #[tokio::test]
    async fn test_state_store_load_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let store = StateStore::new(temp_dir.path().to_path_buf());

        let result = store.load("nonexistent.zip").await.unwrap();
        assert!(result.is_none());
    }
}

//! bytehaul 后端适配层
//!
//! 在现有 DownloadManager trait 下接入 bytehaul，保留调用方和统一取消入口。

use crate::config::user_agent;
use crate::error::{AppError, AppResult};
use crate::services::downloader::manager::{DownloadManager, ProgressCallback};
use crate::services::downloader::types::{
    DownloadOptions, DownloadProgress, DownloadResult, DownloadStatus,
};
use crate::services::network::{get_final_url, get_github_download_url, get_proxy_url};
use async_trait::async_trait;
use bytehaul::{
    DownloadError as BytehaulError, DownloadHandle, DownloadSpec, DownloadState as BytehaulState,
    Downloader, FileAllocation, LogLevel,
};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use tokio::sync::Notify;
use tracing::{debug, warn};
use url::Url;

/// 默认下载目录与现有 RustDownloader 保持一致。
fn default_download_dir() -> AppResult<PathBuf> {
    crate::services::downloader::aria2::get_default_download_dir()
}

/// bytehaul 任务 ID 生成器。
static BYTEHAUL_TASK_ID: AtomicU64 = AtomicU64::new(1);

fn next_task_id() -> String {
    format!(
        "bytehaul-{:016x}",
        BYTEHAUL_TASK_ID.fetch_add(1, Ordering::SeqCst)
    )
}

/// 基于当前配置构造共享 bytehaul 下载器。
fn build_downloader_from_config() -> AppResult<Downloader> {
    let config = crate::config::get_config();
    let mut builder = Downloader::builder()
        .enable_ipv6(!config.setting.download.disable_aria2_ipv6)
        .log_level(LogLevel::Warn);

    if let Some(proxy_url) = get_proxy_url() {
        if !proxy_url.is_empty() {
            builder = builder.all_proxy(proxy_url);
        }
    }

    builder.build().map_err(map_bytehaul_error)
}

fn map_bytehaul_error(error: BytehaulError) -> AppError {
    match error {
        BytehaulError::Http(http_error) => AppError::from(http_error),
        BytehaulError::Io(io_error) => AppError::Io(io_error),
        BytehaulError::Cancelled => AppError::Download("下载已取消".to_string()),
        BytehaulError::Paused => AppError::Download("下载已暂停".to_string()),
        BytehaulError::HttpStatus { status, message } => {
            AppError::Download(format!("HTTP {}: {}", status, message))
        }
        BytehaulError::InvalidConfig(message) => AppError::InvalidArgument(message),
        BytehaulError::ResumeMismatch(message) => AppError::Download(message),
        BytehaulError::ControlFileCorrupted(message) => AppError::Download(message),
        BytehaulError::RetryBudgetExceeded { elapsed, limit } => AppError::Download(format!(
            "重试预算耗尽: 已重试 {:?}，限制 {:?}",
            elapsed, limit
        )),
        BytehaulError::TaskFailed(message) | BytehaulError::Internal(message) => {
            AppError::Download(message)
        }
        BytehaulError::ChannelClosed => AppError::Download("下载通道已关闭".to_string()),
        BytehaulError::ChecksumMismatch { expected, actual } => AppError::Download(format!(
            "校验失败: expected={}, actual={}",
            expected, actual
        )),
    }
}

fn parse_size_to_bytes(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let split_index = trimmed
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(trimmed.len());
    let (digits, suffix) = trimmed.split_at(split_index);
    let base = digits.parse::<u64>().ok()?;
    let multiplier = match suffix.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" => 1024,
        "m" | "mb" => 1024 * 1024,
        "g" | "gb" => 1024 * 1024 * 1024,
        "t" | "tb" => 1024_u64.pow(4),
        _ => return None,
    };

    Some(base.saturating_mul(multiplier))
}

fn guess_filename_from_url(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .path_segments()
                .and_then(|segments| segments.filter(|segment| !segment.is_empty()).next_back())
                .map(ToOwned::to_owned)
        })
        .filter(|filename| !filename.is_empty())
        .unwrap_or_else(|| "download".to_string())
}

fn control_file_path(output_path: &Path) -> PathBuf {
    let mut value = output_path.as_os_str().to_os_string();
    value.push(".bytehaul");
    PathBuf::from(value)
}

fn control_temp_file_path(output_path: &Path) -> PathBuf {
    let mut value: OsString = control_file_path(output_path).into_os_string();
    value.push(".tmp");
    PathBuf::from(value)
}

fn resolve_download_url(url: &str, options: &DownloadOptions) -> String {
    if options.use_github_mirror && url.contains("github.com") {
        get_github_download_url(url)
    } else {
        get_final_url(url)
    }
}

fn resolve_output_path(download_url: &str, save_dir: &Path, options: &DownloadOptions) -> PathBuf {
    let filename = options
        .filename
        .clone()
        .unwrap_or_else(|| guess_filename_from_url(download_url));

    save_dir.join(filename)
}

fn build_download_spec(url: &str, options: &DownloadOptions) -> AppResult<(DownloadSpec, PathBuf)> {
    let download_url = resolve_download_url(url, options);
    let output_dir = match options.save_dir.clone() {
        Some(save_dir) => save_dir,
        None => default_download_dir()?,
    };
    let output_path = resolve_output_path(&download_url, &output_dir, options);
    let max_connections = options.split.max(options.max_connection_per_server).max(1);
    let min_split_size = parse_size_to_bytes(&options.min_split_size).unwrap_or(4 * 1024 * 1024);

    let mut headers = options.headers.clone();
    headers.entry("User-Agent".to_string()).or_insert_with(|| {
        options
            .user_agent
            .clone()
            .unwrap_or_else(|| user_agent().to_string())
    });

    let mut spec = DownloadSpec::new(download_url)
        .output_dir(output_dir)
        .headers(headers)
        .max_connections(max_connections)
        .min_split_size(min_split_size)
        .connect_timeout(options.connect_timeout)
        .read_timeout(options.read_timeout)
        .resume(true)
        .file_allocation(FileAllocation::None);

    if let Some(filename) = options.filename.clone() {
        spec = spec.output_path(filename);
    }

    Ok((spec, output_path))
}

fn convert_status(state: BytehaulState) -> DownloadStatus {
    match state {
        BytehaulState::Pending => DownloadStatus::Waiting,
        BytehaulState::Downloading => DownloadStatus::Active,
        BytehaulState::Completed => DownloadStatus::Complete,
        BytehaulState::Failed => DownloadStatus::Error,
        BytehaulState::Cancelled => DownloadStatus::Removed,
        BytehaulState::Paused => DownloadStatus::Paused,
    }
}

fn convert_snapshot(
    snapshot: bytehaul::ProgressSnapshot,
    task_id: &str,
    filename: &str,
) -> DownloadProgress {
    let total = snapshot.total_size.unwrap_or(0);
    let percentage = if total > 0 {
        (snapshot.downloaded as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    let eta = snapshot
        .eta_secs
        .map(|seconds| seconds.max(0.0).ceil() as u64)
        .unwrap_or(u64::MAX);

    DownloadProgress {
        gid: task_id.to_string(),
        downloaded: snapshot.downloaded,
        total,
        speed: snapshot.speed_bytes_per_sec.max(0.0).round() as u64,
        percentage,
        eta,
        filename: filename.to_string(),
        status: convert_status(snapshot.state),
    }
}

struct BytehaulTask {
    id: String,
    downloader: Arc<Downloader>,
    spec: DownloadSpec,
    output_path: PathBuf,
    progress: RwLock<DownloadProgress>,
    handle: Mutex<Option<DownloadHandle>>,
    final_result: Mutex<Option<AppResult<DownloadResult>>>,
    completion_notify: Notify,
    overwrite_on_first_start: bool,
    started_once: AtomicBool,
}

impl BytehaulTask {
    fn new(
        id: String,
        downloader: Arc<Downloader>,
        spec: DownloadSpec,
        output_path: PathBuf,
        overwrite: bool,
    ) -> Self {
        let filename = output_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("download")
            .to_string();

        Self {
            id: id.clone(),
            downloader,
            spec,
            output_path,
            progress: RwLock::new(DownloadProgress::new(&id, &filename)),
            handle: Mutex::new(None),
            final_result: Mutex::new(None),
            completion_notify: Notify::new(),
            overwrite_on_first_start: overwrite,
            started_once: AtomicBool::new(false),
        }
    }

    async fn start(
        self: &Arc<Self>,
        active_tasks: Arc<RwLock<HashMap<String, Arc<BytehaulTask>>>>,
    ) -> AppResult<()> {
        let is_first_start = !self.started_once.swap(true, Ordering::SeqCst);
        if is_first_start && self.overwrite_on_first_start {
            self.remove_download_files().await?;
        }

        self.final_result.lock().take();
        let handle = self.downloader.download(self.spec.clone());

        let weak_task = Arc::downgrade(self);
        let weak_tasks = Arc::downgrade(&active_tasks);
        handle.on_progress(move |snapshot| {
            if let Some(task) = weak_task.upgrade() {
                task.update_progress(snapshot.clone());

                match snapshot.state {
                    BytehaulState::Paused => {
                        task.handle.lock().take();
                    }
                    BytehaulState::Completed | BytehaulState::Failed | BytehaulState::Cancelled => {
                        task.handle.lock().take();
                        task.finish_terminal_state(snapshot.state);

                        if let Some(tasks) = Weak::upgrade(&weak_tasks) {
                            tasks.write().remove(&task.id);
                        }
                    }
                    BytehaulState::Pending | BytehaulState::Downloading => {}
                }
            }
        });

        self.update_progress(handle.progress());
        *self.handle.lock() = Some(handle);
        Ok(())
    }

    fn update_progress(&self, snapshot: bytehaul::ProgressSnapshot) {
        let filename = self
            .output_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("download")
            .to_string();
        *self.progress.write() = convert_snapshot(snapshot, &self.id, &filename);
    }

    fn finish_terminal_state(&self, state: BytehaulState) {
        let result = match state {
            BytehaulState::Completed => {
                let metadata = std::fs::metadata(&self.output_path).map_err(AppError::Io);
                metadata.map(|file| DownloadResult {
                    path: self.output_path.clone(),
                    filename: self
                        .output_path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("download")
                        .to_string(),
                    size: file.len(),
                    gid: self.id.clone(),
                })
            }
            BytehaulState::Failed => Err(AppError::Download(format!(
                "bytehaul 下载失败: {}",
                self.output_path.display()
            ))),
            BytehaulState::Cancelled => Err(AppError::Download("下载已取消".to_string())),
            BytehaulState::Paused | BytehaulState::Pending | BytehaulState::Downloading => {
                return;
            }
        };

        let mut final_result = self.final_result.lock();
        if final_result.is_none() {
            *final_result = Some(result);
            self.completion_notify.notify_waiters();
        }
    }

    fn request_pause(&self) -> AppResult<()> {
        if let Some(handle) = self.handle.lock().as_ref() {
            handle.pause();
            Ok(())
        } else {
            Err(AppError::Download(format!(
                "任务不存在或不可暂停: {}",
                self.id
            )))
        }
    }

    fn request_cancel(&self) {
        if let Some(handle) = self.handle.lock().take() {
            handle.cancel();
        }

        let mut progress = self.progress.write();
        progress.status = DownloadStatus::Removed;
        progress.speed = 0;
        progress.eta = u64::MAX;
        drop(progress);

        self.finish_terminal_state(BytehaulState::Cancelled);
    }

    async fn wait_for_result(&self) -> AppResult<DownloadResult> {
        loop {
            if let Some(result) = self.final_result.lock().take() {
                return result;
            }
            self.completion_notify.notified().await;
        }
    }

    async fn remove_download_files(&self) -> AppResult<()> {
        for path in [
            self.output_path.clone(),
            control_file_path(&self.output_path),
            control_temp_file_path(&self.output_path),
        ] {
            match tokio::fs::remove_file(&path).await {
                Ok(()) => debug!("已删除 bytehaul 下载文件: {}", path.display()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    warn!("删除 bytehaul 下载文件失败: {} ({})", path.display(), error);
                }
            }
        }

        Ok(())
    }
}

/// bytehaul 后端实现。
pub struct BytehaulBackend {
    started: AtomicBool,
    downloader: RwLock<Option<Arc<Downloader>>>,
    active_tasks: Arc<RwLock<HashMap<String, Arc<BytehaulTask>>>>,
}

impl BytehaulBackend {
    /// 使用给定 downloader 创建后端。
    pub fn new(downloader: Arc<Downloader>) -> Self {
        Self {
            started: AtomicBool::new(false),
            downloader: RwLock::new(Some(downloader)),
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 从当前应用配置创建 bytehaul 后端。
    pub fn from_config() -> AppResult<Self> {
        Ok(Self::new(Arc::new(build_downloader_from_config()?)))
    }

    fn get_downloader(&self) -> AppResult<Arc<Downloader>> {
        if let Some(downloader) = self.downloader.read().clone() {
            return Ok(downloader);
        }

        let downloader = Arc::new(build_downloader_from_config()?);
        *self.downloader.write() = Some(downloader.clone());
        Ok(downloader)
    }

    async fn build_task(
        &self,
        url: &str,
        options: DownloadOptions,
    ) -> AppResult<Arc<BytehaulTask>> {
        let options = options.with_adaptive_parallelism();
        let downloader = self.get_downloader()?;
        let task_id = next_task_id();
        let (spec, output_path) = build_download_spec(url, &options)?;
        spec.validate().map_err(map_bytehaul_error)?;

        Ok(Arc::new(BytehaulTask::new(
            task_id,
            downloader,
            spec,
            output_path,
            options.overwrite,
        )))
    }
}

#[async_trait]
impl DownloadManager for BytehaulBackend {
    async fn start(&self) -> AppResult<()> {
        if self.started.load(Ordering::SeqCst) {
            return Ok(());
        }

        let _ = self.get_downloader()?;
        self.started.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        if !self.started.load(Ordering::SeqCst) {
            return Ok(());
        }

        let tasks: Vec<Arc<BytehaulTask>> = self.active_tasks.read().values().cloned().collect();
        for task in tasks {
            task.request_cancel();
        }

        self.active_tasks.write().clear();
        self.started.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn download(&self, url: &str, options: DownloadOptions) -> AppResult<String> {
        if !self.started.load(Ordering::SeqCst) {
            self.start().await?;
        }

        let task = self.build_task(url, options).await?;
        let task_id = task.id.clone();
        self.active_tasks
            .write()
            .insert(task_id.clone(), task.clone());
        task.start(self.active_tasks.clone()).await?;
        Ok(task_id)
    }

    async fn download_and_wait(
        &self,
        url: &str,
        options: DownloadOptions,
        on_progress: ProgressCallback,
    ) -> AppResult<DownloadResult> {
        if !self.started.load(Ordering::SeqCst) {
            self.start().await?;
        }

        let total_timeout = options.total_timeout;
        let task = self.build_task(url, options).await?;
        let task_id = task.id.clone();
        self.active_tasks
            .write()
            .insert(task_id.clone(), task.clone());
        task.start(self.active_tasks.clone()).await?;

        let progress_task = task.clone();
        let progress_loop = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
            loop {
                interval.tick().await;
                let progress = progress_task.progress.read().clone();
                on_progress(progress.clone());

                if matches!(
                    progress.status,
                    DownloadStatus::Complete | DownloadStatus::Error | DownloadStatus::Removed
                ) {
                    break;
                }
            }
        });

        let result = match tokio::time::timeout(total_timeout, task.wait_for_result()).await {
            Ok(result) => result,
            Err(_) => {
                task.request_cancel();
                Err(AppError::Download(format!(
                    "下载超时，已超过 {} 秒",
                    total_timeout.as_secs()
                )))
            }
        };
        let _ = progress_loop.await;
        self.active_tasks.write().remove(&task_id);
        result
    }

    async fn pause(&self, task_id: &str) -> AppResult<()> {
        let task = self.active_tasks.read().get(task_id).cloned();
        match task {
            Some(task) => task.request_pause(),
            None => Err(AppError::Download(format!("任务不存在: {}", task_id))),
        }
    }

    async fn resume(&self, task_id: &str) -> AppResult<()> {
        let task = self.active_tasks.read().get(task_id).cloned();
        match task {
            Some(task) => {
                if task.progress.read().status != DownloadStatus::Paused {
                    return Err(AppError::Download(format!(
                        "任务未处于暂停状态: {}",
                        task_id
                    )));
                }

                task.start(self.active_tasks.clone()).await
            }
            None => Err(AppError::Download(format!("任务不存在: {}", task_id))),
        }
    }

    async fn cancel(&self, task_id: &str) -> AppResult<()> {
        let task = self.active_tasks.read().get(task_id).cloned();
        match task {
            Some(task) => {
                task.request_cancel();
                Ok(())
            }
            None => Err(AppError::Download(format!("任务不存在: {}", task_id))),
        }
    }

    async fn cancel_all(&self, remove_files: bool) -> AppResult<Option<String>> {
        let tasks: Vec<Arc<BytehaulTask>> = self.active_tasks.read().values().cloned().collect();
        let mut first_path = None;

        for task in &tasks {
            if first_path.is_none() {
                first_path = Some(task.output_path.display().to_string());
            }
            task.request_cancel();
        }

        if remove_files && !tasks.is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            for task in &tasks {
                task.remove_download_files().await?;
            }
        }

        Ok(first_path)
    }

    async fn get_download_progress(&self, task_id: &str) -> AppResult<DownloadProgress> {
        let task = self.active_tasks.read().get(task_id).cloned();
        match task {
            Some(task) => Ok(task.progress.read().clone()),
            None => Err(AppError::Download(format!("任务不存在: {}", task_id))),
        }
    }

    fn is_started(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_size_to_bytes() {
        assert_eq!(parse_size_to_bytes("4M"), Some(4 * 1024 * 1024));
        assert_eq!(parse_size_to_bytes("12mb"), Some(12 * 1024 * 1024));
        assert_eq!(parse_size_to_bytes("1G"), Some(1024 * 1024 * 1024));
        assert_eq!(parse_size_to_bytes("512"), Some(512));
        assert_eq!(parse_size_to_bytes("bad"), None);
    }

    #[test]
    fn test_convert_status() {
        assert_eq!(
            convert_status(BytehaulState::Pending),
            DownloadStatus::Waiting
        );
        assert_eq!(
            convert_status(BytehaulState::Downloading),
            DownloadStatus::Active
        );
        assert_eq!(
            convert_status(BytehaulState::Completed),
            DownloadStatus::Complete
        );
        assert_eq!(convert_status(BytehaulState::Failed), DownloadStatus::Error);
        assert_eq!(
            convert_status(BytehaulState::Cancelled),
            DownloadStatus::Removed
        );
        assert_eq!(
            convert_status(BytehaulState::Paused),
            DownloadStatus::Paused
        );
    }

    #[test]
    fn test_control_file_path_derivation() {
        let output = PathBuf::from("artifact.zip");
        assert_eq!(
            control_file_path(&output),
            PathBuf::from("artifact.zip.bytehaul")
        );
        assert_eq!(
            control_temp_file_path(&output),
            PathBuf::from("artifact.zip.bytehaul.tmp")
        );
    }

    #[test]
    fn test_build_download_spec_uses_explicit_filename() {
        let options = DownloadOptions {
            save_dir: Some(PathBuf::from("C:/tmp")),
            filename: Some("archive.zip".to_string()),
            overwrite: true,
            use_github_mirror: false,
            split: 8,
            max_connection_per_server: 4,
            min_split_size: "8M".to_string(),
            user_agent: Some("TestUA/1.0".to_string()),
            headers: HashMap::new(),
            ..DownloadOptions::default()
        };

        let (spec, output_path) =
            build_download_spec("https://example.com/file.zip", &options).unwrap();

        assert_eq!(output_path, PathBuf::from("C:/tmp").join("archive.zip"));
        assert_eq!(spec.get_output_dir(), Some(Path::new("C:/tmp")));
        assert_eq!(spec.get_output_path(), Some(Path::new("archive.zip")));
        assert_eq!(spec.get_max_connections(), 8);
        assert_eq!(spec.get_min_split_size(), 8 * 1024 * 1024);
        assert_eq!(spec.get_connect_timeout(), options.connect_timeout);
        assert_eq!(spec.get_read_timeout(), options.read_timeout);
        assert_eq!(
            spec.get_headers().get("User-Agent").map(String::as_str),
            Some("TestUA/1.0")
        );
    }

    #[test]
    fn test_build_download_spec_uses_managed_download_dir_by_default() {
        let options = DownloadOptions {
            filename: Some("artifact.bin".to_string()),
            use_github_mirror: false,
            ..DownloadOptions::default()
        };

        let (spec, output_path) =
            build_download_spec("https://example.com/file.zip", &options).unwrap();
        let expected_dir = crate::services::downloader::aria2::get_default_download_dir().unwrap();

        assert_eq!(output_path, expected_dir.join("artifact.bin"));
        assert_eq!(spec.get_output_dir(), Some(expected_dir.as_path()));
        assert_eq!(spec.get_output_path(), Some(Path::new("artifact.bin")));
    }

    #[tokio::test]
    async fn test_remove_download_files_cleans_output_and_control_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let output_path = temp_dir.path().join("artifact.bin");
        let control_path = control_file_path(&output_path);
        let control_tmp_path = control_temp_file_path(&output_path);

        std::fs::write(&output_path, b"payload").unwrap();
        std::fs::write(&control_path, b"state").unwrap();
        std::fs::write(&control_tmp_path, b"state-tmp").unwrap();

        let downloader = Arc::new(Downloader::builder().enable_ipv6(false).build().unwrap());
        let spec =
            DownloadSpec::new("https://example.com/artifact.bin").output_path(output_path.clone());
        let task = BytehaulTask::new(
            "bytehaul-test".to_string(),
            downloader,
            spec,
            output_path.clone(),
            false,
        );

        task.remove_download_files().await.unwrap();

        assert!(!output_path.exists());
        assert!(!control_path.exists());
        assert!(!control_tmp_path.exists());
    }
}

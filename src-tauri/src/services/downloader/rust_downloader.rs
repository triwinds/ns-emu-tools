//! 纯 Rust 下载器实现
//!
//! 实现 DownloadManager trait，提供断点续传、多连接下载等功能

use crate::error::{AppError, AppResult};
use crate::services::downloader::chunk_manager::{ChunkManager, ChunkProgress, RangeSupport};
use crate::services::downloader::client::build_download_client;
use crate::services::downloader::filename::{resolve_filename, resolve_filename_from_url};
use crate::services::downloader::manager::{DownloadManager, ProgressCallback};
use crate::services::downloader::retry_strategy::{ErrorCategory, RetryStrategy};
use crate::services::downloader::state_store::{
    check_disk_space, get_state_filename, get_temp_filename, ChunkState, DownloadState, StateStore,
};
use crate::services::downloader::types::{
    DownloadOptions, DownloadProgress, DownloadResult, DownloadStatus,
};
use crate::services::network::get_github_download_url;
use async_trait::async_trait;
use parking_lot::RwLock;
use reqwest::Client;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// 状态保存间隔
const STATE_SAVE_INTERVAL: Duration = Duration::from_secs(5);

/// 进度更新间隔（向前端发送进度的频率）
const PROGRESS_UPDATE_INTERVAL: Duration = Duration::from_millis(500);

/// 速度采样窗口大小（用于平滑速度计算）
const SPEED_WINDOW_SIZE: usize = 10;

/// 纯 Rust 下载器
pub struct RustDownloader {
    /// 是否已启动
    started: AtomicBool,
    /// 活跃的下载任务
    active_tasks: Arc<RwLock<HashMap<String, Arc<DownloadTask>>>>,
    /// HTTP 客户端
    client: RwLock<Option<Client>>,
}

impl RustDownloader {
    /// 创建新的下载器
    pub fn new() -> Self {
        Self {
            started: AtomicBool::new(false),
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
            client: RwLock::new(None),
        }
    }

    /// 获取或创建 HTTP 客户端
    fn get_client(&self) -> AppResult<Client> {
        let client = self.client.read();
        if let Some(c) = client.as_ref() {
            return Ok(c.clone());
        }
        drop(client);

        let new_client = build_download_client()?;
        let mut client = self.client.write();
        *client = Some(new_client.clone());
        Ok(new_client)
    }

    /// 生成任务 ID
    fn generate_task_id() -> String {
        uuid::Uuid::new_v4().to_string().replace("-", "")[..16].to_string()
    }
}

impl Default for RustDownloader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DownloadManager for RustDownloader {
    async fn start(&self) -> AppResult<()> {
        if self.started.load(Ordering::SeqCst) {
            return Ok(());
        }

        info!("启动 RustDownloader");

        // 预创建 HTTP 客户端
        let _ = self.get_client()?;

        self.started.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        if !self.started.load(Ordering::SeqCst) {
            return Ok(());
        }

        info!("停止 RustDownloader");

        // 取消所有任务
        let tasks: Vec<Arc<DownloadTask>> = {
            let tasks = self.active_tasks.read();
            tasks.values().cloned().collect()
        };

        for task in tasks {
            task.cancel();
        }

        self.started.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn download(&self, url: &str, options: DownloadOptions) -> AppResult<String> {
        if !self.started.load(Ordering::SeqCst) {
            self.start().await?;
        }

        let task_id = Self::generate_task_id();
        let client = self.get_client()?;

        // 处理 GitHub URL
        let final_url = if options.use_github_mirror && url.contains("github.com") {
            get_github_download_url(url)
        } else {
            url.to_string()
        };

        let task = Arc::new(DownloadTask::new(
            task_id.clone(),
            url.to_string(),  // original_url
            final_url,        // current url (可能是镜像)
            options,
            client,
        ));

        {
            let mut tasks = self.active_tasks.write();
            tasks.insert(task_id.clone(), task.clone());
        }

        // 异步启动下载
        let task_clone = task.clone();
        let task_id_clone = task_id.clone();
        let active_tasks = self.active_tasks.clone();

        tokio::spawn(async move {
            let result = task_clone.start().await;

            // 任务完成后从活跃列表移除
            {
                let mut tasks = active_tasks.write();
                tasks.remove(&task_id_clone);
            }

            if let Err(e) = result {
                error!("下载任务 {} 失败: {}", task_id_clone, e);
            }
        });

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

        let task_id = Self::generate_task_id();
        let client = self.get_client()?;

        // 处理 GitHub URL
        let final_url = if options.use_github_mirror && url.contains("github.com") {
            get_github_download_url(url)
        } else {
            url.to_string()
        };

        let task = Arc::new(DownloadTask::new(
            task_id.clone(),
            url.to_string(),  // original_url
            final_url,        // current url (可能是镜像)
            options,
            client,
        ));

        {
            let mut tasks = self.active_tasks.write();
            tasks.insert(task_id.clone(), task.clone());
        }

        // 启动进度轮询
        let task_clone = task.clone();
        let progress_handle = tokio::spawn(async move {
            let mut interval = interval(PROGRESS_UPDATE_INTERVAL);
            loop {
                interval.tick().await;

                let progress = task_clone.get_progress();
                on_progress(progress.clone());

                if matches!(
                    progress.status,
                    DownloadStatus::Complete | DownloadStatus::Error | DownloadStatus::Removed
                ) {
                    break;
                }
            }
        });

        // 执行下载
        let result = task.start().await;

        // 等待进度轮询结束
        let _ = progress_handle.await;

        // 从活跃列表移除
        {
            let mut tasks = self.active_tasks.write();
            tasks.remove(&task_id);
        }

        result
    }

    async fn pause(&self, task_id: &str) -> AppResult<()> {
        let task = {
            let tasks = self.active_tasks.read();
            tasks.get(task_id).cloned()
        };

        if let Some(task) = task {
            task.pause();
            Ok(())
        } else {
            Err(AppError::Download(format!("任务不存在: {}", task_id)))
        }
    }

    async fn resume(&self, task_id: &str) -> AppResult<()> {
        let task = {
            let tasks = self.active_tasks.read();
            tasks.get(task_id).cloned()
        };

        if let Some(task) = task {
            task.resume();
            Ok(())
        } else {
            Err(AppError::Download(format!("任务不存在: {}", task_id)))
        }
    }

    async fn cancel(&self, task_id: &str) -> AppResult<()> {
        let task = {
            let tasks = self.active_tasks.read();
            tasks.get(task_id).cloned()
        };

        if let Some(task) = task {
            task.cancel();
            Ok(())
        } else {
            Err(AppError::Download(format!("任务不存在: {}", task_id)))
        }
    }

    async fn cancel_all(&self, remove_files: bool) -> AppResult<Option<String>> {
        let tasks: Vec<Arc<DownloadTask>> = {
            let tasks = self.active_tasks.read();
            tasks.values().cloned().collect()
        };

        let mut first_path = None;

        // 第一步：取消所有任务
        for task in &tasks {
            task.cancel();
        }

        // 第二步：如果需要删除文件，等待状态保存任务退出
        // 由于我们在状态保存任务中添加了双重检查（tokio::select! + 保存前检查），
        // 给 100ms 的缓冲时间应该足够让任务检测到取消信号并退出
        if remove_files && !tasks.is_empty() {
            debug!("等待状态保存任务退出...");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // 第三步：删除文件
        for task in tasks {
            if remove_files {
                let progress = task.get_progress();
                let save_dir = task.options.save_dir.clone().unwrap_or_else(|| {
                    dirs::download_dir().unwrap_or_else(|| PathBuf::from("."))
                });

                // 删除临时文件
                let temp_path = save_dir.join(get_temp_filename(&progress.filename));
                if temp_path.exists() {
                    match fs::remove_file(&temp_path).await {
                        Ok(_) => debug!("已删除临时文件: {:?}", temp_path),
                        Err(e) => warn!("删除临时文件失败: {:?}, 错误: {}", temp_path, e),
                    }
                }

                // 删除状态文件
                let state_path = save_dir.join(get_state_filename(&progress.filename));
                if state_path.exists() {
                    match fs::remove_file(&state_path).await {
                        Ok(_) => debug!("已删除状态文件: {:?}", state_path),
                        Err(e) => warn!("删除状态文件失败: {:?}, 错误: {}", state_path, e),
                    }
                }

                // 删除可能存在的临时状态文件（.download.tmp）
                let temp_state_path = state_path.with_extension("download.tmp");
                if temp_state_path.exists() {
                    match fs::remove_file(&temp_state_path).await {
                        Ok(_) => debug!("已删除临时状态文件: {:?}", temp_state_path),
                        Err(e) => warn!("删除临时状态文件失败: {:?}, 错误: {}", temp_state_path, e),
                    }
                }

                if first_path.is_none() {
                    first_path = Some(temp_path.to_string_lossy().to_string());
                }
            }
        }

        // 清空任务列表
        {
            let mut tasks = self.active_tasks.write();
            tasks.clear();
        }

        Ok(first_path)
    }

    async fn get_download_progress(&self, task_id: &str) -> AppResult<DownloadProgress> {
        let task = {
            let tasks = self.active_tasks.read();
            tasks.get(task_id).cloned()
        };

        if let Some(task) = task {
            Ok(task.get_progress())
        } else {
            Err(AppError::Download(format!("任务不存在: {}", task_id)))
        }
    }

    fn is_started(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }
}

/// 下载任务
struct DownloadTask {
    /// 任务 ID
    id: String,
    /// 原始下载 URL（用于镜像切换）
    original_url: String,
    /// 当前下载 URL（可能是镜像 URL）
    url: RwLock<String>,
    /// 下载选项
    options: DownloadOptions,
    /// HTTP 客户端
    client: Client,
    /// 下载状态
    status: Arc<RwLock<DownloadStatus>>,
    /// 进度信息
    progress: Arc<RwLock<ProgressInfo>>,
    /// 取消令牌
    cancel_token: CancellationToken,
    /// 是否暂停
    paused: Arc<AtomicBool>,
    /// 重试策略
    retry_strategy: RwLock<RetryStrategy>,
}

/// 进度信息
struct ProgressInfo {
    /// 已下载字节数
    downloaded: u64,
    /// 总字节数
    total: u64,
    /// 下载速度（字节/秒）
    speed: u64,
    /// 文件名
    filename: String,
    /// 速度采样
    speed_samples: VecDeque<(Instant, u64)>,
    /// 上次计算的 ETA（用于平滑）
    last_eta: u64,
}

impl ProgressInfo {
    fn new(filename: &str) -> Self {
        Self {
            downloaded: 0,
            total: 0,
            speed: 0,
            filename: filename.to_string(),
            speed_samples: VecDeque::with_capacity(SPEED_WINDOW_SIZE + 1),
            last_eta: u64::MAX,
        }
    }

    /// 更新速度（滑动窗口平均）
    fn update_speed(&mut self, downloaded: u64) {
        let now = Instant::now();
        self.downloaded = downloaded;

        // 添加新采样点
        self.speed_samples.push_back((now, downloaded));

        // 移除过期采样点
        while self.speed_samples.len() > SPEED_WINDOW_SIZE {
            self.speed_samples.pop_front();
        }

        // 计算滑动窗口平均速度
        if self.speed_samples.len() >= 2 {
            let (first_time, first_bytes) = self.speed_samples.front().unwrap();
            let (last_time, last_bytes) = self.speed_samples.back().unwrap();
            let elapsed = last_time.duration_since(*first_time).as_secs_f64();

            if elapsed > 0.0 {
                self.speed = ((last_bytes - first_bytes) as f64 / elapsed) as u64;
            }
        }
    }

    /// 计算 ETA（使用指数移动平均平滑）
    fn calculate_eta(&mut self) -> u64 {
        if self.total == 0 || self.speed == 0 {
            return u64::MAX;
        }

        let remaining = self.total.saturating_sub(self.downloaded);
        let raw_eta = remaining / self.speed;

        // 使用指数移动平均平滑 ETA
        // alpha = 0.3 表示新值占 30%，旧值占 70%
        if self.last_eta == u64::MAX {
            self.last_eta = raw_eta;
        } else {
            // 避免 ETA 变化太剧烈，只有当新值和旧值差距不太大时才平滑
            let diff_ratio = if raw_eta > self.last_eta {
                raw_eta as f64 / self.last_eta as f64
            } else {
                self.last_eta as f64 / raw_eta as f64
            };

            // 如果差距太大（超过 2 倍），直接使用新值
            if diff_ratio > 2.0 {
                self.last_eta = raw_eta;
            } else {
                // 否则使用指数移动平均
                self.last_eta = ((raw_eta as f64 * 0.3) + (self.last_eta as f64 * 0.7)) as u64;
            }
        }

        self.last_eta
    }

    /// 计算百分比
    fn calculate_percentage(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }

        (self.downloaded as f64 / self.total as f64) * 100.0
    }
}

impl DownloadTask {
    fn new(id: String, original_url: String, url: String, options: DownloadOptions, client: Client) -> Self {
        let filename = resolve_filename_from_url(&url, options.filename.as_deref());

        Self {
            id,
            original_url,
            url: RwLock::new(url),
            options,
            client,
            status: Arc::new(RwLock::new(DownloadStatus::Waiting)),
            progress: Arc::new(RwLock::new(ProgressInfo::new(&filename))),
            cancel_token: CancellationToken::new(),
            paused: Arc::new(AtomicBool::new(false)),
            retry_strategy: RwLock::new(RetryStrategy::default()),
        }
    }

    /// 获取当前 URL
    fn get_url(&self) -> String {
        self.url.read().clone()
    }

    /// 尝试切换到镜像 URL（仅 GitHub）
    ///
    /// 返回 true 表示成功切换到新镜像
    fn try_switch_mirror(&self) -> bool {
        // 只有 GitHub URL 才尝试切换镜像
        if !self.original_url.contains("github.com") && !self.original_url.contains("githubusercontent.com") {
            return false;
        }

        // 重新计算镜像 URL
        let new_url = get_github_download_url(&self.original_url);
        let current_url = self.get_url();

        // 如果新 URL 与当前 URL 不同，则切换
        if new_url != current_url {
            info!("切换到镜像 URL: {} -> {}", current_url, new_url);
            *self.url.write() = new_url;
            true
        } else {
            debug!("镜像 URL 未变化，保持当前 URL");
            false
        }
    }

    /// 获取当前进度
    fn get_progress(&self) -> DownloadProgress {
        let status = *self.status.read();
        let mut progress = self.progress.write();

        let eta = progress.calculate_eta();
        let percentage = progress.calculate_percentage();

        DownloadProgress {
            gid: self.id.clone(),
            downloaded: progress.downloaded,
            total: progress.total,
            speed: progress.speed,
            percentage,
            eta,
            filename: progress.filename.clone(),
            status,
        }
    }

    /// 暂停下载
    fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
        *self.status.write() = DownloadStatus::Paused;

        // 暂停时速度和 ETA 归零
        let mut progress = self.progress.write();
        progress.speed = 0;
        progress.speed_samples.clear();
        progress.last_eta = u64::MAX;
    }

    /// 恢复下载
    fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
        *self.status.write() = DownloadStatus::Active;
    }

    /// 取消下载
    fn cancel(&self) {
        self.cancel_token.cancel();
        *self.status.write() = DownloadStatus::Removed;
    }

    /// 开始下载
    async fn start(&self) -> AppResult<DownloadResult> {
        // 任何阶段的错误都必须把状态切到 Error，否则前端轮询可能无法结束，
        // 也就无法在 progress dialog 中展示错误。
        let result = self.start_impl().await;

        if result.is_err() {
            *self.status.write() = DownloadStatus::Error;

            // 错误时速度和 ETA 归零，避免 UI 继续显示旧数据
            let mut progress = self.progress.write();
            progress.speed = 0;
            progress.speed_samples.clear();
            progress.last_eta = u64::MAX;
        }

        result
    }

    async fn start_impl(&self) -> AppResult<DownloadResult> {
        let current_url = self.get_url();
        info!("开始下载: {} -> {}", current_url, self.progress.read().filename);

        *self.status.write() = DownloadStatus::Active;

        // 确定保存目录
        let save_dir = self.options.save_dir.clone().unwrap_or_else(|| {
            dirs::download_dir().unwrap_or_else(|| PathBuf::from("."))
        });

        // 确保目录存在
        if !save_dir.exists() {
            fs::create_dir_all(&save_dir).await?;
        }

        // 带重试的初始化阶段
        debug!("初始化下载信息: url={}", current_url);
        let (range_support, filename) = self.init_with_retry(&save_dir).await?;
        debug!(
            "初始化完成: filename={}, supports_range={}, total_size={}, etag={:?}, last_modified={:?}",
            filename,
            range_support.supports_range,
            range_support.total_size,
            range_support.etag,
            range_support.last_modified
        );

        // 更新进度信息
        {
            let mut progress = self.progress.write();
            progress.total = range_support.total_size;
            progress.filename = filename.clone();
        }

        // 检查磁盘空间
        if range_support.total_size > 0 {
            check_disk_space(&save_dir, range_support.total_size)?;
        }

        // 状态存储
        let state_store = StateStore::new(save_dir.clone());
        let current_url = self.get_url();

        // 尝试加载已有状态
        debug!("尝试加载状态文件: {}", filename);
        let mut state = if let Some(existing_state) = state_store.load(&filename).await? {
            // 验证一致性
            if existing_state.validate_consistency(
                &current_url,
                None,
                range_support.total_size,
                range_support.etag.as_deref(),
                range_support.last_modified.as_deref(),
            ) {
                info!("恢复下载: {}", filename);
                existing_state
            } else {
                warn!("状态文件不一致，重新下载");
                state_store.cleanup(&filename).await?;
                self.create_new_state(&filename, &save_dir, &range_support)
            }
        } else {
            debug!("未找到状态文件，创建新任务状态: {}", filename);
            self.create_new_state(&filename, &save_dir, &range_support)
        };

        // 更新 PID
        state.pid = Some(std::process::id());
        state.etag = range_support.etag.clone();
        state.last_modified = range_support.last_modified.clone();

        // 计算分块
        let chunk_manager = ChunkManager::new(
            self.options.split,
            &self.options.min_split_size,
        );

        if state.chunks.is_empty() {
            state.chunks = chunk_manager.calculate_chunks(
                range_support.total_size,
                range_support.supports_range,
            );
        }

        debug!(
            "分块信息: supports_range={}, chunks={}",
            range_support.supports_range,
            state.chunks.len()
        );

        // 保存初始状态
        state_store.save(&state).await?;

        debug!(
            "状态初始化完成，准备开始下载: {} (supports_range={}, chunks={})",
            filename,
            range_support.supports_range,
            state.chunks.len()
        );

        // 临时文件路径
        let temp_path = state.temp_file_path();
        let final_path = state.final_file_path();

        debug!(
            "路径信息: temp_path={}, final_path={}",
            temp_path.display(),
            final_path.display()
        );

        // 检查目标文件是否存在
        let t_exists = Instant::now();
        debug!(
            "检查目标文件是否存在: path={}, overwrite={}",
            final_path.display(),
            self.options.overwrite
        );

        let final_exists = fs::try_exists(&final_path)
            .await
            .map_err(AppError::Io)?;
        debug!(
            "目标文件存在检查完成: exists={}, cost_ms={}",
            final_exists,
            t_exists.elapsed().as_millis()
        );

        if final_exists && !self.options.overwrite {
            info!("目标文件已存在，直接使用: {}", final_path.display());

            // 清理临时文件和状态文件
            if temp_path.exists() {
                let _ = fs::remove_file(&temp_path).await;
            }
            let state_path = save_dir.join(get_state_filename(&filename));
            if state_path.exists() {
                let _ = fs::remove_file(&state_path).await;
            }

            // 设置状态为完成
            *self.status.write() = DownloadStatus::Complete;

            // 获取文件大小
            let file_size = fs::metadata(&final_path).await?.len();

            // 更新进度信息
            {
                let mut progress = self.progress.write();
                progress.downloaded = file_size;
                progress.total = file_size;
            }

            return Ok(DownloadResult {
                path: final_path,
                filename,
                size: file_size,
                gid: self.id.clone(),
            });
        }

        // 预分配文件空间
        let t_temp_exists = Instant::now();
        debug!("检查临时文件是否存在: path={}", temp_path.display());
        let temp_exists = fs::try_exists(&temp_path)
            .await
            .map_err(AppError::Io)?;
        debug!(
            "临时文件存在检查完成: exists={}, cost_ms={}",
            temp_exists,
            t_temp_exists.elapsed().as_millis()
        );

        if range_support.total_size > 0 && !temp_exists {
            debug!(
                "预分配临时文件: path={}, size={} bytes",
                temp_path.display(),
                range_support.total_size
            );
            let file = fs::File::create(&temp_path).await?;
            file.set_len(range_support.total_size).await?;
            debug!("预分配完成: path={}", temp_path.display());
        }

        // 启动状态保存任务
        debug!("准备启动状态定时保存任务");
        let state_store_clone = StateStore::new(save_dir.clone());
        let state_clone = Arc::new(RwLock::new(state.clone()));
        let cancel_token_clone = self.cancel_token.clone();

        let state_save_handle = tokio::spawn({
            let state_clone = state_clone.clone();
            async move {
                let mut interval = interval(STATE_SAVE_INTERVAL);
                debug!("状态定时保存任务启动");
                loop {
                    tokio::select! {
                        _ = cancel_token_clone.cancelled() => {
                            debug!("检测到取消信号，状态定时保存任务退出");
                            break;
                        }
                        _ = interval.tick() => {
                            // 在保存前再次检查是否被取消，避免竞态条件
                            if cancel_token_clone.is_cancelled() {
                                debug!("保存前检测到取消信号，跳过保存");
                                break;
                            }

                            let state = state_clone.read().clone();
                            if let Err(e) = state_store_clone.save(&state).await {
                                warn!("保存状态失败: {}", e);
                            }
                        }
                    }
                }
                debug!("状态定时保存任务退出");
            }
        });

        // 带重试的下载阶段
        debug!("准备进入下载阶段（download_with_retry）");
        debug!(
            "开始执行下载阶段: supports_range={}, chunks={}",
            range_support.supports_range,
            state_clone.read().chunks.len()
        );
        let download_result = self.download_with_retry(
            &state_clone,
            &temp_path,
            range_support.supports_range,
        ).await;

        // 停止状态保存任务
        debug!("停止状态定时保存任务");
        state_save_handle.abort();

        // 处理下载结果
        match download_result {
            Ok(()) => {
                // 重命名临时文件
                if temp_path.exists() {
                    if final_path.exists() && self.options.overwrite {
                        fs::remove_file(&final_path).await?;
                    }
                    fs::rename(&temp_path, &final_path).await?;
                }

                // 删除状态文件
                state_store.delete(&filename).await?;

                *self.status.write() = DownloadStatus::Complete;

                let file_size = if final_path.exists() {
                    fs::metadata(&final_path).await?.len()
                } else {
                    range_support.total_size
                };

                info!("下载完成: {} ({} 字节)", filename, file_size);

                Ok(DownloadResult {
                    path: final_path,
                    filename,
                    size: file_size,
                    gid: self.id.clone(),
                })
            }
            Err(e) => {
                // 保存当前状态以便恢复
                let state = state_clone.read().clone();
                let _ = state_store.save(&state).await;

                error!("下载失败: {}", e);
                Err(e)
            }
        }
    }

    /// 带重试的初始化阶段（Range 探测和文件名解析）
    async fn init_with_retry(&self, _save_dir: &PathBuf) -> AppResult<(RangeSupport, String)> {
        loop {
            let current_url = self.get_url();

            // 检测 Range 支持
            let range_result = ChunkManager::check_range_support(&self.client, &current_url).await;

            match range_result {
                Ok(range_support) => {
                    // 获取文件名
                    let filename = if self.options.filename.is_none() {
                        let head_result = self.client
                            .head(&current_url)
                            .send()
                            .await;

                        match head_result {
                            Ok(response) => {
                                resolve_filename(&response, &current_url, self.options.filename.as_deref())
                            }
                            Err(e) => {
                                let error = AppError::Network(format!("HEAD 请求失败: {}", e));
                                if !self.handle_retry_error(&error).await {
                                    return Err(error);
                                }
                                continue;
                            }
                        }
                    } else {
                        self.options.filename.clone().unwrap()
                    };

                    return Ok((range_support, filename));
                }
                Err(e) => {
                    if !self.handle_retry_error(&e).await {
                        return Err(e);
                    }
                    // 继续重试
                }
            }
        }
    }

    /// 带重试的下载阶段
    async fn download_with_retry(
        &self,
        state: &Arc<RwLock<DownloadState>>,
        temp_path: &PathBuf,
        supports_range: bool,
    ) -> AppResult<()> {
        loop {
            // 创建进度通道
            let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<ChunkProgress>();

            // 启动进度聚合任务
            let progress_clone = self.progress.clone();
            let state_for_progress = state.clone();

            let progress_handle = tokio::spawn(async move {
                while let Some(chunk_progress) = progress_rx.recv().await {
                    // 更新分块状态
                    {
                        let mut state = state_for_progress.write();
                        if let Some(chunk) = state.chunks.get_mut(chunk_progress.index) {
                            chunk.downloaded = chunk_progress.downloaded;
                            chunk.completed = chunk_progress.completed;
                        }
                    }

                    // 更新总进度
                    let total_downloaded: u64 = {
                        let state = state_for_progress.read();
                        state.chunks.iter().map(|c| c.downloaded).sum()
                    };

                    let mut progress = progress_clone.write();
                    progress.update_speed(total_downloaded);
                }
            });

            // 执行下载
            let chunk_count = state.read().chunks.len();
            let download_result = if supports_range && chunk_count > 1 {
                // 多连接下载
                self.download_multi_chunk(state, temp_path, progress_tx).await
            } else {
                // 单连接下载
                self.download_single_chunk(state, temp_path, progress_tx).await
            };

            // 等待进度任务结束（通道在下载函数返回后会被关闭）
            let _ = progress_handle.await;

            match download_result {
                Ok(()) => return Ok(()),
                Err(e) => {
                    // 检查是否是取消操作
                    if self.cancel_token.is_cancelled() {
                        return Err(e);
                    }

                    if !self.handle_retry_error(&e).await {
                        return Err(e);
                    }
                    // 继续重试
                    info!("重试下载...");
                }
            }
        }
    }

    /// 处理重试错误
    ///
    /// 返回 true 表示应该重试，false 表示不应重试
    async fn handle_retry_error(&self, error: &AppError) -> bool {
        use crate::services::downloader::retry_strategy::RetryAction;

        // 检查是否被取消
        if self.cancel_token.is_cancelled() {
            return false;
        }

        // 检查是否应该重试（在块中确保锁被释放）
        let should_check = {
            let retry_strategy = self.retry_strategy.read();
            retry_strategy.should_retry(error)
        };

        if !should_check {
            let category = RetryStrategy::categorize_error(error);
            warn!("错误不可重试: {:?}, 错误: {}", category, error);
            return false;
        }

        // 对于 GitHub URL，尝试切换镜像
        let category = RetryStrategy::categorize_error(error);
        if matches!(category, ErrorCategory::Temporary | ErrorCategory::RateLimited | ErrorCategory::DnsError) {
            if self.try_switch_mirror() {
                // 成功切换镜像，重置重试计数
                self.retry_strategy.write().reset();
            }
        }

        // 准备重试动作（不持有锁）
        let retry_action = {
            let retry_strategy = self.retry_strategy.read();
            retry_strategy.prepare_retry(error)
        };

        // 执行重试等待（异步操作，不持有锁）
        let should_retry = match retry_action {
            Some(RetryAction::WaitForNetwork { timeout, retry_num, max_retries }) => {
                warn!(
                    "网络不可用，等待网络恢复（重试 {}/{}）",
                    retry_num, max_retries
                );
                RetryStrategy::wait_for_network(timeout).await
            }
            Some(RetryAction::Sleep { duration, retry_num, max_retries, reason }) => {
                if reason.contains("限流") {
                    warn!(
                        "触发限流，等待 {:?} 后重试（重试 {}/{}）",
                        duration, retry_num, max_retries
                    );
                } else {
                    info!(
                        "{}，等待 {:?} 后重试（重试 {}/{}）",
                        reason, duration, retry_num, max_retries
                    );
                }
                tokio::time::sleep(duration).await;
                true
            }
            None => false,
        };

        // 如果应该重试，增加重试计数
        if should_retry {
            self.retry_strategy.write().increment_retry();
        }

        should_retry
    }

    /// 创建新的下载状态
    fn create_new_state(
        &self,
        filename: &str,
        save_dir: &PathBuf,
        range_support: &RangeSupport,
    ) -> DownloadState {
        DownloadState::new(
            &self.get_url(),
            filename,
            save_dir.clone(),
            range_support.total_size,
            range_support.supports_range,
        )
    }

    /// 多连接下载
    async fn download_multi_chunk(
        &self,
        state: &Arc<RwLock<DownloadState>>,
        temp_path: &PathBuf,
        progress_tx: mpsc::UnboundedSender<ChunkProgress>,
    ) -> AppResult<()> {
        let chunks: Vec<ChunkState> = {
            let state = state.read();
            state.chunks.clone()
        };

        let mut join_set: JoinSet<Result<(), AppError>> = JoinSet::new();
        let current_url = self.get_url();

        for chunk in chunks {
            if chunk.completed {
                continue;
            }

            let client = self.client.clone();
            let url = current_url.clone();
            let path = temp_path.clone();
            let tx = progress_tx.clone();
            let cancel_token = self.cancel_token.clone();
            let paused = self.paused.clone();

            join_set.spawn(async move {
                // 等待暂停解除
                while paused.load(Ordering::SeqCst) {
                    if cancel_token.is_cancelled() {
                        return Err(AppError::Download("下载被取消".to_string()));
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }

                ChunkManager::download_chunk(&client, &url, &chunk, &path, tx, cancel_token).await
            });
        }

        // 等待所有分块完成：按完成顺序处理，避免“先 await 慢分块导致整体卡住”
        while let Some(joined) = join_set.join_next().await {
            match joined {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    join_set.abort_all();
                    while join_set.join_next().await.is_some() {}
                    return Err(e);
                }
                Err(e) => {
                    join_set.abort_all();
                    while join_set.join_next().await.is_some() {}
                    return Err(AppError::Download(format!("分块任务失败: {}", e)));
                }
            }
        }

        Ok(())
    }

    /// 单连接下载
    async fn download_single_chunk(
        &self,
        state: &Arc<RwLock<DownloadState>>,
        temp_path: &PathBuf,
        progress_tx: mpsc::UnboundedSender<ChunkProgress>,
    ) -> AppResult<()> {
        let resume_from: u64 = {
            let state = state.read();
            state.chunks.first().map(|c| c.downloaded).unwrap_or(0)
        };

        // 等待暂停解除
        while self.paused.load(Ordering::SeqCst) {
            if self.cancel_token.is_cancelled() {
                return Err(AppError::Download("下载被取消".to_string()));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let current_url = self.get_url();
        ChunkManager::download_single(
            &self.client,
            &current_url,
            temp_path,
            progress_tx,
            self.cancel_token.clone(),
            resume_from,
        )
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_info_new() {
        let progress = ProgressInfo::new("test.zip");

        assert_eq!(progress.filename, "test.zip");
        assert_eq!(progress.downloaded, 0);
        assert_eq!(progress.total, 0);
        assert_eq!(progress.speed, 0);
    }

    #[test]
    fn test_progress_info_calculate_percentage() {
        let mut progress = ProgressInfo::new("test.zip");

        // 未知大小
        assert_eq!(progress.calculate_percentage(), 0.0);

        // 已知大小
        progress.total = 1000;
        progress.downloaded = 500;
        assert_eq!(progress.calculate_percentage(), 50.0);

        progress.downloaded = 1000;
        assert_eq!(progress.calculate_percentage(), 100.0);
    }

    #[test]
    fn test_progress_info_calculate_eta() {
        let mut progress = ProgressInfo::new("test.zip");

        // 未知大小或速度为 0
        assert_eq!(progress.calculate_eta(), u64::MAX);

        progress.total = 1000;
        assert_eq!(progress.calculate_eta(), u64::MAX);

        // 正常计算 - 第一次调用
        progress.speed = 100;
        progress.downloaded = 500;
        assert_eq!(progress.calculate_eta(), 5); // (1000 - 500) / 100 = 5

        // 第二次调用应该使用平滑值
        progress.downloaded = 600;
        let eta = progress.calculate_eta();
        // 平滑后的值应该在原始值 4 和上次值 5 之间
        assert!(eta >= 3 && eta <= 5);
    }

    #[test]
    fn test_rust_downloader_new() {
        let downloader = RustDownloader::new();

        assert!(!downloader.is_started());
        assert!(downloader.active_tasks.read().is_empty());
    }

    #[test]
    fn test_generate_task_id() {
        let id1 = RustDownloader::generate_task_id();
        let id2 = RustDownloader::generate_task_id();

        assert_eq!(id1.len(), 16);
        assert_eq!(id2.len(), 16);
        assert_ne!(id1, id2);
    }

    #[tokio::test]
    async fn test_rust_downloader_start_stop() {
        let downloader = RustDownloader::new();

        assert!(!downloader.is_started());

        downloader.start().await.unwrap();
        assert!(downloader.is_started());

        downloader.stop().await.unwrap();
        assert!(!downloader.is_started());
    }
}

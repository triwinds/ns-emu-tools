//! Aria2 下载管理器
//!
//! 通过 WebSocket JSON-RPC 与 aria2 通信，提供文件下载功能
//!
//! # 功能
//! - 启动/停止 aria2 守护进程
//! - 添加下载任务
//! - 暂停/恢复/取消下载
//! - 实时进度通知

use crate::config::get_config;
use crate::error::{AppError, AppResult};
use crate::services::network::{get_github_download_url, get_proxy_url, is_using_proxy, request_github_api, CHROME_UA};
use aria2_ws::response::GlobalStat as Aria2GlobalStat;
use aria2_ws::response::Status as Aria2Status;
use aria2_ws::{Client, TaskOptions};
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// aria2 RPC 密钥
const ARIA2_SECRET: &str = "ns-emu-tools-aria2";

/// 全局 Aria2Manager 实例
static ARIA2_MANAGER: OnceCell<Arc<Aria2Manager>> = OnceCell::new();

/// 获取全局 Aria2Manager 实例（如果未初始化则自动初始化）
pub async fn get_aria2_manager() -> AppResult<Arc<Aria2Manager>> {
    // 如果已经初始化，直接返回
    if let Some(manager) = ARIA2_MANAGER.get() {
        return Ok(manager.clone());
    }

    // 未初始化，自动初始化
    info!("Aria2Manager 未初始化，开始自动初始化");
    init_aria2_manager().await
}

/// 初始化全局 Aria2Manager
pub async fn init_aria2_manager() -> AppResult<Arc<Aria2Manager>> {
    if let Some(manager) = ARIA2_MANAGER.get() {
        return Ok(manager.clone());
    }

    let manager = Arc::new(Aria2Manager::new());
    manager.start().await?;

    ARIA2_MANAGER
        .set(manager.clone())
        .map_err(|_| AppError::Aria2("Aria2Manager 已经初始化".to_string()))?;

    Ok(manager)
}

/// 下载进度信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Aria2DownloadProgress {
    /// 下载 GID
    pub gid: String,
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
    /// 下载状态
    pub status: Aria2DownloadStatus,
}

impl Aria2DownloadProgress {
    pub fn new(gid: &str, filename: &str) -> Self {
        Self {
            gid: gid.to_string(),
            downloaded: 0,
            total: 0,
            speed: 0,
            percentage: 0.0,
            eta: 0,
            filename: filename.to_string(),
            status: Aria2DownloadStatus::Waiting,
        }
    }

    /// 格式化已下载大小
    pub fn downloaded_string(&self) -> String {
        format_bytes(self.downloaded)
    }

    /// 格式化总大小
    pub fn total_string(&self) -> String {
        format_bytes(self.total)
    }

    /// 格式化速度
    pub fn speed_string(&self) -> String {
        format!("{}/s", format_bytes(self.speed))
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

/// 下载状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Aria2DownloadStatus {
    /// 等待中
    Waiting,
    /// 下载中
    Active,
    /// 已暂停
    Paused,
    /// 已完成
    Complete,
    /// 出错
    Error,
    /// 已移除
    Removed,
}

impl From<&str> for Aria2DownloadStatus {
    fn from(s: &str) -> Self {
        match s {
            "waiting" => Self::Waiting,
            "active" => Self::Active,
            "paused" => Self::Paused,
            "complete" => Self::Complete,
            "error" => Self::Error,
            "removed" => Self::Removed,
            _ => Self::Waiting,
        }
    }
}

/// 下载结果
#[derive(Debug, Clone)]
pub struct Aria2DownloadResult {
    /// 保存路径
    pub path: PathBuf,
    /// 文件名
    pub filename: String,
    /// 文件大小
    pub size: u64,
    /// GID
    pub gid: String,
}

/// 下载选项
#[derive(Debug, Clone)]
pub struct Aria2DownloadOptions {
    /// 保存目录
    pub save_dir: Option<PathBuf>,
    /// 自定义文件名
    pub filename: Option<String>,
    /// 是否覆盖已存在的文件
    pub overwrite: bool,
    /// 使用 GitHub 镜像
    pub use_github_mirror: bool,
    /// 分块数量
    pub split: u32,
    /// 每服务器最大连接数
    pub max_connection_per_server: u32,
    /// 最小分块大小
    pub min_split_size: String,
    /// 自定义 User-Agent
    pub user_agent: Option<String>,
    /// 额外的 HTTP 头
    pub headers: HashMap<String, String>,
}

impl Default for Aria2DownloadOptions {
    fn default() -> Self {
        Self {
            save_dir: None,
            filename: None,
            overwrite: false,
            use_github_mirror: true,
            split: 4,
            max_connection_per_server: 4,
            min_split_size: "4M".to_string(),
            user_agent: None,
            headers: HashMap::new(),
        }
    }
}

impl Aria2DownloadOptions {
    /// 创建高速下载选项（用于代理环境）
    pub fn high_speed() -> Self {
        Self {
            split: 16,
            max_connection_per_server: 16,
            min_split_size: "4M".to_string(),
            ..Default::default()
        }
    }

    /// 创建 CDN 友好的下载选项
    pub fn cdn_friendly() -> Self {
        Self {
            split: 4,
            max_connection_per_server: 4,
            min_split_size: "12M".to_string(),
            ..Default::default()
        }
    }
}

/// 格式化文件大小 (aria2)
fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    format!("{:.1}{}", size, UNITS[unit_index])
}

/// Aria2 进程管理器
pub struct Aria2Manager {
    /// aria2 子进程
    process: RwLock<Option<Child>>,
    /// RPC 端口
    port: RwLock<u16>,
    /// WebSocket 客户端
    client: Mutex<Option<Client>>,
    /// 是否已启动
    started: AtomicBool,
    /// 活跃下载任务
    active_downloads: RwLock<HashMap<String, Aria2DownloadProgress>>,
}

impl Aria2Manager {
    /// 创建新的 Aria2Manager
    pub fn new() -> Self {
        Self {
            process: RwLock::new(None),
            port: RwLock::new(0),
            client: Mutex::new(None),
            started: AtomicBool::new(false),
            active_downloads: RwLock::new(HashMap::new()),
        }
    }

    /// 启动 aria2 守护进程
    pub async fn start(&self) -> AppResult<()> {
        if self.started.load(Ordering::SeqCst) {
            return Ok(());
        }

        // 查找可用端口
        let port = find_available_port()?;
        *self.port.write() = port;

        info!("启动 aria2 守护进程，端口: {}", port);

        // 确保 aria2 已安装（如果没有则自动下载）
        let aria2_path = ensure_aria2_installed().await?;
        info!("aria2 路径: {}", aria2_path.display());

        // 获取默认下载目录
        let download_dir = get_default_download_dir()?;
        info!("aria2 默认下载目录: {}", download_dir.display());

        // 构建命令行参数
        let mut args = vec![
            "--enable-rpc".to_string(),
            format!("--rpc-listen-port={}", port),
            "--rpc-listen-all=false".to_string(),
            format!("--rpc-secret={}", ARIA2_SECRET),
            format!("--dir={}", download_dir.to_string_lossy()),
            "--async-dns=true".to_string(),
            format!("--stop-with-process={}", std::process::id()),
            "--log-level=info".to_string(),
            "--console-log-level=warn".to_string(),
            "--allow-overwrite=true".to_string(),
            "--auto-file-renaming=false".to_string(),
            "--file-allocation=none".to_string(),
        ];

        // 根据配置添加选项
        let config = get_config();
        if config.setting.download.disable_aria2_ipv6 {
            args.push("--disable-ipv6=true".to_string());
            if config.setting.network.use_doh {
                args.push("--async-dns-server=223.5.5.5,119.29.29.29".to_string());
            }
        } else if config.setting.network.use_doh {
            args.push("--async-dns-server=2400:3200::1,2402:4e00::,223.5.5.5,119.29.29.29".to_string());
        }

        // 删除旧日志
        if config.setting.download.remove_old_aria2_log_file {
            let log_path = std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("aria2.log");
            if log_path.exists() {
                let _ = std::fs::remove_file(&log_path);
            }
        }

        // 添加日志文件
        let log_path = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("aria2.log");
        args.push(format!("--log={}", log_path.display()));

        debug!("aria2 启动参数: {:?}", args);

        // 启动进程
        #[cfg(windows)]
        let process = {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;

            Command::new(&aria2_path)
                .args(&args)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| AppError::Aria2(format!("启动 aria2 失败: {}", e)))?
        };

        #[cfg(not(windows))]
        let process = Command::new(&aria2_path)
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| AppError::Aria2(format!("启动 aria2 失败: {}", e)))?;

        *self.process.write() = Some(process);

        // 等待 aria2 启动
        tokio::time::sleep(Duration::from_millis(500)).await;

        // 连接 WebSocket
        self.connect().await?;

        self.started.store(true, Ordering::SeqCst);
        info!("aria2 守护进程启动成功");

        Ok(())
    }

    /// 连接到 aria2 WebSocket RPC
    async fn connect(&self) -> AppResult<()> {
        let port = *self.port.read();
        let url = format!("ws://127.0.0.1:{}/jsonrpc", port);

        info!("连接 aria2 WebSocket: {}", url);

        // 重试连接
        let mut last_error = None;
        for i in 0..5 {
            match Client::connect(&url, Some(ARIA2_SECRET)).await {
                Ok(client) => {
                    *self.client.lock().await = Some(client);
                    info!("aria2 WebSocket 连接成功");
                    return Ok(());
                }
                Err(e) => {
                    last_error = Some(e);
                    warn!("aria2 连接失败，重试 {}/5", i + 1);
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }

        Err(AppError::Aria2(format!(
            "无法连接到 aria2: {:?}",
            last_error
        )))
    }

    /// 确保客户端已连接
    async fn ensure_connected(&self) -> AppResult<()> {
        if self.client.lock().await.is_none() {
            self.connect().await?;
        }
        Ok(())
    }

    /// 停止 aria2 守护进程
    pub async fn stop(&self) -> AppResult<()> {
        if !self.started.load(Ordering::SeqCst) {
            return Ok(());
        }

        info!("停止 aria2 守护进程");

        // 先尝试优雅关闭
        if let Some(client) = self.client.lock().await.take() {
            let _ = client.shutdown().await;
        }

        // 强制结束进程
        if let Some(mut process) = self.process.write().take() {
            let _ = process.kill();
            let _ = process.wait();
        }

        self.started.store(false, Ordering::SeqCst);
        info!("aria2 守护进程已停止");

        Ok(())
    }

    /// 添加下载任务
    pub async fn download(
        &self,
        url: &str,
        options: Aria2DownloadOptions,
    ) -> AppResult<String> {
        self.ensure_connected().await?;

        // 处理 URL（应用镜像）
        let final_url = if options.use_github_mirror && url.contains("github.com") {
            get_github_download_url(url)
        } else {
            url.to_string()
        };

        info!("添加下载任务: {}", final_url);

        // 选择 User-Agent
        let user_agent = options.user_agent.clone().unwrap_or_else(|| {
            if final_url.contains("e6ex.com") {
                crate::config::user_agent()
            } else {
                CHROME_UA.to_string()
            }
        });

        // 构建 aria2 选项
        let mut task_options = TaskOptions::default();

        // 设置保存目录（如果指定了的话）
        if let Some(ref dir) = options.save_dir {
            task_options.dir = Some(dir.to_string_lossy().to_string());
        }

        // 设置文件名
        if let Some(ref name) = options.filename {
            task_options.out = Some(name.clone());
        }

        // 设置分块选项
        task_options.split = Some(options.split as i32);
        task_options.max_connection_per_server = Some(options.max_connection_per_server as i32);

        // 设置 User-Agent（通过 header）
        let mut headers = vec![format!("User-Agent: {}", user_agent)];

        // 添加自定义头
        for (k, v) in &options.headers {
            headers.push(format!("{}: {}", k, v));
        }
        task_options.header = Some(headers.clone());

        // 设置代理
        let proxy_info = if is_using_proxy() {
            if let Some(proxy) = get_proxy_url() {
                task_options.all_proxy = Some(proxy.clone());
                Some(proxy)
            } else {
                None
            }
        } else {
            None
        };

        // 额外选项
        let mut extra = serde_json::Map::new();
        extra.insert("min-split-size".to_string(), serde_json::json!(options.min_split_size));
        extra.insert("allow-overwrite".to_string(), serde_json::json!(options.overwrite.to_string()));
        extra.insert("auto-file-renaming".to_string(), serde_json::json!("false"));

        task_options.extra_options = extra.clone();

        // 打印任务参数到日志（debug 级别）
        debug!(
            "aria2 下载任务参数: url={}, dir={:?}, out={:?}, split={}, max_connection_per_server={}, user_agent={}, headers={:?}, proxy={:?}, min_split_size={}, allow_overwrite={}, auto_file_renaming=false",
            final_url,
            task_options.dir,
            task_options.out,
            options.split,
            options.max_connection_per_server,
            user_agent,
            headers,
            proxy_info,
            options.min_split_size,
            options.overwrite
        );

        // 添加下载
        let client = self.client.lock().await;
        let client = client
            .as_ref()
            .ok_or_else(|| AppError::Aria2("aria2 客户端未连接".to_string()))?;

        let gid = client
            .add_uri(vec![final_url.clone()], Some(task_options), None, None)
            .await
            .map_err(|e| AppError::Aria2(format!("添加下载任务失败: {}", e)))?;

        info!("下载任务已添加，GID: {}", gid);

        // 记录活跃下载
        let filename = options
            .filename
            .clone()
            .unwrap_or_else(|| extract_filename_from_url(&final_url));
        let progress = Aria2DownloadProgress::new(&gid, &filename);
        self.active_downloads.write().insert(gid.clone(), progress);

        Ok(gid)
    }

    /// 下载并等待完成
    pub async fn download_and_wait<F>(
        &self,
        url: &str,
        options: Aria2DownloadOptions,
        on_progress: F,
    ) -> AppResult<Aria2DownloadResult>
    where
        F: Fn(Aria2DownloadProgress) + Send + 'static,
    {
        let gid = self.download(url, options.clone()).await?;

        // 轮询进度
        let poll_interval = Duration::from_millis(500);
        loop {
            tokio::time::sleep(poll_interval).await;

            let progress = self.get_download_progress(&gid).await?;
            on_progress(progress.clone());

            match progress.status {
                Aria2DownloadStatus::Complete => {
                    // 获取下载文件信息
                    let status = self.get_download_status(&gid).await?;
                    let path = status
                        .files
                        .first()
                        .map(|f| PathBuf::from(&f.path))
                        .unwrap_or_default();
                    let filename = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();

                    // 清理记录
                    self.active_downloads.write().remove(&gid);
                    let _ = self.purge_download_result().await;

                    return Ok(Aria2DownloadResult {
                        path,
                        filename,
                        size: progress.total,
                        gid,
                    });
                }
                Aria2DownloadStatus::Error => {
                    let status = self.get_download_status(&gid).await?;
                    let error_code = status.error_code.map(|c| c.to_string()).unwrap_or_default();
                    let error_msg = status.error_message.clone().unwrap_or_default();

                    // 清理记录
                    self.active_downloads.write().remove(&gid);

                    return Err(AppError::Aria2(format!(
                        "下载失败 (错误码: {}): {}",
                        error_code, error_msg
                    )));
                }
                Aria2DownloadStatus::Removed => {
                    self.active_downloads.write().remove(&gid);
                    return Err(AppError::Aria2("下载已取消".to_string()));
                }
                _ => continue,
            }
        }
    }

    /// 获取下载进度
    pub async fn get_download_progress(&self, gid: &str) -> AppResult<Aria2DownloadProgress> {
        let status = self.get_download_status(gid).await?;

        let downloaded = status.completed_length;
        let total = status.total_length;
        let speed = status.download_speed;

        let percentage = if total > 0 {
            (downloaded as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let eta = if speed > 0 && total > downloaded {
            (total - downloaded) / speed
        } else {
            0
        };

        let filename = status
            .files
            .first()
            .map(|f| {
                PathBuf::from(&f.path)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        let status_str = format!("{:?}", status.status).to_lowercase();

        Ok(Aria2DownloadProgress {
            gid: gid.to_string(),
            downloaded,
            total,
            speed,
            percentage,
            eta,
            filename,
            status: Aria2DownloadStatus::from(status_str.as_str()),
        })
    }

    /// 获取下载状态详情
    pub async fn get_download_status(&self, gid: &str) -> AppResult<Aria2Status> {
        let client = self.client.lock().await;
        let client = client
            .as_ref()
            .ok_or_else(|| AppError::Aria2("aria2 客户端未连接".to_string()))?;

        let status = client
            .tell_status(gid)
            .await
            .map_err(|e| AppError::Aria2(format!("获取下载状态失败: {}", e)))?;

        Ok(status)
    }

    /// 暂停下载
    pub async fn pause(&self, gid: &str) -> AppResult<()> {
        let client = self.client.lock().await;
        let client = client
            .as_ref()
            .ok_or_else(|| AppError::Aria2("aria2 客户端未连接".to_string()))?;

        client
            .pause(gid)
            .await
            .map_err(|e| AppError::Aria2(format!("暂停下载失败: {}", e)))?;

        Ok(())
    }

    /// 恢复下载
    pub async fn resume(&self, gid: &str) -> AppResult<()> {
        let client = self.client.lock().await;
        let client = client
            .as_ref()
            .ok_or_else(|| AppError::Aria2("aria2 客户端未连接".to_string()))?;

        client
            .unpause(gid)
            .await
            .map_err(|e| AppError::Aria2(format!("恢复下载失败: {}", e)))?;

        Ok(())
    }

    /// 取消下载
    pub async fn cancel(&self, gid: &str) -> AppResult<()> {
        let client = self.client.lock().await;
        let client = client
            .as_ref()
            .ok_or_else(|| AppError::Aria2("aria2 客户端未连接".to_string()))?;

        client
            .remove(gid)
            .await
            .map_err(|e| AppError::Aria2(format!("取消下载失败: {}", e)))?;

        self.active_downloads.write().remove(gid);

        Ok(())
    }

    /// 暂停所有下载
    pub async fn pause_all(&self) -> AppResult<()> {
        let client = self.client.lock().await;
        let client = client
            .as_ref()
            .ok_or_else(|| AppError::Aria2("aria2 客户端未连接".to_string()))?;

        client
            .pause_all()
            .await
            .map_err(|e| AppError::Aria2(format!("暂停所有下载失败: {}", e)))?;

        Ok(())
    }

    /// 恢复所有下载
    pub async fn resume_all(&self) -> AppResult<()> {
        let client = self.client.lock().await;
        let client = client
            .as_ref()
            .ok_or_else(|| AppError::Aria2("aria2 客户端未连接".to_string()))?;

        client
            .unpause_all()
            .await
            .map_err(|e| AppError::Aria2(format!("恢复所有下载失败: {}", e)))?;

        Ok(())
    }

    /// 取消所有下载
    pub async fn cancel_all(&self) -> AppResult<()> {
        let gids: Vec<String> = self.active_downloads.read().keys().cloned().collect();

        for gid in gids {
            let _ = self.cancel(&gid).await;
        }

        Ok(())
    }

    /// 清理已完成的下载记录
    async fn purge_download_result(&self) -> AppResult<()> {
        let client = self.client.lock().await;
        let client = client
            .as_ref()
            .ok_or_else(|| AppError::Aria2("aria2 客户端未连接".to_string()))?;

        client
            .purge_download_result()
            .await
            .map_err(|e| AppError::Aria2(format!("清理下载记录失败: {}", e)))?;

        Ok(())
    }

    /// 获取全局统计信息
    pub async fn get_global_stat(&self) -> AppResult<Aria2GlobalStat> {
        let client = self.client.lock().await;
        let client = client
            .as_ref()
            .ok_or_else(|| AppError::Aria2("aria2 客户端未连接".to_string()))?;

        let stat = client
            .get_global_stat()
            .await
            .map_err(|e| AppError::Aria2(format!("获取全局统计失败: {}", e)))?;

        Ok(stat)
    }

    /// 是否已启动
    pub fn is_started(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }

    /// 获取 RPC 端口
    pub fn get_port(&self) -> u16 {
        *self.port.read()
    }
}

impl Default for Aria2Manager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Aria2Manager {
    fn drop(&mut self) {
        // 同步停止进程
        if let Some(mut process) = self.process.write().take() {
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}

/// 下载状态详情（从 aria2 返回）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadStatus {
    pub gid: String,
    pub status: String,
    pub total_length: String,
    pub completed_length: String,
    pub download_speed: String,
    #[serde(default)]
    pub files: Vec<FileInfo>,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
}

/// 文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub length: String,
    #[serde(default)]
    pub completed_length: String,
}

/// 全局统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalStat {
    pub download_speed: String,
    pub upload_speed: String,
    pub num_active: String,
    pub num_waiting: String,
    pub num_stopped: String,
    pub num_stopped_total: String,
}

/// 查找可用端口
fn find_available_port() -> AppResult<u16> {
    use std::net::TcpListener;

    // 尝试在随机端口范围内找一个可用端口
    for _ in 0..100 {
        let port = rand::random::<u16>() % 40000 + 20000; // 20000-60000
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Ok(port);
        }
    }

    Err(AppError::Aria2("找不到可用端口".to_string()))
}

/// 获取 aria2c 可执行文件路径
///
/// 查找顺序：
/// 1. 可执行文件同目录下的 module/aria2c.exe（打包后）
/// 2. 可执行文件同目录下的 aria2c.exe（打包后）
/// 3. 当前工作目录下的 module/aria2c.exe（开发时）
/// 4. 当前工作目录下的 aria2c.exe
/// 5. 项目根目录下的 module/aria2c.exe（开发时，从 src-tauri 运行）
/// 6. PATH 环境变量
///
/// 注意：此函数已被 `ensure_aria2_installed()` 替代，保留用于兼容性
#[allow(dead_code)]
fn get_aria2_path() -> AppResult<PathBuf> {
    let aria2_name = if cfg!(windows) { "aria2c.exe" } else { "aria2c" };

    // 获取可执行文件所在目录（打包后的应用程序目录）
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // 检查可执行文件同目录下的 module 文件夹
            let module_path = exe_dir.join("module").join(aria2_name);
            if module_path.exists() {
                debug!("找到 aria2c: {} (exe/module)", module_path.display());
                return Ok(module_path);
            }

            // 检查可执行文件同目录
            let exe_dir_path = exe_dir.join(aria2_name);
            if exe_dir_path.exists() {
                debug!("找到 aria2c: {} (exe dir)", exe_dir_path.display());
                return Ok(exe_dir_path);
            }
        }
    }

    // 检查当前工作目录下的 module 文件夹（开发时）
    if let Ok(cwd) = std::env::current_dir() {
        let module_path = cwd.join("module").join(aria2_name);
        if module_path.exists() {
            debug!("找到 aria2c: {} (cwd/module)", module_path.display());
            return Ok(module_path);
        }

        // 检查当前工作目录
        let cwd_path = cwd.join(aria2_name);
        if cwd_path.exists() {
            debug!("找到 aria2c: {} (cwd)", cwd_path.display());
            return Ok(cwd_path);
        }

        // 开发时可能从 src-tauri 目录运行，检查上级目录的 module
        let parent_module_path = cwd.join("..").join("module").join(aria2_name);
        if parent_module_path.exists() {
            let canonical = parent_module_path.canonicalize().unwrap_or(parent_module_path);
            debug!("找到 aria2c: {} (parent/module)", canonical.display());
            return Ok(canonical);
        }
    }

    // 检查 PATH 环境变量
    if let Ok(path) = which::which("aria2c") {
        debug!("找到 aria2c: {} (PATH)", path.display());
        return Ok(path);
    }

    Err(AppError::Aria2("找不到 aria2c 可执行文件".to_string()))
}

/// 获取默认下载目录（work_dir/download）
pub fn get_default_download_dir() -> AppResult<PathBuf> {
    let download_dir = std::env::current_dir()
        .map_err(|e| AppError::Aria2(format!("获取当前目录失败: {}", e)))?
        .join("download");

    // 确保目录存在
    if !download_dir.exists() {
        std::fs::create_dir_all(&download_dir)
            .map_err(|e| AppError::Aria2(format!("创建下载目录失败: {}", e)))?;
        info!("创建下载目录: {}", download_dir.display());
    }

    Ok(download_dir)
}

/// 从 URL 提取文件名
fn extract_filename_from_url(url: &str) -> String {
    url.split('/')
        .last()
        .and_then(|s| s.split('?').next())
        .unwrap_or("download")
        .to_string()
}

/// Aria2 发布版本信息
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Aria2ReleaseAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Aria2ReleaseInfo {
    tag_name: String,
    name: String,
    assets: Vec<Aria2ReleaseAsset>,
}

/// 获取 aria2 最新版本信息
async fn get_latest_aria2_release() -> AppResult<Aria2ReleaseInfo> {
    info!("获取 aria2 最新版本信息");

    let api_url = "https://api.github.com/repos/aria2/aria2/releases/latest";
    let data = request_github_api(api_url).await?;

    let release: Aria2ReleaseInfo = serde_json::from_value(data)
        .map_err(|e| AppError::Aria2(format!("解析 aria2 版本信息失败: {}", e)))?;

    info!("获取到 aria2 最新版本: {}", release.tag_name);
    Ok(release)
}

/// 下载 aria2
async fn download_aria2(asset_url: &str, save_path: &PathBuf) -> AppResult<()> {
    info!("开始下载 aria2: {}", asset_url);

    // 根据是否使用代理选择最终的下载 URL
    let final_url = if is_using_proxy() {
        info!("检测到代理，直连 GitHub");
        asset_url.to_string()
    } else {
        info!("未检测到代理，使用镜像源");
        get_github_download_url(asset_url)
    };

    info!("实际下载 URL: {}", final_url);

    // 创建 HTTP 客户端
    let mut client_builder = reqwest::Client::builder()
        .user_agent(CHROME_UA)
        .timeout(Duration::from_secs(600)) // 10分钟超时
        .connect_timeout(Duration::from_secs(30));

    // 如果使用代理，配置代理
    if let Some(proxy_url) = get_proxy_url() {
        if !proxy_url.is_empty() {
            debug!("使用代理: {}", proxy_url);
            let proxy = reqwest::Proxy::all(&proxy_url)
                .map_err(|e| AppError::Aria2(format!("配置代理失败: {}", e)))?;
            client_builder = client_builder.proxy(proxy);
        }
    }

    let client = client_builder
        .build()
        .map_err(|e| AppError::Aria2(format!("创建 HTTP 客户端失败: {}", e)))?;

    // 下载文件
    let response = client
        .get(&final_url)
        .send()
        .await
        .map_err(|e| AppError::Aria2(format!("下载 aria2 失败: {}", e)))?;

    if !response.status().is_success() {
        return Err(AppError::Aria2(format!(
            "下载 aria2 失败，HTTP 状态码: {}",
            response.status()
        )));
    }

    let total_size = response.content_length().unwrap_or(0);
    info!("aria2 文件大小: {} 字节", total_size);

    // 读取响应内容
    let bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::Aria2(format!("读取 aria2 下载内容失败: {}", e)))?;

    // 创建父目录
    if let Some(parent) = save_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppError::Aria2(format!("创建目录失败: {}", e)))?;
    }

    // 写入文件
    let mut file = fs::File::create(save_path)
        .map_err(|e| AppError::Aria2(format!("创建文件失败: {}", e)))?;

    file.write_all(&bytes)
        .map_err(|e| AppError::Aria2(format!("写入文件失败: {}", e)))?;

    info!("aria2 下载完成: {}", save_path.display());
    Ok(())
}

/// 解压 aria2 压缩包
fn extract_aria2(archive_path: &PathBuf, target_dir: &PathBuf) -> AppResult<PathBuf> {
    info!("开始解压 aria2: {}", archive_path.display());

    // 创建目标目录
    fs::create_dir_all(target_dir)
        .map_err(|e| AppError::Aria2(format!("创建目标目录失败: {}", e)))?;

    // 根据文件扩展名选择解压方法
    let extension = archive_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    match extension {
        "zip" => extract_zip(archive_path, target_dir),
        "7z" => extract_7z(archive_path, target_dir),
        _ => Err(AppError::Aria2(format!(
            "不支持的压缩格式: {}",
            extension
        ))),
    }
}

/// 解压 ZIP 文件
fn extract_zip(archive_path: &PathBuf, target_dir: &PathBuf) -> AppResult<PathBuf> {
    let file = fs::File::open(archive_path)
        .map_err(|e| AppError::Aria2(format!("打开 ZIP 文件失败: {}", e)))?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| AppError::Aria2(format!("读取 ZIP 文件失败: {}", e)))?;

    let mut aria2c_path: Option<PathBuf> = None;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| AppError::Aria2(format!("读取 ZIP 条目失败: {}", e)))?;

        let file_name = file.name();

        // 查找 aria2c.exe
        if file_name.ends_with("aria2c.exe") {
            let output_path = target_dir.join("aria2c.exe");
            let mut output_file = fs::File::create(&output_path)
                .map_err(|e| AppError::Aria2(format!("创建输出文件失败: {}", e)))?;

            std::io::copy(&mut file, &mut output_file)
                .map_err(|e| AppError::Aria2(format!("解压文件失败: {}", e)))?;

            info!("已提取 aria2c.exe 到: {}", output_path.display());
            aria2c_path = Some(output_path);
            break;
        }
    }

    aria2c_path.ok_or_else(|| AppError::Aria2("在压缩包中未找到 aria2c.exe".to_string()))
}

/// 解压 7z 文件
fn extract_7z(archive_path: &PathBuf, target_dir: &PathBuf) -> AppResult<PathBuf> {
    sevenz_rust::decompress_file(archive_path, target_dir)
        .map_err(|e| AppError::Aria2(format!("解压 7z 文件失败: {}", e)))?;

    // 在解压后的文件中查找 aria2c.exe
    let aria2c_path = find_aria2c_in_dir(target_dir)?;

    // 如果 aria2c.exe 在子目录中，移动到目标目录
    if aria2c_path.parent() != Some(target_dir.as_path()) {
        let final_path = target_dir.join("aria2c.exe");
        fs::copy(&aria2c_path, &final_path)
            .map_err(|e| AppError::Aria2(format!("移动 aria2c.exe 失败: {}", e)))?;

        info!("已移动 aria2c.exe 到: {}", final_path.display());
        return Ok(final_path);
    }

    Ok(aria2c_path)
}

/// 在目录中查找 aria2c.exe
fn find_aria2c_in_dir(dir: &PathBuf) -> AppResult<PathBuf> {
    use walkdir::WalkDir;

    for entry in WalkDir::new(dir) {
        let entry = entry.map_err(|e| AppError::Aria2(format!("遍历目录失败: {}", e)))?;
        if entry.file_name().to_string_lossy() == "aria2c.exe" {
            return Ok(entry.path().to_path_buf());
        }
    }

    Err(AppError::Aria2("在解压目录中未找到 aria2c.exe".to_string()))
}

/// 确保 aria2 已安装（如果没有则自动下载）
pub async fn ensure_aria2_installed() -> AppResult<PathBuf> {
    // 先尝试查找已安装的 aria2
    if let Ok(path) = try_find_aria2_path() {
        info!("找到已安装的 aria2: {}", path.display());
        return Ok(path);
    }

    info!("未找到 aria2，开始自动下载");

    // 获取最新版本信息
    let release = get_latest_aria2_release().await?;

    // 查找 Windows 64 位版本的资源
    let asset = release
        .assets
        .iter()
        .find(|a| {
            let name = a.name.to_lowercase();
            name.contains("win") && name.contains("64bit") && (name.ends_with(".zip") || name.ends_with(".7z"))
        })
        .ok_or_else(|| AppError::Aria2("未找到适合的 aria2 Windows 版本".to_string()))?;

    info!("选择下载: {} ({}字节)", asset.name, asset.size);

    // 确定保存路径
    let temp_dir = std::env::temp_dir();
    let archive_path = temp_dir.join(&asset.name);
    let install_dir = get_aria2_install_dir()?;

    // 下载
    download_aria2(&asset.browser_download_url, &archive_path).await?;

    // 解压到 exe 所在目录
    let aria2c_path = extract_aria2(&archive_path, &install_dir)?;

    // 清理临时文件
    let _ = fs::remove_file(&archive_path);

    info!("aria2 安装完成: {}", aria2c_path.display());
    Ok(aria2c_path)
}

/// 尝试查找 aria2 路径（不抛出错误）
fn try_find_aria2_path() -> AppResult<PathBuf> {
    let aria2_name = if cfg!(windows) { "aria2c.exe" } else { "aria2c" };

    // 获取可执行文件所在目录（打包后的应用程序目录）
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // 检查可执行文件同目录下的 module 文件夹
            let module_path = exe_dir.join("module").join(aria2_name);
            if module_path.exists() {
                return Ok(module_path);
            }

            // 检查可执行文件同目录
            let exe_dir_path = exe_dir.join(aria2_name);
            if exe_dir_path.exists() {
                return Ok(exe_dir_path);
            }
        }
    }

    // 检查当前工作目录下的 module 文件夹（开发时）
    if let Ok(cwd) = std::env::current_dir() {
        let module_path = cwd.join("module").join(aria2_name);
        if module_path.exists() {
            return Ok(module_path);
        }

        // 检查当前工作目录
        let cwd_path = cwd.join(aria2_name);
        if cwd_path.exists() {
            return Ok(cwd_path);
        }

        // 开发时可能从 src-tauri 目录运行，检查上级目录的 module
        let parent_module_path = cwd.join("..").join("module").join(aria2_name);
        if parent_module_path.exists() {
            if let Ok(canonical) = parent_module_path.canonicalize() {
                return Ok(canonical);
            }
        }
    }

    // 检查 PATH 环境变量
    if let Ok(path) = which::which("aria2c") {
        return Ok(path);
    }

    Err(AppError::Aria2("找不到 aria2c 可执行文件".to_string()))
}

/// 获取 aria2 安装目录（exe 所在目录）
fn get_aria2_install_dir() -> AppResult<PathBuf> {
    // 优先使用可执行文件所在目录
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            return Ok(exe_dir.to_path_buf());
        }
    }

    // 回退到当前工作目录
    let cwd = std::env::current_dir()
        .map_err(|e| AppError::Aria2(format!("获取当前目录失败: {}", e)))?;
    Ok(cwd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0.0B");
        assert_eq!(format_bytes(1023), "1023.0B");
        assert_eq!(format_bytes(1024), "1.0KiB");
        assert_eq!(format_bytes(1024 * 1024), "1.0MiB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0GiB");
    }

    #[test]
    fn test_download_status_from_str() {
        assert_eq!(
            Aria2DownloadStatus::from("active"),
            Aria2DownloadStatus::Active
        );
        assert_eq!(
            Aria2DownloadStatus::from("complete"),
            Aria2DownloadStatus::Complete
        );
        assert_eq!(
            Aria2DownloadStatus::from("error"),
            Aria2DownloadStatus::Error
        );
        assert_eq!(
            Aria2DownloadStatus::from("unknown"),
            Aria2DownloadStatus::Waiting
        );
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
    fn test_download_progress() {
        let mut progress = Aria2DownloadProgress::new("abc123", "test.zip");
        progress.downloaded = 512 * 1024;
        progress.total = 1024 * 1024;
        progress.speed = 100 * 1024;
        progress.percentage = 50.0;
        progress.eta = 5;

        assert_eq!(progress.downloaded_string(), "512.0KiB");
        assert_eq!(progress.total_string(), "1.0MiB");
        assert_eq!(progress.speed_string(), "100.0KiB/s");
        assert_eq!(progress.eta_string(), "5s");
    }

    /// 真实下载测试 - 下载一个小文件验证完整流程
    /// 使用 httpbin.org 提供的测试端点
    #[tokio::test]
    #[ignore] // 需要 aria2c 可执行文件和网络连接，默认忽略
    async fn test_real_download() {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc;

        info!("开始真实下载测试");

        // 创建临时目录
        let temp_dir = std::env::temp_dir().join("aria2_test");
        let _ = std::fs::create_dir_all(&temp_dir);

        // 创建管理器并启动
        let manager = Aria2Manager::new();
        manager.start().await.expect("启动 aria2 失败");

        // 下载一个小文件 (约 1KB)
        let test_url = "https://hub.gitmirror.com/https://github.com/eden-emulator/Releases/releases/download/v0.0.4-rc3/Eden-Windows-v0.0.4-rc3-amd64-msvc-standard.zip";
        let options = Aria2DownloadOptions {
            save_dir: Some(temp_dir.clone()),
            filename: Some("test_download.bin".to_string()),
            overwrite: true,
            use_github_mirror: false,
            ..Default::default()
        };

        // 追踪进度更新
        let progress_count = Arc::new(AtomicU64::new(0));
        let progress_count_clone = progress_count.clone();

        let result = manager
            .download_and_wait(test_url, options, move |progress| {
                progress_count_clone.fetch_add(1, Ordering::SeqCst);
                println!(
                    "进度: {:.1}% - {} / {} @ {}",
                    progress.percentage,
                    progress.downloaded_string(),
                    progress.total_string(),
                    progress.speed_string()
                );
            })
            .await;

        // 停止 aria2
        manager.stop().await.expect("停止 aria2 失败");

        // 验证结果
        let result = result.expect("下载失败");
        assert!(result.path.exists(), "下载文件不存在");
        assert_eq!(result.filename, "test_download.bin");
        assert!(result.size > 0, "文件大小应大于 0");
        assert!(
            progress_count.load(Ordering::SeqCst) > 0,
            "应该有进度更新"
        );

        // 清理
        let _ = std::fs::remove_file(&result.path);
        let _ = std::fs::remove_dir(&temp_dir);

        println!("下载完成: {:?}, 大小: {} 字节", result.path, result.size);
    }

    /// 测试下载暂停和恢复
    #[tokio::test]
    #[ignore]
    async fn test_pause_resume_download() {
        let temp_dir = std::env::temp_dir().join("aria2_test_pause");
        let _ = std::fs::create_dir_all(&temp_dir);

        let manager = Aria2Manager::new();
        manager.start().await.expect("启动 aria2 失败");

        // 下载一个较大的文件以便有时间暂停
        let test_url = "https://httpbin.org/bytes/102400"; // 100KB
        let options = Aria2DownloadOptions {
            save_dir: Some(temp_dir.clone()),
            filename: Some("test_pause.bin".to_string()),
            overwrite: true,
            use_github_mirror: false,
            split: 1, // 单线程以便更容易暂停
            max_connection_per_server: 1,
            ..Default::default()
        };

        // 添加下载任务
        let gid = manager
            .download(test_url, options)
            .await
            .expect("添加下载任务失败");

        // 等待开始下载
        tokio::time::sleep(Duration::from_millis(200)).await;

        // 暂停
        manager.pause(&gid).await.expect("暂停失败");
        let progress = manager.get_download_progress(&gid).await.expect("获取进度失败");
        assert!(
            matches!(progress.status, Aria2DownloadStatus::Paused | Aria2DownloadStatus::Waiting),
            "状态应为暂停或等待"
        );

        // 恢复
        manager.resume(&gid).await.expect("恢复失败");
        tokio::time::sleep(Duration::from_millis(200)).await;
        let progress = manager.get_download_progress(&gid).await.expect("获取进度失败");
        assert!(
            matches!(progress.status, Aria2DownloadStatus::Active | Aria2DownloadStatus::Complete),
            "状态应为活跃或完成"
        );

        // 等待完成或取消
        manager.cancel(&gid).await.ok();
        manager.stop().await.expect("停止 aria2 失败");

        // 清理
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    /// 测试取消下载
    #[tokio::test]
    #[ignore]
    async fn test_cancel_download() {
        let temp_dir = std::env::temp_dir().join("aria2_test_cancel");
        let _ = std::fs::create_dir_all(&temp_dir);

        let manager = Aria2Manager::new();
        manager.start().await.expect("启动 aria2 失败");

        let test_url = "https://httpbin.org/bytes/102400";
        let options = Aria2DownloadOptions {
            save_dir: Some(temp_dir.clone()),
            filename: Some("test_cancel.bin".to_string()),
            overwrite: true,
            use_github_mirror: false,
            ..Default::default()
        };

        let gid = manager
            .download(test_url, options)
            .await
            .expect("添加下载任务失败");

        // 取消下载
        manager.cancel(&gid).await.expect("取消下载失败");

        // 验证状态
        let progress = manager.get_download_progress(&gid).await;
        assert!(progress.is_err() || matches!(progress.unwrap().status, Aria2DownloadStatus::Removed));

        manager.stop().await.expect("停止 aria2 失败");
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

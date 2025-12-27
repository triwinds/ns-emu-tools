# Rust Downloader 实现计划（方案 A：统一下载模块重构）

## 概述

实现一个纯 Rust 的下载器，作为 aria2 的备选方案，并通过“统一下载门面（DownloadManager trait + 全局选择器）”对外提供一个稳定接口。

说明：当前项目的 `Aria2Manager` 会在启动时自动下载/安装 aria2，因此“系统没有 aria2”并不是可靠的分流条件。方案 A 的目标是：

- 所有业务代码只依赖 `src-tauri/src/services/download` 的统一接口
- aria2 与 RustDownloader 两个后端都实现同一 trait
- `auto` 模式下：优先 aria2；启动失败/不可用时自动回退 RustDownloader

## 设计目标

- ✅ **统一入口**：业务代码只调用 `get_download_manager()`，不再直接依赖 `get_aria2_manager()`
- ✅ **备选方案**：`auto` 模式优先 aria2，不可用时回退 RustDownloader
- ✅ **接口对齐**：通过 `DownloadManager` trait 统一接口与类型
- ✅ **断点续传**：自适应单连接/多连接下载
- ✅ **智能重试**：错误分类、网络感知、指数退避

补充目标（为保证与现有行为一致）：

- ✅ **网络行为一致**：复用现有 `services::network` 的镜像/代理/UA 规则（例如 `get_github_download_url`、`get_proxy_url`、`CHROME_UA`）
- ✅ **取消/清理一致**：统一实现“取消下载 + 可选删除已下载文件/状态文件”的语义，避免命令层依赖 aria2 专有清理逻辑

## 架构设计

```
DownloadManager trait (统一接口)
    ├── Aria2Backend (基于现有 Aria2Manager 的适配层)
    └── RustDownloader (新实现)
            ├── ChunkManager (分块下载)
            ├── RetryStrategy (智能重试)
            └── StateStore (断点续传)
```

## 文件结构

### 新增文件

```
src-tauri/src/services/download/
├── mod.rs                    # 模块导出和全局管理器选择
├── manager.rs                # DownloadManager trait 定义
├── types.rs                  # 共享数据类型（兼容 Aria2）
├── aria2_backend.rs           # Aria2Manager 适配层（实现 DownloadManager）
├── rust_downloader.rs        # 纯 Rust 实现
├── chunk_manager.rs          # 分块下载管理
├── retry_strategy.rs         # 重试策略
└── state_store.rs            # 状态持久化
```

### 需修改文件

- `src-tauri/src/services/aria2.rs` - 保留现有 aria2 低层实现；由 `aria2_backend.rs` 组合/调用（尽量少改）
- `src-tauri/src/services/mod.rs` - 导出 download 模块
- `src-tauri/src/config.rs` - 添加下载后端配置项
- `src-tauri/src/services/firmware.rs` / `yuzu.rs` / `ryujinx.rs` / `msvc.rs` - 切换到 `get_download_manager()` 与统一 types
- `src-tauri/src/commands/common.rs` - `cancel_download_command` 改为通过统一下载接口取消/清理
- `Cargo.toml` - 添加新依赖

## 核心实现细节

### 1. Trait 抽象 (`manager.rs`)

定义统一的 `DownloadManager` trait：

```rust
#[async_trait]
pub trait DownloadManager: Send + Sync {
    async fn start(&self) -> AppResult<()>;
    async fn stop(&self) -> AppResult<()>;
    async fn download(&self, url: &str, options: DownloadOptions) -> AppResult<String>;
    async fn download_and_wait<F>(&self, url: &str, options: DownloadOptions, on_progress: F) -> AppResult<DownloadResult>
    where F: Fn(DownloadProgress) + Send + 'static;
    async fn pause(&self, task_id: &str) -> AppResult<()>;
    async fn resume(&self, task_id: &str) -> AppResult<()>;
    async fn cancel(&self, task_id: &str) -> AppResult<()>;
    async fn cancel_all(&self, remove_files: bool) -> AppResult<Option<String>>;
    async fn get_download_progress(&self, task_id: &str) -> AppResult<DownloadProgress>;
    fn is_started(&self) -> bool;
}
```

**关键点**：
- 使用 `async_trait` 支持异步方法
- 方法签名与现有 `Aria2Manager` 能力对齐，并补齐命令层真实需要的 `cancel_all(remove_files)`
- 返回类型使用 `AppResult<T>` 统一错误处理

**`cancel_all(remove_files)` 清理行为定义**：
- `remove_files=true`：删除 `.part` 临时文件 + `.download` 状态文件
- `remove_files=false`：仅停止下载任务，保留文件以便后续恢复
- 必须等待所有 chunk 停止后再执行删除操作

### 2. 数据类型 (`types.rs`)

创建与 Aria2 兼容的数据结构：

```rust
pub struct DownloadOptions {
    pub save_dir: Option<PathBuf>,
    pub filename: Option<String>,
    pub overwrite: bool,
    pub use_github_mirror: bool,
    pub split: u32,                      // 连接数
    pub max_connection_per_server: u32,
    pub min_split_size: String,          // "4M"
    pub user_agent: Option<String>,
    pub headers: HashMap<String, String>,
}

pub struct DownloadProgress {
    pub gid: String,              // 任务 ID
    pub downloaded: u64,
    pub total: u64,
    pub speed: u64,
    pub percentage: f64,
    pub eta: u64,
    pub filename: String,
    pub status: DownloadStatus,
}

pub enum DownloadStatus {
    Waiting, Active, Paused, Complete, Error, Removed,
}

pub struct DownloadResult {
    pub path: PathBuf,
    pub filename: String,
    pub size: u64,
    pub gid: String,
}
```

**关键点**：
- 字段名和类型与 `Aria2DownloadProgress` 完全一致
- 支持 `#[serde(rename_all = "camelCase")]` 序列化
- `gid` 字段兼容 aria2 的唯一标识符概念

备注：为了降低迁移成本，可在 `aria2.rs` 内用 type alias/重导出逐步过渡到统一 types。

### 2.1 HTTP Client 配置 (`client.rs`)

reqwest Client 的配置细节：

```rust
pub struct ClientConfig {
    pub connect_timeout: Duration,           // 连接超时，默认 30s
    pub read_timeout: Duration,              // 读取超时，默认 60s
    pub max_redirects: usize,                // 最大重定向次数，默认 10
    pub pool_idle_timeout: Duration,         // 连接池空闲超时，默认 90s
    pub pool_max_idle_per_host: usize,       // 每个 host 最大空闲连接数，默认 10
    pub danger_accept_invalid_certs: bool,   // 是否接受无效证书（仅调试用）
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(30),
            read_timeout: Duration::from_secs(60),
            max_redirects: 10,
            pool_idle_timeout: Duration::from_secs(90),
            pool_max_idle_per_host: 10,
            danger_accept_invalid_certs: false,
        }
    }
}

pub fn build_client(config: &ClientConfig) -> AppResult<Client> {
    let mut builder = Client::builder()
        .connect_timeout(config.connect_timeout)
        .read_timeout(config.read_timeout)
        .redirect(Policy::limited(config.max_redirects))
        .pool_idle_timeout(config.pool_idle_timeout)
        .pool_max_idle_per_host(config.pool_max_idle_per_host);

    // 代理配置：复用 services::network::get_proxy_url()
    if let Some(proxy_url) = get_proxy_url() {
        let proxy = Proxy::all(&proxy_url)?;
        builder = builder.proxy(proxy);
    }

    if config.danger_accept_invalid_certs {
        builder = builder.danger_accept_invalid_certs(true);
    }

    builder.build().map_err(|e| AppError::Network(e.to_string()))
}
```

**关键点**：
- 代理在 Client 构建时设置，整个 Client 生命周期内生效
- 如需动态切换代理，需重建 Client
- 超时配置应与 aria2 保持一致

### 3. 断点续传 (`state_store.rs`)

使用 JSON 状态文件（`.download`）持久化下载状态：

```rust
pub struct DownloadState {
    pub url: String,
    pub resolved_url: Option<String>,
    pub total_size: u64,              // 0 表示未知长度
    pub supports_range: bool,         // 是否支持 Range
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub chunks: Vec<ChunkState>,      // 分块信息
    pub last_updated: i64,
    pub pid: Option<u32>,             // 记录下载进程 PID，用于检测僵尸状态
}

pub struct ChunkState {
    pub index: usize,
    pub start: u64,
    pub end: u64,
    pub downloaded: u64,
    pub completed: bool,
}
```

**工作流程**：
1. 下载开始前检查是否存在 `.download` 文件
2. 如果存在，必须做一致性校验：URL/最终 URL + (Content-Length/ETag/Last-Modified)；不一致则丢弃状态并重新下载
3. 下载过程中定期保存状态（每 5 秒或进度变化 1%），并使用原子写入（写临时文件 + rename）避免崩溃造成 JSON 损坏
4. 下载完成后删除状态文件

**文件锁机制**：
- 启动时检查状态文件中的 `pid` 字段
- 如果 PID 对应的进程仍在运行，拒绝恢复（避免多实例冲突）
- 如果进程已不存在，视为可恢复的中断下载
- 下载开始时更新 `pid` 为当前进程 ID

### 3.1 临时文件策略

下载过程中的文件命名规范：

```rust
// 文件命名规则：
// - 下载中：filename.zip.part
// - 状态文件：filename.zip.download
// - 下载完成：filename.zip（rename 后）

pub fn get_temp_filename(filename: &str) -> String {
    format!("{}.part", filename)
}

pub fn get_state_filename(filename: &str) -> String {
    format!("{}.download", filename)
}

// 下载完成后的处理流程：
// 1. 验证文件完整性（大小/校验和）
// 2. rename: filename.part -> filename
// 3. 删除 filename.download 状态文件
```

**关键点**：
- `.part` 后缀防止用户误用不完整的文件
- 完成后使用原子 rename 操作
- 如果目标文件已存在且 `overwrite=false`，应在下载开始前检查并报错

### 3.2 文件名解析策略

当 `options.filename` 为 `None` 时，按优先级解析文件名：

```rust
pub fn resolve_filename(response: &Response, url: &str, options: &DownloadOptions) -> String {
    // 1. 优先使用用户指定的文件名
    if let Some(filename) = &options.filename {
        return filename.clone();
    }

    // 2. 从 Content-Disposition 头解析
    if let Some(cd) = response.headers().get(CONTENT_DISPOSITION) {
        if let Ok(cd_str) = cd.to_str() {
            // 解析 filename="xxx" 或 filename*=UTF-8''xxx
            if let Some(filename) = parse_content_disposition(cd_str) {
                return filename;
            }
        }
    }

    // 3. 从 URL 路径提取
    if let Some(filename) = extract_filename_from_url(url) {
        return filename;
    }

    // 4. 使用默认名称
    format!("download_{}", Uuid::new_v4())
}

fn parse_content_disposition(header: &str) -> Option<String> {
    // 支持两种格式：
    // - filename="example.zip"
    // - filename*=UTF-8''%E4%B8%AD%E6%96%87.zip (RFC 5987)
    // 优先使用 filename* 格式（支持非 ASCII 字符）
}
```

### 4. 分块下载 (`chunk_manager.rs`)

自适应策略：

```rust
impl ChunkManager {
    // 1. 检测服务器是否支持 Range
    pub async fn check_range_support(client: &Client, url: &str) -> AppResult<(bool, u64)> {
        // 推荐使用 GET + Range: bytes=0-0 探测：
        // - 206 + Content-Range => 支持 Range
        // - 200/其他 => 视为不支持
        // 同时尽量拿到 Content-Length / Content-Range 总长度
    }

    // 2. 计算分块策略
    pub fn calculate_chunks(&self, total_size: u64, supports_range: bool) -> Vec<ChunkState> {
        if !supports_range || total_size < min_chunk_size {
            // 单连接下载
            return vec![ChunkState { start: 0, end: total_size - 1, ... }];
        }

        // 多连接下载，根据 split 参数分块
        let chunk_count = min(split, total_size / min_chunk_size);
        // 均匀分配字节范围
    }

    // 3. 下载单个分块
    pub async fn download_chunk(...) -> AppResult<()> {
        // 使用 Range: bytes=start-end 请求
        // 注意：并发写入不能依赖同一个 tokio::fs::File 的 seek() 游标
        // 推荐方案：
        // - 使用 FileExt::write_at/pwrite 风格随机写（必要时 spawn_blocking）
        // - 或每个 chunk 独立打开文件句柄
        // 另外建议设置 Accept-Encoding: identity，避免压缩影响字节范围语义
        // 通过 channel 发送进度更新
    }
}
```

**关键点**：
- 并发写入需要使用随机写 API 或独立句柄，避免 seek 并发导致写入错位
- 每个分块独立请求，失败只影响单个分块
- 进度通过 `mpsc::unbounded_channel` 聚合

### 5. 智能重试 (`retry_strategy.rs`)

三层重试机制：

```rust
pub enum ErrorCategory {
    Temporary,           // 超时、连接重置 -> 可重试
    Permanent,           // 404、403 -> 不重试
    NetworkUnavailable,  // 网络不可用 -> 等待网络恢复
    RateLimited,         // 429/Retry-After
    SslError,            // SSL/TLS 握手失败 -> 可能需要用户干预
    DnsError,            // DNS 解析失败 -> 可重试（可能是临时 DNS 问题）
    DiskError,           // 磁盘空间不足/权限问题 -> 不重试，需用户干预
}

impl RetryStrategy {
    // 1. 错误分类
    pub fn categorize_error(error: &AppError) -> ErrorCategory {
        // 分析 reqwest::Error 和 AppError::Network
        match error {
            // HTTP 状态码分类
            AppError::HttpStatus(status) => match status.as_u16() {
                404 | 403 | 401 | 410 => ErrorCategory::Permanent,
                429 => ErrorCategory::RateLimited,
                500..=599 => ErrorCategory::Temporary,
                _ => ErrorCategory::Temporary,
            },
            // reqwest 错误分类
            AppError::Network(msg) => {
                if msg.contains("timeout") || msg.contains("connection reset") {
                    ErrorCategory::Temporary
                } else if msg.contains("certificate") || msg.contains("SSL") || msg.contains("TLS") {
                    ErrorCategory::SslError
                } else if msg.contains("dns") || msg.contains("resolve") {
                    ErrorCategory::DnsError
                } else {
                    ErrorCategory::NetworkUnavailable
                }
            },
            // IO 错误分类
            AppError::Io(io_err) => match io_err.kind() {
                std::io::ErrorKind::PermissionDenied => ErrorCategory::DiskError,
                std::io::ErrorKind::StorageFull => ErrorCategory::DiskError,
                _ => ErrorCategory::Temporary,
            },
            _ => ErrorCategory::Temporary,
        }
    }

    // 2. 重试判断
    pub fn should_retry(&mut self, error: &AppError) -> bool {
        match Self::categorize_error(error) {
            ErrorCategory::Permanent | ErrorCategory::DiskError => false,
            ErrorCategory::SslError => false,  // SSL 错误通常需要用户干预
            _ => self.current_retry < self.max_retries,
        }
    }

    // 3. 指数退避
    pub fn backoff_delay(&self) -> Duration {
        // 注意：Rust 中 ^ 是 XOR，不能用于指数
        // 推荐：base = 1 << current_retry，并增加 jitter 防止雪崩
        Duration::from_secs(1u64.saturating_shl(self.current_retry))
    }

    // 4. 镜像切换（仅 GitHub）
    pub fn try_switch_mirror(&mut self, url: &str) -> Option<String> {
        if url.contains("github.com") {
            // 必须复用现有镜像策略：services::network::get_github_download_url
            // 镜像来源受 config.setting.network.github_download_mirror 控制
            // 对 auto 负载均衡场景，可在一次重试中重新计算镜像 URL
        }
    }

    // 5. 网络检测
    pub async fn check_network_available() -> bool {
        // 尝试连接 8.8.8.8:53, 1.1.1.1:53, 223.5.5.5:53
        // 任意一个成功即可
    }
}
```

**重试流程**：
```
下载失败
  ↓
错误分类
  ├── Permanent/DiskError/SslError → 直接失败（返回明确错误信息）
  ├── NetworkUnavailable
  │     ↓
  │   网络检测循环（每 5 秒检测一次）
  │     ↓
  │   网络恢复 → 重试
  ├── DnsError → 等待 10 秒后重试（DNS 缓存刷新）
  └── Temporary/RateLimited
        ↓
      指数退避等待（RateLimited 优先使用 Retry-After 头）
        ↓
            GitHub URL? → 重新计算镜像 URL（遵循现有配置策略）
        ↓
      重试（最多 5 次）
```

### 6. 下载任务 (`rust_downloader.rs`)

核心数据结构：

```rust
pub struct RustDownloader {
    started: AtomicBool,
    active_tasks: RwLock<HashMap<String, Arc<DownloadTask>>>,
    client: Mutex<Option<Client>>,
}

pub struct DownloadTask {
    id: String,
    url: String,
    options: DownloadOptions,
    status: Arc<RwLock<DownloadStatus>>,
    progress: Arc<RwLock<ProgressInfo>>,
    cancel_token: CancellationToken,
    paused: Arc<AtomicBool>,
}

struct ProgressInfo {
    downloaded: u64,
    total: u64,
    speed: u64,
    last_update: Instant,
    last_downloaded: u64,
    // 滑动窗口用于平滑速度计算
    speed_samples: VecDeque<(Instant, u64)>,
}
```

**下载流程**：
1. `download()` - 创建任务，异步启动
2. `DownloadTask::start()` - 主下载逻辑
   - 加载或创建状态文件
   - 检测服务器支持（Range、文件大小）
    - 生成最终 URL/网络策略：复用 `services::network`（镜像/代理/UA）
   - 创建文件并预分配空间
   - 启动进度聚合任务
   - 循环：下载 → 失败重试 → 保存状态
3. `download_and_wait()` - 轮询进度，调用回调

**速度计算（滑动窗口平均）**：
```rust
const SPEED_WINDOW_SIZE: usize = 5;  // 保留最近 5 个采样点
const SPEED_SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

impl ProgressInfo {
    fn update_speed(&mut self, downloaded: u64) {
        let now = Instant::now();

        // 添加新采样点
        self.speed_samples.push_back((now, downloaded));

        // 移除过期采样点（超过窗口大小）
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
}
```

**ETA 计算**：
```rust
fn calculate_eta(&self) -> Option<u64> {
    if self.total == 0 || self.speed == 0 {
        return None;  // 未知长度或速度为 0 时无法计算
    }
    Some((self.total - self.downloaded) / self.speed)
}
```

**未知长度下载的降级处理**：
```rust
impl DownloadProgress {
    pub fn from_unknown_length(downloaded: u64, speed: u64, filename: &str, gid: &str) -> Self {
        Self {
            gid: gid.to_string(),
            downloaded,
            total: 0,           // 0 表示未知
            speed,
            percentage: -1.0,   // -1 表示无法计算百分比
            eta: u64::MAX,      // MAX 表示未知 ETA
            filename: filename.to_string(),
            status: DownloadStatus::Active,
        }
    }
}

// 前端显示建议：
// - percentage < 0 时显示 "未知" 或仅显示已下载字节数
// - eta == u64::MAX 时显示 "未知" 或 "--:--"
```

### 7. 全局管理器选择 (`mod.rs`)

```rust
static DOWNLOAD_MANAGER: OnceCell<Arc<dyn DownloadManager>> = OnceCell::new();

pub enum DownloadBackend {
    Auto,
    Aria2,
    Rust,
}

pub async fn init_download_manager(backend: DownloadBackend) -> AppResult<()> {
    // Auto：优先 aria2；若 aria2 启动失败/不可用则回退 RustDownloader
    // 注意：当前 Aria2Manager 会自动安装 aria2，但仍可能出现：启动失败、端口占用、ws 连接失败等
    // 这些都应触发 fallback
    ...
}

pub fn get_download_manager() -> AppResult<Arc<dyn DownloadManager>> {
    DOWNLOAD_MANAGER.get().cloned().ok_or(...)
}
```

**启动逻辑**：
```rust
// 在初始化代码中：读取 config.setting.download.backend
// - "auto"(默认)：init_download_manager(Auto)
// - "aria2"：强制 aria2
// - "rust"：强制 RustDownloader
init_download_manager(backend).await?;
```

### 7.1 Aria2Backend 健康检查

```rust
impl Aria2Backend {
    /// 定期检查 aria2 连接状态
    pub async fn health_check(&self) -> bool {
        // 1. 检查 WebSocket 连接是否存活
        // 2. 发送 aria2.getVersion() 验证响应
        // 3. 检查 aria2 进程是否存活（通过 PID）
    }

    /// WebSocket 断开时的重连逻辑
    async fn reconnect(&self) -> AppResult<()> {
        const MAX_RECONNECT_ATTEMPTS: u32 = 3;
        const RECONNECT_INTERVAL: Duration = Duration::from_secs(2);

        for attempt in 0..MAX_RECONNECT_ATTEMPTS {
            log::info!("Attempting to reconnect to aria2 (attempt {}/{})", attempt + 1, MAX_RECONNECT_ATTEMPTS);
            tokio::time::sleep(RECONNECT_INTERVAL).await;

            if self.try_connect().await.is_ok() {
                log::info!("Successfully reconnected to aria2");
                return Ok(());
            }
        }

        log::error!("Failed to reconnect to aria2 after {} attempts", MAX_RECONNECT_ATTEMPTS);
        Err(AppError::Aria2("Connection lost and reconnect failed".into()))
    }

    /// 检测 aria2 进程崩溃
    fn check_process_alive(&self) -> bool {
        if let Some(pid) = self.aria2_pid {
            // 使用 sysinfo 或平台特定 API 检查进程是否存活
            // Windows: OpenProcess + GetExitCodeProcess
            // Unix: kill(pid, 0)
        }
        false
    }
}
```

**Auto 模式下的故障转移**：
- 如果 aria2 健康检查失败且重连失败，自动切换到 RustDownloader
- 切换时记录日志，通知用户（可选）
- 已在进行中的下载任务需要妥善处理（等待完成或迁移）

### 8. 日志策略

关键日志点和级别：

```rust
// 日志级别定义：
// - ERROR: 不可恢复的错误
// - WARN: 可恢复的错误、重试、降级
// - INFO: 关键状态变化
// - DEBUG: 详细执行信息
// - TRACE: 非常详细的调试信息

// 下载生命周期日志
log::info!("Download started: url={}, filename={}, backend={}", url, filename, backend);
log::info!("Download completed: filename={}, size={}, duration={}s", filename, size, duration);
log::error!("Download failed: filename={}, error={}", filename, error);

// 重试相关日志
log::warn!("Download retry triggered: attempt={}/{}, reason={}", attempt, max_retries, reason);
log::warn!("Switching to mirror: original={}, mirror={}", original_url, mirror_url);

// 断点续传日志
log::info!("Resuming download: filename={}, progress={}/{}", filename, downloaded, total);
log::debug!("State file loaded: chunks={}, last_updated={}", chunks.len(), last_updated);

// 后端切换日志
log::warn!("Aria2 unavailable, falling back to RustDownloader: reason={}", reason);
log::info!("Download backend initialized: type={}", backend_type);

// 网络相关日志
log::debug!("Range support detected: supports_range={}, total_size={}", supports_range, total_size);
log::debug!("Using proxy: {}", proxy_url);

// 进度日志（TRACE 级别，避免刷屏）
log::trace!("Progress: {}% ({}/{}), speed={}/s", percentage, downloaded, total, speed);
```

## 关键文件路径

### 需要创建的文件

- `src-tauri/src/services/download/mod.rs` - 模块导出
- `src-tauri/src/services/download/manager.rs` - Trait 定义
- `src-tauri/src/services/download/types.rs` - 数据类型
- `src-tauri/src/services/download/client.rs` - HTTP Client 配置和构建
- `src-tauri/src/services/download/aria2_backend.rs` - Aria2 适配层
- `src-tauri/src/services/download/rust_downloader.rs` - 核心实现
- `src-tauri/src/services/download/chunk_manager.rs` - 分块管理
- `src-tauri/src/services/download/retry_strategy.rs` - 重试策略
- `src-tauri/src/services/download/state_store.rs` - 状态存储
- `src-tauri/src/services/download/filename.rs` - 文件名解析（Content-Disposition 等）

### 需要修改的文件

- `src-tauri/src/services/aria2.rs` - 保持对 aria2 的低层封装；由 `aria2_backend.rs` 组合/调用
- `src-tauri/src/services/mod.rs` - 添加 `pub mod download;`
- `src-tauri/Cargo.toml` - 添加依赖
- `src-tauri/src/services/firmware.rs` / `yuzu.rs` / `ryujinx.rs` / `msvc.rs` - 切换到统一接口
- `src-tauri/src/commands/common.rs` - `cancel_download_command` 切换到统一接口

## 依赖更新

在 `Cargo.toml` 中添加：

```toml
[dependencies]
# 现有依赖保持不变...

# 异步 trait 支持
async-trait = "0.1"

# 取消令牌
tokio-util = { version = "0.7", features = ["sync"] }

# UUID 生成（用于默认文件名）
uuid = { version = "1.0", features = ["v4"] }

# 现有依赖已包含：
# tokio (带 fs feature)
# reqwest
# parking_lot
# serde
# once_cell

[dev-dependencies]
# HTTP 服务器模拟（集成测试）
wiremock = "0.6"
```

## 实现步骤

### Phase 1: 基础框架（核心功能）

1. 创建 `download` 模块结构（含 `aria2_backend` 适配层）
2. 定义 `DownloadManager` trait 和统一数据类型
3. 在 `services/mod.rs` 导出 download 模块，并新增 `get_download_manager()`
4. 调整调用方：`firmware.rs`/`yuzu.rs`/`ryujinx.rs`/`msvc.rs` 改为使用统一接口与 types
5. 调整命令：`cancel_download_command` 改为 `download_manager.cancel_all(remove_files)`
6. 实现 `Aria2Backend`：组合现有 `Aria2Manager` 能力并映射到统一 types
7. 集成后端选择：增加配置项 `download.backend = auto|aria2|rust`（默认 auto）

**验证点**：能够完成单文件下载，显示进度

### Phase 2: 断点续传（增强功能）

1. 实现 `StateStore` - 状态文件读写（原子写 + 远端一致性校验）
2. 实现 `ChunkManager` - Range 探测改为 GET+Range=0-0，支持 unknown-length 降级
3. 实现多连接下载 + 安全并发写入（随机写或独立句柄，避免 seek 并发）
4. 集成断点续传逻辑（resume 时必须校验远端一致性）
5. 实现 pause/resume 的明确定义（暂停时停止调度/取消进行中请求、速度归零等）

**验证点**：下载中断后能够从断点继续

### Phase 3: 智能重试（完善功能）

1. 实现 `RetryStrategy` - 错误分类（含 HTTP 状态码、429/Retry-After）
2. 实现指数退避 + jitter
3. 实现网络感知
4. GitHub 镜像策略：复用 `services::network::get_github_download_url`（遵循现有配置）
5. 集成到下载主流程

**验证点**：网络故障自动恢复，GitHub 下载自动切换镜像

### Phase 4: 测试和优化

#### 4.1 单元测试

```rust
// retry_strategy.rs
#[cfg(test)]
mod tests {
    #[test]
    fn test_categorize_http_404() {
        let error = AppError::HttpStatus(StatusCode::NOT_FOUND);
        assert_eq!(RetryStrategy::categorize_error(&error), ErrorCategory::Permanent);
    }

    #[test]
    fn test_categorize_http_429() {
        let error = AppError::HttpStatus(StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(RetryStrategy::categorize_error(&error), ErrorCategory::RateLimited);
    }

    #[test]
    fn test_backoff_delay_with_jitter() {
        let strategy = RetryStrategy::new(5);
        let delay1 = strategy.backoff_delay();
        let delay2 = strategy.backoff_delay();
        // 由于 jitter，两次延迟应该不完全相同
    }
}

// chunk_manager.rs
#[cfg(test)]
mod tests {
    #[test]
    fn test_calculate_chunks_small_file() {
        let manager = ChunkManager::new(4, "4M".parse().unwrap());
        let chunks = manager.calculate_chunks(1024 * 1024, true); // 1MB
        assert_eq!(chunks.len(), 1); // 小于 min_split_size，单连接
    }

    #[test]
    fn test_calculate_chunks_large_file() {
        let manager = ChunkManager::new(4, "4M".parse().unwrap());
        let chunks = manager.calculate_chunks(100 * 1024 * 1024, true); // 100MB
        assert_eq!(chunks.len(), 4);
    }

    #[test]
    fn test_calculate_chunks_no_range_support() {
        let manager = ChunkManager::new(4, "4M".parse().unwrap());
        let chunks = manager.calculate_chunks(100 * 1024 * 1024, false);
        assert_eq!(chunks.len(), 1); // 不支持 Range，单连接
    }
}

// state_store.rs
#[cfg(test)]
mod tests {
    #[test]
    fn test_state_serialization_roundtrip() {
        let state = DownloadState { ... };
        let json = serde_json::to_string(&state).unwrap();
        let restored: DownloadState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, restored);
    }

    #[test]
    fn test_atomic_write() {
        // 验证原子写入不会产生损坏的中间状态
    }
}
```

#### 4.2 集成测试

使用 `wiremock` 模拟 HTTP 服务器：

```rust
// tests/download_integration.rs
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header};

#[tokio::test]
async fn test_download_with_range_support() {
    let mock_server = MockServer::start().await;

    // 模拟支持 Range 的服务器
    Mock::given(method("GET"))
        .and(path("/file.zip"))
        .and(header("Range", "bytes=0-0"))
        .respond_with(ResponseTemplate::new(206)
            .insert_header("Content-Range", "bytes 0-0/1000000")
            .body("x"))
        .mount(&mock_server)
        .await;

    // 测试下载逻辑
}

#[tokio::test]
async fn test_download_without_range_support() {
    let mock_server = MockServer::start().await;

    // 模拟不支持 Range 的服务器（返回 200 而非 206）
    Mock::given(method("GET"))
        .and(path("/file.zip"))
        .respond_with(ResponseTemplate::new(200)
            .insert_header("Content-Length", "1000000")
            .body(vec![0u8; 1000000]))
        .mount(&mock_server)
        .await;

    // 验证降级为单连接下载
}

#[tokio::test]
async fn test_retry_on_429() {
    let mock_server = MockServer::start().await;

    // 第一次返回 429，第二次成功
    Mock::given(method("GET"))
        .and(path("/file.zip"))
        .respond_with(ResponseTemplate::new(429)
            .insert_header("Retry-After", "1"))
        .up_to_n_times(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/file.zip"))
        .respond_with(ResponseTemplate::new(200)
            .body("content"))
        .mount(&mock_server)
        .await;

    // 验证重试成功
}

#[tokio::test]
async fn test_resume_interrupted_download() {
    // 1. 开始下载
    // 2. 模拟中断（取消 token）
    // 3. 验证状态文件已保存
    // 4. 重新开始下载
    // 5. 验证从断点继续
}

#[tokio::test]
async fn test_content_disposition_filename() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200)
            .insert_header("Content-Disposition", "attachment; filename=\"测试文件.zip\"")
            .body("content"))
        .mount(&mock_server)
        .await;

    // 验证文件名正确解析
}
```

#### 4.3 手动测试场景

| 场景 | 测试方法 | 预期结果 |
|------|----------|----------|
| 大文件下载 (>1GB) | 下载 Linux ISO 或类似大文件 | 多连接下载，进度正常，完成后文件完整 |
| 慢速网络 | 使用 tc 或代理限速 | 速度显示正确，ETA 合理 |
| 网络中断恢复 | 下载中断开网络，等待后恢复 | 自动检测网络恢复并继续 |
| 代理环境 | 配置 HTTP/SOCKS5 代理 | 通过代理正常下载 |
| 不支持 Range | 下载某些 CDN 资源 | 降级为单连接，正常完成 |
| 进程崩溃恢复 | 下载中 kill 进程，重启 | 从断点继续，不重新下载 |
| 并发下载 | 同时启动多个下载任务 | 各任务独立进行，互不影响 |
| 磁盘空间不足 | 在剩余空间不足的分区下载 | 提前检测并报错，不产生损坏文件 |

#### 4.4 性能测试

```rust
#[tokio::test]
async fn bench_download_speed() {
    // 使用本地服务器测试最大下载速度
    // 对比 aria2 和 RustDownloader 的性能差异
}

#[tokio::test]
async fn bench_memory_usage() {
    // 监控大文件下载时的内存占用
    // 确保不会因为缓冲区过大导致 OOM
}
```

**验证点**：
- 各种下载场景正常工作
- 断点续传可靠
- 重试逻辑正确
- 性能与 aria2 相当（或在可接受范围内）

## 兼容性说明

### 方案 A 的兼容性取舍

方案 A 的核心是“统一入口”，因此需要修改现有调用点（`firmware.rs`, `yuzu.rs`, `ryujinx.rs`, `msvc.rs`, `commands/common.rs`）。

迁移后的调用形式：

```rust
use crate::services::download::{get_download_manager, DownloadOptions};

let manager = get_download_manager()?;
let result = manager
    .download_and_wait(url, DownloadOptions::default(), |progress| {
        // 进度回调
    })
    .await?;
```

### 配置兼容

- `DownloadOptions` 与 `Aria2DownloadOptions` 字段完全一致
- `DownloadProgress` 与 `Aria2DownloadProgress` 字段完全一致
- 前端无需任何修改
- 增加新配置项：`download.backend = auto|aria2|rust`（默认 auto）

## 性能优化建议

1. **缓冲区大小**：每个分块使用 64KB 缓冲区读取
2. **并发控制**：限制最多 16 个并发分块
3. **进度更新频率**：最多每 100ms 更新一次进度
4. **状态保存频率**：最多每 5 秒保存一次状态
5. **文件预分配**：使用 `set_len()` 预分配文件空间避免碎片

## 风险和限制

1. **不支持 BitTorrent**：aria2 支持 BT，纯 Rust 实现不支持（当前项目不需要）
2. **不支持 Metalink**：aria2 支持，纯 Rust 实现不支持（当前项目不需要）
3. **磁盘空间检查**：需要在下载前检查磁盘空间是否足够
4. **并发文件写入**：需要确保 tokio 文件 IO 的线程安全

补充风险：

5. **重定向与内容变化**：同一 URL 可能指向不同内容，必须用 ETag/Last-Modified/Length 校验断点状态
6. **未知长度下载**：无 Content-Length 时无法精确百分比/ETA，需降级策略
7. **跨平台文件名**：Windows 文件名有特殊字符限制，需要清理非法字符

### 磁盘空间检查实现

```rust
use std::path::Path;

#[cfg(unix)]
fn get_available_space(path: &Path) -> AppResult<u64> {
    use std::os::unix::fs::MetadataExt;
    let stat = nix::sys::statvfs::statvfs(path)?;
    Ok(stat.blocks_available() * stat.block_size())
}

#[cfg(windows)]
fn get_available_space(path: &Path) -> AppResult<u64> {
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    // 调用 Windows API 获取可用空间
}

/// 下载前检查磁盘空间
pub fn check_disk_space(save_dir: &Path, required_size: u64) -> AppResult<()> {
    let available = get_available_space(save_dir)?;
    // 预留 10% 或 100MB 的缓冲空间
    let buffer = std::cmp::max(required_size / 10, 100 * 1024 * 1024);

    if available < required_size + buffer {
        return Err(AppError::DiskSpace(format!(
            "Insufficient disk space: required {}MB, available {}MB",
            (required_size + buffer) / 1024 / 1024,
            available / 1024 / 1024
        )));
    }
    Ok(())
}
```

### 文件名清理

```rust
/// 清理文件名中的非法字符
pub fn sanitize_filename(filename: &str) -> String {
    let mut result = filename.to_string();

    // Windows 非法字符
    #[cfg(windows)]
    {
        const ILLEGAL_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
        for c in ILLEGAL_CHARS {
            result = result.replace(*c, "_");
        }
        // Windows 保留名称
        const RESERVED_NAMES: &[&str] = &[
            "CON", "PRN", "AUX", "NUL",
            "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
            "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];
        let upper = result.to_uppercase();
        for name in RESERVED_NAMES {
            if upper == *name || upper.starts_with(&format!("{}.", name)) {
                result = format!("_{}", result);
                break;
            }
        }
    }

    // 通用清理
    result = result.trim().to_string();
    if result.is_empty() {
        result = "download".to_string();
    }

    // 限制长度（考虑 .part 和 .download 后缀）
    const MAX_FILENAME_LEN: usize = 200;
    if result.len() > MAX_FILENAME_LEN {
        let ext = Path::new(&result)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let stem_max = MAX_FILENAME_LEN - ext.len() - 1;
        let stem = &result[..stem_max];
        result = if ext.is_empty() {
            stem.to_string()
        } else {
            format!("{}.{}", stem, ext)
        };
    }

    result
}
```

## 后续扩展

1. **SHA256 校验**：下载完成后校验文件完整性
2. **限速功能**：添加下载速度限制选项
3. **HTTP/2 支持**：reqwest 默认支持，可以优化性能
4. **更智能的分块**：根据网络速度动态调整分块大小

## 总结

这个实现计划提供了一个完整的、生产级的下载器体系：以 `services::download` 作为统一入口，aria2 与 RustDownloader 作为可替换后端。方案 A 的收益是长期可维护、行为一致、可控回退；代价是需要一次性迁移现有下载调用点。

- ✅ 自适应断点续传（单连接/多连接）
- ✅ 智能重试（错误分类、网络感知、指数退避、镜像切换）
- ✅ 完整的进度跟踪（速度、ETA、百分比）
- ✅ 前端无需修改（数据结构与序列化保持一致）
- ✅ 业务下载调用统一迁移到 `services::download`

实现优先级按 Phase 1-4 分阶段进行，每个阶段都有明确的验证点，确保稳步推进。

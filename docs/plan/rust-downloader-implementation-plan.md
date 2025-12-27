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

### 3. 断点续传 (`state_store.rs`)

使用 JSON 状态文件（`.download`）持久化下载状态：

```rust
pub struct DownloadState {
    pub url: String,
    pub resolved_url: Option<String>,
    pub total_size: u64,
    pub supports_range: bool,     // 是否支持 Range
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub chunks: Vec<ChunkState>,  // 分块信息
    pub last_updated: i64,
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
}

impl RetryStrategy {
    // 1. 错误分类
    pub fn categorize_error(error: &AppError) -> ErrorCategory {
        // 分析 reqwest::Error 和 AppError::Network
        // 根据错误消息判断类型
    }

    // 2. 重试判断
    pub fn should_retry(&mut self, error: &AppError) -> bool {
        match Self::categorize_error(error) {
            ErrorCategory::Permanent => false,
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
  ├── Permanent → 直接失败
  ├── NetworkUnavailable
  │     ↓
  │   网络检测循环（每 5 秒检测一次）
  │     ↓
  │   网络恢复 → 重试
  └── Temporary
        ↓
      指数退避等待
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

**速度计算**：
```rust
// 每秒更新一次
if elapsed >= 1.0 {
    speed = (downloaded - last_downloaded) / elapsed;
    last_update = now;
    last_downloaded = downloaded;
}
```

**ETA 计算**：
```rust
eta = (total - downloaded) / speed  // 秒
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

## 关键文件路径

### 需要创建的文件

- `src-tauri/src/services/download/mod.rs` - 模块导出
- `src-tauri/src/services/download/manager.rs` - Trait 定义
- `src-tauri/src/services/download/types.rs` - 数据类型
- `src-tauri/src/services/download/aria2_backend.rs` - Aria2 适配层
- `src-tauri/src/services/download/rust_downloader.rs` - 核心实现
- `src-tauri/src/services/download/chunk_manager.rs` - 分块管理
- `src-tauri/src/services/download/retry_strategy.rs` - 重试策略
- `src-tauri/src/services/download/state_store.rs` - 状态存储

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

# 现有依赖已包含：
# tokio (带 fs feature)
# reqwest
# parking_lot
# serde
# once_cell
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

1. 测试各种下载场景（大文件、小文件、不支持 Range）
2. 测试断点续传
3. 测试重试逻辑
4. 性能优化（并发控制、缓冲区大小）
5. 错误处理完善

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

# NS Emu Tools: Rust + Tauri 重构规划

## 1. 项目概述

### 1.1 重构目标

将当前基于 Python + Eel + Vue.js 的 NS Emu Tools 重构为 Rust + Tauri + Vue.js 架构，以实现：

- **更小的打包体积**：Tauri 应用通常只有几 MB，而当前 PyInstaller 打包后超过 100MB
- **更快的启动速度**：Rust 原生编译，无需 Python 运行时
- **更好的跨平台支持**：Tauri 原生支持 Windows、macOS、Linux
- **更高的性能**：Rust 的零开销抽象和内存安全
- **更好的安全性**：Tauri 的 IPC 安全模型和 Rust 的内存安全保证

### 1.2 当前架构概览

```
┌─────────────────────────────────────────────────────────────┐
│                   当前架构 (Python + Eel)                     │
├─────────────────────────────────────────────────────────────┤
│  Frontend: Vue 3 + Vuetify 3 + Pinia + TypeScript           │
│                        ↕ Eel Bridge                          │
│  Backend: Python 3.11+                                       │
│    - api/ (Eel exposed functions)                           │
│    - module/ (业务逻辑)                                       │
│    - repository/ (数据访问)                                   │
│    - utils/ (工具函数)                                        │
│    - config.py / storage.py (配置管理)                       │
│  External: aria2 (下载引擎)                                   │
└─────────────────────────────────────────────────────────────┘
```

### 1.3 目标架构

```
┌─────────────────────────────────────────────────────────────┐
│                   目标架构 (Rust + Tauri)                     │
├─────────────────────────────────────────────────────────────┤
│  Frontend: Vue 3 + Vuetify 3 + Pinia + TypeScript           │
│                        ↕ Tauri IPC                           │
│  Backend: Rust                                               │
│    - commands/ (Tauri commands)                              │
│    - services/ (业务逻辑)                                     │
│    - repositories/ (数据访问)                                 │
│    - utils/ (工具函数)                                        │
│    - config.rs (配置管理)                                     │
│  External: reqwest (HTTP) + tokio (异步运行时)                │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. 模块映射与重构计划

### 2.1 API 层重构 (api/ → commands/)

| Python 模块 | Rust 模块 | 说明 |
|------------|-----------|------|
| `api/common_api.py` | `commands/common.rs` | 通用 API 命令 |
| `api/yuzu_api.py` | `commands/yuzu.rs` | Yuzu/Eden/Citron 管理命令 |
| `api/ryujinx_api.py` | `commands/ryujinx.rs` | Ryujinx 管理命令 |
| `api/cheats_api.py` | `commands/cheats.rs` | 金手指管理命令 |
| `api/save_manager_api.py` | `commands/save_manager.rs` | 存档管理命令 |
| `api/updater_api.py` | `commands/updater.rs` | 程序更新命令 |
| `api/common_response.py` | `utils/response.rs` | 统一响应格式 |

#### Tauri Command 示例

```rust
// src-tauri/src/commands/yuzu.rs
use tauri::command;
use crate::services::yuzu::YuzuService;
use crate::utils::response::ApiResponse;

#[command]
pub async fn get_yuzu_config() -> Result<ApiResponse<YuzuConfig>, String> {
    let service = YuzuService::new();
    match service.get_config().await {
        Ok(config) => Ok(ApiResponse::success(config)),
        Err(e) => Err(e.to_string())
    }
}

#[command]
pub async fn install_yuzu(
    version: String,
    branch: String,
    window: tauri::Window
) -> Result<ApiResponse<()>, String> {
    let service = YuzuService::new();
    
    // 发送进度事件到前端
    service.install(&version, &branch, |progress| {
        window.emit("install-progress", progress).ok();
    }).await?;
    
    Ok(ApiResponse::success(()))
}
```

### 2.2 业务逻辑层重构 (module/ → services/)

| Python 模块 | Rust 模块 | 关键依赖替换 |
|------------|-----------|-------------|
| `module/yuzu.py` | `services/yuzu.rs` | 文件操作: `std::fs`, 进程: `std::process` |
| `module/ryujinx.py` | `services/ryujinx.rs` | 同上 |
| `module/firmware.py` | `services/firmware.rs` | NCA 解析需自行实现或使用相关 crate |
| `module/downloader.py` | `services/downloader.rs` | `reqwest` 替代 aria2 |
| `module/network.py` | `services/network.rs` | `reqwest` + `trust-dns-resolver` |
| `module/save_manager.py` | `services/save_manager.rs` | `std::fs` + `sevenz-rust` |
| `module/updater.py` | `services/updater.rs` | `tauri::updater` |
| `module/common.py` | `services/common.rs` | 标准库 |
| `module/msg_notifier.py` | `services/notifier.rs` | Tauri events |
| `module/hosts.py` | `services/hosts.rs` | 标准库 |
| `module/cheats/` | `services/cheats/` | 解析逻辑直接移植 |
| `module/nsz_wrapper.py` | `services/nca_parser.rs` | 需要实现 NCA 解析 |
| `module/dialogs.py` | Tauri 原生对话框 | `tauri::api::dialog` |
| `module/sentry.py` | `services/sentry.rs` | `sentry` crate |

#### 下载器重构方案

Python 版本使用 aria2 作为下载引擎，Rust 版本建议使用 `reqwest` + 自定义进度追踪：

```rust
// src-tauri/src/services/downloader.rs
use reqwest::Client;
use tokio::io::AsyncWriteExt;
use futures_util::StreamExt;

pub struct Downloader {
    client: Client,
}

impl Downloader {
    pub async fn download<F>(
        &self,
        url: &str,
        save_path: &Path,
        on_progress: F
    ) -> Result<(), DownloadError>
    where
        F: Fn(DownloadProgress) + Send + 'static,
    {
        let response = self.client.get(url).send().await?;
        let total_size = response.content_length().unwrap_or(0);
        
        let mut file = tokio::fs::File::create(save_path).await?;
        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();
        
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            
            on_progress(DownloadProgress {
                downloaded,
                total: total_size,
                speed: 0, // 计算速度
            });
        }
        
        Ok(())
    }
}
```

#### 多线程下载支持（可选）

如果需要 aria2 的多线程下载能力，可以考虑：
1. 保留 aria2 作为外部依赖
2. 使用 Rust 实现分段下载
3. 使用 `aria2c` 命令行工具通过 `std::process::Command`

### 2.3 数据访问层重构 (repository/ → repositories/)

| Python 模块 | Rust 模块 | HTTP 客户端 |
|------------|-----------|-------------|
| `repository/yuzu.py` | `repositories/yuzu.rs` | `reqwest` |
| `repository/ryujinx.py` | `repositories/ryujinx.rs` | `reqwest` |
| `repository/my_info.py` | `repositories/app_info.rs` | `reqwest` |
| `repository/domain/release_info.py` | `models/release.rs` | `serde` |

#### 数据模型示例

```rust
// src-tauri/src/models/release.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub download_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseInfo {
    pub name: String,
    pub tag_name: String,
    pub description: String,
    pub assets: Vec<ReleaseAsset>,
}

impl ReleaseInfo {
    pub fn from_github_api(data: &serde_json::Value) -> Result<Self, ParseError> {
        Ok(Self {
            name: data["name"].as_str().unwrap_or_default().to_string(),
            tag_name: data["tag_name"].as_str().unwrap_or_default().to_string(),
            description: data["body"].as_str().unwrap_or_default().to_string(),
            assets: data["assets"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|a| {
                    Some(ReleaseAsset {
                        name: a["name"].as_str()?.to_string(),
                        download_url: a["browser_download_url"].as_str()?.to_string(),
                    })
                })
                .collect(),
        })
    }
    
    pub fn from_gitlab_api(data: &serde_json::Value) -> Result<Self, ParseError> {
        // GitLab API 格式解析
        todo!()
    }
    
    pub fn from_forgejo_api(data: &serde_json::Value) -> Result<Self, ParseError> {
        // Forgejo API 格式解析
        todo!()
    }
}
```

### 2.4 工具层重构 (utils/ → utils/)

| Python 模块 | Rust 模块 | 实现方案 |
|------------|-----------|----------|
| `utils/admin.py` | `utils/admin.rs` | Windows: `windows-rs`, Unix: `nix` |
| `utils/common.py` | `utils/common.rs` | 标准库 + `sysinfo` |
| `utils/doh.py` | `utils/doh.rs` | `trust-dns-resolver` |
| `utils/hardware.py` | `utils/hardware.rs` | `sysinfo` |
| `utils/package.py` | `utils/archive.rs` | `zip`, `sevenz-rust`, `tar` |
| `utils/string_util.py` | `utils/string.rs` | `encoding_rs` |
| `utils/webview2.py` | 不需要 | Tauri 自带 WebView2 检测 |

### 2.5 配置管理重构

| Python 模块 | Rust 模块 | 序列化方案 |
|------------|-----------|-----------|
| `config.py` | `config.rs` | `serde` + `serde_json` |
| `storage.py` | `storage.rs` | `serde` + `serde_json` |

#### 配置结构示例

```rust
// src-tauri/src/config.rs
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YuzuConfig {
    pub yuzu_path: PathBuf,
    pub yuzu_version: Option<String>,
    pub yuzu_firmware: Option<String>,
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RyujinxConfig {
    pub path: PathBuf,
    pub version: Option<String>,
    pub firmware: Option<String>,
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSetting {
    pub firmware_download_source: String,
    pub github_api_mode: String,
    pub github_download_mirror: String,
    pub use_doh: bool,
    pub proxy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub yuzu: YuzuConfig,
    pub ryujinx: RyujinxConfig,
    pub network: NetworkSetting,
    // ... 其他配置
}

// 全局配置管理
lazy_static::lazy_static! {
    pub static ref CONFIG: RwLock<Config> = RwLock::new(Config::load().unwrap_or_default());
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        let config_path = Self::config_path();
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }
    
    pub fn save(&self) -> Result<(), ConfigError> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(Self::config_path(), content)?;
        Ok(())
    }
}
```

---

## 3. 前端改动

### 3.1 通信层改动

当前前端通过 Eel 与 Python 后端通信：

```typescript
// 当前方式 (Eel)
const config = await window.eel.get_config()();
```

需要改为 Tauri invoke：

```typescript
// 目标方式 (Tauri)
import { invoke } from '@tauri-apps/api/tauri';

const config = await invoke<Config>('get_config');
```

### 3.2 事件监听改动

当前的消息通知机制：

```typescript
// 当前方式 (Eel expose)
window.eel.expose(handleMessage, 'update_message');
```

需要改为 Tauri 事件监听：

```typescript
// 目标方式 (Tauri)
import { listen } from '@tauri-apps/api/event';

const unlisten = await listen<ProgressPayload>('install-progress', (event) => {
    console.log('Progress:', event.payload);
});
```

### 3.3 类型定义更新

需要创建与 Rust 后端对应的 TypeScript 类型定义：

```typescript
// src/types/api.ts
export interface Config {
    yuzu: YuzuConfig;
    ryujinx: RyujinxConfig;
    // ...
}

export interface ApiResponse<T> {
    code: number;
    data?: T;
    msg?: string;
}

// src/types/events.ts
export interface ProgressPayload {
    downloaded: number;
    total: number;
    speed: number;
}
```

### 3.4 对话框 API 变更

```typescript
// 当前方式 (可能通过 Python)
await window.eel.ask_folder()();

// 目标方式 (Tauri)
import { open } from '@tauri-apps/api/dialog';

const selected = await open({
    directory: true,
    multiple: false,
});
```

---

## 4. Rust Crate 依赖规划

### 4.1 核心依赖

```toml
# src-tauri/Cargo.toml
[dependencies]
# Tauri 框架
tauri = { version = "2.0", features = ["dialog", "shell", "updater", "fs"] }

# 异步运行时
tokio = { version = "1", features = ["full"] }

# HTTP 客户端
reqwest = { version = "0.12", features = ["json", "stream"] }

# 序列化
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# 错误处理
thiserror = "1.0"
anyhow = "1.0"

# 日志
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# 压缩/解压
zip = "2.1"
sevenz-rust = "0.6"
tar = "0.4"
xz2 = "0.1"

# 文件系统
walkdir = "2.5"
directories = "5.0"

# 进程管理
sysinfo = "0.31"

# DNS over HTTPS
trust-dns-resolver = { version = "0.24", features = ["dns-over-https"] }

# 全局状态
lazy_static = "1.5"
once_cell = "1.19"

# 异步流
futures-util = "0.3"

# 时间处理
chrono = { version = "0.4", features = ["serde"] }

# 正则表达式
regex = "1.10"
```

### 4.2 平台特定依赖

```toml
[target.'cfg(windows)'.dependencies]
# Windows API
windows = { version = "0.58", features = [
    "Win32_System_Registry",
    "Win32_UI_WindowsAndMessaging",
    "Win32_System_Threading",
    "Win32_Security",
] }

[target.'cfg(unix)'.dependencies]
nix = { version = "0.29", features = ["user"] }
```

### 4.3 可选依赖

```toml
# 错误追踪 (可选)
sentry = { version = "0.34", optional = true }

# NCA 解析 (可能需要自行实现)
# 或者使用 FFI 调用 nsz Python 库

[features]
default = []
sentry = ["dep:sentry"]
```

---

## 5. 项目结构规划

### 5.1 Rust 后端结构

```
src-tauri/
├── Cargo.toml
├── tauri.conf.json
├── build.rs
├── src/
│   ├── main.rs                 # 应用入口
│   ├── lib.rs                  # 库入口
│   ├── error.rs                # 错误类型定义
│   │
│   ├── commands/               # Tauri 命令 (对应 api/)
│   │   ├── mod.rs
│   │   ├── common.rs
│   │   ├── yuzu.rs
│   │   ├── ryujinx.rs
│   │   ├── cheats.rs
│   │   ├── save_manager.rs
│   │   └── updater.rs
│   │
│   ├── services/               # 业务逻辑 (对应 module/)
│   │   ├── mod.rs
│   │   ├── yuzu.rs
│   │   ├── ryujinx.rs
│   │   ├── firmware.rs
│   │   ├── downloader.rs
│   │   ├── network.rs
│   │   ├── save_manager.rs
│   │   ├── updater.rs
│   │   ├── notifier.rs
│   │   └── cheats/
│   │       ├── mod.rs
│   │       ├── parser.rs
│   │       └── types.rs
│   │
│   ├── repositories/           # 数据访问 (对应 repository/)
│   │   ├── mod.rs
│   │   ├── yuzu.rs
│   │   ├── ryujinx.rs
│   │   └── app_info.rs
│   │
│   ├── models/                 # 数据模型
│   │   ├── mod.rs
│   │   ├── release.rs
│   │   ├── config.rs
│   │   └── storage.rs
│   │
│   ├── utils/                  # 工具函数 (对应 utils/)
│   │   ├── mod.rs
│   │   ├── admin.rs
│   │   ├── archive.rs
│   │   ├── doh.rs
│   │   ├── hardware.rs
│   │   └── common.rs
│   │
│   └── config.rs               # 配置管理
│
├── icons/                      # 应用图标
└── resources/                  # 打包资源
```

### 5.2 前端结构 (基本保持不变)

```
frontend/
├── src/
│   ├── components/
│   ├── layouts/
│   ├── pages/
│   ├── stores/
│   ├── plugins/
│   ├── router/
│   ├── types/                  # 需要更新 TypeScript 类型
│   │   ├── api.ts              # API 响应类型
│   │   ├── config.ts           # 配置类型
│   │   └── events.ts           # Tauri 事件类型
│   └── utils/
│       ├── common.ts
│       ├── markdown.ts
│       └── tauri.ts            # 新增: Tauri API 封装
├── package.json
└── vite.config.ts
```

---

## 6. 迁移策略

### 6.1 阶段一：基础架构搭建 (1-2 周)

1. **初始化 Tauri 项目**
   - 创建 `src-tauri` 目录
   - 配置 `tauri.conf.json`
   - 设置开发环境

2. **建立基础模块**
   - 创建 error.rs 统一错误处理
   - 实现 config.rs 配置管理
   - 创建 models/ 数据模型

3. **前端适配**
   - 添加 `@tauri-apps/api` 依赖
   - 创建 Tauri API 封装层
   - 更新类型定义

### 6.2 阶段二：核心功能迁移 (2-3 周)

1. **网络层**
   - 实现 downloader.rs (替代 aria2)
   - 实现 network.rs (HTTP 请求、代理、镜像)
   - 实现 doh.rs (DNS over HTTPS)

2. **数据访问层**
   - 实现 repositories/yuzu.rs
   - 实现 repositories/ryujinx.rs
   - 实现 repositories/app_info.rs

3. **基础服务**
   - 实现 services/firmware.rs
   - 实现 archive.rs (解压缩)

### 6.3 阶段三：模拟器管理迁移 (2-3 周)

1. **Yuzu 系列管理**
   - 实现 services/yuzu.rs
   - 实现 commands/yuzu.rs
   - 测试 Eden/Citron 安装更新

2. **Ryujinx 管理**
   - 实现 services/ryujinx.rs
   - 实现 commands/ryujinx.rs
   - 测试 mainline/canary 分支

### 6.4 阶段四：辅助功能迁移 (1-2 周)

1. **金手指管理**
   - 实现 services/cheats/parser.rs
   - 实现 commands/cheats.rs

2. **存档管理**
   - 实现 services/save_manager.rs
   - 实现 commands/save_manager.rs

3. **自动更新**
   - 配置 Tauri 内置更新器
   - 实现 commands/updater.rs

### 6.5 阶段五：完善与测试 (1-2 周)

1. **功能完善**
   - 实现所有工具函数
   - 完善错误处理
   - 添加日志系统

2. **测试与调试**
   - 功能测试
   - 性能测试
   - 跨平台测试

3. **打包发布**
   - 配置 CI/CD
   - 生成安装包
   - 发布测试版

---

## 7. 技术挑战与解决方案

### 7.1 NCA 文件解析

**问题**：当前使用 Python 的 nsz 库解析 NCA 文件获取固件版本。

**解决方案**：
1. **方案一**：用 Rust 重新实现 NCA 解析逻辑
2. **方案二**：通过 FFI 调用 Python nsz 库
3. **方案三**：使用现有的 Rust NCA 解析库（如果有）
4. **方案四**：简化为读取已知位置的版本文件

**建议**：优先采用方案一，NCA 头部解析相对简单，可以参考 nsz 源码实现。

### 7.2 aria2 多线程下载

**问题**：aria2 提供强大的多线程断点续传功能。

**解决方案**：
1. **方案一**：使用 reqwest 实现简单的单线程下载（大多数情况足够）
2. **方案二**：保留 aria2，通过 `std::process::Command` 调用
3. **方案三**：使用 Rust 实现多线程分段下载

**建议**：初期采用方案一，后续根据需求升级。

### 7.3 进程管理

**问题**：需要检测运行中的模拟器进程并终止它们。

**解决方案**：使用 `sysinfo` crate：

```rust
use sysinfo::{ProcessExt, System, SystemExt};

pub fn kill_process_by_name(name: &str) -> Result<(), ProcessError> {
    let mut system = System::new_all();
    system.refresh_all();
    
    for (pid, process) in system.processes() {
        if process.name() == name {
            process.kill();
        }
    }
    
    Ok(())
}
```

### 7.4 Windows 注册表访问

**问题**：需要读取已安装软件列表。

**解决方案**：使用 `windows-rs` crate：

```rust
use windows::Win32::System::Registry::*;

pub fn get_installed_software() -> Result<Vec<SoftwareInfo>, RegistryError> {
    // 使用 Windows API 读取注册表
    todo!()
}
```

### 7.5 管理员权限提升

**问题**：某些操作需要管理员权限。

**解决方案**：
1. Tauri 配置中设置 `allowlist.shell.execute`
2. 使用 `runas` 或 ShellExecuteEx 提权

---

## 8. 性能对比预期

| 指标 | Python + Eel | Rust + Tauri |
|-----|-------------|--------------|
| 打包体积 | ~100MB | ~10-20MB |
| 启动时间 | 2-5 秒 | <1 秒 |
| 内存占用 | 80-150MB | 30-50MB |
| CPU 占用 | 中等 | 低 |
| 跨平台 | Windows 为主 | 全平台原生 |

---

## 9. 风险评估

### 9.1 高风险

- **NCA 解析**：需要重新实现核心解析逻辑
- **学习曲线**：团队需要熟悉 Rust 和 Tauri

### 9.2 中风险

- **功能完整性**：确保所有 Python 功能都能在 Rust 中实现
- **测试覆盖**：需要为 Rust 代码编写测试

### 9.3 低风险

- **前端改动**：通信层改动相对简单
- **配置兼容**：JSON 格式配置可以保持兼容

---

## 10. 建议的开发顺序

1. **先易后难**：先实现简单的工具函数，再处理复杂业务逻辑
2. **保持兼容**：配置文件格式保持与 Python 版本兼容
3. **逐步迁移**：可以先实现部分功能，Python 和 Rust 版本并行一段时间
4. **持续测试**：每完成一个模块就进行测试

---

## 11. 参考资源

### 11.1 官方文档

- [Tauri 官方文档](https://tauri.app/v2/guides/)
- [Rust 官方文档](https://doc.rust-lang.org/)
- [Serde 文档](https://serde.rs/)

### 11.2 相关 Crates

- [reqwest](https://docs.rs/reqwest/) - HTTP 客户端
- [tokio](https://tokio.rs/) - 异步运行时
- [sysinfo](https://docs.rs/sysinfo/) - 系统信息
- [windows-rs](https://docs.rs/windows/) - Windows API

### 11.3 示例项目

- [Tauri + Vue 模板](https://github.com/tauri-apps/tauri-vite-template)
- [Tauri 官方示例](https://github.com/tauri-apps/tauri/tree/dev/examples)

---

## 12. 总结

将 NS Emu Tools 从 Python + Eel 重构为 Rust + Tauri 是一个有价值的技术升级，可以显著提升应用性能和用户体验。重构需要约 2-3 个月的开发时间，建议采用渐进式迁移策略，优先实现核心功能，逐步完善辅助功能。

关键成功因素：
1. 充分的前期规划和架构设计
2. 对 Rust 生态的熟悉
3. 完善的测试策略
4. 与现有用户的良好沟通

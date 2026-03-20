# Dev

## 当前技术栈

项目现在已经从旧的 Python/Eel 方案切换到 **Rust + Tauri 2**：

- 桌面端后端：Rust（`src-tauri`）
- 前端：Vue 3 + Vite + Vuetify（`frontend`）
- 前端构建产物：`web`

本文档中的开发流程以当前的 Rust/Tauri 架构为准，旧的 Python 启动方式已经不再适用。

## 开发环境需求

- Rust stable 工具链（`rust-version = 1.70` 及以上）
- Tauri CLI
- Bun
- Node.js 20+

如果你在 Windows 上开发，通常还需要：

- Visual Studio C++ Build Tools（MSVC）
- WebView2 Runtime

## 环境准备

### Step 1 安装前端依赖

```shell
cd frontend
bun install
```

### Step 2 安装 Tauri CLI

如果本机还没有安装 Tauri CLI，可以执行：

```shell
cargo install tauri-cli
```

### Step 3 确认 Rust 工具链

```shell
rustup show
cargo --version
```

## 本地运行

在 `src-tauri` 目录启动 Tauri 开发模式：

```shell
cd src-tauri
cargo tauri dev
```

说明：

- `cargo tauri dev` 会读取 `src-tauri/tauri.conf.json`
- 会自动执行前端 dev server（`bun run --cwd ../frontend dev`）
- 前端默认运行在 `http://localhost:3000`

## 前端单独调试

如果只想调试页面样式或交互，可以单独启动 Vite：

```shell
cd frontend
bun dev
```

需要注意，这种方式只会启动前端页面；如果页面依赖 Tauri 的 Rust 命令，仍然建议通过 `cargo tauri dev` 联调。

## 构建

### 仅构建前端资源

```shell
cd frontend
bun build
```

构建结果会输出到项目根目录下的 `web`。

### 构建桌面应用

```shell
cd src-tauri
cargo tauri build
```

说明：

- `cargo tauri build` 会自动调用前端构建命令
- 最终产物由 Tauri 按平台进行打包

## Rust 开发与检查

Rust 代码主要位于：

- `src-tauri/src/commands`：Tauri 命令入口
- `src-tauri/src/services`：核心业务逻辑
- `src-tauri/src/repositories`：数据访问与平台适配
- `src-tauri/src/models`：数据模型

日常开发建议至少执行：

```shell
cd src-tauri
cargo fmt
cargo check
```

如需运行测试：

```shell
cd src-tauri
cargo test
```

## 调试建议

- 调试前后端联动时，优先使用 `cargo tauri dev`
- 调试 Rust 逻辑时，可直接使用支持 Rust 的 IDE（如 RustRover、VS Code + rust-analyzer）
- 修改前端后如需验证最终构建结果，可先执行 `bun build`，再执行 `cargo tauri build`

## 说明

以下旧流程已经废弃，不再适用于当前项目：

- `poetry run python main.py`
- `uv run python ui.py`
- `uv sync`
- 基于 Eel/gevent 的 Python 调试方式

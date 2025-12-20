目前项目正在进行从 python eel 到 rust + tauri 的重构过程中


tauri 重构计划文档：[doc](docs\plan\rust-tauri-refactoring-plan.md)

python 架构文档：[doc](docs\architecture.md)


## 构建

### 1. 先构建前端

```bash
cd frontend
bun run build
```

### 2. 构建后端

```bash
cd src-tauri
cargo build --release
```
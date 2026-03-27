# bytehaul 迁移计划（修订版）

本文基于当前仓库代码修订。目标是用 `bytehaul` 替换自研 Rust 下载实现，并保留 aria2 作为兼容 fallback。对尚未在当前仓库中核验的 `bytehaul` API，不再做方法级断言。

## 1. 已确认的现状

### 1.1 当前下载入口

| 路径 | 当前实现 |
|------|----------|
| `src-tauri/src/services/yuzu.rs` | 通过 `get_download_manager().download_and_wait()` 下载 Eden 包 |
| `src-tauri/src/services/ryujinx.rs` | 通过统一下载接口下载 Ryujinx 包 |
| `src-tauri/src/services/firmware.rs` | 通过统一下载接口下载固件 |
| `src-tauri/src/services/msvc.rs` | 直接调用 `get_aria2_manager()`，未走统一接口 |
| `src-tauri/src/services/updater.rs` | 自己用 `reqwest` 流式下载更新文件，不走统一下载接口 |

结论：

- 迁移不只影响 `services/downloader` 目录。
- `msvc.rs` 是必须提前处理的直接 aria2 依赖。
- `updater.rs` 当前是独立下载链路，应明确写成“暂不纳入第一阶段”，而不是默认算进 bytehaul 替换范围。

### 1.2 当前下载模块结构

当前下载模块仍然是“统一 trait + 双后端”结构：

- `manager.rs`：`DownloadManager` trait
- `types.rs`：`DownloadOptions` / `DownloadProgress` / `DownloadResult`
- `mod.rs`：全局管理器、后端选择、统一取消、Windows 下 aria2 安装 UI
- `rust_downloader.rs`：纯 Rust 下载实现
- `aria2.rs` + `aria2_backend.rs`：aria2 进程/RPC 与 trait 适配层
- `aria2_install.rs`：Windows 下 aria2 自动下载与安装
- `chunk_manager.rs` / `state_store.rs` / `retry_strategy.rs` / `client.rs` / `filename.rs`：纯 Rust 下载器内部支撑模块

### 1.3 与迁移强耦合的外围模块

以下文件会随着迁移被直接或间接影响：

- `src-tauri/src/commands/yuzu.rs`
- `src-tauri/src/commands/ryujinx.rs`
- `src-tauri/src/services/installer.rs`
- `src-tauri/src/services/msvc.rs`
- `src-tauri/src/services/mod.rs`
- `src-tauri/src/config.rs`
- `src-tauri/src/logging.rs`
- `src-tauri/src/models/progress.rs`
- `src-tauri/src/services/network.rs`
- `src-tauri/src/services/doh.rs`

需要特别修正的事实：

- `download_source` 表示“下载源/镜像名称”，不是“aria2 / rust / bytehaul 后端标识”。
- GitHub、Ryujinx、Eden 的镜像和源名称逻辑已经集中在 `services/network.rs`，不应该在新下载层再发明一套 `resolve_download_url(url, use_mirror)`。
- `hickory-resolver` 不能因为下载器替换就直接删掉；`services/doh.rs` 仍然依赖它。

### 1.4 当前配置与语义边界

当前与下载相关的配置不是只有 `download.backend`：

- `setting.download.backend`
- `setting.download.disable_aria2_ipv6`
- `setting.download.remove_old_aria2_log_file`
- `setting.network.proxy`
- `setting.network.github_download_mirror`
- `setting.network.ryujinx_git_lab_download_mirror`
- `setting.network.eden_git_download_mirror`
- `setting.network.use_doh`

这意味着：

- “替换下载后端”和“删除 aria2 相关配置”不是同一步。
- 代理、镜像、DoH、下载源显示等行为必须保留现有语义，不能只围绕 `DownloadOptions.use_github_mirror` 这个布尔值设计。

## 2. 修订后的迁移原则

1. 第一阶段保留 `DownloadManager`、`DownloadOptions`、`DownloadProgress` 这条兼容边界，不做大范围调用方改写。
2. 本轮迁移只替换 `RustDownloader`，不以完全移除 aria2 为目标。
3. 将 `services/updater.rs` 明确列为第一阶段范围外，后续单独评估是否统一到 bytehaul。
4. 任何 `bytehaul` 能力都先做上游 API 核验，再写入正式实现；在未核验前，不在计划中写具体方法名。
5. Windows 安装前置流程、统一取消入口、下载源显示必须与现有行为保持兼容。

## 3. bytehaul 上游能力核验清单

在真正改代码前，先按仓库需求核验 `bytehaul` 是否支持以下能力：

| 仓库需求 | 当前依赖位置 | 是否阻塞迁移 | 说明 |
|---------|-------------|-------------|------|
| 指定输出目录/文件名 | `DownloadOptions.save_dir` / `filename` | 是 | 必须能稳定映射到现有调用方 |
| 进度快照：已下载、总量、速度、ETA | Yuzu / Ryujinx / Firmware 安装 UI | 是 | 必须能转换为 `DownloadProgress` |
| 单任务取消与统一取消 | `cancel_download_command` | 是 | 需要覆盖全局管理器和临时任务 |
| 保留文件取消 / 删除文件取消 | `cancel_all(remove_files)` | 是 | 要重新定义 bytehaul 下的清理语义 |
| 断点续传（至少进程内恢复） | 统一下载接口 | 是 | 若连基本恢复都不支持，不能替换现有下载器 |
| 进程重启后的续传 | `state_store.rs` 当前能力 | 是 | 若不支持，应保留 aria2 fallback 或延后迁移 |
| 自定义请求头 / User-Agent | `DownloadOptions.user_agent` / `headers` | 是 | 未核验前不能宣称已支持 |
| 代理配置兼容 | `setting.network.proxy` | 是 | 需兼容系统代理和自定义代理 |
| 镜像预处理后的 URL 下载 | `services/network.rs` | 是 | 下载层应消费“已解析后的 URL”，而非重写镜像策略 |
| 覆盖已存在文件 | `DownloadOptions.overwrite` | 是 | 必须定义与现有行为一致的覆盖策略 |
| 错误可归类、便于 UI 提示 | 安装流程错误展示 | 是 | 即使不完全复刻，也要保证错误可观测 |
| 暂停/恢复 | trait 暴露，但当前 Rust/Tauri 侧无明确调用 | 否 | 可作为兼容项；若上游不支持，可先保留 trait 但返回“不支持”或延后 |
| 校验、限速、预分配 | 当前非主流程硬依赖 | 否 | 属于增强项，不应阻塞主迁移 |

如果上表中任一“阻塞迁移”能力无法满足，计划应自动降级为：

- 保留 aria2 作为长期 fallback；或
- 暂停 bytehaul 迁移，只做局部试点。

## 4. 推荐落地方案

### Phase 0：上游 API 核验与小型试验 [已完成]

目标：先验证 `bytehaul` 是否满足仓库真实需求，再决定是否进入实现阶段。

1. 选定明确的 crate 版本，不写模糊的 `0.1` 占位。
2. 用一个最小试验验证：
   - 输出路径控制
   - 进度订阅
   - 取消
   - 续传
   - 自定义 header / User-Agent
   - 代理
3. 形成最终映射表后，再开始正式集成。
4. 如果缺少阻塞能力，及时止损，不继续推进“完全替换”。

本轮已完成：

- 已选定并接入 `bytehaul = 0.1.3`
- 已基于上游文档核验输出路径、进度订阅、取消、暂停后恢复、代理、IPv6 开关、自定义 header / User-Agent 等公开 API
- 已新增 `src-tauri/tests/bytehaul_phase0_verification.rs`，覆盖显式输出路径、自定义请求头、非法配置提前拒绝这组最小验证

### Phase 1：在现有 trait 下接入 bytehaul 适配层 [已完成]

目标：不改调用方，先让 `bytehaul` 成为一个可选后端。

建议改动：

1. 保留以下文件作为稳定接口：
   - `src-tauri/src/services/downloader/manager.rs`
   - `src-tauri/src/services/downloader/types.rs`
   - `src-tauri/src/services/downloader/mod.rs`
2. 新增 `src-tauri/src/services/downloader/bytehaul_backend.rs`，实现 `DownloadManager` trait。
3. 在 `DownloadBackend` 中新增 `Bytehaul`，但此阶段不要改 `Auto` 默认策略。
4. 下载 URL 的解析继续复用 `services/network.rs`：
   - GitHub 走 `resolve_github_download_target`
   - 下载源显示继续走 `get_download_source_name`
5. 保留 `TransientDownloadManagerGuard` 和 `cancel_active_downloads()` 的整体结构，先保证统一取消能力不回退。

这一阶段不做的事：

- 不移除 `DownloadManager` trait
- 不删除 aria2
- 不修改 `services/updater.rs`
- 不删除下载相关配置项

本轮已完成：

- 新增 `src-tauri/src/services/downloader/bytehaul_backend.rs`，在现有 `DownloadManager` trait 下接入 bytehaul
- 在 `DownloadBackend` 中新增 `Bytehaul`
- 保持 `Auto` 默认策略不变，仍然优先 aria2
- 保留全局下载管理器和统一取消入口，不改调用方
- 已新增 / 更新验证用例，覆盖 `DownloadBackend::from("bytehaul")` 与 Phase 1 基础行为

### Phase 2：迁移直接 aria2 依赖与前置流程 [已完成]

目标：把“没走统一下载接口”的路径先收敛回来。

必须处理：

1. 将 `src-tauri/src/services/msvc.rs` 改为使用统一下载接口。
2. 调整以下逻辑，使 aria2 前置安装只在“实际选择 aria2”时触发：
   - `src-tauri/src/services/downloader/mod.rs`
   - `src-tauri/src/commands/yuzu.rs`
   - `src-tauri/src/commands/ryujinx.rs`
   - `src-tauri/src/services/installer.rs`
3. 保证 Windows 下安装流程在切换到 bytehaul 后，不再无意义地提示“检查下载工具/安装 aria2”。

本轮已完成：

- `src-tauri/src/services/msvc.rs` 已改为走统一下载接口，不再直接依赖 aria2 管理器
- Windows 下 aria2 前置检查仍只在 `download.backend` 为 `auto` / `aria2` 时触发；当切换到 `bytehaul` 时，不会进入 aria2 安装前置流程
- `services/updater.rs` 继续维持现状，未纳入本轮改动

### Phase 3：切换默认后端并验证主流程

目标：在保留 fallback 的前提下，让 `bytehaul` 成为默认路径。

建议顺序：

1. 将 `DownloadBackend::Auto` 调整为优先 `Bytehaul`。
2. 显式保留 `Aria2` 作为 fallback 一段时间。
3. 逐项验证以下场景：
   - [ ] Eden 下载/安装
   - [ ] Ryujinx 下载/安装
   - [ ] 固件下载/安装
   - [ ] Windows 下 MSVC 安装包下载
   - [ ] 取消下载并保留文件
   - [ ] 取消下载并删除文件
   - [ ] 断点续传
   - [ ] 代理环境下载
   - [ ] GitHub / Ryujinx / Eden 镜像模式下载
   - [ ] 前端 `download_source` 显示仍然是“源站/镜像名称”

说明：

- `services/updater.rs` 继续维持现状，不纳入这阶段验收。
- 这阶段的目标是“默认路径可用”，不是“仓库里再也没有 aria2”。

### Phase 4：保留 aria2 fallback 的清理方案

本阶段固定采用分支 A，不推进完全移除 aria2。

在 `bytehaul` 已成为默认下载路径后，仅删除纯 Rust 下载器内部实现：

- `rust_downloader.rs`
- `chunk_manager.rs`
- `state_store.rs`
- `retry_strategy.rs`
- `client.rs`
- `filename.rs`

同时：

- 保留 `aria2.rs`、`aria2_backend.rs`、`aria2_install.rs` 作为 fallback 与 Windows 前置能力
- 更新测试
- 清理不再使用的 re-export
- 重新检查 `types.rs` 注释中“与 Aria2 兼容”的描述
- 保留并校验 `download.backend`、`disable_aria2_ipv6`、`remove_old_aria2_log_file` 等现有配置语义

说明：

- `aria2` 相关命令层、日志过滤、安装前置逻辑仍需维护，但应以 fallback 身份存在，而不是默认主路径。
- “完全移除 aria2”不在当前计划范围内，后续如需推进，应另起文档重新评估。

## 5. 不应继续保留的原计划表述

以下表述已被当前仓库代码证明不准确，应从计划中移除：

- “`services/eden.rs` 使用统一下载接口”
- “`download_source` 字段用于区分 aria2 / rust 后端”
- “只要接入 bytehaul，就可以顺手删掉 `hickory-resolver`”
- “Phase 1 就移除 `DownloadManager` trait”
- “可以新写一个简单的 `resolve_download_url(url, use_mirror)` 替代现有镜像逻辑”
- “Phase 3 可以直接删除 `aria2_install.rs`，不会影响命令层”

## 6. 验收标准

只有同时满足以下条件，才算迁移成功：

1. `yuzu.rs`、`ryujinx.rs`、`firmware.rs`、`msvc.rs` 均不再依赖自研 Rust 下载实现。
2. Windows 安装流程在默认配置下不会再出现多余的 aria2 安装步骤。
3. 统一取消入口在“保留文件”和“删除文件”两种模式下都行为正确。
4. `download_source` 仍正确展示镜像/源站名称。
5. aria2 fallback 相关配置、日志、命令层与安装前置逻辑仍自洽，且不会干扰默认的 bytehaul 路径。

## 7. 总结

修订后的结论不是“立刻用 bytehaul 取代一切”，而是：

- 先把 `bytehaul` 作为现有下载抽象下的一个新后端接入。
- 先迁移 `msvc.rs` 这类直接 aria2 依赖，再切默认到 bytehaul。
- 最终态保留 aria2 fallback，只移除自研 Rust 下载器内部实现。
- 把所有未核验的 `bytehaul` 方法级设想，降级为 Phase 0 的核验项。

这样改，计划才和当前仓库结构、配置语义以及实际调用链一致，也更适合按风险递进落地。


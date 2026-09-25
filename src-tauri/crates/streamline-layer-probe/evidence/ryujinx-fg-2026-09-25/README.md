# 原版 Ryujinx / 王国之泪：有界 2× FG 激活

2026-09-25，用户指定 `D:/Ryujinx_test/Ryujinx.exe` Canary 1.3.351（EXE SHA-256 见 target-profile.json），游戏为 TOTK 1.0.0。RTX 5070 Ti Laptop / 610.88，Vulkan，VSync 关闭；子进程层环境隔离 ReShade。没有修改目标 EXE 或游戏文件。构建和 SDK 运行时按各 session 输入记录冻结。

| 运行 | 游戏呈现 | 原生呈现 | SDK 工作线程原生呈现 | 请求 On | SDK x2 报告 | 输入完成最大值 | 退出 |
|---|---:|---:|---:|---:|---:|---:|---|
| sdk-off | 3808 | 3808 | 0 | 0 | 0 | 0 | 0，映射归零 |
| passed-001 | 1312 | 1856 | 1089 | 546 | 545 | 817 | 0，映射归零 |
| passed-002 | 2775 | 2833 | 117 | 60 | 59 | 1319 | 0，映射归零 |

两轮 FG 均观察到真正的 SDK 异步工作线程额外调用下层原生 present，所有原生呈现 fence 完成。所有记录的 FG status=0。失去前台后执行 Off+SDK flush，最后仍可 x1 呈现；正常关闭后输入纹理退休、SDK shutdown=0、设备和实例映射归零。应用层 Present 线程没有被 SDK 工作线程重入。SDK 帧数、原生帧数在启停边界不同，不把它们等同于显示输出帧数。

`target-result.json` 由代码自动判定，拒绝缺少额外原生呈现、工作线程、输入完成值、Off、资源退休或正常退出的运行。负向测试覆盖只报计数、无工作线程、缺失清理和 fence 失败。第二轮在用户点击窗口后激活，切回聊天后 Off；其较短 On 区间也验证了前台门控。并未完成 600 帧自然上限退出的实测。

## 修复

- `slSetVulkanInfo` 延后至 Loader 完成设备创建后的首个应用设备调用。此前在层内 CreateDevice 尚未返回时注册会导致 NvLL invalid handle / 无 Vulkan 支持；SDK-off 修复后 3808 帧全部代理成功。
- 应用的 Vulkan 1.2 创建需求升级为满足 SDK 的 1.3；复制应用特性 pNext，不修改应用原始链。保留应用 family 0 的 2 个队列，另保留 SDK graphics 1、compute 2，总共 5/16。
- Win32 surface 使用 SDK 直接导出创建关联 HWND 的 shadow surface，交换链只代理一次；应用仍使用自己的原生 surface 查询能力。
- `failed-loader-query` 是首次 FG 失败：查询 present modes 误经 Loader 入口接收层下 physicalDevice。已改为保存的下层 GIPA 函数，后两轮通过。保留失败日志，不隐藏错误。
- FG 实验将 MAILBOX 改为已查询支持的 IMMEDIATE，提供固定零深度/零运动矢量/合成相机、真实 PresentStart/End；没有伪造 Simulation/Render 标记。

## 边界

**这只证明游戏内 FG 调用与额外原生呈现链路接通，P4 未通过。** 没有高帧率最终输出采集、逐帧有效中间画面、显示节奏或端到端延迟测量。没有启用 Vulkan validation layer，不能宣称验证错误为零。诊断每次原生呈现都等待 fence，会影响性能，不是产品节奏策略。

每轮保留 9 行 SDK warning，包括应用先前调用 Vulkan、未支持的非 FG hook、GetState 同步提醒、初次重复选项、Vulkan async marker 未实现、backbuffer extent 默认值、RSync 未初始化。SDK-off 有 6 行。两轮 FG 均无 `[error]`；这些 warning 未被当作已经解决，也不据此声称低延迟契约完整通过。

窗口使用线程 CBT hook，在活跃时取消 MOVESIZE/MINMAX/DESTROYWND，并请求呈现线程 Off+flush；被取消的窗口操作不自动重放。微软 CBTProc 文档允许 return 1 阻止这些操作：<https://learn.microsoft.com/zh-cn/windows/win32/winmsg/cbtproc>。仍需解决和验证前置事件竞争、操作恢复、resize/最小化循环、停止/重开游戏与多窗口生命周期。目前只能使用固定窗口、单设备生命周期，AcquireNextImage2 SDK 代理未支持。故不进入 P5 产品集成。

初期实验的 `target-inputs.json` / `target-exit.json` 及部分设备/交换链事件中的 `fg_enabled:false` 是初始化时静态字段，不能当作逐帧实际状态。实际请求以 `fg_experiment_requested` 和 `target_fg_frame.requested_on` 为准；新启动器输出已改为 `fg_requested`。

## 复现与归档

仓库根目录构建：

```powershell
cargo build --locked --all-features --manifest-path src-tauri/crates/streamline-layer-probe/Cargo.toml
& src-tauri/crates/streamline-layer-probe/target/debug/streamline-layer-probe.exe --target-probe --fg --runtime D:/py/ns-emu-tools/src-tauri/target/streamline-route-runtime-v3 --layer D:/py/ns-emu-tools/src-tauri/crates/streamline-layer-probe/target/debug/streamline_probe_layer.dll --session D:/py/ns-emu-tools/src-tauri/target/ryujinx-fg-NEW --game 'D:/game/yuzu_game/totk/TOTK [0100F2C0115B6000][v0].xci'
```

保持窗口在前台；Off 后正常关闭。session 必须不存在。`--target-probe --verify <session>` 可重复判定已有完整运行。归档只保存日志、输入、结果；完整 trace gzip 压缩并记录解压内容 SHA-256，不分发 SDK DLL。复核时解压到独立目录的 layer.jsonl，保留 runtime/sl.log 等相对路径。

代码检查：cargo fmt；19 项测试通过；host 和显式 x86_64-pc-windows-msvc 的默认/全部 feature、all-targets cargo check 均无错误和警告。未改动本任务外已存在的应用/前端修改。

## 后续复验：600 帧停用与基础帧率下降

- `passed-003-budget`：应用 1962 次 / 原生 2559 次呈现，工作线程 1195 次；恰好启用 600 帧后以 `frame_budget` 停用，只有一次 `target_fg_stopped`，设备空闲等待完成，之后继续 x1。SDK x2 报告 598 次，输入 timeline 递增 597 次，正常退出和资源退休通过。
- 用户报告状态栏掉到约 20 FPS。该轮 On 的应用帧间隔中位数 49.935 ms，后台 Off 阶段也约 49.152 ms，不能把下降全部归因于插帧计算。
- `passed-004-no-frame-limit`：取消游戏路径上沿用诊断宿主的 `ReflexOptions.frameLimitUs=16667`（改为 SDK 默认 0），保留 LowLatency 和真实 Present 标记；缓存每个交换链的 ash 设备分发表，避免逐帧重复查询和写日志。诊断宿主仍保留原有限帧。
- 修复后用户确认恢复 30 FPS；109 个 On 帧的应用间隔中位数 33.351 ms；Reflex begin/sleep 中位数 94 μs、options 7 μs、代理 Present 138 μs、输入等待 2418 μs。应用 2131 次 / 原生 2237 次，工作线程 213 次，SDK x2 报告 107 次，输入递增 106 次。失焦停用后正常退出，全部呈现 fence 和清理通过。
- 两项性能修改在同一轮验证，且没有固定镜头和负载，所以不声称已单独量化每项收益。109 个 On 帧只是短期复验；状态栏、应用 Present 间隔均不等于最终显示帧率。每次原生呈现的 fence 等待仍保留，显示节奏、长时间性能与延迟仍未验收。

同时修复停用后每帧重复 device idle、窗口钩子部分安装失败的清理，以及重复 `--verify` 无法刷新派生报告的问题。新增尺寸边界、SDK 状态、预热、前台、600 帧上限、终止状态和输入 timeline 停滞/倒退测试；24 项测试通过，默认/全部 feature 的 host 与显式 Windows all-targets 检查均无错误警告。前两轮原始日志也通过加强后的 timeline 检查。新测试不代替真实窗口循环或 SDK 故障注入。

## 单变量 Reflex A/B/A（修正此前归因）

`passed-005-reflex-ab` 使用同一个进程、同一缓存函数表实现，FG 连续 600 帧。前 200 帧限帧为 0，中间 200 帧恢复 16667 μs，最后 200 帧回到 0。每段取相对帧 21～190，排除切换边界；日志同时记录本帧 Sleep 实际沿用的选项和随后提交的选项，防止错配一帧。三段均 170 个观测点 / 169 个完整间隔，原生呈现均 338 次。

| Reflex 限帧 | 应用 FPS | 原生呈现 Hz | 应用间隔中位数 | Begin/Sleep 中位数 |
| --- | --- | --- | --- | --- |
| 0，第一段 | 29.9944 | 59.9888 | 33.343 ms | 94 μs |
| 16667 μs | 29.9964 | 59.9928 | 33.353 ms | 2857 μs |
| 0，第三段 | 30.0078 | 60.0157 | 33.304 ms | 93 μs |

**这次恢复旧限帧没有复现 20 FPS，因此此前“限帧叠加是主要原因”的推测尚不能成立为根因结论。** 旧路径的重复函数表查询、日志开销及与 pacing 的交互仍未完成单变量对照。保留游戏路径限帧默认 0。`--fg --reflex-ab` 仅用于显式诊断，不改变普通运行。

全程应用 2071 次 / 原生 2668 次、工作线程 1195 次；SDK x2 报告 598 次，600 帧后自动停用，输入 timeline 递增 597 次；正常退出、原生 fence 和资源清理通过，无 SDK error，9 行既有 warning。25 项测试及 host/Windows 默认和全部 feature 的 all-targets 检查通过。

**已经测得约 30 应用 FPS / 60 原生呈现次每秒，未测得最终屏幕 60 张有效不同画面。** 原生 retirement 日志的时间戳不是扫描输出时间戳，不能拿它冒充显示间隔或端到端延迟。明细在 `reflex-ab-result.json`。


## 游戏内观感反馈与安装界面

2026-09-25，用户反馈“fg 在游戏中看着正常”，并要求开始编写安装界面。此反馈作为用户观感记录，不替代显示间隔、端到端延迟或长期生命周期验收，P4 状态不变。

工具箱图形增强页开始提供独立 FG 安装方案与只读条件检查：核对冻结主程序哈希、用户选择的 Vulkan 后端、已知目录冲突文件；显卡和全局图层检查仍标为待确认。当前没有正式部署包或安装/启动命令，UI 明示等待安装包，不把诊断程序或文件存在误报成已安装、正在插帧。该 UI 工作由用户此次明确要求启动。

### 本地安装服务与无帧数上限实测

`installed-unbounded-live.json` 来自工具箱 Rust 安装与启动服务实际部署后的《王国之泪》会话 `game-ipmYSp`。采样时 FG On 共 1,746 帧、1,740 帧 SDK reported presented=2，超过 600 的 On 帧有 1,146 帧，SDK status 均为 0。记录两次失焦暂停及其后恢复。当前为运行中快照，未将 CPU/SDK 呈现计数视为屏幕扫描输出证明，也未声称本次退出收尾或 P4 全部通过。

首轮安装会话 `game-0oEJLh` 因旧的终止式失焦门控在 38 On 帧后停用，正常退出成功。其问题在普通模式中改为失焦暂停/前台恢复；显式 bounded 诊断保持原来的停止行为。窗口缩放/最小化/销毁保护仍可能终止本次 FG，尚未扩大该生命周期验收范围。

### 额外运动估计画质反馈与保留方案

用户对块匹配光流实验的实测反馈为“更差”，随后明确选择继续保持零运动矢量。保留参考 FG 常量参数 + 零运动矢量 + 常量深度，不启用 `--estimate-motion`。该估计原型默认关闭，不因计算计数或 SDK 双帧呈现成功而视为画质通过，也不作为安装默认项。`motion-prototype-live.json` 仅证明运行链路及耗时（采样中位 717us、P95 1130us），不证明视觉收益。

# NR 队列、虚拟交换链和读回修复（2026-10-01）

本轮保留 Rust 兼容层修复。用户确认最终测试“画面正常，数秒停顿已消失”，并选择暂不扩展到 Ryujinx C# 后端。最后一轮完成 4,209 次 NR → DLAA，4,273 次原生呈现提交和退休匹配，正常退出码为 0；严格验证仍有 **209 条错误、30 条未审查警告**，P1/P2 尚未通过。

原始日志和 PPM 在忽略目录 `src-tauri/target/nr-safety-validation-001` 至 `004`。固定输入、各轮 DLL 摘要、分类计数、读回、控制和退出证据及原始文件 SHA256 保存在 [summary.json](summary.json)。这些测试保留 core/synchronization validation、NVOF、合成深度、NR 强度 1、DLAA 100%，实际 FG 关闭；新链尾路径使用默认选择。不能将带验证层的本轮帧率当作正式性能结果。

## 已修复的兼容层问题

- **原生队列外部同步。** 应用和 SDK 下游的 Submit、Submit2（含 KHR alias）、BindSparse、QueueWaitIdle、DeviceWaitIdle、SDK 原生 Present 共用每设备的锁。锁只覆盖真实原生调用，SDK hook、工作线程等待和 CPU fence 等待在锁外执行，避免让 SDK 等待持锁工作线程。此前 `vkQueueSubmit` 与 `vkDeviceWaitIdle` 同时使用队列的消息未在本轮四次会话复现。
- **SDK 虚拟交换链的布局与依赖。** SDK 暴露给应用的是普通 transfer-source 图像。仅当图像属于已登记的 SDK proxy、由 proxy getter 返回、且确认来自 SDK 下游 `vkCreateImage` 时，才将逻辑 PRESENT 转换为 TRANSFER_SRC。经典 `vkCmdPipelineBarrier` 同时补齐前序写入与传输读取的阶段/访问依赖；原生交换链图像保持原布局。修复后会话 002–004 未出现 `fake-swapchain-buffer` 布局或 WAW 消息。
- **验证归因。** 日志增加线程局部 `application_call`，保留所有错误和未审查警告。应用入口、嵌套调用、异常恢复有回归覆盖；空值表示没有被此上下文覆盖，不能据此认定属于 SDK。
- **读回缓存。** 优先选择兼容 buffer `memoryTypeBits` 的 HOST_VISIBLE | HOST_COHERENT | HOST_CACHED 内存，缺少 cached 类型时回退 coherent 类型。GPU fence 完成后，映射数据一次复制到普通主机 RAM，立即 unmap，再做有限值、哨兵、差异、SHA256 和 PPM 检查。没有关闭读回或降低其验收要求。

## 窗口缩放停顿的定位

会话 002 的单次 CPU 读回耗时为 **5,374–6,100 ms**，NR 重建后的前三帧各做一次捕获；GPU fence 通常为几十毫秒，不能解释数秒停顿。最初把主要开销归因于半精度转换；会话 003 换成位运算转换后仍为 **5,225–5,562 ms**，用户也确认停顿未消失。该失败结果原样保留。

会话 004 改为 cached/coherent 类型 3（flags=14）并批量复制至主机 RAM后，九次 CPU 读回为 **79.440–97.942 ms**。全部输出有限、无哨兵残留，有输入/输出差异与输出摘要。用户确认运动画面正常，调整窗口后的数秒停顿消失。

该计时包含映射/复制、统计、摘要和 PPM 写入，不代表整个窗口缩放耗时；两轮窗口大小和游戏活动不同，也不是确定性性能回放。正式性能路径关闭读回，因此不由此声称一般 FPS 提高。半精度转换保留完整 65,536 种编码的等价性检查。

## 实机记录

| 会话 | NR → DLAA | 原生提交/退休 | 错误/警告 | 队列并发 | SDK 假交换链错误 | CPU 读回 |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| 001：队列锁与调用归因 | 4,356 | 4,422 / 4,422 | 187 / 30 | 0 | 20 | 5,502–6,524 ms |
| 002：虚拟 PRESENT 与传输依赖 | 6,926 | 7,014 / 7,014 | 266 / 30 | 0 | 0 | 5,374–6,100 ms |
| 003：普通图像身份验证、半精度转换 | 1,939 | 1,999 / 1,999 | 229 / 30 | 0 | 0 | 5,225–5,562 ms |
| 004：cached 读回与主机 RAM 分析 | 4,209 | 4,273 / 4,273 | 209 / 30 | 0 | 0 | 79–98 ms |

会话 003 另有一条 **VkFence 并发访问**：`GPU.MainThread` 的 `vkWaitForFences` 与 `GUI.RenderThread` 的 `vkQueueSubmit` 同时使用 fence（ID 1010413496）。它不是本次修复的 VkQueue Submit/DeviceWaitIdle 竞争；最终会话未出现，但没有修复或正式排除该 fence 问题。所有测试进程均正常退出、NR/SR 完成和清理检查通过，launcher 因严格验证失败返回 1。不同会话的总计数受场景、时长、重复消息上限影响，不能按总数比较修复效果。

最终窗口覆盖 `800×461 → 800×474 → 2560×1335 → 800×474`，四个 proxy 各有三个 SDK 创建的普通图像。NR feature 重建三次，九次读回有效；控制更新后 NR/DLAA 均关闭并正常退出，无未退休呈现。人工暂停/恢复与最小化本轮未明确确认，不能将后台恢复或零运动记录当作其验收。

## 尚未修复的验证问题

最终 209 条 ERROR 的记录分类：

| 分类 | 消息数 | 证据与后续处理 |
| --- | ---: | --- |
| 资源过早销毁和失效命令缓冲连锁错误 | 141 | buffer-in-use 9、image-view-in-use 10、失效命令缓冲 122。首两类来自应用 `vkDestroyBuffer` / `vkDestroyImageView` 入口；需追踪模拟器的引用、提交、退休和重录逻辑。 |
| 游戏纹理传输/附件同步 | 20 | transfer 写入后 renderpass LOAD 的 RAW 10、普通游戏图像 mip 传输 WAW 10。后者虽与旧假交换链 WAW 使用相同 ID 1544472022，但对象和访问不同。 |
| 渲染状态、shader/格式能力 | 26 | wideLines 未启用 10、clip/cull 合计超限 10、格式不支持深度比较 5、feedback-loop 动态状态未设置 1。 |
| Query 范围不匹配 | 20 | begin/end query 跨 renderpass/subpass 范围。 |
| 压缩纹理拷贝范围 | 1 | BC1 小 mip 的拷贝宽度不符合 texel block / 子资源边缘要求。 |
| Device 清理 | 1 | 设备销毁时仍报告 32 个未销毁子对象；本层 NR/SR 清理成功不等于整个 device 零泄漏。 |

30 条 WARNING 仍为三类各 10 条：未使用 fragment 输出、未使用 vertex attribute、3D image barrier 的 maintenance9 未来语义提醒。未静默、未自动标记 reviewed。

入口归因支持继续查 Ryujinx 后端，尚不能指认具体 C# 源码错误，或用非纯模拟器基线证明本层/SDK完全无关。Rust 层不能仅延后 `Destroy*` 或插入全设备等待就保证尚未提交的命令、绑定内存和资源所有权正确。本轮按用户选择保留修复并记录限制。

## 检查与边界

125 项 Rust 测试通过、1 项专用硬件测试忽略。`cargo fmt`、crate 宿主/显式 Windows MSVC all-features/all-targets、默认/SDK-only，以及主应用宿主/Windows 检查通过，无编译错误或警告。最终 `git diff --check` 通过；本轮未创建提交。

虚拟布局适配验证的是固定目标使用的经典 `vkCmdPipelineBarrier` 路径，尚未对 synchronization2 或任意 renderpass 隐式布局转换做通用验收。默认原生 NR 实验和正式画质/性能门控保持既有状态；画面正常不能代替零错误严格验收。

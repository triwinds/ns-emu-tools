# 新路线默认选择与严格验证复核（2026-10-01）

用户选择继续使用新路线后，`native-nr` 构建在未设置 `NS_STREAMLINE_DEFER_PRESENT` 时默认启用有界延后回收。`0` 恢复同步链尾，`1` 显式启用新路线。原有门控保留：实际 FG 关闭、没有待消费 FG timeline、已识别原生输入、本帧不读回。标准工具箱构建仍不默认启用原生 NR。

此处仅改变默认选择。此前同场景测试已经显式设置 `1`，对应相同处理逻辑；本次没有新增硬件验收。原始 README 和 summary.json 保留当时 opt-in 状态与测试结果。

## 错误分类

对 `src-tauri/target/nr-tail-validation-001/nr-validation.jsonl` 按 severity 和 message ID 重新分类：238 条 ERROR，共 30 种 ID。它们是已记录的重复消息数，不是 238 个独立根因；计数不代表全部实际发生次数。

| 分类 | 消息数 | 含义与处理 |
| --- | ---: | --- |
| 资源生命周期和连锁命令缓冲失效 | 157 | 137 条报告已销毁 buffer 使 command buffer 失效；另有 10 条 buffer-in-use、10 条 image-view-in-use。先查资源所有者、最后提交和完成信号，在安全完成后再销毁或重录，不能仅屏蔽连锁报错。 |
| 布局和 GPU 同步 | 30 | 10 条 SDK `nv.sl.dlss_g.tex2d.fake-swapchain-buffer` 的 TRANSFER_SRC/PRESENT 布局不符；10 条同类图像布局转换 WAW；10 条 transfer 写入与 renderpass LOAD 读取 RAW。按实际写入阶段/访问和消费阶段补齐依赖，并核对 SDK 假交换链的布局交接。 |
| 队列并发访问 | 4 | `<tera::ImageResourceMgr>` 的 `vkQueueSubmit` 与 `GUI.RenderThread` 的 `vkDeviceWaitIdle` 同时使用队列。需追踪重建和等待的调用者，统一队列外部同步，或在证明所需范围后用本层完成 fence 替换全设备等待。仅锁 Present 线程不能保护后台提交。 |
| 渲染状态、shader/设备能力 | 26 | wideLines 未启用（10）、clip/cull 合计超限（10）、采样格式不支持深度比较（5）、feedback-loop 动态状态未设置（1）。需识别创建/绑定这些对象的上游，按设备能力修正状态或生成 shader。 |
| Query 范围 | 20 | begin/end query 不在同一 renderpass/subpass 范围。需成对核对 query 记录与 renderpass 边界。 |
| Device 清理 | 1 | 销毁 device 时尚有 32 个 image/view 等子对象，VUID 05137。NR/SR 自身完成记录通过不能代替整个 device 的零泄漏验收；需追踪对象创建与销毁归属。 |
| 合计 | 238 | 严格验证仍失败。 |

本次错误中没有 `nv.ngx.dlssnr.resource` 命名对象，不能把全部错误归结为早期 NR 内部纹理的四条 UNDEFINED/GENERAL 布局报错，也不能据此证明 NR 或新路线与所有错误无关。

## 警告分类

30 条 WARNING，共三种 ID，每种 10 条：

- Fragment shader 写 location 2，但 subpass 没有对应 color attachment，写入未使用（ID -1744492148）。
- Vertex attribute location 1 未被 vertex shader 使用（ID -937765618）。
- 3D image 带 2D_ARRAY_COMPATIBLE，barrier 的 layerCount=1；maintenance9 当前关闭时表示所有 depth slices，开启后语义不同，建议使用 VK_REMAINING_ARRAY_LAYERS 保持兼容（ID -1693624763）。

前两类是未使用接口提醒，第三类是未来特性语义兼容提醒；当前日志未将它们判为非法调用。它们优先级低于生命周期/同步错误，但仍需要确认调用者与意图后才能标记 reviewed，不能为了通过验收全局忽略。

## 历史对照和归因限制

- `nr-p1-native-001`：192 条错误、30 条警告，实际 NR 评估为 0；本次 30 种错误 ID 中 28 种在该会话已有，包括 image-view-in-use。此基线仍使用集成层/SDK，不能当作纯模拟器基线。
- `nr-handoff-validation-001`：220 条错误、30 条警告；本次 ID 有 29 种与其重合，只有 image-view-in-use ID 1672225264 相对该紧邻会话新出现。
- 队列并发消息 ID -1604639890 在前次也出现，但前次报告的是 fence 并发访问，本次为 queue submit/device idle；ID 相同不等于相同根因。
- 137 条失效命令缓冲错误引用两个已销毁 buffer（`0x718d000000718d`、`0x721f000000721f`）。必须查首个销毁/失效位置，而不是逐条修补后续 draw、bind、copy 等调用。

修复优先级：队列并发和资源生命周期 → SDK 交换链布局/同步 → 其他状态与清理 → 警告确认。需要用受控纯模拟器、集成但 NR/DLAA 关闭、NR 单开、NR → DLAA，以及旧/新链尾对照定位归属。现有日志支持分类与风险判断，尚不支持宣称全部来自模拟器、SDK 或本层。

正式稳定运行前应消除真实生命周期/同步错误或提供可复核的验证层误报证据。关闭验证层只减少检查开销，不会修复这些错误。此前用户确认画面正常和完成记录匹配，支持继续实验，不能替代零错误验收。

规范核对：

- [vkDestroyBuffer](https://docs.vulkan.org/refpages/latest/refpages/source/vkDestroyBuffer.html)：buffer 使用必须完成后才能销毁。
- [vkDestroyImageView](https://docs.vulkan.org/refpages/latest/refpages/source/vkDestroyImageView.html)：引用 image view 的提交必须完成后才能销毁。
- [vkDeviceWaitIdle](https://docs.vulkan.org/refpages/latest/refpages/source/vkDeviceWaitIdle.html)：需要对设备所属队列的主机访问进行外部同步。
- [Synchronization](https://docs.vulkan.org/spec/latest/chapters/synchronization.html)：执行和内存依赖需覆盖先前访问与后续访问。

默认选择改动已完成 cargo fmt、crate（all-features/all-targets）与工具箱 host/Windows cargo check，无编译错误或警告；native-nr release 层/launcher 构建通过。新 DLL SHA256 为 `72ac3203265188fd11c01e9a355e3f88518dfc65d055d33e703852232a12307e`，launcher 为 `2358fd754aea4faf9b232f40431e712b1c660a770e50636ff9386ec555e486a9`。此前 119 项通过、1 项忽略为上一轮完整测试结果；本次未改动 GPU 提交或生命周期算法，未重复硬件测试。

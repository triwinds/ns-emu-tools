# SDK/NGX 插帧 transfer 同步修复（2026-10-01）

本轮继续处理此前独立 SDK 测试的 20 条同步错误。修复位于 Rust 下游路由，默认启用；同一 DLL、同一诊断 EXE 的关闭/开启对照分别报告 20 / 0 条错误，开启时核心和同步验证均通过，警告为 0。未修改 SDK C++、运行库二进制或 Ryujinx。

## 原因和范围

SDK/NGX 将一些 GENERAL 图像同时用于 shader 和 transfer 操作，但现有 ALL_COMMANDS 屏障只声明 SHADER_READ | SHADER_WRITE：

- 图像转换之后的清除缺少 TRANSFER_WRITE，报告 WRITE_AFTER_WRITE。
- fake-swapchain-buffer 的复制读取缺少 TRANSFER_READ，报告 READ_AFTER_WRITE。
- 先修复图像后，验证层暴露出同名 NGX buffer 连续 vkCmdFillBuffer 的 WRITE_AFTER_WRITE：两次填充之间已有 UAV 屏障，其访问范围同样只有 shader 读写。

固定 Streamline 2.12.0 downstream-v3 源码的 `platforms/sl.chi/vulkan.cpp:2602` 对 image/buffer UAV 屏障都使用 shader-only 范围。真实下游调用跟踪与其吻合；NGX 内部实现不可见，判断依据是实际创建、命名、屏障、填充和验证消息。

`sdk_transfer_access.rs` 只选择 SDK 路由中创建、成功绑定并匹配精确调试名称的对象。图像名称限定为 `nv.ngx.dlssg.resource`、`nv.sl.dlss_g.tex2d.fake-swapchain-buffer`、`nv.sl.dlss_g.clone.dlfg-output_0/1`；buffer 只接受 `nv.ngx.dlssg.resource`。还要求创建参数符合所观察契约、屏障没有 pNext、队列族为 IGNORED、两端阶段为 ALL_COMMANDS，并且原访问范围恰好为 shader 读写。图像另检查 COLOR 和 GENERAL/UNDEFINED → GENERAL 的相关访问端。

按对象实际 TRANSFER_SRC/DST usage，给相关访问端补上 TRANSFER_READ/WRITE。GENERAL、UNDEFINED 和其他布局、图像子资源、buffer offset/size 均保留。首次 clone 布局适配先执行；本修复随后补齐访问范围。销毁及设备退休清除状态，重复调用不会再次扩大已修正的 mask。没有新增 GPU 命令、提交、fence 或 CPU 等待。它不适配 NR、游戏对象或原生交换链。

ALL_COMMANDS 包含 transfer 执行阶段，但访问范围仍决定哪些读写受内存依赖保护；RAW/WAW 需要相应内存依赖。[Vulkan 同步与访问范围规范](https://docs.vulkan.org/spec/latest/chapters/synchronization.html#synchronization-access-types)。GENERAL 布局允许复制和清除，保留 GENERAL 可以通过正确的访问范围解决本轮冲突。

`NS_STREAMLINE_SDK_TRANSFER_ACCESS=0` 关闭该修复；`NS_STREAMLINE_SDK_OUTPUT_INIT=1` 保持之前的首次布局修复。`NS_STREAMLINE_SDK_LAYOUT_TRACE=1` 启用有限的创建、绑定、原始屏障、复制和填充记录。修复自身每对象最多记录 8 次适配样本，并在设备退休报告总数。

## 独立 SDK 对照

固定 RTX 5070 Ti Laptop、610.88、VVL 1.4.363.0、Streamline 2.12.0 downstream-v3，1280×800 合成运动窗口，600 个应用帧。NR 未请求，不运行模拟器；最后一帧关闭 FG，完成输入和原生呈现等待后销毁。使用 `streamline-fg-diagnostics --sdk-fg --validation-dir`，未按消息 ID 或文本过滤错误。

| 会话 | transfer 修复 | 验证错误 / 警告 | SDK 报告呈现 | 输入等待成功 | 原生呈现完成 | 子进程退出 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| transfer-001 | 仅图像 | 10 / 0 | 1,015 | 600 | 1,014 | 1 |
| transfer-002 | 图像 + buffer | 未完成 | 无有效活动样本 | 未完成 | 未完成 | 0xc0000409 |
| transfer-003 | 图像 + buffer | 0 / 0 | 1,199 | 600 | 1,198 | 0 |
| transfer-004 | 关闭 | 20 / 0 | 677 | 600 | 676 | 1 |

001 的图像错误消失，但同 ID 的 buffer fill 错误此前受默认重复消息限制遮蔽；该失败原样保留。002 等待前台超时，没有有效 FG 样本，退出发生 abort；不能算作修复成功或有效对照。随后延长诊断前台等待到五分钟，并记录两种修复开关。

003/004 的 DLL SHA-256 同为 `6eb484c65a039b7ae051bf2b78d307cf2f9ca3718c80491b17aa389dd5768550`，诊断 EXE 同为 `429047f4ca0b2f604ead98d878ceb281e949ee911589c2ba6e398caa6d3eddf9`。两轮首次布局修复均开启，仅 transfer 开关不同；004 重新出现 RAW/WAW 两类各 10 条报告。10 是验证层默认每 ID 重复上限，不能解释成实际只发生十次。

003 实际 FG timeline 最大 599，SDK worker 原生呈现 1,197 次，所有 native present fence 完成，SDK shutdown 返回 0，进程正常退出 0。共适配 75,405 个已有图像/buffer 屏障。SDK 报告呈现和原生完成计数相差 1，保留其不同定义，不作为显示扫描或性能证明。

VVL 的 0 警告不等于 SDK 文本日志无警告：SDK 仍保留既有初始化、未支持 hook、Reflex/RSYNC 和参数提示；全文摘要保存在 `sdk-device-result.json`。003 的该报告还有一个历史字段错误：硬编码 `validation_layer_enabled=false`；真实 callback 日志和 `nr-validation-result.json` 明确启用了核心及同步验证。随后修正报告字段，并让诊断默认启用的适配器也请求 debug_utils；原始报告不改写。

## 游戏链复测与边界

`nr-sdk-transfer-validation-001` 启用严格验证、真实 NVOF、NR → DLAA → FG 和读回。用户反馈“已就位，画面正常”。日志确认 528 帧三者同时实际工作、FG 每帧报告额外呈现并完成输入等待；NR → SR 输入关系及 3 次读回均通过检查，NR 完成 3,107 次评估。

后续额外 NR 开关采样在第一阶段等待前台时超时，FG 原因为 background；没有执行或接受 NR off/resume 连续消费者对照。本轮不重复宣称八组合验收。最后明确写入全关控制，确认三者 revision 4 已应用，再正常关闭窗口。游戏进程退出 0，NR 与呈现链均清理成功，3,835 次原生呈现的 fence 全部完成。

游戏总体严格验证仍失败：215 条错误、30 条未审查警告，保留了已知模拟器资源生命周期、能力、查询、同步及线程问题。RAW/WAW ID 在游戏日志也达到默认重复上限，因此不能根据未看到新的 SDK 消息，宣称整条游戏链零 SDK 错误。独立 SDK 的零错误结论不受模拟器消息遮蔽；游戏复测只补充实际执行、画面反馈与退休证据。按用户要求不修改 Ryujinx，不把这些错误加入验证白名单。

本轮不是性能或正式画质验收，使用合成深度。安全失败回退、长期画质及活跃 FG 下的窗口操作仍需分别验证。

## 检查和原始记录

139 项 Rust 测试通过，1 项硬件专用测试忽略。已运行 cargo fmt、crate 宿主及 Windows all-features/all-targets 检查、默认及 SDK-only 检查，以及主应用宿主/Windows 检查，无编译错误或警告。原生 NGX 构建使用动态 CRT。

原始会话位于忽略的 `src-tauri/target/sdk-fg-transfer-validation-001` 至 `004` 及 `src-tauri/target/nr-sdk-transfer-validation-001`。输入 runtime/VVL/DLL hash、失败记录、完整关键报告、消息分组、适配样本和原始文件摘要见 [summary.json](summary.json)。诊断报告修正后 EXE 重新构建，当前运行层 DLL 与 003/004 实测相同。

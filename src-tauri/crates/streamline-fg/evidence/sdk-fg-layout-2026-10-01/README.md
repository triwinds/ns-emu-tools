# SDK 插帧输出首次布局修复（2026-10-01）

本轮修复八组合测试中的两条 `dlfg-output_0/1` 首次布局错误。独立 SDK 诊断不运行 Ryujinx，子进程明确将 NR 请求设为关闭；两条错误仍可复现。按用户要求，Ryujinx 原有错误不在本轮修复范围。

## 原因与修复

固定 Streamline 2.12.0 / downstream-v3 的 Vulkan 创建代码使用 `initialLayout=UNDEFINED`。新增可选跟踪从 SDK 下游路由捕获真实 `vkCreateImage`、成功绑定、调试名称、屏障和复制调用。两张输出纹理为 BGRA8、1280×800、单 mip/单层、usage 31，命名之前仅有绑定记录。SDK 工作线程第一次屏障却写成：

```text
oldLayout = TRANSFER_SRC_OPTIMAL
newLayout = GENERAL
srcAccess = TRANSFER_READ
dstAccess = SHADER_READ | SHADER_WRITE
srcStage = dstStage = ALL_COMMANDS
```

这次屏障发生在首次 NGX 输出工作之前；当时没有把新建纹理从 UNDEFINED 转成 TRANSFER_SRC 的屏障。首次提交的验证消息因此要求 TRANSFER_SRC，而验证层记录仍为 UNDEFINED。后续使用中的 GENERAL、复制和 TRANSFER_SRC 转换则继续发生。

Rust 适配只在固定 SDK 下游路由上生效。它要求匹配创建参数、成功绑定和两种精确名称，再检查第一次屏障的布局、访问范围、阶段、子资源和忽略队列族。将该屏障的旧布局改为 UNDEFINED、源访问改为 0，其余字段保留。任何先前屏障或图像复制都会消耗新建状态；销毁和设备退休清除记录。普通游戏纹理、原生交换链、格式不符或已经使用的纹理不适配。本修复没有新增提交、fence 或 CPU 等待。

规范允许旧布局为 UNDEFINED 或实际当前布局；使用 UNDEFINED 可以丢弃原内容，因此适配必须限制在这两张首次使用的输出上。[Vulkan 图像布局转换规范](https://docs.vulkan.org/spec/latest/chapters/synchronization.html#image-layout-transitions)。这不是对已写入结果的末尾屏障重新声明布局。

默认启用；`NS_STREAMLINE_SDK_OUTPUT_INIT=0` 用于关闭对照。`NS_STREAMLINE_SDK_LAYOUT_TRACE=1` 启用每张 SDK 图像最多 32 个早期调用的证据，销毁记录保留截断计数。调试跟踪不是完整 Vulkan 调用捕获器；本适配依赖固定 runtime 及已观察到的首次屏障契约，不推广到其他 SDK 版本。

## 独立实机对照

固定 RTX 5070 Ti Laptop、驱动 610.88、VVL 1.4.363.0、Streamline 2.12.0 / downstream-v3。使用既有 SDK 合成运动窗口，1280×800、固定深度和零运动向量；用户点击窗口使其处于前台，程序采样 600 帧，末帧关闭 FG，然后等待、销毁及关闭 SDK。NR 未初始化，未运行模拟器，不作为正式画质或性能测试。

新增 `--sdk-fg --validation-dir` 诊断检查固定 VVL 的 manifest/DLL hash，启用核心及同步验证并保存原始消息。窗口不能取得前台时先等待，不把后台运行当作实际 FG。严格版本隔离 FG，避免此前命令/resize 实验的消息干扰；诊断设备启用 SDK bookkeeping 使用的 privateData，代理后缓冲包含允许图像 view 的 COLOR_ATTACHMENT usage。

| 会话 | 首次布局修正 | 首次布局错误 | 总错误 / 警告 | SDK 报告呈现 | 输入等待成功 | 原生呈现退休 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| validation-004 | 关闭 | 2 | 22 / 0 | 644 | 600 | 643 |
| validation-005 | 开启 | 0 | 20 / 0 | 1,199 | 600 | 1,198 |

004/005 的测试 DLL 和诊断 EXE 完全相同，只改变修正开关。005 恰好修正两次首次屏障；两条布局消息消失，没有按 ID 或文本过滤消息。两轮实际 FG 均有非零 timeline 和 SDK 工作线程呈现；原生呈现 fence 全部完成，SDK 关闭返回 0，子进程正常退出 1（严格错误检查失败）。SDK 报告呈现数与原生退休数是不同证据，两者相差 1 原样保留，不用于显示扫描验收。前台/预热时间不同，因此也不以呈现总数比较性能。

原始会话位于忽略目录 `src-tauri/target/sdk-fg-layout-validation-001` 至 `005`，输入、消息全文、首屏障、结果和原始文件摘要见 [summary.json](summary.json)。001 未取得前台、FG 未实际活动，并带旧诊断代理布局错误，最终 abort；002 的布局跟踪确认首次屏障，但此前实验占用了布局 ID 的默认重复消息配额，因此不能用来计算 clone 布局消息数；003 初版修复已消除首次布局错误，另有诊断 privateData/usage 错误。这三轮失败记录保留，不作为最终对照。

## 保留的问题与检查

004/005 除两条首次布局消息外，仍有两个 SDK/NGX 同步错误类别：

- `READ_AFTER_WRITE`：NGX 转换 fake-swapchain-buffer 到 GENERAL 后，实际复制读取要求 TRANSFER_READ，但屏障仅覆盖 shader 访问。
- `WRITE_AFTER_WRITE`：`nv.ngx.dlssg.resource` 的清除、输出复制或后续转换使用 transfer 访问，现有屏障只覆盖 shader 读写。

两类各收到 10 条错误后达到验证层默认重复消息限制；20 是报告条数，不代表只发生 20 次，也不能作为新修复抑制后的计数。这些消息明确来自独立 SDK/NGX 路径，不归入 Ryujinx 原有问题。它们尚未修复；原始 strict gate 和 SDK error 检查仍返回失败，不宣称零错误验收。真实 NVOF 游戏链、NR 失败回退、长期画质以及性能仍需分别验证，本轮没有再次进行八组合游戏测试。

136 项 Rust 测试通过、1 项硬件专用测试忽略。crate 的 `cargo fmt`、宿主及显式 Windows all-features/all-targets 检查、默认和 SDK-only 检查，以及主应用宿主/Windows 检查通过，无编译警告或错误。原生 NR/NGX 构建保留动态 CRT 选项。本轮只修改 `src-tauri` 下 Rust 侧代码与证据，没有修改 SDK C++、运行库二进制、Ryujinx 或标准工具箱的安装包。

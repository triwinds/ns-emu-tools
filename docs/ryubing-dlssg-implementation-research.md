# Ryubing DLSS-G 分支实现调研

调研日期：2026-09-25。

## 结论与范围

本次以用户提供的 `D:\ryubing-dlss5-win-x64` 程序为研究对象，通过单文件包提取、托管程序集反编译和已有运行日志交叉核查实现。后续研究转向该分支的 Streamline / Vulkan 呈现路径，不再以修复 AIO 为主线。

核心结论：

1. 分支通过 Streamline DLSS-G 代理接管模拟器的 Vulkan 交换链，包括创建、获取图像、呈现和销毁；不是 AIO 的独立 D3D12 / DirectComposition 输出方式。
2. 查到的 FG 调用链没有接入真实游戏深度、游戏原生运动矢量、真实相机矩阵或 jitter，也没有提供分离 HUD 后的颜色图。
3. 分支存在可选的 VORT 屏幕空间运动估计实现，而且接到了 FG 输入端；估计与向 FG 供给估计结果由两个独立开关控制，默认均关闭。
4. 最近一次检查的运行日志显示：4× DLSS-G、常量深度、零运动矢量、Reflex 已启用、Vulkan 交换链已通过 Streamline 代理创建。开启 4× 不等于已证明持续输出四倍有效显示帧。
5. 用户报告该分支有明显效果。当前证据不能把效果归因于真实游戏引导数据；应继续核查其初始化、交换链、资源同步与帧调度实现。

静态调查没有启动或修改原程序。本次没有进行新的 GPU 测试、屏幕帧间隔测量或完整原生 DLL 逆向。结论限定于这个构建及已追踪的调用链，不外推到所有版本。

## 样本与分析产物

| 项目 | 标识 |
| --- | --- |
| 程序 | `D:\ryubing-dlss5-win-x64\Ryujinx.exe` |
| 日志版本标识 | `2026.911.47` |
| EXE SHA-256 | `BB518E2EC059D46DFAD7DF2DCDB3ACC608CC8B77BDE38E3AC0852EC3CEB103ED` |
| 打包方式 | .NET single-file，bundle 格式 6.0 |
| 包清单 | 196 项，其中 194 项标记为托管程序集 |
| 反编译工具 | ILSpyCmd `11.1.0.9782`，来自 NuGet |
| 本地分析目录 | `src-tauri/target/ryubing-inspect/` |

分析目录包含 `bundle-index.json`、提取的 `Ryujinx.Graphics.Vulkan.dll`、`Ryujinx.dll` 和以下反编译文件。没有把完整反编译源码或第三方二进制纳入文档目录。

| 文件 | 用途 |
| --- | --- |
| `Dlss.DlssFrameGenHooks.cs` | Vulkan 代理函数接入 |
| `Window.cs` | 交换链和每帧呈现调用方 |
| `FrameGen.FrameGenPipeline.cs` | 深度、运动矢量资源来源及 FG 输入标签 |
| `FrameGen.VortMotionEstimator.cs` | 可选屏幕空间运动估计 |
| `Dlss.StreamlineDlss.cs` | 帧 token、常量、矩阵、jitter |
| `Dlss.StreamlineFrameGen.cs` | DLSS-G 选项、资源标签、Reflex、状态统计 |
| `Dlss.NrStagePipeline.cs` | Vulkan ↔ D3D12 的升采样 / NR 桥接调用 |

上述文件位于临时构建目录，清理 `target` 会删除它们。下面的行号对应本次反编译结果；重新反编译时应优先按类型和方法名定位。因未补齐所有依赖，输出带有部分类型解析警告，不应把它当作可以直接构建的原始工程。

## 插帧输入：实际来源与默认行为

### 深度

`FrameGenPipeline.Snapshot` 创建自己的引导资源，并执行：

```text
ClearImage(cbs, _depthImage, _depthValue, 0f)
```

`_depthValue` 由 `RYUJINX_FG_DEPTH` 解析，默认值为 `0f`。这只是全图填充的常量，不是读取游戏深度缓冲。

证据：`FrameGen.FrameGenPipeline.cs:172`、`:783`。

另一个容易混淆的值是升采样 / NR 路径：`NrStagePipeline` 将自己的深度纹理填为 `0.5f`。该路径的常量不能当作 FG 默认值，更不能当作真实深度。证据：`Dlss.NrStagePipeline.cs:317`。

### 运动矢量

分支实现了 `VortMotionEstimator`，加载 `VortMvFeat.spv`、`VortMvDown.spv`、`VortMvCalc.spv`、`VortMvFilter.spv`、`VortMvFinal.spv`，通过计算着色器处理相邻画面。它是屏幕空间估计，不是游戏原生运动矢量。

| 开关 | 职责 | 默认 |
| --- | --- | --- |
| `RYUJINX_MV_ESTIMATE` | 启用运动估计 | 关闭 |
| `RYUJINX_FG_MV` | 将估计结果提供给 FG | 关闭 |

两个开关接受 `1`、`true` 或 `on`。要让估计结果进入 FG，需要同时启用，并且本帧估计成功产出。`Tag` 根据 `_feedFrameGen && _mvProduced` 选择估计纹理或零矢量纹理；失败会回退零矢量。

证据：`FrameGen.FrameGenPipeline.cs:262`、`RunMotionEstimation`、静态构造函数（`:756` 起）；`FrameGen.VortMotionEstimator.cs:119` 起。

估计器还为 NR 提供像素单位的运动图；FG 使用其对应的运动资源。应区分“有估计器”“本次启用估计器”和“估计结果实际送入 FG”，不能仅凭类名或旧更新日志判断运行状态。

### HUD、相机和 jitter

`StreamlineFrameGen.OnFrameEvaluated` 支持可选 HUD-less 资源标签，但实际调用方 `FrameGenPipeline.TagCore` 传入的是 `default(DlssTexture)`，即空资源。接口支持不等于已经实现 HUD 分离。

`StreamlineDlss.PrepareFrame` 调用 `BuildConstants(..., 0f, 0f)`；相机相关变换矩阵填写单位矩阵，其余相机参数也是固定设置。当前 FG 路径没有输入真实相机数据或游戏 jitter。

证据：`FrameGen.FrameGenPipeline.cs:329`；`Dlss.StreamlineFrameGen.cs:728` 起；`Dlss.StreamlineDlss.cs:663`、`:670` 起。

## 实际调用链与呈现方式

```text
Window.Present
  ├─ FrameGenPipeline.BeginFrame
  ├─ Snapshot / Estimate（按开关和画面条件执行）
  ├─ NrStagePipeline.Run（选择相应缩放路径时执行）
  ├─ FrameGenPipeline.Tag → TagCore
  │    ├─ StreamlineDlss.PrepareFrame：frame token + 常量
  │    └─ StreamlineFrameGen.OnFrameEvaluated
  │         ├─ 绑定功能、启用 Reflex、设置 FG 选项
  │         └─ slSetTagForFrame：深度 + 运动矢量，HUD-less 为空
  ├─ 提交命令与同步资源
  ├─ StreamlineFrameGen.BeforePresent
  ├─ ActiveSwapchainApi.QueuePresent（Streamline 代理）
  └─ StreamlineFrameGen.AfterPresent：标记 + slReflexSleep + 统计
```

这只是已读代码的流程摘要，部分阶段受尺寸、缩放模式及失败回退条件控制，不代表每帧无条件执行所有阶段。

### 交换链接管

`DlssFrameGenHooks` 列出六个代理接口：

- `vkCreateSwapchainKHR`
- `vkDestroySwapchainKHR`
- `vkGetSwapchainImagesKHR`
- `vkAcquireNextImageKHR`
- `vkQueuePresentKHR`
- `vkDeviceWaitIdle`

证据：`Dlss.DlssFrameGenHooks.cs:12`；`Window.cs:730`、`:922` 起。

创建交换链时，FG 路径优先选择 Immediate；代码还包含不可用时的回退选择。检查到的这次日志实际使用了 `PresentModeImmediateKhr`，不是仅设置了 UI 开关。

证据：`Window.cs:402` 起；运行日志第 258–259 行。

### Reflex 与生命周期

`EnableReflex` 设置低延迟模式；`BeforePresent` 发送标记，`AfterPresent` 发送呈现结束标记并调用 `slReflexSleep`。可确认这些调用存在，但不能据此推导输入延迟数值或证明所有标记时机都正确。

尺寸变化会停用 FG，并等待尺寸稳定后重新启用。状态统计中还存在持续 x1 时的可选 watchdog 重启逻辑。这些分支需要后续结合设备初始化和销毁代码继续审查。

证据：`Dlss.StreamlineFrameGen.cs:485`、`:552`、`:800`、`:806`、`ReportCounter`。

### 不要把 D3D12 桥接和 FG 混为一谈

包内的 `nr-stage.dll` 与 `NrStagePipeline` 承担升采样 / NR 的 Vulkan ↔ D3D12 桥接。已追踪到的 FG 呈现入口则是 Vulkan 交换链的 Streamline 代理。不能仅因存在 D3D12 桥接 DLL，就认定 FG 使用与 AIO 相同的独立输出窗口。

## 运行日志交叉核查

检查文件：

```text
D:\ryubing-dlss5-win-x64\portable\Logs\Ryujinx_2026.911.47_2026-09-25_14-44-09.log
```

| 日志行 | 观察 |
| --- | --- |
| 99–109 | 初始化 Streamline、注册 Vulkan 设备、安装交换链相关代理 |
| 258–259 | 切换到 Immediate，代理创建交换链 |
| 263 | `depth constant, MV zero, DLSS-G optical flow` |
| 265 | Reflex 低延迟模式已启用 |
| 266–267 | 实际经过代理呈现与取帧 |
| 268 | DLSS-G 设置为 4×，日志明确提示 HUD 已包含在颜色画面中 |
| 269 | 初始状态返回呈现字段为 1；该单点不足以判断后续持续倍率 |

上述运行记录与反编译所得默认输入路径一致，不能用它证明可选 VORT 路径的实际质量。

## FPS 显示的证据边界

`ReportCounter` 读取 `slDLSSGGetState` 的 `NumFramesActuallyPresented`，再乘以应用呈现频率，估算输出 FPS。它不是屏幕扫描输出的测量值。

代码只有在该字段至少为 2 时才更新 `UiMultiplier`，所以不能把某一时刻 UI 上保留的倍率直接理解成当前持续有效倍率。这里记录的是实现行为，不在未核对 SDK 字段语义和完整状态刷新逻辑前断言统计一定错误。

证据：`Dlss.StreamlineFrameGen.cs:853` 起。

## 对独立 Vulkan 层路线的意义

这个分支证明了“修改模拟器后端接入 Streamline”的具体做法；它没有证明“完全不改模拟器、仅用独立 Vulkan 层”已经可行。

可借鉴的重点是设备创建前的接入、交换链代理、资源标签、同步和生命周期。真实深度、原生运动矢量和 HUD 分离不是本构建已使用的前提，不能以缺少这些数据直接否定复现方向。

ReShade 可以作为辅助界面或图像处理组件，但普通 `.fx` 无法承载这里的设备和交换链接管职责。独立层还必须解决装载顺序、Vulkan 分发表、代理递归、设备扩展、队列与资源所有权等问题，不能把模拟器内的几个调用原样搬过去就视为完成。

后续建议按以下顺序推进，不再以旧 AIO 调研替代分支实现分析：

1. 补齐 `Streamline`、`VulkanRenderer` 及设备创建相关类型的调用链，查清初始化顺序、扩展与特性启用、支持检测和回退。
2. 追踪每帧资源标签对应图像的布局、有效期、队列同步，以及交换链重建和设备销毁顺序。
3. 核对本构建使用的 Streamline 版本及结构体布局，特别是状态字段、Reflex 标记和手动接入要求。
4. 明确哪些信息由模拟器内部直接提供，哪些能由独立 Vulkan 层获取，形成移植边界清单。
5. 若实现原型，先验证透明转发，再验证固定窗口、单交换链、2× FG，最后测量有效新帧与显示间隔；不能只验收初始化成功或 FPS 数字。

## 参考资料

- [分支网站](https://ryu.dotslash.pro/)
- 样本目录内 `CHANGELOG.md`：r1 记录交换链代理与扩展；r4 修正 r3 关于运动估计已送达 FG 的说法。
- 样本目录内 `patches/nr-stage-mvlowres.patch`：升采样引导标志修正，不是原生深度或运动矢量提取实现。
- [NVIDIA Streamline 手动接入说明](https://github.com/NVIDIA-RTX/Streamline/blob/main/docs/ProgrammingGuideManualHooking.md)
- [NVIDIA DLSS-G 接入说明](https://github.com/NVIDIA-RTX/Streamline/blob/main/docs/ProgrammingGuideDLSS_G.md)
- [Khronos Vulkan Loader 层接口](https://github.com/KhronosGroup/Vulkan-Loader/blob/main/docs/LoaderLayerInterface.md)

在线主分支文档会变化；复现时应固定与所用二进制相匹配的版本。本地反编译与日志证据优先于网站概述和历史更新日志。

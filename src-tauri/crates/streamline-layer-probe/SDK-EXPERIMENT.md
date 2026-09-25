# P0：初始化 SDK 后的分发表实验

日期：2026-09-25。状态：该诊断实验通过；P0 未通过，FG 未启用。

## 结果与证据

最终代码在 streamline-sdk-device-003 会话实测成功。文本证据保存在 [evidence/sdk-initialized-2026-09-25](evidence/sdk-initialized-2026-09-25/inputs.json)，不包含 DLL。完整 SDK 日志和 Loader 层链日志一并保留，inputs 记录宿主、诊断层、Loader 和七个运行库的 SHA-256。

- slInit、三个功能的 slGetFeatureRequirements、slIsFeatureSupported、slSetVulkanInfo、slShutdown 均返回 0。
- 主线程与宿主创建的工作线程分别调用初始化后的 SDK vkDeviceWaitIdle，均返回 VK_SUCCESS。
- SDK 地址位于会话内 sl.interposer.dll，system GDPA 地址位于诊断层 DLL，保存的 next 地址位于 nvoglv64.dll。
- layer.jsonl 的 phase 7、8 分别记录一次 vkDeviceWaitIdle，线程不同；正常关闭后 instance/device 分发表计数归零。
- SDK 日志同时记录两次 DLSS-G slHookVkDeviceWaitIdle / flushAll。这次确实执行了初始化后的代理和插件钩子，不只是查询地址或执行未初始化透传。
- 没有交换链、没有应用渲染提交、没有生成帧，也没有验证 SDK 自己的 FG 工作线程。

这直接证实了：当前冻结 interposer 经 slSetVulkanInfo 建立的 idle 调用路径会重入本层。若在本层入口无条件转回同一个 SDK 代理，将形成递归。本实验的层只转发下一层，所以正常结束。

这不是“独立层不可能实现”的证明，也不是要求修改模拟器的结论。仍需找到可审核的 SDK→next 路由机制，并覆盖 SDK 自己的线程、NGX 和各个代理入口；C ABI shim 本身只解决类型边界，不改变 SDK 内部取址。

## 真实需求和队列配置

查询发生在创建 Vulkan 实例之前，功能列表为 DLSS-G (1000)、Reflex (3)、PCL (4)。不请求 SR。Preferences flags 为 133，手动接管、禁用 CL 状态跟踪、按帧标签；关闭 OTA 下载和加载。诊断宿主使用自己的 Custom engine/version/project ID，applicationId=0。

| 项目 | DLSS-G 实测 |
| --- | --- |
| 附加 graphics / compute 队列 | 1 / 2 |
| native optical flow 队列需求 | 1；本实验选择文档允许的 interop 路径 |
| Vulkan 1.2 features | timelineSemaphore、descriptorIndexing、bufferDeviceAddress |
| Vulkan 1.3 features | synchronization2 |
| 实例扩展 | external_memory_capabilities、external_semaphore_capabilities、get_physical_device_properties2，均带 VK_KHR_ 前缀 |
| 设备扩展 | 原始 11 项清单见 sdk-requirements.json |
| VSync / HWS | 返回 flags 要求 VSync off、硬件 GPU 调度 |
| 返回的 requiredTags | 0、1、2、23；仅记录原始结果，未用于推断哪些标签可省略 |

Reflex 另要求 VK_NV_low_latency；PCL 没有额外 Vulkan 扩展或队列。本机 NVIDIA RTX 5070 Ti Laptop、驱动 610.88，三个功能 adapter support 均为 eOk。

Vulkan API 1.3；family 0 分配四条队列：宿主 index 0，SDK graphics index 1，SDK compute index 2、3。每条用途互不重叠，不照搬参考分支把 graphics/compute 起始索引都设为 2 的做法。所有返回扩展先核对枚举支持再启用；四项特性先查询支持再置 VK_TRUE，未知特性名直接拒绝。

固定 SDK manual-hooking 文档 5.2.1 明确说明没有 native OF queue setup 时使用 optical-flow interop。因此本实验保留返回的 VK_NV_optical_flow 扩展，但不启用 native opticalFlow 特性/队列，VulkanInfo.useNativeOpticalFlowMode=false。此设置下 slSetVulkanInfo 成功，不代表实际光流/FG 已验证。

## ABI 和构建边界

C++ 只负责构造 SDK 类型、调用动态取得的函数、复制需求字段并截住 C++ 异常。没有新增 C++ 呈现运行时。Rust 持有所有 Vulkan 生命周期和诊断状态。

| 类型 | 固定头文件构造的版本 | MSVC x64 实测大小 |
| --- | --- | --- |
| Preferences | 1 | 144 |
| FeatureRequirements | 2 | 184 |
| VulkanInfo | 3 | 96 |
| 自有 C 需求副本 | 固定内部布局 | 33116 |

自有副本限制每类 64 个名字、每个 128 字节、64 个 tag。超长、空指针或溢出直接失败，不截断；Rust 在解释时再次检查长度和 UTF-8。SDK 指针不跨查询持有。SDK 版本值由头文件直接提供。

Streamline 头文件复用 preflight 哈希清单。Vulkan-Headers 固定 v1.4.341 / b5c8f996196ba4aa6d8f97e52b5d3b6e70f7e4e2，新增完整 include 文件哈希清单。构建时只从核验后复制到 OUT_DIR 的头文件编译；默认 feature 不需要 C++ SDK 头文件。

## 尚未消除的 SDK 运行警告

完整 sl.log 中保留以下警告，sdk-device-result.json 也提取原文：

1. slInit 提示可能已经调用 DX/VK API。宿主实际在 slInit 前只加载 DLL、取 SL 函数地址并读取桥接 ABI，没有调用 Vulkan API。固定源码 sl.api/sl.cpp 中 ConfigureLogOverrides 先于 hasInterface 检查，非 production 分支可以在配置读取时调用 getInterface；这是源码中的可能解释，尚不能断言冻结二进制的警告一定由此造成。未屏蔽此警告。
2. 未启用 VK_EXT_debug_utils，所以禁用 debug names/markers；不影响本次 idle 观察。
3. sl.common 的 CmdBindPipeline、CmdBindDescriptorSets、BeginCommandBuffer hooks 显示 NOT supported。当前测试未执行这些路径；不能从 idle 成功推断命令记录或完整 FG 功能正常。

编译无警告不等于 SDK 运行无警告。未安装/启用 Khronos validation layer，本次也不声称 Vulkan 验证错误为零。SDK 日志含 NGX telemetry 行；关闭 OTA 不等同于禁止所有运行库网络活动。

## 验证与后续门槛

- cargo fmt 通过。
- 默认和 all-features 的 host、显式 x86_64-pc-windows-msvc cargo check 均通过，无编译错误/警告。
- all-features 六项测试通过，覆盖 Loader ABI、需求副本边界、队列溢出、缺失/重复/错误线程证据等。
- 原有默认基线/诊断层模式回归通过（streamline-probe-005），仍各两轮生命周期、共 12 次原生呈现。
- 本次 SDK device 模式最终会话成功；sdk-device-001、002 为开发过程，最终证据来自 003。

下一步先明确 SDK→next 的合法路由方案，不能仅在主线程加 TLS 布尔值后宣称解决异步重入。随后在 FG 关闭状态扩展 SDK surface/交换链代理生命周期、重复启动和销毁实验，并核查上面的 SDK 命令钩子警告。窗口操作前停用路径、实际生成帧、同步所有权和原版 Ryubing 构建身份仍是独立待办；不进入生产层/UI 集成。
后续进展：初始化期间临时选择 next idle 地址的候选方案已经实测排除，证据和下一步源码适配审查边界见 [ROUTING-EXPERIMENT.md](ROUTING-EXPERIMENT.md)。本报告的初始化基线仍然有效。

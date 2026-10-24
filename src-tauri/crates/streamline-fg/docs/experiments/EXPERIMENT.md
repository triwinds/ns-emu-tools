> 历史实验记录：当前目录、构建命令和运行状态以 [运行组件说明](../../README.md) 为准；部分原始日志已清理。

# P0 调用链实验：stock Streamline 的透传会重新经过本层

日期：2026-09-25。最终实验会话：`src-tauri/target/streamline-probe-004`。

## 结果

**已证实 SDK 透传路径回到系统 Loader 和当前层链。尚未初始化 SDK，不是 SDK proxy 的递归复现实验，也未证明完整接入可行或不可能。**

| 检查 | 结果 |
| --- | --- |
| 原生基线 | 同进程两轮完整生命周期，每轮三次原生清屏呈现成功 |
| 加诊断层 | 同样两轮、六次原生呈现；设备和实例映射每轮归零 |
| SDK GetDeviceQueue | 与 system 查询同址、与保存的 next 地址不同；调用确实进入本层 |
| 工作线程 SDK GetDeviceQueue | 同样进入本层，线程标识不同；不是 SDK 自有工作线程 |
| 保存的 next GetDeviceQueue | 返回同一队列句柄，phase=3 没有本层 GetDeviceQueue 事件 |
| GetDeviceQueue2 / AcquireNextImage2KHR | SDK 取址等于 system；原生变体实际运行成功 |
| surface GIPA | 返回系统 Loader 函数，创建调用进入本层 |
| surface 直接 DLL 导出 | 位于 interposer，与其 GIPA 返回地址不同 |
| SDK swapchain/present/idle 代理 | 地址位于 interposer；只记录，未执行 |
| SDK 初始化/代理生命周期/FG | 未验证/未启用，P0 仍未通过 |

机器：NVIDIA GeForce RTX 5070 Ti Laptop GPU，Loader 1.4.341.0，本次 vulkaninfo 报告 NVIDIA 驱动 610.88。隐藏窗口实际客户区 304×201，FIFO、SDR。小尺寸只用于原生调用链实验，绝不代表满足 DLSS-G 最小尺寸。

最终结果有 64 条诊断事件。地址带 ASLR，数值不跨运行比较；比较发生于同一进程内。完整地址、DLL 归属、原始轨迹、输入哈希和 Loader 层顺序保存在 [evidence/2026-09-25](evidence/2026-09-25)，未复制 DLL 到源码目录。

## 实际拓扑

```text
宿主 → SDK GDPA(device, GetDeviceQueue)
       → 系统 vulkan-1.dll 查询结果
       → 调用该函数
       → 系统 Loader / 当前层链
       → 本诊断层 GetDeviceQueue
       → 保存的 next GetDeviceQueue（本机位于 nvoglv64.dll）
```

`vkAcquireNextImage2KHR` 的 SDK 取址直接落在诊断 DLL；GetDeviceQueue 和 surface 的取址落在 Loader trampoline，仍由调用轨迹证明经过诊断层。不能仅凭地址所属 DLL 判断是否绕过了本层。

`vkCreateSwapchainKHR` 的 next 地址在本机属于 Loader，本身也不能据此判定错误重入；其原生调用成功到达驱动，说明必须区分合法的 Loader terminator 与从层链顶部重新进入。

原样把本层的代理 hook 指向 stock SDK，再让 SDK 从系统 GIPA/GDPA 建表，存在重新选中本层 hook 的实际风险。这次实验确认了该风险所依赖的“透传会回到当前层链”前提；没有运行初始化后的代理，因此不把它写成已发生无限递归或已证明必须修改 SDK。

## 环境与失败记录

本机存在 ReShade 隐式层重复注册，其中一份原始 DLL 加载失败。两个实验子进程明确排除 ReShade，原注册保持不变；AMD switchable graphics、NVIDIA Optimus 和 NV_present 驱动层仍在链中。证据中的环境变量与层日志用于重现这个前提。

首次会话 001 的基线成功，但 Loader 未发现带 `\\?\` 扩展路径前缀的 manifest 目录；已用 `dunce::canonicalize` 提供普通 Windows 路径修复。会话 002 验证基本实验；003 增加了地址 DLL 归属和完整自动断言后重新执行成功；004 明确使用 validation_layer_enabled 字段，并对最终代码重跑全部实验成功。

## 验证与剩余工作

`cargo fmt`、宿主 `cargo check`、显式 `x86_64-pc-windows-msvc` 检查及三项测试通过，编译无警告。测试覆盖 x64 Loader ABI、部分/重复/线程错误/next 重入证据拒绝和空报告拒绝。两组真实 Vulkan 实验通过。没有 validation layer 或最终画面测量，不声明生产透明层验收通过。

下一项可实施实验是加入按固定头文件编译的最小 C ABI bridge：在隔离宿主里查询功能需求，合规创建队列后调用 `slSetVulkanInfo`，记录它实际缓存的取址链；随后才考虑执行 FG 关闭的代理生命周期。若表内确实捕获本层入口，需设计显式 next-dispatch 注入或等价可证明方案，并覆盖 SDK 自有异步线程。不能通过 TLS 标记当前宿主线程就声称覆盖 SDK 异步回调。

这次没有修改 SDK interposer、初始化 DLSS-G、注册全局 layer 或启动模拟器。窗口前置停用、资源退休、SDK 实际需求和原版目标身份仍是独立门槛。
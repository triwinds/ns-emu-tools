# XeSS FG 性能修正（2026-10-18）

热切换版的 XeSS 路径使用一组 D3D12 命令资源，每帧开始/末尾均等待 GPU，
并在导入共享色彩后再次在 CPU 等待 Vulkan 栅栏，串行化了两个 API 的处理。
窗口每帧重复 SetWindowPos，固定引导纹理也在每帧重新上传。切到 XeSS 时还
设置了 DLSSG 的 eRetainResourcesWhenOff，使停用引擎保留了不必要的分配。

## 修正

- 三组 D3D12 allocator / command list，记录每组的完成值，只在即将复用时等待。
- Vulkan→D3D12 继续使用共享时间线栅栏在 GPU 上等待，省去相同依赖的 CPU 等待。
- 保留 Vulkan 私有命令缓冲复用时的栅栏；销毁前额外等待 D3D12 最终读取完成，
  再释放导入的 Vulkan 内存和信号量；显式效果控制改变时先退役异步 D3D12 工作。
- 固定零运动、0.5 深度纹理初始化并 GPU 回读校验一次，以后每帧只重新标记。
- 只在显示状态或尺寸改变时异步更新 XeSS 窗口，继续保留独立消息线程。
- XeSS 接管时释放 DLSS FG 的历史/输出分配。切回仍使用同一 Streamline 交换链，
  DLSS 插帧资源由 SDK 重新准备，短暂停顿时的原有 retain 行为保留。

## 性能对照

本地证据在 `build/xess-fg-performance-20261018`。基线为前一交付构建，
使用同一模拟器、同一游戏及输入长边 1920 / DLSS SR Quality、146%、L / NR C /
NVOF Quality / XeSS FG 2×。没有修改用户设置。统计启动后第 30～60 秒的
`xess_fg_frame.processing_cpu_us`，包含 XeSS 呈现和 Streamline 后续呈现处理。

| 构建 | 样本帧 | 中位数 | P90 |
| --- | ---: | ---: | ---: |
| 前一交付版 | 872 | 4.102 ms | 4.761 ms |
| 第一轮减少重复工作 | 901 | 3.840 ms | 4.410 ms |
| 三组帧资源 + 释放停用 FG 分配 | 902 | 3.357 ms | 3.980 ms |

中位数下降约 18.2%，P90 下降约 16.4%。场景自身约 30 源 FPS / 60 SDK 提交 FPS，
不能据此推断整机 FPS 提高同等百分比，也没有测量屏幕实际扫描帧率。显存策略
修正有代码及 SDK 标记验证，但没有取得可靠的逐进程显存测量，不报告显存节省数。

曾尝试只回收隐藏的 Vulkan 图像、跳过底层 WSI 呈现，以避免两套显示更新。
该实验中位耗时反而增加到 4.330 ms，因此完整撤回，生产呈现路由保持原样。
相关约束参考 [Khronos 图像回收说明](https://docs.vulkan.org/refpages/latest/refpages/source/vkReleaseSwapchainImagesKHR.html)。

## 验证

图形组件 cargo fmt、主机及 x86_64-pc-windows-msvc cargo check 通过，无警告；
兼容 xess-component / native-nr 构建检查通过。库单测 145 项通过，7 项独立硬件诊断
按原设计忽略。XeSS 选中时既不启用 DLSS 插帧，也不保留其分配，原有 DLSS 短暂停顿
继续保留资源；这一边界有单测断言。

实际游戏回归记录与最终交付哈希见同目录的验证 JSON。

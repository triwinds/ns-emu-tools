# 独立 XeSS FG：AMD Vulkan 游戏运行验证（2026-10-17）

本文记录独立 FG 验证阶段；后续的 AA+FG 组件与工具界面接入见
[独立 XeSS 组件说明](xess-component-integration.md)。

按用户选择，以《异度神剑 3》验证 **零运动向量、固定深度 0.5** 的 XeSS FG。
没有接入外部光流、DLSS 或 XeSS SR；本轮只接受运行、呈现、开关与退出结果，
不把占位引导数据作为运动画质或低延迟验收。

## 实现

SDK 3.0.2 的公开 FG 接口为 D3D12，运行库自报 FG 1.3.1、XeLL 1.3.2。
采用同显卡 Vulkan → D3D12 共享纹理/共享 fence → XeSS FG proxy swapchain。
DXGI 与 Vulkan 按唯一 LUID 匹配。没有让 AMD 图像经过 NVIDIA 显卡。
参考：[Intel SDK 开发指南](https://github.com/intel/xess/blob/v3.0.2/doc/xess_fg_developer_guide_english.md)。

`streamline-fg` 仅复用 Vulkan layer 和启动宿主；`xess-fg` 特性构建不启用
`sdk-bridge`，并在编译期禁止混合 Native AA/Streamline 特性。本轮独立运行库
模块为测试 layer、`libxess_fg.dll`、`libxell.dll`，未加载 `sl.*`、NGX、NVOF。
已有图形组件未替换，主程序 UI 未接入这个实验入口。

Vulkan 原生交换链转移到隐藏窗口，FG 的 D3D12 交换链显示到原游戏 HWND。
原生 acquire/images/present 保留；present 二进制等待信号量由 GPU 拷贝消费
一次，拷贝完成后向隐藏交换链 present，不重复等待。跨 API 用 timeline 值
`2n+1` 交付颜色、`2n+2` 归还资源。窗口变化时先回收旧 FG context，再重建。
退出时等待 GPU、释放导入资源、释放 proxy、依次销毁 FG/XeLL context，
并 join 控制线程，避免 Vulkan loader 卸载 DLL 后线程返回到已释放代码。

未注入 jitter，运动向量全零，深度为 0.5，矩阵为单位矩阵，无 UI mask。
初始化时将真实 GPU 引导纹理读回逐像素核对；上传平面独立按 512 字节对齐。
XeLL 是 SDK 必需依赖，present hook 的占位 marker 不代表接入了游戏的输入、
模拟阶段，也不能用于真实输入延迟结论。

## 验证范围

- AMD Radeon 610M，Nextendo 1.9.4，Vulkan；《异度神剑 3》实际版本 1.0.0，
  内部倍率 1、模拟器 AA None、原有 FSR 显示缩放保留，VSync 关闭。
- 游戏最终版本会话：`build/xess-fg-20261017/game-amd-zero-003`。
  测试 layer SHA256：`fbbe393463d2d5fb1756a83fd307e914ddc499ba03e495d8ac22f68ec20f7133`。
- 原窗口呈现纹理先为 1200×656，放大后为 2560×1391；送入 FG 的颜色分别为
  640×350、640×348。为控制核显成本，使用普通 blit 限制长边到 640，再显示
  到原窗口；这不是 XeSS SR，也不代表原生分辨率画质验证。
- 对应运动/深度纹理 320×175、320×174，GPU 回读均为 **0 像素差异**。
- 用户用手柄进入开场战斗/教学场景，操作和窗口放大后继续呈现。
  同一进程执行 FG 开—关—开。关闭 FG 保留同一桥接，因此关闭段不能当作
  完全原生渲染路径的性能基线。

按场景确认的保守时间边界排除早期加载，并排除每段前 32 帧后，
最终 640×350 处理尺寸的结果：

| 状态 | 计入的源帧 | 时长 | 源 present FPS | SDK 提交 FPS | 桥接 CPU wall p50 / p95 |
|---|---:|---:|---:|---:|---:|
| FG 开（前段） | 1483 | 50.194 s | 29.5256 | 59.0513 | 20.679 / 23.425 ms |
| FG 关 | 858 | 29.066 s | 29.4847 | 29.4847 | 16.615 / 18.917 ms |
| FG 开（后段） | 1455 | 48.956 s | 29.7004 | 59.4007 | 19.512 / 22.115 ms |

开启段每个源帧均报告提交 2 帧，关闭段均为 1 帧；共 **3796** 个计入的
源帧，FG 状态和原生 Vulkan present 全部返回 0。没有运行时失败事件。
游戏正常退出 **code 0**，设备/实例剩余数量为 0；恢复临时显卡选择至原值
`null`，保留用户当前窗口等状态。源帧接近游戏 30 FPS 上限，不能推导无上限
速度提升。采样时场景包含开场教学及操作，不是冻结负载的性能基准。

完整原始记录在会话目录的 `layer.jsonl`、`game-report.json`、
`loaded-graphics-modules.json` 和 `target-exit.json`；轻量证据另存于
`src-tauri/tools/xess-fg-diagnostics/evidence`，包含诊断二进制和运行库哈希。

## 独立 Vulkan 桥接验证

`amd-vulkan-fg-final`：640×360、180 个源帧，预热后 148 帧全部插帧成功，
SDK 报告 296 帧，源帧 40.031638 FPS、SDK 提交 80.063276 FPS；Vulkan 核心/
同步验证 0 错误/警告，两次颜色逐像素回读均 0 差异。

修正上传对齐后的 `amd-odd-size-alignment`：642×351、90 个源帧，预热后
58 帧全部插帧成功，116 帧提交，颜色回读 0 差异，Vulkan 验证 0 错误/警告。
两项均为已知平移、解析运动向量的合成场景，不能替代游戏运动质量验证。

D3D12 debug layer 在本机仍报 `DXGI_ERROR_SDK_COMPONENT_MISSING`；严格
`--validation` 会失败，没有将其算作通过，也没有修改系统安全设置。
游戏 layer 本轮未开启 Vulkan validation，不把独立程序的验证结论扩大到游戏。

## 复现与边界

入口、环境变量、开关与构建步骤见
[`src-tauri/tools/xess-fg-diagnostics/README.md`](../src-tauri/tools/xess-fg-diagnostics/README.md)。
`game_probe.py` 支持显式模拟器/游戏路径；`game_metrics.py` 排除切换后 32 帧，
并可用场景观察边界排除启动加载阶段。SDK queued FPS 是 SDK 提交数，
不是独立测得的屏幕实际扫描帧率。桥接串行等待 GPU，CPU wall time 包括
GPU 等待和 Present，不是纯 FG GPU 时间。

当前仍为 opt-in 实验：单交换链 present、SDR RGBA8/BGRA8 UNORM、图形队列，
没有完整设备丢失恢复。零运动/固定深度无法正确描述遮挡与相机运动，
所以这轮通过也只意味着能运行和提交生成帧，不意味着运动画质可交付。

中间会话 `game-zero-001` 暴露控制线程卸载竞态（退出 AV），已修复；
`game-amd-zero-002` 验证该修复后退出 code 0，但上传对齐尚未修正，
仅保留开发记录，最终引导数据与采样结论使用 `003`。

已运行 cargo fmt，独立 FG、Native AA、既有 native-nr 及 FG 诊断工具的宿主/
显式 Windows target cargo check；最终检查无警告。FG 特性下 98 项单测通过，
包括应用 Vulkan 特性链复制和未知节点拒绝。

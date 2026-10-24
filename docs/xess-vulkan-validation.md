# XeSS Native AA / SR Vulkan 验证（2026-10-17）

已完成独立 Vulkan 实机验证，未接入工具箱图形组件、模拟器启动链、插帧或低延迟。
新增程序位于 `src-tauri/tools/xess-vulkan-diagnostics`，直接调用公开 XeSS C API。

结论：XeSS Vulkan 可以作为独立重建后端继续研究，跨厂商执行路线成立。
当前无 jitter 的输入条件下，Native AA 有继续做游戏验证的价值；SR 的静态细纹
重建收益不能直接沿用标准输入的结果。尚不能宣称游戏画质改善或 FPS 提升。

## 版本和设备

- 官方 SDK：`intel/xess` 的 `v3.0.2`。SR DLL 自报版本为 **2.0.2**，与 SDK
  套件版本不同。
- 原始 `libxess.dll` SHA-256：
  `251659dd84a3e84de67c886a4186e01f3eca49b00641906fe38bb6b807e5d5b7`。
  本机 Authenticode 状态为 Valid，签名者 Intel Corporation。
- NVIDIA GeForce RTX 5070 Ti Laptop GPU，驱动 610.88：720p 验证、1080p/1440p
  画质和性能、1080p 退化输入。
- AMD Radeon 610M：640×360，Native AA / Quality / Balanced 的真实执行、输出
  回读和核心/同步验证。该分辨率仅验证跨厂商路线，不能用于与 NVIDIA 的性能比较。
- 未测试 Intel 显卡；未锁定笔记本频率、功耗或温度。

SDK、原始许可、第三方声明和头文件保留在 `src-tauri/target/xess-sdk-files-v3.0.2`。
`prepare-sdk.ps1` 按版本和 SHA-256 准备这些文件，运行程序再次验证 DLL 哈希。
未修改、逆向或修补 Intel DLL，未向现有组件安装目录写入文件。

## 测试方法与验收边界

合成场景包含斜线、细棋盘纹理、同心曲线、平滑背景和独立运动的遮挡物。
0–47 帧静止，48 帧开始亚像素平移及独立前景运动，112 帧显式切镜头并重置历史。
标准输入使用真实 Halton 子像素采样、解析运动向量、解析深度，或高分辨率
已膨胀运动向量且不传深度。其运动指引比模拟器最终画面的估算光流更理想。

保存第 47、95、112、143 帧的输入、XeSS 输出、原生无 AA、对齐 jitter 的双线性
放大和 8×8 空间采样参考图。比较在未截断的线性 RGB 中计算，PNG 显示时转为 sRGB。
指标不等同于主观画质，特别是细棋盘的对比度、锐化和模糊会显著影响 PSNR。

分别检查了四组 jitter 符号。当前直接 compute 生成输入的坐标约定使用 `(1,1)`；
不能直接套用另一个光栅视口示例的 Y 轴变换。低/高分辨率运动向量路径在相同
720p 条件下的 Quality 运动帧 PSNR 为 23.082 / 23.103 dB，输出基本一致。
这两条路径在这里分别使用输入/输出像素单位。

核心和同步验证均为 **0 错误、0 警告**：NVIDIA 低/高分辨率运动路径，以及 AMD
高分辨率运动路径。四条精确匹配的、主动禁用隐式图层的 loader 通知独立记录；
不计为 Vulkan 验证告警，未忽略其他告警。SDK 未报告 warning/error。
每帧等待有 30 秒上限；不确定的提交/等待状态会结束诊断进程，避免释放在用资源。

## 性能结果

下表来自独立、不截图的连续计时：每模式 1000 帧，排除前 32 帧，保留 968 个样本。
GPU timestamp 只包围 XeSS 命令；场景生成在另一个已完成的提交中，文件写入、
回读、参考图生成和呈现均不在计时区间。程序串行提交，不代表流水化游戏 FPS。

| 输出 | 模式 | SDK 查询的输入 | GPU 中位数 | GPU P95 |
| --- | --- | --- | --- | --- |
| 1920×1080 | Native AA | 1920×1080 | 1.173 ms | 1.508 ms |
| 1920×1080 | Quality | 1130×636 | 1.118 ms | 1.367 ms |
| 1920×1080 | Balanced | 960×540 | 1.104 ms | 1.353 ms |
| 2560×1440 | Native AA | 2560×1440 | 1.776 ms | 3.207 ms |
| 2560×1440 | Quality | 1506×848 | 1.747 ms | 2.054 ms |
| 2560×1440 | Balanced | 1280×720 | 1.740 ms | 2.091 ms |

同输出尺寸下三种模式的重建成本接近，Balanced 的意义主要在于上游能否少渲染
像素。当前试验没有减少模拟器的渲染工作，因此不能把 SR 处理直接称为提速。
1440p Native AA 仍有较长尾延迟；未控制系统调度、GPU 时钟和笔记本功耗，
不能将其归因于某个具体 SDK 内部阶段。

SDK 查询的临时纹理堆需求约为 1080p **70.78 MiB**、1440p **115.31 MiB**，
临时缓冲堆为 64 字节。这不包含 SDK 私有持久显存，也不包含验证程序的图像资源。
首次 1080p AA 初始化约 717 ms，随后上下文初始化约 25–36 ms；这包含本次进程
和驱动缓存状态，不能当作普遍启动耗时。

## 画质结果

静止细线和曲线的阶梯能减轻，但运动时存在变软、细纹对比度变化、遮挡边缘
和棋盘纹理失真。Native AA 在标准输入中没有稳定优于基线；不能仅因执行成功
就开放“画质提升”结论。Quality 相对 Balanced 保留细纹更好。

1080p 全图线性 RGB PSNR，单位 dB，值越高表示越接近此空间采样参考。
下面只比较同档位、同帧、同输入条件的 XeSS 和双线性基线：

| 条件与模式 | 静止帧 47：XeSS / 基线 | 运动帧 95：XeSS / 基线 |
| --- | --- | --- |
| 标准输入，Quality | 25.63 / 23.85 | 22.88 / 22.96 |
| 无 jitter，Quality | 19.94 / 21.81 | 21.69 / 20.64 |
| 标准输入，Balanced | 24.57 / 20.32 | 21.84 / 20.27 |
| 无 jitter，Balanced | 18.31 / 20.27 | 20.94 / 20.36 |

**无 jitter 时静态细纹重建明显退化。** 这与缺少跨帧子像素信息一致；
部分运动帧仍可能受益，不能简化为所有内容都变差。

Native AA 也不能简单套用这一结论：本场景无 jitter 的第 95 帧达到 27.28 dB，
原生无 AA 为 23.00 dB；标准 jitter 的 Native AA 为 22.26 dB。对应的图像显示
无 jitter Native AA 在这个合成场景里更稳定、更清楚。这个结果有场景和模型
滤波偏好，不能推广为“XeSS 不需要 jitter”或模拟器游戏已经验收。

切镜头重置的第 112 帧，标准 Quality / Balanced 只有 18.43 / 17.75 dB，低于
各自双线性基线 20.65 / 20.35 dB；第 143 帧恢复到 22.05 / 21.54 dB。
应保留重置与历史恢复状态，不能只挑静止收敛后的截图展示。

零 jitter 加平面深度的低分辨率运动路径也成功执行。它仍使用理想解析运动，
没有验证 NVOF 估计误差、UI、透明物、真实游戏切镜头或游戏中已有 TAA 的影响。

## 产物、复现和后续取舍

- 源码和完整命令：`src-tauri/tools/xess-vulkan-diagnostics/README.md`。
- 小型结果摘要：`src-tauri/tools/xess-vulkan-diagnostics/evidence/2026-10-17.json`。
- 本地完整数据：`build/xess-vulkan-20261017`，含逐帧计时、SDK/验证日志、PNG、
  2× 裁剪对照和运动 GIF。GIF 是有损调色板预览，以原 PNG 判断颜色和细节。
- 最终计时：`benchmark-1080`、`benchmark-1440`。
- 最终图像：`standard-1080`、`standard-1440`、`no-jitter-1080`、
  `flat-depth-no-jitter-1080`。
- 验证：`validation-low-003`、`validation-current-high`、`amd-610m-validation`。
  早期 `validation-low-001/002`、`validation-final-low` 等属于坐标和单位试验，
  不作为最终验收结果。
- 已运行 `cargo fmt`；独立诊断程序及 Tauri 主程序的 host / 显式
  `x86_64-pc-windows-msvc` 检查通过且无警告。该工作区的增量缓存硬链接告警
  用 `CARGO_INCREMENTAL=0` 消除后重新检查。未创建安装包或修改现有交付组件。

下一步优先在独立后端里做实际游戏的 **Native AA、零 jitter** A/B，验证是否
值得每帧约 1 ms 的成本。SR 保留为实验选项，先验证低分辨率原始画面来源和
实际光流质量；没有正确 jitter 时不承诺静态细节重建。暂不扩展 FG/XeLL。

官方参考：

- [XeSS v3.0.2](https://github.com/intel/xess/releases/tag/v3.0.2)
- [版本对应的 SR 指南](https://github.com/intel/xess/blob/v3.0.2/doc/xess_sr_developer_guide_english.md)
- [Vulkan 公共 API](https://github.com/intel/xess/blob/v3.0.2/inc/xess/xess_vk.h)
- [SDK 许可](https://github.com/intel/xess/blob/v3.0.2/LICENSE.txt)

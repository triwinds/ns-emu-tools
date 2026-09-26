# 参考分支 SR 实现核查

日期：2026-09-25。范围：只读反编译用户提供的参考构建、读取随包补丁与已有日志；没有启动游戏或做本次 GPU 验收。

## 样本与证据

- `D:/ryubing-dlss5-win-x64/Ryujinx.exe`，构建 `2026.911.47`，SHA-256 `bb518e2ec059d46dfad7df2dcdb3acc608cc8b77bde38e3ac0852ec3ceb103ed`。
- `dlss/nr-stage.dll`，SHA-256 `b9d97f846b27ca1c03a8e7a93b341179becd18836fe1ef24da80f105b66dd597`。
- 用 NuGet ILSpyCmd `11.1.0.9782` 重新提取/反编译 single-file 6.0 包中的 `Ryujinx.Graphics.Vulkan.dll`。产物在 `../../target/ryubing-inspect/`，属可清理临时文件。
- 主要类型：`Window`、`Dlss.NrStagePipeline`、`Dlss.NrStage`、`Dlss.ScalingPresentation`。反编译存在缺依赖导致的类型显示警告；不视作可编译原工程。
- 随包 `patches/nr-stage-mvlowres.md` 记录桥接源版本 `e4a2f2a537d93bfbfc748c1aa17a814c9a98895d` 加 `nr-stage-mvlowres.patch`。本次未取得完整原生桥接源码，不声称已经检查其全部实现。

## 核心结论

参考构建的 SR 使用 `nr-stage.dll` 的 Vulkan ↔ D3D12 共享纹理桥接，经 NGX 执行超分；FG 使用 Streamline `sl.dlss_g` 交换链代理。SR 主路径不是 `sl.dlss`，参考目录也没有 `sl.dlss.dll`。应先研究这条桥接路径，不能仅给现有 FG feature 列表加上 DLSS 就认为复现了参考实现。

## 颜色来源与调用顺序

`Window.cs:727,788–851`：选择 DLSS 缩放滤镜后，将 `TextureView`、源裁剪坐标、目标矩形及翻转信息交给 `NrStagePipeline.Run`。源是呈现缩放前的纹理，不是最终交换链图像；不代表拿到了游戏 TAA 前颜色。

`NrStagePipeline.cs:226–244,308`：验证裁剪与多采样状态，然后直接复制源裁剪区域至桥接颜色纹理。遇到无效源时明确拒绝重建已缩放的交换链。

调用链：

```text
Window.Present
  -> 源 TextureView + ScalingPresentation（crop / destination / flips）
  -> 可选：源画面的相邻帧运动估计
  -> NrStagePipeline.Run
       -> 源裁剪复制到共享 Color；准备 Depth / MV
       -> 提交 Vulkan 命令并在 CPU 等输入 fence
       -> nr_stage_evaluate(value, jitterX=0, jitterY=0, reset, hostInputsReady=1)
       -> nr_stage_wait(value, timeout=200ms)
       -> 输出复制/缩放回交换链，处理黑边和翻转
  -> FG 输入标签
  -> 提交呈现命令、Streamline 代理 present
```

SR 失败/尺寸等待时走普通 Blit；SR 初始化尚未稳定且本帧未产出时，调用方会跳过 FG Tag。SR 与 FG 的设置独立，选 DLSS 滤镜不是启用 FG 的前提。

## 尺寸和档位

`Dlss.ScalingPresentation.cs:17` 的 `ReconstructionSize` 保留源输入尺寸，决定 NGX 输出尺寸与 quality，不按档位主动降低游戏渲染分辨率。

令 `scale = min(destinationWidth/sourceWidth, destinationHeight/sourceHeight)`：

| 条件（依顺序） | NGX quality | NGX 输出 |
| --- | --- | --- |
| scale ≤ 1 或手动 DLAA | 5 / DLAA | 源尺寸 |
| 手动画质 0–3 且 scale > 1 | 指定档位 | 目标矩形尺寸 |
| Auto，scale ≤ 1.1 | 5 / DLAA | 源尺寸 |
| Auto，1.1 < scale < 1.4 | 2 / Quality | 目标矩形尺寸 |
| Auto，1.4 ≤ scale < 1.8 | 1 / Balanced | 目标矩形尺寸 |
| Auto，1.8 ≤ scale < 2.5 | 0 / Performance | 目标矩形尺寸 |
| Auto，scale ≥ 2.5 | 3 / Ultra Performance | 目标矩形尺寸 |

数字按固定 SDK 的 `external/ngx-sdk/include/nvsdk_ngx_defs.h` 核对；它们是 NGX 枚举，不能直接使用 Streamline DLSSMode 的数值。DLAA、原尺寸或缩小窗口时先按源尺寸重建，再适配窗口。

## 引导输入

`NrStagePipeline.cs:313–341,360`：

- Depth：R32 float，填充常量 0.5。
- Motion：RG16 float，默认零；可选接收屏幕空间估计得到的输入像素单位运动。
- 只有当前/上一输入帧身份匹配、完整源画面且无需历史重置时才采用估计；否则清零或重置历史。
- Jitter：Evaluate 明确传 0,0。
- 并未读取真实游戏深度、游戏原生运动矢量或抖动。随包更新记录承认合适的 TAA 输入点仍未解决。
- MVLowRes 补丁使 guide 尺寸 ≤ 输入尺寸时设置该标志，包含 1:1 DLAA；默认 AutoExposure 0x40 + MVLowRes 0x02 = 0x42。

## 共享资源与同步

`NrStage.cs` 动态解析 `nr_stage_probe/init/evaluate/wait/shutdown`。Init 返回 Color、Output、Depth、MV 及 FenceIn/FenceOut 的 NT 句柄；`NrStagePipeline.TryImportImage` 使用 D3D12 resource 外部内存类型导入 Vulkan，颜色输入/输出分别为源尺寸/重建尺寸，引导为输入尺寸。

每帧同步是 host-sync v1：CPU 等 Vulkan 输入 fence 后，以 hostInputsReady=1 执行，随后 CPU 等原生输出（200ms）。不是已完成的 GPU timeline 异步流水线。

尺寸/格式/quality 变化会触发重建；运行中先等 30 帧尺寸稳定，期间普通缩放。初始化失败最多尝试 5 次，重试间隔 180 帧。呈现映射变化、暂停等会重置历史。

`NrStage.Init` 当前传空 LUID；已有 `dlss/nr-stage.log` 提示原生桥接会选择第一块硬件适配器。移植时需要显式传 Vulkan GPU 的 LUID，不能沿用这一多显卡隐患。

## 日志交叉验证与限制

已有 `dlss/nr-stage.log` 记录：加载 `nvngx_dlss.dll`、初始化 NGX D3D12、SuperSampling.Available=1、共享纹理 Color 1920×1080 / Output 2373×1335 / guides 1920×1080、quality=2、flags=0x42、feature ready。此记录支持桥接和尺寸策略的静态判断，不证明本次测试成功或特定画质收益。

日志还记录 D3D12 EvaluateFeature 被 addon 挂接；该次会话可能叠加 RenoDX NR，不能把那次全部画质效果单独归因于 SR。项目官网说明 SR 与 NR 为不同功能，NR 另需 ReShade/RenoDX。

## 对当前 Rust 外置层的直接影响

最关键的移植工作是找到窗口缩放前的源图、crop、目标尺寸及翻转，而不是直接处理 present 时的最终图。现有 FG 层只掌握交换链边界；还需研究原版模拟器最终缩放 draw/blit 和对应资源跟踪，尚不能断言外置层可可靠取得所有源信息。

若只对最终交换链做处理，应明确称为另一种后处理原型，不能声称等价复现参考 SR。接下来应先验证源纹理定位，再固定原生桥接 ABI、显卡匹配、共享纹理与同步，并做 SR 独立和 SR+FG 对照。

在线交叉来源：https://ryu.dotslash.pro/ 。本次研究依据以该固定二进制的反编译、随包补丁及日志为主。

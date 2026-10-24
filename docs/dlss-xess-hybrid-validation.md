# DLSS SR／NR → XeSS FG 混合模式验证（2026-10-18）

本地组件 `local-7a48d4f3960f-a1a5a04499de` 通过显式
`hybrid-xess-fg` 特性启用。独立 XeSS 组件仍是
`local-xess-7504af9236e203ac`，未改变其 Native AA／FG 构建。

## 路径与入口

DLSS 页 → FG 高级配置 → 帧生成引擎 → XeSS FG。两套组件校验安装后，
从 DLSS 页重新启动游戏。处理顺序为可选输入缩放、可选 NR、DLSS SR／DLAA、
XeSS FG。XeSS 使用现有零运动向量和 0.5 固定深度，固定 2×；DLSS FG 和
Reflex 关闭，XeLL 启用。光流仍供 DLSS SR／NR 使用。引擎切换只在下次
启动应用，FG 暂停恢复和 SR／NR 参数继续使用现有会话控制。

Streamline 创建 Vulkan 设备；混合模式改用原生隐藏交换链，将处理后的颜色
交给现有 Vulkan → D3D12 XeSS 桥接。已固定哈希的 `sl.common.dll` 通过
插件入口接收 present 前后通知；C++ 桥接保留其 `bool&` ABI。首轮检查发现
遗漏通知导致 `presentCommon() was not observed`，补齐后重测无该错误。

## 验证记录

原始日志位于工作区 `build/hybrid-xess-fg-20261018`；`summary.json`
汇总各会话，`nr-controls-verification.json` 保存每次控制后的实际状态。

- 混合层与主程序均完成格式检查、主机和显式 Windows MSVC 目标检查。
  关闭增量缓存后检查无警告；图形层按 NGX 所需的动态 CRT 构建。
- 图形层单元测试 144 项通过，7 项硬件专项测试未在此轮重复运行。
- Vue 类型检查和生产构建通过。
- `game-1792326301`：DLAA → XeSS FG、FG 暂停恢复、1.5× SR 和 SR 关闭。
  无 SDK 错误，正常退出，设备和实例计数归零。
- `game-1792326558`：NR 与 DLAA、XeSS FG 同开；七次 FG／SR／NR 切换均由
  实际状态确认。无 SDK 错误，正常退出，设备和实例计数归零。
- `game-1792326985`：最终图形 DLL 再测 NR + DLAA + XeSS FG。
  844 次 SR 输出交接；首帧 XeSS 返回 `WARNING_TOO_FEW_FRAMES` 并提交 1 帧，
  随后 843 次均成功提交 2 帧。无负数 SDK 结果或失败的原生 present，
  正常退出，设备和实例计数归零。

最终交付通过逐文件 SHA-256 验证 NVIDIA 包 23 项、XeSS 包 13 项，
并确认主程序内嵌对应版本和哈希、图形 DLL 包含混合模式能力标记。
本地交付只生成 `src-tauri/target/release/NsEmuTools.exe` 与旁边的组件目录。

## 实测边界

SDK 排队帧数不等于独立测得的屏幕显示帧率。原始约 30 FPS 的会话可观察到
约 60 帧／秒的 SDK 提交计数，NR 同开会增加负载。Streamline 既有启动警告
仍存在；本轮游戏会话没有启用严格 Vulkan validation。

快速转头时的鬼影与闪烁需要在原场景进行视觉对照，本记录只确认组合执行、
控制切换与退出，不声称画质问题已消除。目前仍沿用 DLSS 启动器的 RTX 40／50
FG 硬件门槛，没有在本次变更中扩大到 RTX 20／30。

# FG 热切换验证（2026-10-18）

支持新专用 NVIDIA 会话中 DLSS FG / XeSS FG 双向切换。首次加载新版组件仍须重新
启动一次模拟器；旧独立 XeSS / XeSS Native AA 会话没有 Streamline 设备，保持重启限制。
当前 NVIDIA FG 路径沿用 RTX 40 / 50、Vulkan、SDR 的要求。

## 实现

- 新能力标记 `NS_EMU_FG_SWITCH_V1` 和启动参数 `--fg-switchable`。
- 启动时校验并锁定两套运行库，Vulkan / Streamline 交换链保持同一身份。
- 控制消息原子更新引擎、开关及递增修订；帧边界先完成旧 GPU 消费及处理提交，
  暂停 XeSS / XeLL 或 DLSS FG / Reflex，重置 SR、NR、光流和动态估计历史，再选择新引擎。
- XeSS 使用 Vulkan–D3D12 共享色彩 / 时间线栅栏，显示于独立消息线程的无输入子窗口。
  DLSS 时隐藏该层；两套 FG 不同时插帧。Streamline 仍处理每次 Present 的生命周期。
- 关闭时销毁 D3D12 运行时、向显示线程发送退出消息并等待完成，防止 DLL 卸载后仍执行代码。
- 实际引擎与已应用修订由遥测确认；界面展示待确认、失败重试，保留 DLSS 原有倍数能力。

## 卡死修复

早期实验把显示子窗口创建在模拟器渲染线程，该线程不处理窗口消息，导致窗口/UI
线程同步等待它。等待链见 `build/dual-fg-switch-20261018/hang-waitchain.json`：
UI 线程 53232 的 SendMessage 等待 `GUI.RenderThread` 56420。仅禁止父窗口通知没有解决问题。
最终改为独立窗口线程和持续消息循环，大小/显示变化使用异步窗口 API。
早期实验构建未交付。

## 自动检查

- 图形组件 `cargo fmt`，hybrid-xess-fg 主机及显式 x86_64-pc-windows-msvc 检查通过，无警告。
- 兼容构建 `xess-component` 和 `native-nr` 检查通过，无警告。
- 图形组件库单测 145 通过，7 个独立硬件诊断按原设计忽略。
- 主程序主机及 Windows 目标 cargo check 通过。
- 前端类型检查、ESLint、生产构建通过；路由测试 6 项、25 个断言通过。
- 主程序组件原子文件/安装测试 7 项通过；需正常 Windows 权限运行，沙箱临时文件替换会报拒绝访问。

## 实机游戏回归

Ryujinx / Vulkan，DLAA J + NR，零运动 / 固定深度 XeSS FG。完整记录保存在
`build/dual-fg-switch-20261018`；最终消息线程修复后的会话：

| 会话 | 初始 FG | 验证 | 退出 |
| --- | --- | --- | --- |
| game-1792331754 | DLSS | 六次往返，包含两引擎暂停切换和 XeSS 恢复 | 0，SDK shutdown=0，设备/实例均为 0 |
| game-1792331852 | XeSS | 双向切换，暂停切换，XeSS 恢复；从 XeSS 插帧状态关闭 | 0，SDK shutdown=0，设备/实例均为 0 |

每次切换沿用同一 swapchain，修订号和 backend 均收到实际确认；SR / NR 保持运行。
XeSS 活跃时 SDK 提交 2 帧，约 30 源 FPS / 60 提交 FPS。开发过程也观测到切回 DLSS
恢复生成帧；最后两轮在浏览器验收期间，DLSS 按后台保护暂停，因此不把后台样本计为生成帧。
汇总见 `verified-game-results.json`、`roundtrips.json` 和 `xess-start-roundtrips.json`。
没有进行独立显示扫描帧率测量或长时间画质评估。

## 界面验收

浏览器 mock IPC 验证实时切换等待实际回执后更新引擎，失败时保留旧引擎、禁用其恢复
开关并显示重试。旧会话/独立 XeSS 仍按能力提示重新启动。测试预览源码留存在
`build/dual-fg-switch-20261018/qa-sources`，不进入生产构建。

# XeSS Native AA 独立游戏验证（2026-10-17）

按用户要求，XeSS 面向非 NVIDIA 用户，独立于 Streamline、DLSS、NVOF 和插帧。
已完成 Vulkan 调用、AMD 执行和《异度神剑 3》真实画面处理验证。
用户随后终止了 DLAA 对比，不继续该项；本记录不包含三路画质排名。

## 实现边界

`streamline-fg` 仅复用已有 Vulkan layer/启动器宿主；新增 `xess-native-aa`
构建不启用 `sdk-bridge`，同时启用两者会编译失败。测试二进制位于独立的
`src-tauri/target/xess-native-aa/release`，现有已验证图形组件没有被替换。
独立入口不启动光流，不加载 Streamline/NVIDIA SDK，不检查 NVIDIA 厂商 ID。

Native AA 输入/输出严格 1:1。当前模拟器没有引擎运动向量及 jitter 注入，
这次使用零 jitter、零运动向量、每帧 `resetHistory=1`，以避免错误历史重投影。
这验证的是 XeSS Native AA 预设的逐帧处理，不能等同完整时域 AA 或重建质量。
颜色路径为 sRGB 解码、FP16 线性 XeSS、sRGB 编码；输出原样回写 swapchain。
当前仅接受 SDR RGBA8/BGRA8，使用独立图像、一次消费 present 等待信号量、
有界 fence 等待和设备销毁时资源回收。

## 实机记录

- SDK 3.0.2，SR 运行库自报 2.0.2；固定 DLL 哈希见上一份 Vulkan 验证记录。
- AMD Radeon 610M，640×360，144 帧，前 32 帧不计时：
  最后复测纯 XeSS GPU p50 **3.03144 ms**、p95 **3.30984 ms**；
  核心/同步验证 **0 错误、0 警告**，SDK 无警告。该项是合成场景执行验证。
- RTX 5070 Ti Laptop，Nextendo 1.9.4，《异度神剑 3》v1.0.0，
  模拟器内部倍率 1，工具 AA 关闭，原有 FSR 显示缩放保留。
  实际呈现纹理为 **2560×1301**；没有用显示分辨率冒充游戏内部原生分辨率。
- 独立进程模块检查只发现测试 layer 和 `libxess.dll`，没有 Streamline、
  DLSS 或 NVOF。该模块清单位于 `xess-independent-002/loaded-graphics-modules.json`。
- `xess-independent-004` 正常退出（code 0）。取得战斗画面和人工缓慢转镜头的
  **64 组同帧输入/输出**，均已完成 GPU fence 后读回。

固定镜头下的第二组开关采样，去掉切换边缘，排除截图阶段：

| 状态 | 样本 | 实际 present 平均 FPS | 间隔 p50 / p95 | 处理 GPU p50 / p95 |
|---|---:|---:|---:|---:|
| XeSS 开 | 373 | 30.0369 | 33.329 / 35.021 ms | 2.09530 / 2.81293 ms |
| 关闭（前段） | 370 | 30.0456 | 33.315 / 34.564 ms | 无附加处理 |
| 关闭（后段） | 253 | 30.0657 | 33.3345 / 34.743 ms | 无附加处理 |

开启段的纯 XeSS GPU p50/p95 为 1.95446/2.67770 ms，CPU 处理与等待
p50/p95 为 3.204/4.011 ms。另一开启段 GPU p50 为 2.64504 ms，体现笔记本
未锁定频率/功耗的波动。上述 GPU 区间包含解码、XeSS 和编码，不包含全部
呈现拷贝。开启后再关闭保留已分配资源。游戏约 30 FPS 上限，因此本轮不能
推出无上限性能或速度提升；不同显卡的分辨率和负载也不能直接比较。

同帧裁剪可见部分地表纹理变软，没有足够证据宣称轮廓、细线或运动稳定性
明确改善。暂不据此建议默认开启，现阶段结论仅为 **可以独立执行并输出游戏画面**。

## 证据与复现

完整本地证据：`build/xess-game-ab-20261017`。

- `amd-zero-motion-reset-final/report.json`：AMD 核心/同步验证及逐帧结果。
- `xess-independent-004/ab-metrics.json`：四段原始统计；表格采用其第二开启段。
- `xess-independent-004/frame-captures/003950-same-frame-crops.png`：战斗同帧，左关闭、右 XeSS。
- `xess-independent-004/frame-captures/008531-same-frame-crops.png`：转镜头样本，左关闭、右 XeSS。
- `layer.jsonl` / `target-inputs.json`：零运动、无光流、逐帧重置、二进制哈希和调用结果。

使用 `src-tauri/tools/xess-vulkan-diagnostics/game_ab.py xess` 启动独立测试，
`game_metrics.py <session>` 采样，`game_images.py <captures> <frame>` 导出无损图。
`xess-control.json` 控制 `enabled`，通过递增 `captureRevision` 请求 1–8 帧，
每进程最多 64 帧。不将截图/编码阶段用作性能数据。

已完成 cargo fmt、Native AA 和原有 native-nr 的宿主/显式 Windows target
cargo check、主程序宿主/Windows check，以及特性链复制/未知节点拒绝测试。
新增着色器已通过 glslang 和 spirv-val；真实游戏入口未开启 Vulkan validation，
不能把 AMD 独立程序的零验证错误扩展为游戏 layer 已通过严格验证。
测试修改的模拟器键盘映射已恢复为原手柄配置。

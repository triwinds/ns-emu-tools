# 原生 NR 性能对照（2026-10-01）

**本次约 7 FPS 的主要瓶颈是 Vulkan core/synchronization 验证层的 CPU 开销。** 同一存档地点、同一视角，NR → DLAA 在关闭验证层后达到当前约 30 FPS 的基础帧率。完整统计、原始文件摘要和运行库标识见 [summary.json](summary.json)。P1/P2 仍未验收；性能模式不会把缺失的验证或回读证据记为通过。

## 同场景对照

用户分别在两次进程中确认回到相同地点并保持视角。使用同一 Release 层 DLL、固定模拟器、NR runtime A、调用桥和 Streamline 2.12.0；RTX 5070 Ti Laptop / 驱动 610.88。NR、DLAA 输入为 1920×1080，窗口交换链为 2560×1335；NR 强度 1、SR scale 100、实际 FG 关闭、Reflex 限帧值为 0。未修改模拟器配置。

每组在控制版本已应用后等待三秒，再采样约 30 秒。GPU 时间戳和观察器计时在两次进程中保持开启；无验证层进程的已加载模块确认不含 Khronos validation。表中 FPS 是应用两次 `vkQueuePresentKHR` 间隔中位数的倒数，不代表显示器扫描率。

| 验证层 | NR | DLAA | 源跟踪 | 帧数 | 帧间隔中位数 | 对应 FPS | Present hook 中位耗时 |
| --- | --- | --- | --- | ---: | ---: | ---: | ---: |
| 开 | 开 | 开 | 开 | 215 | 137.78 ms | 7.26 | 18.69 ms |
| 开 | 开 | 关 | 开 | 230 | 129.80 ms | 7.70 | 13.62 ms |
| 开 | 关 | 关 | 开 | 254 | 117.39 ms | 8.52 | 1.23 ms |
| 开 | 关 | 关 | 关 | 266 | 112.85 ms | 8.86 | 1.20 ms |
| 关 | 开 | 开 | 开 | 908 | 33.34 ms | 29.99 | 13.95 ms |
| 关 | 开 | 关 | 开 | 907 | 33.33 ms | 30.01 | 10.86 ms |
| 关 | 关 | 关 | 开 | 908 | 33.33 ms | 30.00 | 0.64 ms |
| 关 | 关 | 关 | 关 | 908 | 33.34 ms | 29.99 | 0.64 ms |

全部八组稳定区间没有回读帧。基线使用 `--nr-readback`，六次样本都发生在这些区间之外；各次 CPU 读取、逐像素转换、比较和保存耗时约 5.5–5.8 秒。这能解释开场或输出契约变化后的短暂停顿，不能解释持续 7 FPS。分析脚本仍明确排除回读帧，不依靠启动后固定等待来假设它已完成。

## 耗时与利用率

无验证层、NR → DLAA 区间的 GPU 中位耗时：NR 准备 0.11 ms、NR Evaluate 7.82 ms、DLAA Evaluate 2.41 ms、SR 全段 2.55 ms。NVOF 全段约 1.86 ms，包含跨队列间隙，不能当成纯光流引擎耗时。NR GPU 查询只在既有完成 fence 后读取，不新增查询 WAIT；CPU fence 等待仍含上游 NVOF。

源画面观察器计时约 1.8–1.9 ms/帧，包括模型更新和锁等待，未包括其计时器、计数器与钩子分发本身。验证层开启、所有增强与跟踪关闭时，渲染线程仍达到约 0.97 个核心的占用；NR → DLAA 基线整进程平均占用约 1.79 个核心。多核机器的总 CPU 百分比因此可能很低，而关键线程已成为瓶颈；GPU 会等待 CPU 提交。NR → DLAA 的 hook 外耗时从 119.33 ms 降到 19.39 ms，是本次对照最大的变化。

所有无验证层组都达到约 30 FPS，所以不能通过这组数据推断无限帧率下的峰值吞吐量。测试为用户确认视角的实机场景，不是录制回放；只覆盖这个场景和分辨率。仍使用合成深度，并对轮换的物理源图像逐帧 reset，不能据此宣布时序画质合格。

## 实现与复测

启动器新增显式 `--nr-performance`，要求 `--native-nr`，并拒绝与 `--nr-readback` 或 `--validation-dir` 混用。这个开关只取消该次进程的 core/synchronization 验证层和回读，保留实际布局转换、同步、运行库校验和 NGX 调用。默认实验仍要求固定验证层；安装和 GUI 默认行为未改变。

计时使用以下子进程环境；两次运行保持一致：

```powershell
$env:NS_STREAMLINE_SOURCE_MEASURE='1'
$env:NS_STREAMLINE_SOURCE_TRACK_ONLY='1'
$env:NS_STREAMLINE_NVOF_TIMING='1'
$env:NS_STREAMLINE_NR_TIMING='1'
$env:NS_STREAMLINE_SR_TIMING='1'
```

基线按 [P1 参数](../nr-p1-2026-10-01/README.md) 启动，附加 `--nr-readback --validation-dir <固定 VVL Bin>`。无验证层对照去掉这两个参数，附加 `--nr-performance`。保留 `--native-nr --sr-mode dlaa --sr-scale 100` 和同一组绝对路径，使用新会话目录。实际命令、输入和全部原始数据保存在 ignored `src-tauri/target/nr-perf-validation-001` 与 `nr-perf-no-validation-001`；采样/分析工具位于 ignored `src-tauri/target/nr-perf-tools`。源跟踪停止是该进程内终止操作，只在 NR/SR 关闭后测试，之后不重新开启增强。

两次进程均先关闭 NR/SR，再请求正常关闭，退出码为 0，NR 参数/feature/snippet 与共享 SDK 均完成释放。基线 NR 成功评估 1818 次、NR → SR 完整交接 1555 次；无验证层会话对应 3898 和 2880 次。这套关闭步骤不证明此前的 managed WaitHandle 退出异常已修复。

基线仍有 228 条验证错误、30 条未核查警告，严格会话验收失败；日志原样保留。无验证层会话没有验证证据或读回证据，`validation_passed`、`session_verified`、`p1_passed`、`p2_passed` 均为 false；成功的调用链另记 `call_chain_completed`，不能替代输出或验证验收。

Rust 检查：`cargo fmt`、crate 与主应用的宿主和显式 Windows MSVC `cargo check` 均无错误或警告，SDK-only 检查通过。102 个测试通过，1 个专用硬件测试保持 ignored。

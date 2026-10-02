# 原生 NR：模拟器接入记录（2026-10-01）

已实现进程内 Vulkan NR 单独运行，以及 NR 输出进入 Streamline DLAA。**P1、P2 尚未验收通过。** 完整派生统计、运行库标识、原始日志摘要与失败结果见 [summary.json](summary.json)。

## 本轮实现

`native-nr` 是独立、默认关闭的 Cargo feature。启动器核对层 ABI、NR DLL、调用桥和验证层的 SHA256，再将运行库放进独立会话目录；不改动安装记录或 GUI。当前只支持固定模拟器构建、一个 Vulkan 设备生命周期和既定 SDR 交换链。

```text
未处理的真实呈现帧 → NVOF
已识别的源画面 → gamma RGBA16F 私有输入
                       ↓
             NR feature 18，同尺寸输出
                       ↓
          可选 SR / DLAA → 交换链 → Present
```

实际 FG 默认关闭；NR 不依赖 FG 活动或前台焦点。NR、SR、FG 在帧边界读取同一份控制快照，状态分别记录。NR 参数对象、feature 和图像独立于 SR；NR 停止不关闭共享 NGX core，最终由 Streamline 收尾。

输入保留源纹理实际尺寸、crop、flip、gamma 编码和呈现区域。可变 SRGB 源先复制编码字节至 UNORM，再裁剪到私有 gamma 输入；运动由呈现 UV 转换到内容区域，并以 NR 输入尺寸缩放。深度明确为合成常量。NVOF 缺少连续帧对时暂停 NR；恢复后的首次 NR 评估重置历史。

NVOF、NR、SR 的等待逐段消费，二进制 semaphore 不重复使用。NR 每次提交均等待有界 fence 后才复用输出；SR 直接读取本帧的 NR 输出纹理。未提交前的资源准备失败可退回原画面；已提交工作的完成状态不明时终止实验。尺寸重建和退休先等待设备静止，再释放 feature、参数和资源。

`--nr-readback` 在每个强度或输出契约的前三帧保存输入/输出 PPM，检查有限值、输出哨兵和逐像素差异。文件编号在进程内唯一。此操作及验证层会影响速度，本轮不作性能结论。

## 实测结果

硬件为 RTX 5070 Ti Laptop，驱动 610.88；使用 Streamline 2.12.0 和 VVL 1.4.363.0。固定 NR/bridge/SDK 契约沿用 [P0](../nr-p0-2026-09-30/coexistence-history.md)。

| 会话（位于 `src-tauri/target`） | NR 成功评估 | NR → SR 完整交接 | 退出结果 |
| --- | ---: | ---: | --- |
| `nr-p1-native-001` | 0 | 0 | 正常；初版前台限制导致 NR 暂停，保留未评估基线 |
| `nr-p1-native-002` | 3,521 | 0 | 正常；修正 NR 前台限制，SR/FG 均关闭 |
| `nr-p1-nr-sr-001` | 5,789 | 2,353 | 正常；超过 10 分钟，覆盖 NR-only、NR → DLAA 和强度切换 |
| `nr-p1-final-001` | 1,789 | 1,788 | 异常；最后一次交接未完成，不能当作清理成功 |

长测中，用户进入运动场景并反馈画面正常。18 次回读均有限、无哨兵残留：强度 0 的三次读回与本帧输入完全一致；0.5 和 1 的游戏画面读回有非零差异，连续输出摘要发生变化。最初三个样本是加载期间的黑画面，不用于运动或画质验收。NR-only 和 NR → DLAA 均有至少 300 次连续评估，实际 FG 开启帧数为零。

长测期间 NR 关闭 82 帧，运动暂不可用时暂停 24 帧。暂停中的下游重置信号被保留；SR 恢复后应用 reset。最终构建的十次 NR 开关均由实时状态确认：SR 一直活动，NR 关闭时 SR 读原始源画面，开启时读 `nr_output`，实际 FG 一直关闭。早期固定时长的遥测采样有超时，原始记录保留；开关效果另由实际帧日志核对，不将超时记录改写为成功。

## 验收阻塞与下一步

1. **验证层仍有错误。** 未评估基线有 192 条错误、30 条未核查警告；长测有 247 条错误、30 条未核查警告。包括 clip/cull 数组超限、静态线宽、查询/命令缓冲状态、资源使用中销毁及图像布局错误。这些类别也出现在未评估基线，但基线仍含自有层/Streamline，且场景不同，不能据此把所有错误归为模拟器或忽略它们。剩余警告包含未被 shader 使用的顶点属性，保留全文，尚未放宽验收。
2. **已修复两条接入层实例依赖错误。** 内部无窗口实例在启用 surface maintenance 时缺少 `VK_KHR_surface`；最终构建已补齐，复测中对应 `VUID-vkCreateInstance-ppEnabledExtensionNames-01388` 为零。原失败日志不修改。
3. **时间历史尚不合格。** 模拟器在同一呈现路径轮换源图句柄，当前保守策略每次变化都重置 NR；全部成功评估记录为 `SourceChanged`。NVOF 分析的未处理呈现帧对保持连续。下一步需要证明逻辑源身份和图像生命周期，再区分正常轮换与真正的来源切换，不能直接取消 reset。当前画面正常不证明时间稳定性和残影合格。
4. **正常退出不稳定。** 长测按 feature → 参数 → NR snippet → capability map → Streamline core → device/instance 顺序正常释放。最终复测请求关闭窗口后发生 `System.ObjectDisposedException`，对象为 `Microsoft.Win32.SafeHandles.SafeWaitHandle`，堆栈在 `Ryujinx.Ava.Systems.AppHost.<RenderLoop>b__125_1()`，第 1177 行调用 `EventWaitHandle.Set()`。进程退出码 `-532462766`，没有完整 NR/SR 释放记录；尚未证明它与层的时序关系，需要以相同场景和关闭步骤做未注入、SDK-off、NR-only 和 NR+SR 对照。
5. **其余 P1/P2 覆盖仍待完成。** resize、最小化和暂停恢复尚未完成系统验收；合成深度不能作为真实几何引导验收；实际 FG 活动下的组合、NR 失败回退、GPU 时间及无验证层性能采样尚未完成。后端通过前不进入默认安装与 GUI 开放。

原始消息中未见名称含 `dlssnr` 的内部图像错误；游戏图像仍出现 `09600`，这不等于所有 NR 新增资源都已证明无误。所有错误均写入严格验收报告，不屏蔽 VUID 或改动验证层状态。

错误计数为验证层已发出的消息数；其默认重复消息上限不代表错误的总发生次数。

失败会话也生成 `target-result.json`：重新计数完整 `nr-validation.jsonl`，校验摘要一致性、进程退出和 device/instance 清理。SDK 临时实例的早期摘要不能代替最终结果。`execution_verified` 只描述 NR 执行及交接证据；`session_verified` 还要求进程正常退出和严格验证零错误。`p1_passed`、`p2_passed`、画质与性能标记全部保持 false。

## 构建与复测

```powershell
cargo build --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --release --features native-nr --lib --bin streamline-layer-probe
```

`streamline-layer-probe --help` 列出实验参数。启用 `--target-probe --native-nr --nr-readback`，传入固定 target、layer、Streamline runtime、NR runtime、`nvngx.dll` bridge、VVL Bin 和全新 session 的绝对路径。默认 `--sr-mode off` 且不传 `--fg`；串联验证再使用 `--sr-mode dlaa --sr-scale 100`。各会话 `target-inputs.json`、`target-command.json` 保存实际配置。重新验收用 `--target-probe --verify <session>`；失败退出码应保留。

实时控制文件 `control.json` 独立使用 `nrEnabled`、`nrIntensity`（0..1）和递增的 `nrRevision`；SR 使用 `srMode`/`srRevision`，FG 使用 `enabled`/`revision`。建议写临时文件后原子替换；检查 `telemetry.json` 的 `fresh` 和已应用版本，避免把旧状态当作本次切换。

Rust 验证：100 个测试通过，1 个专用硬件测试保持 ignored；运行 `cargo fmt`，主应用与 crate 的宿主和显式 Windows MSVC `cargo check` 均无错误或警告，原 SDK-only 和 NR-diagnostics 构建也通过。

# 通用 NR 管线实施记录

日期：2026-10-05。对应 [实施计划](dlss5-generic-nr-pipeline-plan.md)。

当前已完成 P0 代码落点调研，以及 P2 基础 Look、P3 空间 Look、P4 双实例与重复输入复用、P5 可选时间 Look、P6 预设与配置集成的代码。P1 的完整模拟器实机验收、Eden/Citron 视频画质和性能验收仍待完成；不把代码实现、隔离 NGX 或固定输入测试等同于各阶段全部验收。以下按阶段保留当时的记录，当前状态以本摘要及最新章节为准。

最新 Citron 接入复测已实际执行两遍 NR 与空间/时间 Look，并修复扩展依赖、SDK 验证日志崩溃及 RG16F 运动格式问题；严格 Vulkan 验证和正常退出仍未通过，详见末章。

随后完成 Eden 两轮同类接入验证与纯模拟器退出对照。Eden 的 NR 执行、双遍与 Look 历史证据通过，但严格 Vulkan 验证失败；其退出异常在不加载增强图层的对照中也复现，详见最新 Eden 章节。

最新 Eden 组合复测已覆盖 NR/SR/FG 八种实际组合，以及 SR/FG 控制不变时 NR 的独立开关和强度调整。组合、双遍、Look 历史及交接检查通过；修复了初始关闭 NR 会话绕过 NR 验收的问题。严格验证与退出限制继续保留，详见末章。

computer-use 初始化故障已修复并实际验证窗口操作。随后完成关闭 Vulkan 验证和图像读回的 Release 标题场景 FPS 对照：七阶段均约 30 应用呈现 FPS，FG 阶段约 60 原生呈现调用 FPS；这不是游玩性能、逐阶段 GPU 耗时或扫描输出验收。

## P0：实际工程落点

- 运行组件已在本仓库：`src-tauri/crates/streamline-fg` 的 `native-nr` Vulkan 图层，直接调用固定 NGX/NR 接口，无需新增 feeder 或 ReShade add-on。
- `target_nr.rs` 负责私有输入/输出、同队列提交、NR fence 和 NR → SR semaphore；`nr_runtime.rs` 持有单一设备会话及独立 Feature；`nr_history.rs` 负责历史重置。
- 模型输入/输出为 gamma RGBA16F，同尺寸；SRGB backing image 的原生颜色经私有 UNORM 原始字节拷贝解释。Look 采用明确的 **SDR sRGB nonlinear** 契约，在线性 RGB 中处理，之后编码回同一模型输出格式。交换链颜色空间不符合此契约时保留 NR 输出并报告原因。本轮没有 HDR codec、曝光还原或 tone mapping。
- 深度为 `synthetic_constant`，运动为 NVOF 估算，并按现有 viewport/尺寸约定裁剪和缩放。没有新增原生深度或原生运动能力声明。
- 调度入口仍由 Present 驱动。P4 前置工作改为比较完成 crop/flip/codec 转换后的实际 NR gamma 输入，并为其维护独立颜色观察编号；重复输入复用输出，不再次调用 NR/Look。这不是模拟器原生渲染帧编号，两遍实例和真实游戏重复呈现验收仍未完成，本轮不开放多遍。
- `sdk/nr-contract.json` 固定 SDK commit `e8aaa6eaac968711fb62473d4ae8256dde20919b`；Streamline 为现有固定 2.12.0，NR 为 310.8.0，允许的模型/bridge SHA-256 沿用 `src-tauri/crates/streamline-nr-contract.rs`。本轮没有修改二进制运行库契约。
- 沿用仓库 GPL-3.0。Look shader 与调度代码为本轮新实现，没有复制计划中 RenoDX/Feeder 的源码；它们不成为构建或运行依赖。
- 现有一遍/NR → SR 基线见 `src-tauri/crates/streamline-fg/evidence/nr-tail-2026-10-01`。其中已记录严格 Vulkan 验证失败及尚未验证的暂停/窗口变化场景；本轮没有通过新的 Eden/Citron 游戏会话消除这些限制。

## P2：配置与行为

新增 `streamline_advanced.nr.look`，模型选项仍沿用现有结构：

| 字段 | 默认 | 范围及含义 |
| --- | --- | --- |
| `schemaVersion` | 1 | 只接受 1 |
| `enabled` | true | false 直接绕过 Look，保留调节值 |
| `amount` | 100 | 0–200%，0 返回 NR 输入 RGB，保留 NR 输出 alpha |
| `brighten` / `darken` | 100 | 提亮/压暗变化的独立增益，0–200% |
| `brightenCap` / `darkenCap` | 0 | 0 不限制；1–1600 为百分之一档的软上限 |
| `color` / `hue` | 100 | log opponent 平面中径向/切向变化的增益，0–200% |
| `shadows` / `midtones` / `highlights` | 100 | 按输入线性亮度划分区域，0–200% |

全部增益为 100%、软上限为 0 时不分配 Look pipeline、不 dispatch、不做额外颜色转换。恢复 Look 中性只恢复这组字段；关闭 Look 保留字段。旧配置缺失 Look 时补中性值，默认 Look 不序列化进旧协议选项；未知字段、未知 schema、越界、负数、小数和非数值输入被拒绝。

Shader 对最终一遍的真实输入 `P` 和输出 `N` 解码后求逐通道 log2 变化，将亮度投影和色度分开调整。近黑区域根据输入亮度和逐通道可信度保留 NR 结果，避免将 epsilon 驱动的变化放大。无色输入的色相没有定义，其色度变化由 color 增益控制。亮度软上限使用 tanh，在整体及区域增益之后施加；颜色变化和暗部拒绝不属于这项亮度投影上限的保证范围。

Look 使用两个 storage-image descriptor，逐像素写回 NR 私有输出，不读邻居、不增加图像纹理、不增加提交或 CPU 图像读回。输入始终是独立的私有图像，写回输出不会覆盖输入。只有显式诊断测试/既有读回开关使用 CPU 读回。

Look pipeline 在前一帧完成后的边界按需创建，并在 NR 资源释放前销毁。创建失败保留 NR 输出，记录 `preparation_failed`，在本资源生命周期内不逐帧重试；调整尺寸触发资源重建或重新专用启动后重试。模型参数变更按原策略重置历史；仅 Look 调整不重建 NR Feature、不重置 NR 历史，但重置接受新颜色的 SR/FG 历史。

## 界面、协议和诊断

- NR 高级配置增加 Look 滑块、软上限、独立旁路和“恢复 Look 中性配置”。草稿与已保存的嵌套 Look 配置使用独立副本。
- 新图层导出 `NS_EMU_NR_LOOK_V1` 能力标记，启动器与工具箱检查标记；实时状态新增 `nrLookSupported`，不支持的会话拒绝非默认 Look 设置。
- `nr.look` 状态包含请求、实际活动、旁路/创建失败原因、错误和颜色契约；NR 未运行时不报告 Look 活动。
- `target_nr_profile.gpu_stages` 增加独立 `look` 阶段，六个 timestamp 输出四项 GPU 时间：prepare、evaluate、look、output_and_optional_readback。消费诊断数据时按阶段名解释。
- 需要重新构建和打包 `native-nr` 图层及启动器后使用。线上 manifest 沿用已有版本，本轮没有发布或替换已安装组件。旧包会明确要求更新，不能用旧二进制验证 Look。

## 验证

使用本机 NVIDIA GeForce RTX 5070 Ti Laptop GPU、驱动 610.88、Vulkan SDK/VVL 1.4.363.0。Shader 由仓库缓存的 `glslc --target-env=vulkan1.1` 编译并通过 `spirv-val`；源码与 SPIR-V 摘要加入既有 `shaders/motion.json` 构建校验。

`nr_look_tests.rs::gpu_look_fixed_inputs` 是独立 Vulkan GPU 测试，不初始化 NGX、不启动游戏。19×3 的输入覆盖非整工作组边界、黑色、subnormal 暗部、明暗变化、颜色变化及白色，检查中性/旁路逐位一致、幅度 0 的源颜色选择、NR alpha、明暗抑制、软上限、高增益有限值、颜色/色相/区域参数效果及非有限模型 RGB 的拒绝。

测试通过时要求核心与同步验证为 0 错误、0 警告。系统隐式图层禁用提示单列；首次未隔离运行出现 ReShade 重复安装及 DLL 加载失败提示，不能作为干净验收。隔离后报告 6 条已识别的禁用提示，验证消息为 0。

从仓库根目录复现硬件测试（环境变量仅设置在测试终端）：

```powershell
$env:RUSTFLAGS='-C target-feature=-crt-static'
$env:VK_LAYER_PATH=(Resolve-Path src-tauri/target/vulkan-sdk-1.4.363.0/Bin).Path
$env:VK_LOADER_LAYERS_DISABLE='~implicit~'
$env:VK_LOADER_LAYERS_ENABLE=$null
$env:VK_LOADER_LAYERS_ALLOW=$null
$env:NS_NR_LOOK_GPU_VALIDATION='1'
cargo test --locked --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --features native-nr --lib gpu_look_fixed_inputs -- --ignored --nocapture
```

Rust 回归覆盖配置导入/旧协议、原子实时更新和 NR/SR/FG 历史边界。运行组件 `--all-features --all-targets` 共 177 项通过、5 项专用硬件测试默认忽略；Look 硬件测试另行显式执行，工具箱共享配置测试 1 项通过。按仓库要求运行 `cargo fmt`、工具箱与运行组件的 host/显式 Windows MSVC `cargo check`，均无错误或警告；运行组件额外使用 `--all-features --all-targets`。前端生产构建、类型检查及改动文件 ESLint 通过。

上述为 P2 首轮记录。后续接续实现 P3 如下，仍需对重新打包的图层完成 Eden/Citron 相同场景视频对照及启停/窗口变化回归。多遍需要可靠源帧身份和独立 Feature/中间 codec/history 的设计；时序平滑与 HDR 仍未实现。

## P3：空间 Look 代码与固定输入验证

本轮构建、shader/日志摘要和检查结果见 [验证摘要](../src-tauri/crates/streamline-fg/evidence/nr-spatial-look-2026-10-05/summary.json)。

2026-10-05 接续实现。新增 `look.spatial`，默认省略该字段，保持 P2 组件的配置格式兼容：

| 字段 | 默认 | 范围及含义 |
| --- | --- | --- |
| `enabled` | false | 独立开关，关闭后保留调节值 |
| `lighting` | 100 | 大范围亮度变化增益，0–200% |
| `detail` | 100 | 细小亮度变化增益，0–200% |
| `radius` | 8 | 1–32，NR 工作分辨率像素，指最外层采样偏移 |
| `halo` | 0 | 光晕抑制强度，0–100% |

`lighting` 与 `detail` 为 100%、`halo` 为 0 时，即使打开空间开关也绕过空间 dispatch 和首次纹理创建。仅调整半径不会使中性处理生效。整体 Look 关闭或 `amount=0` 时也不执行空间阶段。空间参数仍属于外部 Look，不改变 NR Feature 或 NR 历史。

`nr_look_band.comp` 使用当前 NR 实际输入/输出，在独立的一张 RGBA16F 纹理中写入低频亮度变化和亮侧边缘证据。固定 5×5 稀疏模板，以输入 log 亮度做双边引导并拒绝非有限邻居；半径不会按显示分辨率再次缩放。这是有限采样近似，较大半径可能遗漏很细的边缘，不能等同于完整大核滤波。

空间分离先只读 NR 输出，随后 COMPUTE → COMPUTE barrier 完成邻域读取并发布 band 写入，最后原有合成 shader 逐像素写回 NR 输出。没有并发邻域读写同一张颜色纹理，不增加队列提交或普通运行的 CPU 图像读回。

光晕抑制要求当前像素有负亮度变化、相对低频分量有负的局部残差，以及原输入中存在比当前像素更暗的邻域边缘。只减弱这部分局部残差；空间增益中性时不将压暗变化翻转成提亮变化。暗侧文字和整片阴影不自动视为光晕。该启发式不能区分所有新生成阴影与光晕，默认强度仍为 0，正式 P3 画质验收待真实视频完成。

Band pipeline/纹理按需创建，参数更新和关闭空间处理时保留并复用至 NR 资源生命周期结束，在 GPU 完成后释放。纹理数量最多新增一张；状态记录真实 Vulkan 内存需求的 allocationBytes。空间创建失败会保留基础 Look，缓存错误并停止逐帧重试；若基础参数也为中性，则直接保留 NR 原输出。尺寸重建或重新启动创建新的资源生命周期。

组件增加 `NS_EMU_NR_SPATIAL_LOOK_V1` 标记及 `nrSpatialLookSupported` 能力；启动器、工具箱启动和实时控制拒绝不支持的非默认空间字段。`nr.look.spatial` 报告实际活动、错误、半径、纹理数量及分配字节数。NR 暂停时基础/空间状态都报告不活动。高级配置增加独立空间开关、两个增益、半径、光晕滑块及恢复默认；嵌套草稿仍与保存值分离。

本轮沿用前述 GPU/驱动/VVL，扩展同一硬件测试，覆盖：

- 默认/中性不分配 band，实际分离后切回中性逐位保持 NR 输出。
- 平坦表面的统一光照变化可去除；细节抑制保留统一光照，并减弱单列细小变化。
- 亮侧局部压暗轮廓减弱，没有新增亮侧溢出；暗侧文字与整片阴影样例保留。
- 非有限邻居拒绝、1/32 像素半径边界、反复切换时同一纹理复用、完成后销毁/重建及新布局初始化。
- 空间准备失败不报告已生效，保留基础路径，不每帧重试。

硬件测试通过，核心/同步验证 0 错误、0 警告，6 条隐式图层禁用提示单列。完整运行组件回归 178 项通过、5 项硬件测试默认忽略；上述 Look 硬件测试另行显式执行。工具箱共享配置测试 4 项通过。工具箱与组件 host/Windows `cargo check`、Rust 格式、前端生产构建/类型/改动文件 ESLint 均通过。Shader 源码、共享 include 与 SPIR-V 加入原有构建摘要验证。性能、真实场景阴影/文字稳定性和长期窗口切换尚未验收。

## P4 前置工作：实际输入重复检测与输出复用

`nr_source_frames.rs` 管理独立于 Present/FG 编号的 NR 颜色观察编号。`nr_input_difference.comp` 在 GPU 上精确比较当前与上一张私有 RGBA16F 输入，覆盖 RGB、alpha 和整个工作分辨率，包括原生源 crop/flip/UNORM 解释后的结果。相同输入不推进编号、不调用模型、不再次执行 Look，而是复用已有最终输出。暂停恢复、输入映射/尺寸改变或指导资源失效会开启新的历史边界；边界后即使颜色相同，也允许首次重置推理。编号与 source identity/mapping 及 NR 资源生命周期一起解释，不能当作模拟器原生游戏帧编号。

比较器新增一张与 NR 输入同尺寸的 RGBA16F 私有纹理及一个 HOST_VISIBLE/HOST_COHERENT 结果 buffer。普通运行只读取四字节比较结果，不读取颜色图像。输入准备与比较命令消费原等待信号一次，并在有界 input fence 后读结果；独立的 evaluate/output 命令不重放这些等待。重复帧保留输出像素，仍执行必要的呈现写回或 SR semaphore 交接。资源销毁和复用沿用既有 NR/SR 完成保护。

这为每次 NR 调用增加一次输入提交和 CPU fence 等待，以及一张输入纹理的复制/比较成本；此前已有分离输入的路径改为在所有 NR 路径上执行。性能收益取决于重复输入比例，尚未进行真实游戏性能验收。非有限输入不允许缓存；有限 FP16 输入按解码后浮点位比较，区分正负零。

重复输入期间修改模型或 Look 参数，设置会保留为待应用，直到下一张新的颜色输入；模型重置请求也保留至此。实际应用时重置下游 SR/FG，Look 变化继续保留 NR 模型历史。实时字段 `active` 与 `evaluated` 分别表达输出使用和本次推理；另记录 `outputReused`、`sourceFrameId`、`sourceFrameBasis`、`controlsPending`、`appliedRevision`、`appliedIntensity` 和 `appliedOptions`。界面显示复用及等待新画面的原因，等待期间仍可关闭 NR。NR 禁用确认独立于新颜色到达。

验收器同步区分 GPU 输出提交与 NGX evaluate 次数，复用提交仍须完成 fence/semaphore 检查。连续历史使用颜色观察编号；复用期间不得变更编号、映射或实际强度，也不得声称执行了推理。测试拒绝伪造编号、重复推进、未完成交接和矛盾的组合状态，同时保留旧日志的兼容验收。

本轮验证：完整 Rust 回归 188 项通过、5 项硬件测试默认忽略；同一离屏 GPU 测试显式执行通过，核心/同步验证 0 错误、0 警告，6 条隐式图层禁用提示单列。比较覆盖单 FP16 位变化、alpha、右/下边界、正负零、NaN/Inf 拒绝、每次清零结果及销毁重建；纯状态与验收测试覆盖连续重复、参数延迟、重置保留和复用计数。host/Windows 编译检查、格式、前端生产构建/类型及改动文件 ESLint 通过。证据摘要保存在 `src-tauri/crates/streamline-fg/evidence/nr-source-frames-2026-10-05/summary.json`。

上述硬件测试只证明比较 shader 的输入行为、同步和资源生命周期，没有运行 NGX 或游戏，也没有覆盖实际 NR→SR→FG 交接的端到端缓存复用。P4 后续仍需第二实例的独立参数/历史、GPU ping-pong、最终成功遍的 Look 输入配对、第二遍失败后的安全降级与显式重试，并用真实会话验证源帧观察、资源退休和性能。运行包尚未发布或替换已安装组件。

## P4：两遍 NR 实现与真实 NGX 隔离验证

2026-10-05 接续完成第二实例代码。前述“后续仍需第二实例”是前置工作的历史记录；本节描述目前实现。构建与原始日志摘要见 [验证摘要](../src-tauri/crates/streamline-fg/evidence/nr-two-pass-2026-10-05/summary.json)。

新增 `nr.secondPass`，默认对象序列化时省略，旧配置继续默认一遍：

| 字段 | 默认 | 语义 |
| --- | --- | --- |
| `enabled` | false | 开启第二实例，最多两遍 |
| `inherit` | true | 继承第一遍的实际强度和模型参数；不继承外部 Look 或管线配置 |
| `intensity` | 100 | 独立模式强度，0–200% |
| `style` | a | 独立模式 a/b/c 风格 |
| `globalTone/localTone/localStructure` | null | 独立模式仍可链接本遍强度；明确值 0–200% |
| `skinStructure/autoMask` | 0/false | 独立模式人物结构 0–200% 与人物遮罩 |
| `retry` | 0 | 0–65535 的显式重试令牌；按钮递增，范围内循环 |

NGX 两遍各自 Allocate/Populate 参数块、Create/Evaluate/Release Feature，并共用已有串行 Session/NGX 核心。第一遍私有输入 P0 输出 R1；第二遍只读 R1，写入新增 RGBA16F R2。当前帧合成深度与同一裁剪/缩放后的 NVOF 运动资源分别提供给两遍，运动基准与 UV scale 沿用第一遍；没有新增游戏原生指导资源。

源颜色观察仍只推进一次。普通调参和 Look 更新在重复输入期间等待新颜色，重复输入跳过两遍及 Look、复用已有最终输出。第二遍拥有独立历史：首次/重建、上游重置、自身有效模型参数或强度改变、观察编号不连续时重置。仅修改第二遍参数不重置第一遍；仅修改 Look 不重置任一模型。遍数改变、显式重试开启新的管线历史 epoch，即使颜色重复也执行重置评估，以免继续引用已退休 R2。暂停恢复、映射/尺寸变化仍沿用前述边界。

第二遍资源在无待完成工作时创建。第一遍先提交并完成 input fence，然后用独立 command buffer 录制第二遍，带有跨提交的内存依赖。第二遍准备失败或 SDK 录制失败会降级一遍；后者先完整 reset 尚未提交的录制，再释放第二实例/纹理，使用已完成的 R1 继续 Look 和输出。错误缓存到显式重试、切换开关或资源重建，不逐帧重新创建。提交或等待失败返回错误，不将可能仍在执行的工作当成可降级结果。退役检查在资源忙时返回错误；关闭 NR 时等待 GPU 完成并释放两遍资源，第二遍关闭/重试在已完成工作边界退休。整体 NGX shutdown 仍由已有 Streamline 共存所有权约束。

Look 只执行一次：成功两遍用 `(R1,R2)`，降级或一遍用 `(P0,R1)`，保留最终模型 alpha。截图/诊断读回也选择相同配对及最后成功遍的强度。为预备回退，可能同时保留两组 Look 缓存；非中性空间设置最多各新增一张 band。第二遍退休释放其 Look/band，不保留到第一遍的后续生命周期。

高级配置增加第二遍开关、参数继承、完整独立模型设置、重试与恢复默认。实际遍数和错误来自运行状态；降级一遍不会显示第二遍已生效。组件添加 `NS_EMU_NR_TWO_PASS_V1`/`nrTwoPassSupported`，启动和实时导入明确拒绝旧组件不支持的非默认配置。NR 关闭命令仍可在旧组件能力不足时发送。

`target_nr_pass_frame` 与 `nr.pipeline.passes` 记录实际成功遍的独立 instance、参数块/handle、输入输出/指导图像、颜色观察编号、参数和重置。会话验收核对串联图像、独立 handle/参数、共同指导和观察编号、第二遍自身连续历史、最终输出交接与第二实例的平衡释放。只请求两遍不能作为实际两遍证据，重复提交不计为模型推理，缺失新格式记录也不能通过。旧日志继续保留兼容路径。

计时新增 `gpu_pass_us` 与 `cpu_pass_evaluate_call_us`，分别记录两遍；未推理时逐遍 GPU 值为空。整体 evaluate 的 timestamp 区间包含两次提交之间的 CPU/GPU 间隙，因此标注 `evaluate_including_intermediate_gap`，不能当作两遍纯模型耗时之和。另列 `cpu_intermediate_fence_wait_us`。此实现每个新颜色的两遍路径增加一次 CPU fence 等待及私有 R2，优先保证可丢弃第二遍和回退；暂未进行游戏性能验收。状态报告外部私有 NR 纹理、活跃 Feature 数与 R2 实际 allocationBytes，SDK 内部显存仍未知；Look band 的分配由 Look 状态单列。

新增隔离 `streamline-nr-diagnostics --two-pass`，要求核心/同步验证、固定哈希运行组件与既有内部布局初始化修复，不与 resize、SR 共存或无验证模式混用。它使用真实 NGX，640×360 合成移动输入和指导，在本机 RTX 5070 Ti Laptop GPU / 610.88 / VVL 1.4.363.0 完成 300 帧：

- 独立参数块和 handle；第一遍调用 300 次，第二遍提交 299 次。
- 两遍正常连续历史，第一遍只在第 0 帧重置。第 225 帧单独改第二遍为 B 风格、50% 强度，仅第二遍重置。
- 第 150 帧在有效 SDK 录制后、提交前完整丢弃第二遍并释放 handle，GPU 中 R1 与旧 R2 均保持不变；下一帧重建第二实例并重置。
- 第二遍输出均为有限值、没有 sentinel、持续随移动输入更新，并不同于第一遍。两个实例及参数块在退出前释放。
- 核心/同步验证 0 错误、0 警告；6 条隐式图层禁用 loader 提示逐条保留和单列。

最终诊断原始会话为 `src-tauri/target/nr-two-pass-ngx-002`，首次继承参数会话 `nr-two-pass-ngx-001` 也保留。独立离屏 Look 测试增加 `(R1,R2)` 中性/amount=0/alpha 配对验证，连同空间 Look 和输入比较测试显式执行通过，核心/同步验证 0 错误、0 警告。

运行组件完整 Rust 回归 199 项通过、5 项专用硬件测试默认忽略；工具箱共享配置 5 项和基础配置 8 项通过。运行组件与工具箱 `cargo fmt`、host/显式 Windows MSVC `cargo check` 通过，无错误或警告；另检查 sdk-bridge 单独配置。前端生产构建/类型检查与改动文件 ESLint 零警告通过。

这些硬件证据验证固定组件双实例及安全丢弃，不执行实际 target_nr/模拟器整条链。Eden/Citron 的视频对照、暂停/窗口/尺寸反复切换、重复源帧及第二遍→SR→FG 交接、长期资源回落、逐遍计时真实性与性能仍需端到端验收。此前记录的模拟器 strict validation/退出问题未由本轮证明解决。P5 时序 Look、HDR 和 P6 预设完善尚未实现；本轮仅构建本地开发组件，未发布或替换已安装组件。

## P5：可选时间 Look 与固定序列 GPU 验证

2026-10-05 接续实现时间平滑；前述 P4 记录的“P5 尚未实现”描述当时的状态。当前 P5 代码已接入，正式视频画质与性能验收仍未完成。证据见 [验证摘要](../src-tauri/crates/streamline-fg/evidence/nr-temporal-look-2026-10-05/summary.json)。

新增 `nr.look.temporal`，默认对象序列化时省略，默认不执行新增 dispatch 或创建历史纹理：

| 字段 | 默认 | 范围/语义 |
| --- | --- | --- |
| `enabled` | false | 只启用 Look 的变化量历史，不改变 NGX 历史策略 |
| `timeMs` | 80 | 1–500 ms，历史指数衰减时间 |
| `strength` | 75 | 0–90%，历史混合比例上限；0 绕过时间阶段 |
| `rejection` | 50 | 10–400，输入颜色差异拒绝阈值，以百分之一档表示 |

每组 Look 按需拥有两张 RGBA16F 变化量纹理，交替只读上一结果、写入本次结果；第三张 RGBA16F 保存未经最终 Look 调整的上一输入。历史保存线性颜色中的原始模型 log 变化量，不保存调整后的整张输出，不重复累积基础 Look 增益。第二遍成功时历史对应 `(R1,R2)`，一遍或降级时对应 `(P0,R1)`，两组历史互不混用。

时间阶段使用现有同帧、同工作尺寸的 RG32F 当前到上一帧 UV 运动，按既有 viewport/crop 的 UV 基准比例重投影。历史与上一输入以手工双线性采样读取。每像素先检查有限值、历史有效标记、坐标范围和位移上限（各轴不超过工作尺寸 25%），再以重投影输入与当前输入的逐通道 log 颜色差异及暗部亮度计算置信度。没有可信运动或上下文时保持基础/空间 Look，时间阶段不声称生效。

历史变化量限制到当前 3×3 邻域范围及额外 ±0.25 档容差；跨越明显输入亮度边缘的邻居不参与范围。容差允许有限的低幅闪烁平滑，而非完全将旧值压到当前单值。当前模型零变化不引入旧增强；无效历史不参与混合。变化小于 0.001 档时忽略 FP16 历史量化差异，避免稳定模型输出产生新漂移。历史被拒绝、首次或重置时，中性基础/空间设置逐位保留 NR 原结果和 alpha；其它 Look 参数仍按原有路径计算。

运动来自现有 NVOF 估算，并非游戏原生数据。当前接入没有为此阶段提供双向一致性、光流 cost 或真实遮挡/深度置信度，输入颜色检查与邻域限制只是保守启发式，不能保证所有遮挡、同色切场景或快速转向都没有拖影。时间平滑保持默认关闭，真实视频验收是独立门槛。

`nr_look_history.rs` 使用相邻新颜色观察的单调墙钟间隔，混合上限为 `min(strength/100, exp(-intervalMs/timeMs))`。这是接入层的颜色观察时间，不是模拟器原生模拟帧时间。重复颜色不 dispatch、不推进时间历史；源编号不连续、上游模型重置、Look 配置改变、非正间隔或超过 250 ms 的间隔均重新建立 Look 历史。第二遍自身重置会传播到其 Look；仅改变 Look 不重置任一 NGX 模型。历史编号和索引在成功提交后确认，下次复用仍要求既有 NR/SR 完成保护。资源重建自然清空历史。

GPU 次序为只读 NR 输出的时间阶段、保存本次原始输入、可选空间 band、逐像素最终合成。空间 band 和合成均使用相同的平滑变化量。阶段间 barrier 完成邻域读取、发布变化量写入，并保证原输入拷贝的读写顺序；没有并发邻域读写 NR 输出，没有新增队列提交或普通运行 CPU 图像读回。时间处理开关关闭后保留缓存至 Look/NR 资源退休，以免滑块切换反复分配；第二遍退休会释放其时间缓存。两组 Look 均准备时最多额外六张历史纹理，按实际 Vulkan allocationBytes 单列，不能只统计活跃最后一组。准备失败保留其它 Look，缓存错误到资源重建，不逐帧重试。

组件增加 `NS_EMU_NR_TEMPORAL_LOOK_V1` 和 `nrTemporalLookSupported`。启动器、工具箱启动和实时配置拒绝旧组件的非默认时间字段；旧默认配置仍保持兼容。高级配置加入开关、平滑时间、混合上限、拒绝阈值、恢复默认与实际失败提示。`look.temporal` 报告实际活动/评估、历史是否建立、颜色编号、间隔、最大历史权重、重置原因、纹理数和分配字节。最大权重是全局上限，每像素还乘置信度；未测量 GPU 实际接受比例，不能将它当成所有像素都混入历史。NR 暂停时状态报告不活动。`pipeline.lookCacheResources` 统计第一/第二组保留缓存。

会话验收新增独立 Look 历史检查，拒绝保存整张 RGB 输出的声明、超限权重、连续历史中的编号跳跃、重置仍声称混合、重复输出期间改变历史编号/间隔/权重等矛盾。NGX 模型历史与 Look 历史仍分别报告；没有真实视频不会自动通过画质门槛。

同一 RTX 5070 Ti Laptop GPU、610.88、VVL 1.4.363.0 上，扩展离屏固定序列测试并显式运行通过，核心/同步验证 0 错误、0 警告，6 条隐式图层禁用提示单列。覆盖无运动/上下文绕过、首帧逐位一致、降低变化量闪烁、alpha、长间隔和上游重置、颜色切换拒绝、NaN/大位移、整数和小数重投影、UV crop 缩放、越界、无效历史、空间组合、纹理复用、准备失败和销毁重建。首次测试暴露稳定历史量化的一位漂移，已在 shader 中修复并重新验证；首次失败日志也保留。

完整运行组件回归 205 项通过、5 项硬件测试默认忽略；工具箱共享配置 6 项和基础配置 8 项通过。运行组件与工具箱 Rust 格式、host/显式 Windows MSVC 编译检查通过，无错误或警告；sdk-bridge 独立配置检查通过。前端生产构建/类型检查和改动文件 ESLint 零警告通过。所有修改的 shader 均重新编译、通过 spirv-val，并更新构建摘要。

本轮测试使用合成颜色、模型输出与运动，未执行游戏或真实 NGX+时间 Look 的完整接入链。Eden/Citron 慢平移、快速转向、遮挡、切场景和暂停恢复的视频对照、两遍 NR→SR→FG 组合、长期显存/窗口回归及 GPU 性能仍待完成。HDR 与 P6 预设完善也未实现。本地开发组件已构建，运行包未发布，已安装组件未替换。

## P6：NR 预设、迁移与配置集成

2026-10-05 接续完成 P6 的配置及界面代码。此前章节的“P6 未实现”是当时状态；本轮没有新增模拟器游戏验收证据，P6 的 Eden/Citron 实测门槛仍未通过。

NR 高级配置新增可展开的预设区，支持保存当前参数、选择预设、导入 JSON、完整参数/版本预览、保存导入项、应用、导出 JSON 和删除。预设库使用现有原子配置保存机制，位于 `setting.other.streamline_nr_presets`，最多 64 项。旧工具配置缺失该字段时为空库；新增库不改变现有 NR/SR/FG 默认值或开关。

应用只更新 NR 开关、第一遍强度和 NR 高级参数，保留 SR/FG 及其它配置，沿用现有实时能力检查和生效确认。导入、选择、保存或删除预设不自动应用。预设中的模拟器、游戏和显示模式是用户标记，用于区分配置，不自动切换模拟器、游戏、分辨率或显示模式；应用仍是全局 NR 设置，不是按游戏自动加载。保存同名项会追加独立条目，不隐式覆盖。

### 可移植格式

首版正式导出为 v2；兼容约定的 v1 精简格式。二者使用相同字段名，v1 可省略 `settings.options` 和版本元数据；导入时补齐中性 Look、一遍 NR、关闭时序的默认值并输出 v2，保留所有显式提供的参数，包括 0、false、null、第二遍继承和独立覆盖值。不将工具完整配置、任意第三方 JSON 或未知版本当成预设。

| 字段 | 语义 |
| --- | --- |
| `schemaVersion` | 导入只接受整数 1 / 2，规范化输出 2；Look 自身 schema 仍为 1 |
| `name` | 必填非空名称，最多 160 字符，不含控制字符 |
| `emulator` | `eden` / `citron` / `yuzu` / `ryujinx` / `other` |
| `game` / `displayMode` | 可选标签，缺失时为空；各最多 160 字符 |
| `settings.enabled` / `settings.intensity` | NR 开关及第一遍 0–200 的整数百分比，均必填 |
| `settings.options` | 共用已验证的 `NrOptions`：逐遍模型参数、Look、空间和时序策略，缺失字段按既有默认值补齐 |
| `environment.toolboxVersion` | 保存时工具版本 |
| `environment.componentVersion` / `componentSha256` | 已校验安装包版本及 Vulkan 图层哈希；缺失/损坏时空字符串/null |
| `environment.modelVersion` / `modelSha256` | 已校验 NR 模型版本/哈希；未安装时空字符串/null |
| `environment.targetSha256` | 当前模拟器主程序摘要；读取失败时 null |

JSON 文件最多 64 KiB。Rust 在导入和配置反序列化时校验整数范围、枚举、各层未知字段、重复字段、标签长度/控制字符和 SHA-256 格式；小数、负数、非有限/非数值输入不能进入运行配置。导入预览及应用草稿深拷贝 Look、空间、时序、第二遍和元数据，不修改库中对象。导出通过文件保存对话框选择目标，使用已有 Tauri 文件权限，无新增自动写入路径。

版本记录明确描述**保存时已安装的快照**，不声称它是当前会话已加载的私有副本。读取组件版本前验证安装记录、当前主程序及完整 payload；没有安装时，不把内嵌发布 manifest 的候选版本报告为已安装版本。预览区比较模拟器、图层和模型摘要，并展示缺失/不同环境、未连接游戏、NR 未准备及旧组件不支持模型/两遍/Look/空间/时间配置的提示。导入元数据仅用于比较，不作为安装或启动授权。实际活动、失败降级、HDR/codec 限制和生效确认仍来自运行状态。

新增独立“一遍 NR · 绕过 Look”按钮：保留 NR 开关、第一遍模型参数/强度、第二遍覆盖值及全部 Look 调参，只关闭第二遍和 Look。重新打开各开关可恢复原调参；“恢复默认配置”则恢复强度 100%、模型默认、一遍、中性 Look、空间/时间关闭，并沿用当前 NR 开关。二者分别验证，NR 默认开关行为保持原有约定。

### 本轮验证与范围

- 配置测试 23 项通过，包含 5 项新增导入/迁移/库往返测试，覆盖显式 0、默认补齐、所有阶段、无效值、重复/未知字段、摘要格式、超大文件和 64 项库限制。
- 图形组件回归 36 项通过、8 项下载/本机专用测试按原约定忽略；新增元数据测试验证无安装/仅有 receipt 时拒绝伪报内嵌候选版本。
- 前端 Bun 测试 6 项通过，覆盖旧 wire 默认补齐、预览深拷贝、旁路保留调参、缺失版本/未连接、三类哈希差异和非默认关闭项的能力检查。
- `cargo fmt`、工具箱 host 与显式 `x86_64-pc-windows-msvc` 的 `cargo check --all-targets` 通过，无错误或警告。前端生产构建和类型检查、预设组件/工具/测试及 NR 控件 ESLint 零警告通过。单独扫描既有 `utils/tauri.ts` 仍有三处此前的 `no-explicit-any`，本轮仅增加类型字段，未改动这些位置。
- 日志保留于 `src-tauri/target/nr-presets-{config-tests,graphics-tests,host-check,windows-check,frontend-tests,frontend-build}.log`。Bun 测试位于 `frontend/tests/nrPresets.test.ts`，通过 `bun test tests/nrPresets.test.ts` 复现。

本轮没有修改 shader 或运行组件调度，不重跑无变化的 GPU 算法测试；前述 P4/P5 隔离证据继续有效于其原测试范围。尚未通过实际桌面 WebView 操作文件选择/保存对话框，也没有 Eden/Citron 的真实游戏视频、窗口/暂停/分辨率循环、NR→SR→FG 全链路或性能验收。此前模拟器严格 Vulkan 验证及退出问题仍未证明解决；HDR 不受支持。运行包未发布，已安装组件未替换。

## Citron 接入复测与运行修复

2026-10-05 使用工作区中的隔离 Citron Nightly `0237a9b88` 与《王国之泪》1.0 XCI，开启核心/同步 Vulkan 验证、NVOF、原有合成深度和 NR 图像读回。SR/FG 均关闭；没有替换用户已安装组件。目标摘要为 `059c7a4d4dc361e042eaf3654e966da2dc6902ec12d3b86ce101f85b613e1ba6`；SDK、模型、bridge 与 VVL 使用前述已冻结版本及摘要。新建会话位于 `src-tauri/target/nr-game-integration-20261005`，失败日志与后续复测分别保留。

### 修复的三个实际问题

1. **设备扩展依赖缺失。** 首次会话报 `VUID-vkCreateDevice-ppEnabledExtensionNames-01387`。模拟器请求 `VK_NV_low_latency2`，合并 SDK 需求后仍缺少 present-ID 扩展。`target_device_plan.rs` 现在根据设备公布的支持列表补齐 `VK_KHR_present_id` 或 `VK_KHR_present_id2`；已有依赖时不重复添加，无可用依赖时明确失败。只补扩展名称，不启用应用未使用的 present-ID 功能；不修改借用的创建结构。`target_device_plan` 记录实际扩展列表及新增依赖。第二次起，该错误消失。依赖关系见 [Vulkan 官方扩展参考](https://docs.vulkan.org/refpages/latest/refpages/source/VK_NV_low_latency2.html)。
2. **SDK 验证消息中的 `%` 导致日志回调崩溃。** 前三次会话在首张游戏画面交接前以访问违例退出。第三次使用匹配的 DLL/PDB 与既有 WER 转储，通过 PE unwind 得到 `ucrtbase → sl.interposer → sl.common → Vulkan 验证层 → Citron` 调用栈；早期栈内地址扫描不作为调用栈证据。冻结 SDK 的 `platforms/sl.chi/vulkan.cpp` 中 `debugUtilsMessengerCallback` 把 `pMessage` 直接传给 `SL_LOG_ERROR`，错误原文包含 `%SubgroupEqMaskKHR`。新增 `sdk_validation_log.rs` 仅在目标 SDK 的 `slSetVulkanInfo` 同步创建阶段捕获 messenger，通过 SDK 专用 GIPA 转义字面 `%`。printf 最终仍输出原文；severity、type、ID、其它 metadata、原 user-data 和回调返回值保留。独立 Vulkan 原始日志和错误计数不改变，不过滤 VUID、不放宽会话验收。存储保持至下游 messenger 销毁返回，实例销毁后清理剩余记录。第四、五次均越过该点并持续执行 NR，原始 JSONL 仍含未经转义的 `%SubgroupEqMaskKHR`。
3. **真实 UV 运动纹理为 RG16F。** 第四次两遍 NR 已生效，但时间 Look 报准备失败；之前离屏 fixture 是 RG32F，生产 NR 引导使用 RG16F。时间阶段现按实际 image-view 格式选择 RG16F/RG32F 两个 SPIR-V，共享同一 shader 源码和算法，仅 storage-image 格式不同。FP16 变体用 `-DNR_MOTION_FORMAT=rg16f` 构建，不新增转换纹理或提交。尺寸不匹配、其它格式继续明确失败。两种二进制均通过 `spirv-val --target-env vulkan1.1`，源码和二进制摘要已更新。

### 实际观察与范围

第五次加载图层摘要 `00e13d3bed17b042b36ea864ceaa702024b29999d536edc39cea97c135faa035`。会话脚本只操作该测试的 `control.json`，保留逐阶段命令与遥测快照。五次独立 NR revision 均观察到实际生效：

| 阶段 | 观察结果 |
| --- | --- |
| 开启第二遍、Look amount 75/color 80、空间 lighting 75/detail 125、时间默认参数 | 实际 2 遍，第二遍输入等于第一遍输出；Look、空间、时间均 active，无准备错误 |
| 一遍 NR、关闭 Look，保留调参 | 实际 1 遍，第二实例退休，调参保留 |
| 关闭 NR | applied revision 3、active=false，实例与参数退休 |
| 重新开启两遍及 Look | applied revision 4、实际 2 遍，时间历史重新建立 |
| 恢复默认 | applied revision 5、实际 1 遍，默认 Look 精确旁路 |

时间快照显示 `raw_log_model_delta`、3 张历史纹理、historyReady=true、连续历史及实际观察间隔。例如第一阶段快照间隔 79 ms，最大历史权重约 0.37251；未测量逐像素接受比例，不能将全局上限当作全部像素的实际混合比例。2560×1312 下单组时间纹理 allocationBytes 为 86,507,520，空间 band 为 28,835,840；两组 Look 缓存共 8 张、230,686,720 字节，第二遍输出另占 28,835,840 字节，SDK 私有显存仍未知。遥测显示缓存总量，不只统计当前第二组。

最终额外下发 revision 6 关闭 NR，观察到关闭生效后清理测试进程。日志中第一遍 Feature/参数各创建与释放 3 次，第二遍各 2 次。请求正常窗口退出未结束进程，因此强制清理仅针对本轮已记录 PID 的隔离测试进程；不能把资源控制切换计数平衡当成完整 SDK/设备/实例正常退出证明。未加载增强图层的核心验证对照进程持续存活，但未取得完整游戏状态与正常退出证据，且该对照没有启用同步验证，不能用它归因全部剩余错误。

**严格验收仍失败。** 第四、五次原始验证各 43 错误、10 警告，含 Flat shader 装饰、draw 布局、render-pass/query 作用域、READ_AFTER_WRITE/WRITE_AFTER_WRITE 同步问题及 storage-image 格式警告。部分消息涉及 SDK `dlss_g` fake-swapchain/pacer，尚未证明各错误归属或修复。源错误没有被隐藏，`session_verified` 仍必须失败；强制结束也不能通过正常退出及完整 present-retirement 门槛。时间活动快照只证明所述配置实际运行，不是逐帧 Look 历史会话门槛、画质或性能验收。没有执行 Eden 本轮游戏验收、用户输入驱动的视频场景、暂停/窗口/分辨率循环、NR→SR→FG 全组合或长期显存测试。

### 本轮验证证据

- `cargo fmt` 与运行组件 `cargo check --locked --all-features --all-targets` 的 host、显式 Windows MSVC 两次检查通过，无编译错误或警告。
- 完整运行组件回归 209 项通过、5 项硬件测试默认忽略；新增设备扩展依赖测试及 3 项 SDK 回调测试覆盖字面百分号、非 UTF-8、原 metadata/user-data/返回值、创建失败、作用域恢复和存储退休。
- 显式 GPU Look 测试通过，增加 RG16F 的首帧、变化平滑、alpha、大运动拒绝与长间隔重置覆盖；原 RG32F 用例继续通过。核心/同步验证 0 错误、0 警告；6 条已单列的隐式图层禁用提示保持可见。
- Rust 日志为 `src-tauri/target/nr-game-{host-check,windows-check,regression}.log`，GPU 日志为 `nr-game-fp16-look-gpu.log`。原始会话、匹配构建快照、第三次实际 unwind、阶段快照、`live-results.json` 和 `integration-summary.json` 位于上述本地会话目录。启动器退出后的验收可能较慢，最终结果以各会话 `target-result.json` 为准。

本轮仅构建开发图层和启动器，未发布运行包，未修改已安装组件或线上组件 manifest；前端预设对话框的实际桌面操作门槛保持待验。

## Eden 接入验证与退出对照

2026-10-05 在隔离 Eden Nightly `master-d3550c4571` 上执行两轮 NR 接入验证。目标摘要 `46e710d93bee1507764b6ddf825cf13baea99edd51987b34ed18ff7773c2e7e0`；游戏为副本既有《异度之刃 3》XCI，沿用其更新/DLC、存档与配置，不能称为纯 v0 基线。SDK、NR、bridge、VVL 与前章相同。两轮均开启核心/同步验证、NR 读回以及 `NS_STREAMLINE_NR_TIMING=1` 的逐帧阶段证据；SR/FG 关闭。该严格运行包含验证、读回与记录开销，不是性能验收。

computer-use 技能的 `@oai/sky` 初始化及重置恢复均因 `windows sandbox failed: helper_unknown_error: setup refresh had errors` 失败，未通过自制 UI 自动化绕过。启动与 NR 控制使用现有 CLI/会话协议；用户确认窗口已显示标题/游戏内容，并分别正常操作两轮增强窗口及纯模拟器窗口退出。本轮没有强制结束 Eden 游戏进程。保存的 GPU 读回包含启动提示画面，没有取得真实游玩的慢平移、快速转向或遮挡视频。

### 实际生效及门槛结果

两轮都观察到五个独立 revision 生效：两遍 NR＋非中性 Look＋空间/时间处理、保留调参的一遍旁路、关闭 NR、重新开启两遍与 Look、恢复默认。阶段命令、完整遥测、GPU 输入/输出、模型与 Look 历史、销毁顺序均保留。第二轮会话为 `src-tauri/target/nr-game-integration-20261005/eden-strict-002`，图层摘要 `127a73bbbafd419c484e7dfdb63162cf61bc823c3e97c03adac8732132d5e56f`。

第二轮最终 `target-result.json` 报告：

| 检查 | 结果及范围 |
| --- | --- |
| NR 调用链/执行 | `call_chain_completed=true`、`execution_verified=true`，2,626 次第一遍成功评估 |
| 图像读回 | 16 项读回有效，非零强度确实改变输出；不等于画质通过 |
| 双遍 | `passes.valid=true`，224 帧提交第二遍，两个第二实例均完成创建与释放 |
| Look 历史 | `look_history.checked=true/valid=true`，224 次历史更新，5 次缓存输出检查 |
| 模型历史 | `temporal_history.valid=true`，连续评估与重置检查通过 |
| NR/第二实例退休 | `clean_nr_shutdown=true`、`clean_second_shutdown=true`；SDK shutdown 后设备/实例计数均为 0 |
| Present 交接 | 完整 present-chain 检查无错误 |
| 严格 Vulkan / 整体会话 | **失败**：40 错误、10 未豁免警告，`session_verified=false` |
| 正常进程退出 | **失败**：用户正常退出后进程仍以 `0xC0000409` 结束 |

1712×933 的阶段快照显示空间/时间 Look 均 active、无准备错误，时间历史为 `raw_log_model_delta`、historyReady=true。示例间隔 34 ms、最大历史权重约 0.65377。时间三纹理为 42,074,112 allocationBytes，单空间 band 为 14,024,704；两组 Look 保留缓存共 112,197,632 字节，第二遍输出另占 14,024,704 字节，SDK 私有显存未测量。逐像素历史接受比例仍未知。

### 新发现与修复

首轮原始请求启用 `VK_KHR_shader_quad_control`，但未包含它需要的 `VK_KHR_shader_maximal_reconvergence`，触发 `VUID-vkCreateDevice-ppEnabledExtensionNames-01387`。`target_device_plan.rs` 新增根据实际支持列表补齐依赖的纯函数，并接入 SDK 合并后的设备创建参数；已列出的依赖不重复添加，不支持时明确失败，不改动原始借用结构或开启额外 feature 位。实际新增依赖写入 `added_quad_control_dependency`。关系依据 [Vulkan 官方 quad-control 参考](https://docs.vulkan.org/refpages/latest/refpages/source/VK_KHR_shader_quad_control.html)。第二轮该错误消失，原始两轮日志均保留。

剩余错误包含 StencilExport SPIR-V capability/extension、查询作用域、SDK `dlss_g` pacer/fake-swapchain 布局，以及第二轮的深度 attachment READ_AFTER_WRITE/WRITE_AFTER_WRITE；另有 10 条 maintenance9/3D barrier layerCount 警告。首轮 23 错误、0 警告，第二轮 40 错误、10 警告；两个窗口尺寸和实际渲染轨迹不同，不能用总数变化评价单个修复。未过滤任何错误或警告，也没有把 SDK 内部对象标签当成所有错误归属已确认的证据。

### 退出对照

两轮增强会话的用户正常退出均产生 `0xC0000409`，stderr 为 `terminate called after throwing an instance of 'std::system_error'` / `Invalid argument`。随后在相同副本、游戏及当时配置上启动 `eden-plain-exit-001`，清除增强/Vulkan 注入环境，仅保留隐式第三方图层禁用；不加载本项目图层、SDK、NR 或验证层。用户正常退出后，该对照仍得到同一退出码和 stderr。

纯对照 WER 转储的模块清单确认无 `streamline`、`sl.*`、`nvngx`、`NvLowLatency*` 或 `VkLayer*` 模块。其 PE unwind 与第一轮增强会话匹配到相同的 Eden/ucrt 模块及偏移序列。退出异常因此可以独立于 NR/Streamline 复现；尚未取得 Eden 符号或修复其 `ForceStop`/线程清理路径，不能仅凭对照认定完整根因。增强会话资源退休证据通过，也不能抵消进程异常退出的整体失败。

### 验证与未覆盖范围

扩展修复后执行 `cargo fmt`；运行组件 host 与显式 Windows MSVC 的 `cargo check --locked --all-features --all-targets` 均无错误或警告。完整回归 210 项通过、5 项硬件测试默认忽略；新增依赖测试覆盖支持、缺失、已有、无关请求和重复补齐。没有修改 shader 或 GPU 算法，因此不重复无变化的离屏算法测试。开发图层已构建，未发布或替换已安装组件。

日志为 `src-tauri/target/nr-eden-{host-check,windows-check,regression}.log`。两轮会话、匹配 DLL/PDB、阶段快照、退出对照、实际 unwind、模块核查及 `eden-integration-summary.json` 位于 `src-tauri/target/nr-game-integration-20261005`。这些证据证明所列配置实际运行及局部门槛，不代表 P1/P2/P5/P6 全部验收。实际游玩画质、暂停/窗口/分辨率循环、NR→SR→FG 全组合、长期显存与性能、工具箱预设 WebView 操作仍待完成。

首轮约 62 MB 原始事件使 debug 验收进程持续计算超过八 CPU 分钟；在第二轮完整报告和纯模拟器对照均完成后，只清理该首轮验收进程，记录 `verifier-cleanup.json`。首轮游戏此前已经由用户正常退出，其原始事件、验证消息、退出码和转储全部保留；没有补造首轮完整 `target-result.json`，本章完整门槛数值均来自第二轮。

## Eden NR→SR→FG 组合复测

2026-10-05 按用户要求暂缓剩余 Vulkan 问题，继续组合验收。沿用上一章的隔离 Eden、游戏、运行库及图层，冻结图层摘要仍为 `127a73bbbafd419c484e7dfdb63162cf61bc823c3e97c03adac8732132d5e56f`。会话为 `src-tauri/target/nr-game-integration-20261005/eden-combinations-001`，核心/同步验证、NR 读回及阶段时间记录保持开启。新增 `--fg --reference-params --nr-initial-off --sr-mode off`；参考相机参数、合成深度和 NVOF 运动仍是实验条件。SR 使用 balanced、preset K、scale 100，实际输入/输出为 1712×933，未验证改变输出尺寸的上采样。

computer-use 再次因相同 sandbox 初始化错误失败。Eden 实际前台状态和插帧由遥测/逐帧完成证据确认；控制脚本只通过会话协议切换设置。最后关闭 NR/SR/FG，用户正常退出窗口，没有强制终止游戏进程。

### 实际组合与独立控制

NR 开启阶段使用第二遍、Look amount 75/color 80、空间 lighting 75/detail 125及默认时间处理。最终验收器按实际 NR/SR 消费、SDK 双呈现、完成 fence/value、颜色来源和请求状态筛选帧，结果为 `combinations.valid=true`、`all_combinations_observed=true`、`invalid_frames=[]`：

| NR | SR | FG | 符合组合证据的帧数 |
| --- | --- | --- | --- |
| 关 | 关 | 关 | 2,244 |
| 开 | 关 | 关 | 41 |
| 关 | 开 | 关 | 39 |
| 开 | 开 | 关 | 21 |
| 关 | 关 | 开 | 979 |
| 开 | 关 | 开 | 3 |
| 关 | 开 | 开 | 34 |
| 开 | 开 | 开 | 326 |

这些是有限时长的运行覆盖；NR 开、SR 关、FG 开仅有 3 个合格帧，不作为长期稳定性或画质结论。805 个尚未实际满足请求的帧没有混入组合计数，包括重复输入/历史重置时的插帧暂停。

revision 10 建立 SR/FG 后，NR revision 11 关闭、12 重新开启、13 调整强度为 1.4；SR/FG revision 均保持 10。强度复测使用 NR revision 16、SR/FG revision 15。对应 NR 开关及强度变化的下游重置均有效，SR 保持同一完成 fence 和 revision；FG 在切换帧暂停生成，随后观察到 SDK 双呈现恢复，因此不能声称切换当帧连续生成。共检查 51 次 SR 重置、56 次 FG 重置。

初次强度脚本精确比较 JSON 浮点数，实际 `f32` 为 `1.399999976158142`，导致 revision 13 被脚本标记为未观测；保留该结果及快照。改为 `1e-6` 容差后 revision 16 复测成功。此修正仅涉及测试判断，没有改动强度协议或运行时数值。

最终 NR 执行检查通过：第一遍成功评估 374 次，两遍共提交 748 次；第二实例创建/释放各 5 次，完整 NR 和第二实例退休通过。NR→SR 交接 770 帧有效。Look 历史 374 次更新、446 次缓存输出检查有效，模型历史有效。Present 交接和原生 present fence 检查通过，SDK shutdown 成功且设备/实例计数归零。

### 初始关闭会话的验收修复

原验收器仅以启动时 `nr_requested` 选择 NR 分支。本会话启动 NR 关闭、`nr_available=true`，后来实时开启 NR；异常退出因此直接早退，没有写完整报告，成功退出也会漏掉 NR 严格检查。`session_verify.rs` 现以 `nr_available || nr_requested` 识别原生 NR 会话，兼容没有 available 字段的历史记录。初始关闭不再绕过 NR 验收；失败退出仍保留原始退出状态并拒绝整体通过。已有失败会话回归增加初始关闭、成功/失败退出两种情况，均验证不会把验证错误当成通过。

修改后用 `--target-probe --verify` 重放本轮原始日志，未修改输入、退出码、Vulkan 消息或阶段记录。重放启动器摘要 `a6127ab3fbd6a66f486ecf2fa0b30d984db2929ea2381192ab532c3e67d18130`，与冻结图层分别记录；验收退出码仍为 1。`cargo fmt`、host 与显式 Windows MSVC 的全特性/全目标检查、`cargo fmt --check` 均通过且无编译警告；完整回归仍为 210 通过、5 项硬件测试默认忽略。日志为 `src-tauri/target/nr-combinations-{host-check,windows-check,regression,build}.log`。

**整体严格验收仍失败。** 本轮 Vulkan 原始计数为 53 错误、10 未豁免警告，汇总匹配原文；错误按用户要求暂缓，不进行归属推断或豁免。用户正常退出仍得到既有 `0xC0000409`，故 `session_verified=false`。保留 `target-result.json`、全部阶段命令/遥测、读回、`eden-combinations-summary.json` 及重放元数据。代码未发布，已安装组件未替换。

组合覆盖已补齐，真实游玩画质/视频、暂停与窗口/分辨率循环、改变 SR 输出尺寸、长期显存与无验证开销的性能、工具箱预设 WebView 操作、安全失败场景仍待验；参考参数下的 SDK 双呈现不等于已确认显示扫描节奏或输入延迟。

## computer-use 恢复与 Release FPS 对照

2026-10-05 排查确认 computer-use 故障发生在 JavaScript 内核启动前。Codex sandbox 的 setup refresh 无法为工作区目录添加 write ACE，也无法为 `.git` 添加 deny ACE：两个目录由旧 Windows 账户拥有，当前普通令牌没有修改其 ACL 的权限。重置 node_repl 不能解决这个权限问题。

管理员所有权修复首次被自动审批拒绝。用户随后明确授权仅将 `D:\py\ns-emu-tools` 与其 `.git` 两个目录的所有者改为当前账户。先保存原始 SDDL，再执行非递归脚本；结果确认两个原 DACL 均保持不变。其后的正常 sandbox setup refresh 记录 `errors=[]`。没有关闭 sandbox、切换弱隔离模式或移除保护规则。诊断、备份、脚本和执行结果在 `src-tauri/target/computer-use-repair-20261005`。

node_repl 最小代码、`@oai/sky` 初始化和窗口列表恢复；实际验证 Eden 的窗口选择、前台激活、截图、UIA 文本、鼠标点击、游戏区域焦点及菜单键盘操作。遮挡时曾读到其它前台图像，激活目标后取得正确截图；Qt 多窗口的 UIA 索引曾失效，重新观察后改用当前截图坐标。操作全程使用 computer-use 技能 API，没有自制 UI 自动化或自动处理 UAC。工具正常关闭更新提示，保持固定 Eden 版本。

### 启动、构建与测量条件

运行组件以 `cargo build --release --locked --features native-nr --lib --bin streamline-layer-probe` 构建成功。没有修改 Rust 代码，本轮无需重复此前无变化的完整回归。冻结 Release 图层摘要为 `6d6602f73c390a6e3be6b5f826c30b624c5aed532df7e049efcd1fbf3208fc76`，启动使用 `--graphics-launch`，移除 `--validation-dir` 和 `--nr-readback`，设置 `NS_STREAMLINE_TRACE_FRAMES=0`、`NS_STREAMLINE_TRACE_VERBOSE=0`，没有启用 GPU timestamp instrumentation。进程模块核查确认 Vulkan 验证模块为 0；原始输入记录也确认验证和读回均关闭。真实 GPU 交接、完成等待与退休路径保持执行。

首次在普通 sandbox 账户启动的窗口没有出现在可操作桌面，因此不接受任何性能数据，仅清理记录 PID 的隔离测试进程；保留强制清理标记。第二次桌面启动在检测到用户输入后退出，没有完成采样。用户确认继续且暂时不操作 Eden 后，第三次 `eden-performance-003` 完成下表。前两次失败记录没有被覆盖。

固定场景为《异度之刃 3》的标题动画，截图显示游戏 Ver. 2.1.0、Eden 缩放 1x、30 FPS 上限；尚未进入存档世界。基线仍加载本项目图层、SDK 与 NVOF，只关闭 NR/SR/FG，不能称为纯 Eden 基线。NR Look 参数沿用前章；SR balanced、preset K、scale 100；FG 2x，参考相机参数与合成深度限制仍在。

每阶段先确认实际 revision、NR/SR active、NR 实际遍数及 FG 请求状态，稳定 5 秒后采集约 16 秒。所有已保存阶段的设置快照均匹配；完整原始率样本和快照在各阶段 `*-samples.json`。

| 阶段 | 应用呈现 FPS 均值 | 原生呈现调用 FPS 均值 | 有效率样本数 |
| --- | --- | --- | --- |
| 增强关闭，前基线 | 30.12 | 30.12 | 15 |
| 一遍 NR，默认 Look 精确旁路 | 30.05 | 30.05 | 15 |
| 一遍 NR＋空间/时间 Look | 30.04 | 30.04 | 15 |
| 两遍 NR＋空间/时间 Look | 30.11 | 30.11 | 15 |
| 两遍 NR＋Look＋SR | 30.06 | 30.06 | 15 |
| 两遍 NR＋Look＋SR＋FG | 30.06 | 60.13 | 14 |
| 增强关闭，后基线 | 30.05 | 30.05 | 15 |

FG 阶段的 16 次状态采样均为 `generationObserved=true`；所有 Look 开启阶段的 16 次快照均显示时间处理 active。应用呈现率以会话现有计数计算，不是 NGX 评估次数；原生调用率不是显示扫描率。30 FPS 上限使本轮无法分辨上限以下的剩余 GPU 预算，不能把相近 FPS 当作各阶段耗时相同，也不能将不同启动画面的早期严格会话与本轮直接相减归因所有差异。

采样脚本首轮在一遍旁路阶段遇到 `temporal=null` 而退出。修正空值读取后，保留已完成前基线，以新的 revision 重采剩余阶段；失败段不计入结果，记录 `sampling-recovery.json`。没有改运行时选项或处理结果。

`TRACE_FRAMES=0` 目前仍留下 `route_present_submitted`、新增 `target_nr_pass_frame` 和 `target_nr_second_evaluate` 等记录；因此本轮只是关闭验证、读回及常规逐帧诊断后的 FPS 对照，不能称为完全无日志开销。没有在采样期间编译或运行其它性能测试。完整结果在 `performance-results.json`，Release 构建日志为 `src-tauri/target/nr-performance-release-build.log`。

最后 NR/SR/FG 均关闭，由 computer-use 正常点击窗口关闭并确认退出。SDK shutdown 成功、设备和实例计数归零；仍得到此前纯 Eden 也能复现的 `0xC0000409`，未修复退出异常。本轮 `strict_acceptance_run=false`，不会以未加载验证层声称严格验证通过。真实游玩、视频画质、逐阶段 GPU 耗时、长期显存、窗口/分辨率循环和预设 WebView 操作仍待验。

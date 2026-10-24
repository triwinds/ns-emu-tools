# 通用 NR 管线实施记录

日期：2026-10-05。对应 [实施计划](dlss5-generic-nr-pipeline-plan.md)。

2026-10-06 代码交接：用户要求先完成代码，功能测试自行进行。本轮补齐 M5／M6 的工具箱配置、启动接入和实际状态展示，见本文末节；不再以录像、实机或诊断验收作为继续编码的前置条件。已有证据保持原有通过／失败状态。

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

## Magpie 补充计划：M1、M2 基础与 M3 Look 缓存

2026-10-05 接续实现第 10 节的独立基础工作。固定参考 tag 完整 SHA 为 `27c5df91177a29b33be612e98274169f3d2fca49`，由远端 tag 与隔离参考仓库 HEAD 分别核实。没有复制 Magpie shader/源码、改变 NR 模型或引入新运行依赖。该 tag 的 LICENSE blob 首次获取发生 TLS 错误，使用 Git OpenSSL 后端重试成功，确认是 GPL v3 文本；未复制外部实现，也不据此宣称已完成所有文件版权与第三方依赖复核。

### M1：范围与独立原图引导

新增 `look.scope=final_pass/chain_total`，默认与缺省均为 `final_pass`。两遍的基底分别为 R1/P0，一遍或第二遍安全降级都使用 P0；最终模型 alpha 保留。范围不参与 NGX 模型参数，默认、中性及显式旁路保持最终 NR 输出逐位一致。第二遍 Look 在 GPU 完成的边界更新输入 descriptor，不为切换范围新建模型实例。

时间 shader 新增独立 P0 descriptor；历史原图复制和邻域颜色引导都读取完成当前 crop/flip/codec 的 P0，与待累积的 `(R1,R2)` 或 `(P0,R2)` 变化量分开。切换范围或实际成功遍数、输入映射、上游重置均清理相关历史。普通合成增益不影响原始 log-delta 的连续性；历史时钟只比较范围/时间设置和源帧边界。

### M2：静态与逐采样点验证

保留 `temporal.enabled` 的关闭语义；缺省 `mode=optical_flow`、`sampling=bilinear` 保持旧路线和 timeMs/strength/rejection 显式值。新增显式 `mode=static`，shader 不声明/读取运动图像，在相同位置检查 P0 的颜色和 3×3 邻域稳定性。NR 模型本身仍要求现有 NVOF 引导；这没有使整个 NR 接入支持无运动输入，也没有自动静态回退。

实验 `sampling=per_tap` 分别检查四个角点的历史有效标记、有限值和 P0 颜色相容性，再按双线性权重与接受度归一化。有效支持不足 0.25 时使用当前变化量，避免放大微小支持；支持总和调制历史混合上限。这仍是颜色启发式，不能视为真实几何遮挡或光流 cost。普通模式继续保留零修正立即退出的策略，尚未加入光流累积＋的支持/条件幅度分离。

静态、RG16F 和 RG32F 三个 SPIR-V 均由同一源码编译，通过 `spirv-val --target-env vulkan1.1`，摘要加入 `motion.json`。模式切换在完成边界退休旧时间资源并重新建立历史；采样策略变化重置时钟。

### M3：暂停画面纯 Look 重合成

每个已准备的 Look 对新增一张同尺寸 RGBA16F 私有缓存，保存未经 Look 覆盖的模型输出。新 NR 结果在合成前刷新缓存；仍有 Present 的重复源画面上的纯 Look 更新先恢复缓存，再执行必要的空间/合成和原有交接，不调用任一 NGX Evaluate、不增加源颜色观察、不写入时间历史。兼容范围与时间设置时复用已稳定的变化量；范围/时间设置改变时拒绝旧时间缓存，不把重算当作新时间样本。Eden 完全停止呈现的模拟暂停没有执行入口，恢复后才应用参数；该场景的即时重合成仍未实现，不能把固定输入测试算作它的实机验收。

`source_frames` 分开记录 `look_recompute`，提交成功后才确认 Look revision，更新 SR/FG 的下游边界。Look 资源在完成输入比较与源帧计划后，按此次实际执行或复用的配置准备；待应用模型参数附带的 scope/mode 修改不能提前破坏当前已缓存输出。模型参数变化仍等待新颜色；**第一/第二遍参数后缀重算尚未实施**。关闭第二遍时保存其参数/retry，或修改继承模式下未使用的覆盖值，不扰动活动模型及下游历史。

原 `outputReused` 现明确描述模型结果复用；新增 `lookRecomputed` 与 `finalOutputReused` 区分重合成及未变最终输出，界面分别显示。`lookCacheResources` 包含原始模型缓存的纹理数、实际 Vulkan allocationBytes 和 ready 状态；默认中性未准备 Look 时不新增缓存。准备后的缓存沿用原 GPU 完成/退休生命周期。

### 配置、版本与验证

新增 `NS_EMU_NR_LOOK_SCOPE_V1`/`nrLookScopeSupported` 及 `NS_EMU_NR_TEMPORAL_MODES_V1`/`nrTemporalModesSupported`。启动器、工具箱专用启动、实时控制和预设预览均拒绝不支持的非默认范围/模式/采样，不能让旧组件静默忽略设置。预设导出 schema 3，schema 1/2 迁移为 3，保存原数值并填充 final-pass、原有光流及双线性采样；默认扩展字段在旧选项协议中省略。

本轮完整组件回归 216 项通过（含后来补齐的两项日志策略测试）、5 项硬件测试默认忽略；Look GPU 硬件测试另行通过，核心/同步验证 0 错误、0 警告，4 条隐式图层禁用通知单列。测试新增三张独立 P0/R1/R2 固定输入、两个范围的 0/中性/旁路/alpha、原图与 R1 变化分离、无运动纹理的静态模式、局部边缘变化拒绝、模式切换、混合有效/无效采样角点和微小支持拒绝。

缓存 GPU 测试在不重新上传模型输出的条件下连续重合成 12 次，结果没有累计漂移；恢复原 Look 控制复现相同稳定结果，恢复中性逐位还原原模型输出，历史状态/编号/间隔/权重不变。资源销毁保持严格验证干净。工具箱预设测试 6 项、共享高级设置 7 项通过；前端预设 7 项、生产构建/类型检查和改动文件 ESLint 通过。工具箱/组件 host 与显式 Windows MSVC cargo check、cargo fmt 均通过，无 Rust 错误或警告。日志位于 `src-tauri/target/nr-followup-*.log`；仓库内证据摘要见 `src-tauri/crates/streamline-fg/evidence/nr-scope-static-cache-2026-10-05/summary.json`。

### 本轮 Eden 与人工交接

冻结 debug 图层后启动 `src-tauri/target/nr-followup-20261005/eden-strict-003`，启用核心/同步验证、读回和逐帧/时间诊断，两遍、整链 amount 75、静态累积实际活动，P0 guide、三张时间纹理与连续历史快照均已观察。前两次启动分别因参数索引偏移和 NR-only 使用 FG 参考参数在启动器校验阶段失败，日志保留，没有模拟器执行证据。

用户在进入游戏时报告约 6 FPS。该严格诊断会话不用于性能验收；已下发 revision 1 关闭 NR，并确认 `active=false`、`appliedRevision=1`。其后采样约 11–12 应用呈现 FPS，验证层仍在；不能将残余低帧率归因于 NR，也不声称已修复整帧性能。停止前一项双遍 profile 的 GPU Evaluate 约 6.24/6.89 ms，整段 evaluate 含中间间隙约 14.25 ms、Look 约 0.55 ms；单项样本不是统计性能结论。

最终原始验证计数为 53 错误、10 警告，未过滤或归属豁免；M0 严格门槛仍失败，不能推进 M5 集中提交或宣称整链已验收。用户按物理 Escape 停止 computer-use 后，本轮不再操作窗口。随后进程退出，记录 `success=false`、`code=0xCFFFFFFF`，没有正常退休验收证据。显式 `--target-probe --verify` 保存 `target-result.json` 并拒绝验收：即使 SDK 局部摘要显示 0，完整原始日志仍有 53/10，`summary_matches_log=false`、`session_verified=false`；退出及呈现退休检查也失败。

冻结 debug 实机会话与当时源码/Release 分开记录：该 debug 构建没有覆盖随后修改的历史时钟与按实际配置准备 Look 的代码。这些修改先完成回归/GPU 测试，再进入下述 Release 对照。首个 Release 图层 SHA-256 为 `fb61c05812c4e1d91f48e4dcaf7a3e0412b11fda967058ad50e08b92dadd6ddf`。`src-tauri/target/nr-followup-20261005/launch-performance.ps1` 默认 `-Nr off`，可指定 `one`/`two`，每次生成独立会话，关闭验证、读回、逐帧日志开关和 GPU timestamp。

## Release 实际游玩对照与日志开关补齐（2026-10-05）

用户进入开场 Everblight Plain 区域后，在 `eden-performance-20261005-212620-850-off` 用首个冻结 Release 完成四阶段对照。模块核查确认 Vulkan 验证模块为 0；验证、读回、timestamp 与逐帧日志开关关闭，SR/FG 全程关闭。Look 为 chain_total、amount 75、静态累积，输入 1718×950、Eden 1x、游戏 30 FPS 上限。每阶段确认实际 revision/active，稳定 5 秒后采集 20 秒，均保留 20 个独立率样本。

| 阶段 | 应用呈现 FPS 均值 | 实际 NR 遍数 |
| --- | --- | --- |
| 增强关闭，前基线 | 30.04 | 0 |
| 一遍 NR＋静态 Look | 30.08 | 1 |
| 两遍 NR＋静态 Look | 30.13 | 2 |
| 增强关闭，后基线 | 30.07 | 0 |

初始战斗在一遍截图前结束；一遍/两遍/末尾关闭保持同一待机镜头，粒子并非确定性重放。30 FPS 上限掩盖剩余 GPU 预算，不能据此声称 NR 没有开销，也不能把与 debug 严格会话的全部差异归因于验证层。6 FPS 在本次 Release 场景未复现；真实运动画质与整链严格验收仍未完成。

Eden 模拟暂停停止 Present，源 ID 保持 459，旧 amount 75 保留；恢复后 amount 50 生效并恢复连续历史。没有捕获首个恢复帧的 reset。完全无 Present 的即时 Look 调参仍未实现；固定输入缓存测试证明持续呈现的精确重复输入路径。

补齐 `TRACE_FRAMES=0` 的遗漏：同时过滤 route_present_submitted、target_nr_pass_frame、target_nr_second_evaluate，并避免构造关闭的成功日志与退休详情；帧证据开启时缓冲写入。API 失败、第二遍降级、释放和独立的呈现退休失败事件保留。没有改变 GPU 提交、围栏或等待算法。

最终 quiet 构建图层 SHA-256 为 `46143b37d1da83687fe020d2de9aa62af249fc65598542637cbdc9684d0f588b`。独立 `eden-performance-20261005-214004-144-two` 核查的 12 个状态均为两遍 NR/静态 Look 活动，观察 ID 前进 304，日志增长 0 字节，五类逐帧成功/呈现事件计数均为 0。启动、生命周期与错误日志仍保留；此构建未重测上述四阶段 FPS。更新后的脚本默认使用 `release-build-quiet`，原 FPS 构建与日志保留。

最新完整回归 216 项通过、5 项硬件测试默认忽略；Look GPU 严格测试另行通过，0 错误/0 警告。工具箱与组件 host/Windows cargo check、格式检查均通过，无 Rust 错误或警告。首个 Release 会话执行正常关闭流程，NR/SDK shutdown 成功，设备/实例归零，最终仍有 `0xC0000409`。用户明确说明关闭增强也出现该问题，要求不用处理，故从本次修复范围排除；保留原始退出记录，不据此豁免其它验证错误。

用户再次按 Escape 停止 computer-use 后停止界面操作。M0 的原始验证问题、真实运动视频、完全无 Present 的即时调参、持续性增强、模型后缀重算与 M4–M6 仍待完成。证据摘要包含上述实测和日志核查，不将它们算作整个计划完成。

## 模型后缀重算、光流累积＋与颜色诊断（2026-10-05）

本轮继续完成 M2/M3/M4 的代码；上一节的待办状态为当时快照。加上随后代理布局/Submit2 修复后的完整回归 221 项通过，5 项硬件测试默认忽略；本轮另行执行实际 GPU Look 与真实 NGX 隔离诊断。工具箱和组件均通过 host/Windows cargo check，无错误或警告；前端类型/生产构建、8 项预设测试及变更文件 ESLint 通过。

- **模型后缀重算：** 精确重复输入上修改第二遍有效参数只调用第二个 Feature，第一遍缓存不写；第一遍参数、图结构、显式重试或指导/reset 边界重算必要前缀。颜色没变时源 ID 保持不变，包括恢复边界；状态区分 `sourceObserved`、`modelRecomputed`、`firstEvaluated`、`lookRecomputed` 和最终输出复用。重算 NGX 使用 reset；不向 Look 提交时间观察，相关 Look 历史失效，下一张新颜色重新播种。只有 GPU 提交成功才发布参数和 ID。粘滞第二遍失败的单遍输出可能已被 Look 修改，无法直接用于前缀，故该退化状态保守重算第一遍；普通参数修改仍不自动重试失败 Feature。保留现有第二遍前的安全 fence，不把后缀缓存实现当作 M5 提交重构。
- **真实 NGX：** `nr-suffix-ngx-003` 的 300 帧双实例检查通过，包含一次未提交录制丢弃及重建。固定源上的第二遍 75→100→75 调整增加 3 次第二遍调用、0 次第一遍调用、0 次源观察；第一遍逐位不变，75 的两次输出哈希相同。重复输入继续复用，下一张源颜色的第二遍历史可连续推进。进程正常退出，核心/同步错误 0、未审查警告 0，loader 安装通知原样保留。受限环境中的 `nr-suffix-ngx-001` 在 core shutdown 超时，保存为失败记录；002/003 完整运行通过。它们是合成输入，不是游戏/SR/FG 整链验收。
- **光流累积＋：** 显式实验模式 `optical_flow_plus`，默认仍为原光流模式。ping-pong 的 RGB 保存条件 log 幅度，alpha 保存 RGB 修正共同的存在支持，另增一张 RGBA16F 合成变化量纹理；普通模式仍为原来的三张历史纹理。缺失观察衰减支持而不把零混入条件幅度；出现/消退采用 60/180 ms 实验默认值，`timeMs` 控制条件幅度。存在阈值为 0.01 stops，支持低于 0.001 时清零。输入不相容、反向修正、无效/越界运动和重置立即拒绝旧值。可能改变时间均值，不默认替换普通模式。GPU 检查覆盖长期消失、再次出现、反向、真实颜色变化、运动失效、模式切换和纯 Look 重合成；RG16F/RG32F 两个实际 SPIR-V 路径均通过。
- **颜色保护/Oklab：** 增加 0～100% 的色相、暗部、高光和过度修正抑制，默认均为 0。暗部/高光由所选范围基底的线性明度决定；过度修正随 log 修正幅度的平方平滑增加。色相保护同时抑制垂直色度变化及越过中性的径向反转。Oklab 需显式选择，只改变色度的径向/垂直分解；明度与软上限仍为 stops，未重新解释旧参数单位。最终使用 SDR 线性 RGB 色域裁剪；中性、0、旁路、极值、暗部、高光、非有限值、alpha 和色相保护有实际 GPU 检查，真实场景阈值和视觉收益待验证。
- **诊断：** 原图、第一遍、最终原始模型、原始/受控变化量、低/高频明度、保护保留比例、历史有效性和逐像素实际历史权重视图。原始模型缓存和第二遍前缀不读诊断输出；进入/退出沿用 Look 修订的 SR/FG reset。低/高频视图单独请求 band 资源，准备失败回退到正常输出；固定输入检查验证两范围的数据来源和退出时恢复原模型。实际接受比例没有增加 CPU 读回统计，`pixelHistoryAcceptanceMeasured=false` 仍如实保留；权重视图不能被当作全局接受率测量。
- **配置/界面：** 预设 schema 4，v1/v2/v3 保留既有显式参数并填充默认值；Oklab、保护、诊断和光流累积＋有独立 DLL 能力标记及启动/实时拒绝检查。进阶颜色控制折叠只改变展示。完全无 Present 的暂停即时 Look 调参仍受旧限制；在持续呈现的重复输入上可重合成和模型后缀重算。

证据见 [本轮摘要](../src-tauri/crates/streamline-fg/evidence/nr-suffix-persistence-color-2026-10-05/README.md)。冻结 Release 图层 SHA-256 为 `38b83b463384c55beb91aa5ea482226c01007d25ed0e80068929b9d5ca67e77c`。原有 30 FPS 四阶段结果对应上一轮构建，本轮不沿用为新版性能结论。

新版 Release 的 **NR 关闭、严格核心/同步验证开启、读回和 timestamp 关闭** 基线已由用户操作进入 Everblight Plain。NR 关闭时已存在 54 条错误、10 条警告，其中 10 条为 SDK fake-swapchain buffer 的 TRANSFER_SRC/PRESENT 不匹配；其余涉及 stencil extension、SPIR-V 图像类型、查询范围和深度访问。原始日志保留，不因没有 NR 调用标记而把所有消息直接归给模拟器。VVL 每 ID 在 10 条后停止重复报告，条数不代表错误帧数。

定位到代理图像登记和应用 barrier 适配依赖可选 source probe，关闭探测时未生效。现将必要登记/布局转换从探测开关独立出来，补齐 legacy / synchronization2 barrier，并保证 Submit2 的共享队列锁不因关闭探测而跳过。只转换属于 live SDK proxy 且由 SDK 创建的普通图像；原生交换链保留 PRESENT，保留 queue family、subresource、pNext 和其它依赖。修复布局的运行构建 `de0c7d8d…` 已登记每链 3 张普通图像，当前严格日志不再出现该 fake-swapchain 布局错误；包含 Submit2 后续修复的最终冻结图层为 `250746cbbbda2547ab30e4054bb51037cb19f1b10e46893da2fc5bb942c2ec85`，与已加载的构建分开记录。

修复前四阶段粗粒度采样均为后台状态（约 13.20 / 11.25 / 10.32 / 13.30 FPS），**不能作为正常游玩性能对照**。已将后台拒绝加入采样脚本，重新在同一会话固定镜头采集前台数据；这也不将以前关闭验证的 30 FPS 当作严格验证下的性能结论。真实运动视频和稳定退休未验收，其余 Vulkan 错误未豁免。M5 保持严格基线门槛，M6 保持模型尺寸/重建/视频门槛；用户排除的 Eden `0xC0000409` 不在修复范围。当前不宣布整个计划完成。

随后在修复布局的同一会话、1724×962 固定镜头重测，拒绝后台状态后，关闭 / 单次 NR / 双次 NR / 再关闭的平均 app FPS 为 11.66 / 10.02 / 9.60 / 12.10，分别有 20 / 20 / 21 / 21 个约 1 秒样本；全部为前台、Look/SR/FG 关闭，并核对实际成功遍数。此结果含严格验证开销，仅为粗粒度吞吐，不代表各阶段 GPU 耗时，也不与不同尺寸/镜头、后台或无验证数据直接比较。

同一会话实际应用全部 10 种诊断视图及双次 NR、chain-total、P0 guide、Oklab 保护与光流累积＋，状态报告连续历史及 4 张时间纹理、56,623,104 字节 Vulkan 分配（54 MiB）。退出低/高频诊断后已有 band 纹理仍缓存，未声称退出即释放；原始输出和实际逐像素历史权重画面已观察，权重截图保存于会话目录。普通光流切换后回到 3 张时间纹理、42,467,328 字节（40.5 MiB）。这些是实际运行和诊断显示检查，不是视频画质验收。

用户完成镜头/遮挡动作后明确反馈“帧数太低，看不出拖影”，因此未将该动作记录认作视觉通过。严格会话退出后呈现停止，但进程持续占用 CPU；结束该测试进程后保存 `forced-retirement.json` 与最终 `target-result.json`，正常退休未通过（退出码 -1、缺少 clean vkDestroyDevice）。这不是用户排除的 `0xC0000409`，没有扩大豁免范围。

为继续实际画质与性能对照，另开 `eden-experiments-quality-20261005-232743-320`，使用包含 Submit2 修复的最终冻结 Release；`target-inputs.json` 核对图层哈希 `250746cb…`、`graphics_launch=true`、`nr_validation_requested=false`、`nr_readback_requested=false`、`frame_trace_enabled=false`，启动脚本同时关闭 GPU 计时。初始 NR/Look/SR/FG 关闭。使用已有 `--graphics-launch` 正常运行入口；其结果与严格验收分开，不以无验证运行豁免上述错误。启动/标题动画观察到约 30 FPS，真实游玩场景尚需用户进入后重新测量。

用户随后完成第一场战斗并关闭教程；四阶段采样保持当前 Everblight Plain、目标 55m 固定镜头（与严格会话的镜头不完全一致）。NR 输入 1724×962，采样全部前台、Look/SR/FG 关闭，先稳定 5 秒再采集 20 秒；记录模块列表只有本次 probe 图层，没有 Vulkan 验证模块。

| 阶段 | app FPS 均值 | 独立样本 | 实际 NR 遍数 |
| --- | --- | --- | --- |
| 增强关闭，前基线 | 30.0691 | 21 | 0 |
| 一遍 NR | 30.0844 | 20 | 1 |
| 两遍 NR | 30.0809 | 20 | 2 |
| 增强关闭，后基线 | 30.0771 | 20 | 0 |

这证明最终构建在该游玩场景的正常运行模式下没有复现之前的低帧率；30 FPS 游戏上限仍掩盖 GPU 余量，不声称两遍零开销。之后启用双次 NR、整链 Look、Oklab 保护、逐点验证和光流累积＋；核对实际模式/历史活动、诊断关闭，开始重新请求用户评估慢速平移、快速转向和遮挡。运动画质仍等待反馈，不把状态采样当作视频。

随后 45 秒增强状态记录包含 45 个独立前台/NR 活动样本，app FPS 均值 30.0826，范围 29.7221～30.8006；该记录未同步证明用户动作发生时段，尚未收到本次运动观察反馈。当前会话保持双次 NR＋光流累积＋活动以供人工评估。

用户已回复本次“动作完成”，已保存动作完成标记和随后画面，但未给出异常观察结论，故另外请求明确的有/无异常反馈。最终构建的普通光流、静态、光流累积＋三种实时模式均在前台应用成功，实际 NR 遍数为 2；普通/静态使用 3 张时间纹理，累积＋使用 4 张。检查后恢复累积＋，不将三种模式应用成功等同于运动画质通过。

随后用户明确反馈累积＋“未看到明显异常”，并在普通光流完成同样动作后回复“动作完成，未看到明显异常”。两种模式的人工观察通过，保存原文；没有录像，不因此完成视频门槛或推断哪种模式更好。普通光流的 45 秒状态采样只有 2 个活动前台样本，其余为后台，故不把该采样当作运动期间的前台性能对照。

通过 Eden 菜单暂停：控制修改前后源 ID 均为 13183，已应用修订仍为 9，新修订 10／Look amount 75 尚未应用；状态不新鲜且无呈现率。恢复后约 3 秒，源 ID 13280、修订 10 和 amount 75 已应用，NR 活动，约 30.75 FPS，历史状态连续；没有捕获首个恢复帧的 reset。恢复 amount 100 和累积＋后继续正常退休检查。完全无 Present 的即时 Look 调参限制仍未解决。

为继续 M0 错误归属调查，增加严格验证专用的应用 API 调用上下文，覆盖 shader/pipeline 创建、render pass/rendering 开始、深度/附件清除、query end 和 draw。关闭严格验证时不返回这些额外包装；保留 SDK/driver 下一层函数和原始参数，未知或底层不支持的入口不被广告。它只给回调标记应用入口，不直接判定最终错误所有者，也不修改/过滤验证消息或修补渲染行为。两项测试覆盖正常入口门控、回调上下文恢复、原始 query 参数转发及缺失入口。最新完整回归 223 项通过、5 项默认忽略，组件/工具箱 host 和 Windows check 无错误/警告；新冻结图层 SHA-256 `17bfa392b2c7fd7711aec18a2c67d172e7347fea182af00365323c8931f4b845`。上述画质/FPS 仍归属于旧的 `250746cb…` 构建，新构建尚待严格会话核查。

正常会话退出时两遍资源释放、NR snippet shutdown 和 SDK shutdown 成功，设备/实例归零；最终退出仍为 `0xC0000409`，沿用用户既有排除并保存 `launch-result.json`，不豁免之前严格会话的强制退休失败。新的 `eden-attribution-baseline-20261005-235024-521` 用户已确认进入 Everblight Plain（目标 53m）；原始快照为 44 错误/10 警告，代理布局错误为 0。应用入口标记不直接证明最终错误所有者。2026-10-06 用户指出这些错误已确认在关闭增强时出现，要求停止重复核验；归属调查至此停止，后续推进 M5/M6。

## 集中提交与推理降尺寸实验（2026-10-06）

按用户澄清，终止重复模拟器基线调查，撤回未完成的独立 renderer-baseline 入口，保留此前未过滤日志。已知模拟器错误不再阻塞 M5/M6 实施；前文“不能推进 M5”为当时快照。Eden `0xC0000409` 按既有要求不继续排查。

M5 新增默认关闭的 `--nr-consolidated`。输入比较与借用资源退休保留独立提交/等待；第一遍与第二遍/Look/输出保留各自命令缓冲，在同队列的一次提交中按顺序执行，以 GPU 内存 barrier 连接。Feature、参数、历史和前缀结果保持独立。第二遍录制失败时丢弃未提交命令，单独提交并完成第一遍，再释放失败实例、录制第一遍 Look/输出；提交或等待失败直接失败，不确认源帧/控制修订。现有 NR→SR semaphore 和最终完成门槛保留，尚未扩展跨帧资源环。

真实 NGX 隔离诊断新增 `--two-pass --consolidated`，读回使用第三张命令缓冲，避免覆盖尚未合并提交的前缀。640×360 split/combined 的 300 帧中，各 299 张正常第二遍输出、第一遍输出和固定源帧 75/100/75 后缀重算哈希逐项一致；均完成一次有效第二遍录制丢弃、实例重建和前缀安全完成。移动帧推理提交从 599 次降至 300 次，中间等待从 300 次降至 1 次（故障恢复）。排除前 5 个正常样本后，CPU 录制/提交/等待中位数为 7.731/7.315 ms，P95 为 10.167/9.864 ms；包含验证、合成输入准备和模型执行，不是游戏 FPS 或纯 GPU Evaluate 时间。

原始日志均无核心/同步错误，4 条隐式图层禁用通知单列。但 split、combined 及 combined 重试均在 feature/参数释放和 snippet shutdown 成功后于 NGX 核心 shutdown 超时。失败原样保留，正常核心退休门槛尚未通过，不将输出验证成功写成整个诊断成功。

M6 新增 `--nr-inference-scale 50..100`，默认 100；50 表示每边减半。原尺寸 P0 和前一源图用于精确源观察，入口只将 P0 降采样一次，所有模型实例、各遍结果、Look 与时间历史共用推理尺寸。运动纹理同步重采样，仍为归一化 UV，保持原 viewport 基准缩放；深度仍明确为合成常量。原 crop/flip 与 encoded-byte 接入在缩小前完成；原尺寸/映射变化沿用完成边界、实例重建与历史重置。

出口 `nr_reconstruct.comp` 双线性重建受控输出与低分辨率 P0 的总线性 SDR RGB 差，再加回原尺寸 P0，按 sRGB 编码并限制 SDR 范围，逐位保留原始 alpha。零差直接返回原图，避免细线/HUD、黑色和次正规数被整图插值或再次编码。诊断视图直接放大显示，不解释为修正；重建结果不反馈模型，模型前缀缓存仍在低分辨率。状态公开两张原尺寸 RGBA16F 纹理的实际 allocationBytes；复制、重建带宽和显存成本需另测，不承诺必然更快。

降尺寸入口限定 SDR、比例 50–100%、缩小后的尺寸至少 320×180，拒绝旧的等尺寸 NR 读回；这是保守实验边界，不声称找到模型最小尺寸。320×180 真实 NGX 双实例 300 帧已完成有限输出、运动更新、丢弃/重建与后缀确定性检查，核心关闭结果另记。重建硬件测试使用 19×3→37×5 非整数比例，覆盖交错细节、黑/白/次正规数、正负修正、SDR 截断、重复使用和 alpha，核心/同步验证 0 错误/0 警告，4 条 loader 通知单列；该 shader 测试不证明模型支持 19×3。

两个实验都有独立 DLL 能力标记，旧组件在启动前拒绝，普通工具箱默认启动行为不变。视频画质、实机性能、长帧/显存、NR→SR→FG 组合与正常退出仍需单独验收。可选低频时域重建仅在证据证明有需要时实施，当前人工观察没有明显异常，未因此增加算法。详细日志/哈希和验收状态见 `src-tauri/crates/streamline-fg/evidence/nr-scheduling-resizing-2026-10-06/summary.json`。

新版正常 Release 会话在 Everblight Plain、目标 51m 的固定镜头验证双次 NR＋集中提交＋75% 推理。23 个独立前台样本均为原图 1724×967、推理 1293×725、一次推理提交／零中间等待，平均 30.101 FPS；30 FPS 上限掩盖余量，未证明优化收益。NR 保持开启时一遍／两遍实时切换通过，实际 Feature 数量分别为 1／2，最终恢复两遍。

用户完成慢速转镜头、快速转向及岩石边缘遮挡后明确反馈“未看到明显异常”，新路径人工观察通过。45 秒状态记录的 46 个独立样本中，32 个活动前台样本平均 30.072 FPS；没有同步动作时段或录像，不据此完成视频验收，也不推断优于普通光流。

动态尺寸冒烟检查覆盖 1399×967→1049×725、2560×1325→1920×993、1920×1080→1440×810（原图→推理）；各次资源重建后维持双 Feature、8 张私有 NR 纹理。两张重建纹理分配分别为 22,937,600／57,671,680／35,389,440 字节，未包含所有 Look／时间历史／NVOF／SDK 内存。扩窗拖动曾因端点超出窗口被工具拒绝，未执行；改用最大化完成放大检查。最终使用 1080p 预设，未恢复精确的初始窗口尺寸。

1080p 下 NR＋SR、NR＋FG、NR＋SR＋FG 实时组合分别有 18／17／18 个有效前台样本，FG 明确报告生成帧和两次 SDK 呈现，均通过本次无验证层冒烟检查；对应 app FPS 均值 30.063／30.075／28.680，未作为同条件降尺寸性能收益证据。检查后恢复 SR／FG 关闭。

本次正常 UI 退出中第一遍 Feature／参数创建与释放各 4 次，第二遍各 5 次，NR release、snippet shutdown、SDK shutdown 全部成功，设备与实例归零，未强制终止。最终 Eden 退出码仍为用户排除的 `0xC0000409`；本次游戏资源退休通过，不豁免前述隔离 NGX 核心关闭超时。当前待完成项收窄为同场景运动录像对照、受控耗时／长帧／显存对照及严格整链与隔离核心退休验收；已通过的人工观察、尺寸和组合冒烟不重复要求用户操作。

## 工具箱启动实验接入与代码交接（2026-10-06）

按用户最新要求，只完成代码与构建检查，后续功能测试由用户执行；此前请求录像已不作为本轮交接前提。本轮未启动模拟器、运行 GPU／NGX 诊断或执行功能测试。

- 在 NR 高级配置增加“启动实验”：集中提交开关、每边 50～100% 推理尺寸和恢复默认按钮。配置分别保存为 `streamline_nr_consolidated`／`streamline_nr_inference_scale`，旧配置缺省为关闭／100%；非法比例在 Rust 反序列化与启动入口均拒绝。
- 这两个选项仅在下次专用启动生效，不混入实时模型／Look 修订或游戏画质预设。启动时核对各自 DLL 能力标记，非默认设置需要已安装 NR 运行库，显式转成 `--nr-consolidated`／`--nr-inference-scale`；父进程继承的同名环境变量被清除。
- 运行状态增加实际 `inferenceScalePercent`，前端同时展示实际推理尺寸及集中／逐遍提交方式。只读当前会话的管线状态，不以保存值冒充已应用。
- 清理集中接入审查中发现的重复累积＋能力检查与状态赋值。保留旧版组件字段缺省兼容，组件／模型版本与历史测试冻结目录不改写。

低频时域重建仍是计划中的条件项，当前没有证明需要它的证据，未增加新算法。完全无 Present 的模拟器暂停不触发本组件的呈现处理，参数在恢复呈现后应用；重复呈现同一源图时已有即时 Look／后缀重算。独立呈现线程、跨帧命令环、HDR 和更低模型尺寸不包含在当前代码交接中。

编译交接要求为组件和工具箱 `cargo fmt`、host／Windows `cargo check` 无错误警告，前端 app／node 类型检查与生产构建。功能测试、视频比较和完整性能／退出验收由用户继续，不能把本次构建成功当作这些验收通过。

本轮上述格式化、四项 Rust 编译检查、前端 app／node 类型检查、Vite 生产构建及组件／工具箱 Release 构建已完成。修改的 Vue、状态类型、默认配置文件 ESLint 零错误警告；`tauri.ts` 全文件检查报告 3 个既有 `any` 规则错误，均位于本轮未修改的代码，未扩展修改范围。标准前端脚本的增量缓存写入受沙箱限制，因此类型检查改用相同项目的非增量无输出命令，Vite 以获准的构建写入权限完成。

独立构建快照位于 `src-tauri/target/nr-code-handoff-20261006/`，包含 `NsEmuTools.exe`、NR 图层、启动器与 NR 诊断程序，哈希见 `hashes.json` 和原证据摘要的 `code_handoff`。它不是已经发布或安装的运行包：工具箱新选项仍需要与新图层／启动器匹配的组件包。未替换已安装组件、修改模型或覆盖旧测试构建。可用既有 `package-local.ps1`／`package-runtime.ps1` 打包流程制作对应组件，发布和功能测试不属于本轮执行。

## 本地完整打包与组件更新（2026-10-06）

用户随后要求“打包吧，组件也换成最新的”。已生成工具箱 0.6.3 的 Windows x64 安装包与完整便携包，并将本机 Release 目录中 Eden／Ryujinx 的托管组件切换到 `local-abc947812335-4d67386cfb92-nr3108-v8`。更新前核对目标程序、原清单与收据，更新过程持有组件存储锁；旧不可变版本、原选择指针备份和既有配置保留，游戏会话未改动。

组件通过既有 `package-local.ps1` 的 `native-nr` Release 配置构建，完整包共 23 个文件，包含模型、SDK、许可证与来源记录。它与先前包含 `nr-diagnostics` 的代码快照具有不同构建哈希，不能沿用该快照或旧游戏测试作为新包的功能验收。NR 权重仍固定为 310.8.0，稳定运行库未更换；本轮“最新”对应最新本地工具箱、图层和启动器源码构建，没有下载新的 NVIDIA 模型或公开发布。

工具箱嵌入完整离线清单，并修正本地实验构建的组件更新选择：非在线的 `local-` 清单始终使用随包组件，检查更新不查询公开发行版，避免误装旧图层；公开在线清单沿用原更新行为。界面相应显示本地安装／校验信息。此改动后重新执行 `cargo fmt`、工具箱 host 与 Windows target 的 `cargo check`，零错误／警告，再完成 Release 与 NSIS 构建。

打包使用独立 Tauri 配置覆盖版本为 0.6.3，将各组件逐文件映射到主程序旁的 `streamline-fg-package/`。初次绝对目录映射产生错误目标路径的安装包已单独留作诊断，不对外交付；修正后重新生成 NSIS，逐项核对全部 23 个目标路径与源文件哈希。工具箱二进制内嵌版本和全部组件哈希均匹配清单。便携包仅包含主程序、23 个组件、清单、说明和空 `config.json`；核对 ZIP 全部 27 个文件的 CRC、名称和 SHA-256，未包含用户配置、模拟器、游戏、存档或会话数据。

交付目录：`output/NsEmuTools-0.6.3-nr-v8-20261006/`。便携包应完整解压到新目录；覆盖旧目录前需保留原 `config.json`。安装到新工具箱目录时可从附带组件完成本地安装，不依赖公开组件下载。原代码快照与测试证据保持原哈希；本轮未运行游戏或 GPU 功能测试，后续功能测试仍由用户执行。

| 构建／交付文件 | SHA-256 |
| --- | --- |
| `NsEmuTools.exe` | `f4bc0d7ede5c9564734ea8f104fdc6f71c3d4faddc8ab1788fe7ee4f4716f06e` |
| `streamline_probe_layer.dll` | `4d67386cfb9269ece0adf5d24df0be4fb850141882e40da0b8a4ff7979897e0e` |
| 安装包 `NsEmuTools-0.6.3-nr-v8-windows-x64-setup.exe` | `33f7f3a76851ef303e52d4429e716a531462646ae9f19c5d209b63edc8f4bbad` |
| 便携包 `NsEmuTools-0.6.3-nr-v8-windows-x64-portable.zip` | `a83b3df8984e3ac66e87f0bb40a7f2d80c721ab59779e090d4d4840444e03f2d` |

完整静态核对结果见交付目录的 `package-verification.json` 与 `SHA256SUMS.txt`。组件小包、稳定包及源代码包保存在 `src-tauri/target/runtime-release/`，未上传或发布。

## 推理尺寸前置与固定长边上限（2026-10-06）

按用户要求，将推理尺寸从底部折叠的“启动实验”移到 NR 高级配置第一项，默认展开；集中提交仍在底部独立的“提交方式实验”。尺寸提供两种方式：按输入比例（保留旧的 50～100%）和固定长边上限（320～8192 整数像素，首次切换填入 1920）。旧配置继续使用比例 100%，不自动打开上限模式。

新增启动配置 `streamline_nr_inference_max_edge`，缺省 0 表示比例模式。非零上限取代已保存的比例：以输入最长边为基准，超过上限时通过整数有理缩放向下取整，保持宽高比；较小输入不放大。2560×1325、上限 1920 对应 1920×993；输入继续增大时最长推理边仍不超过 1920，原始输入与最终输出保持原尺寸。固定模式允许有效比例低于 50%，但缩小后的宽／高仍需至少 320／180，且沿用 SDR、模型尺寸与旧读回边界；极窄输入可被保守尺寸检查拒绝。

配置反序列化、工具箱启动、独立启动器和组件入口都校验上限。启动器新增 `--nr-inference-max-edge` 和环境变量；工具箱清除父进程继承值，并检查独立 `NS_EMU_NR_INFERENCE_CAP_V1` 能力标记，旧组件明确拒绝该模式。所有 NR 实例、Look 和出口重建复用原有尺寸资源链路，输入尺寸变化仍触发资源重建；没有增加第二次入口采样或修改 shader。

状态增加实际 `inferenceSizingMode`／`inferenceMaxEdge`，固定模式的 `inferenceScalePercent` 报告当前有效整数百分比，主面板显示实际长边上限与推理尺寸。保存值不冒充当前会话状态。尺寸设置仍需关闭游戏并重新以画面增强启动；不进入实时控制或画质预设。非法输入／空输入不保存，也不暗中切回比例模式。

组件与工具箱 `cargo fmt`、host／Windows target 编译检查均通过，零错误／警告；前端 app／node 类型检查、修改文件 ESLint 与 Vite 生产构建通过。沿用用户自行测试的要求，本轮未启动游戏、执行 GPU／NGX 诊断或功能测试。组件版本更新为 `local-1129d1740d45-85e70280ea65-nr3108-v9`，模型与稳定运行库保持固定版本；本机 Release 的 Eden／Ryujinx 已切换，旧组件和选择指针备份保留。

固定上限 v9 交付核对：安装包与便携 ZIP 已生成，仍包含 23 个离线组件。嵌入清单／组件哈希、23 个 NSIS 资源目标路径及 ZIP 的全部 27 个文件 CRC／SHA-256 均通过；未复用旧版游戏验收作为本次功能结论。

| 文件 | SHA-256 |
| --- | --- |
| `NsEmuTools.exe` | `e3350bd2a9f2b337e4ab672344289dc0a48d9dd9209d42e951b89021e153f21b` |
| `NsEmuTools-0.6.3-nr-v9-windows-x64-setup.exe` | `978162a1edca1cad4845e3eeb9b9d41a35f02c8946913e86f878fd641a303f13` |
| `NsEmuTools-0.6.3-nr-v9-windows-x64-portable.zip` | `3affd4b8fdcc88c79aae64d4b354b03e239c405caf126abfe5d385fb600dc435` |

交付目录：`output/NsEmuTools-0.6.3-nr-v9-20261006/`。旧 v8 交付目录保留，但不含本次固定上限功能；新版请使用 v9。完整清单见该目录的 `package-verification.json`／`SHA256SUMS.txt`。

## 独立公共输入尺寸缩放（2026-10-06）

用户要求将尺寸控制拆出 NR，命名为“输入尺寸缩放”并放到效果上方，允许 NR 关闭时使用。旧实现只降低 NR 推理尺寸，NR 会将变化量重建到原图后交给 SR／FG，不能直接视为它们的输入尺寸已经缩小。本轮增加独立公共输入阶段，界面从 NR 高级配置移到主面板 NR／SR／FG 之前。

配置改为 `streamline_input_scale`／`streamline_input_max_edge`，旧的 NR 字段通过 serde alias 迁移数值；缺省仍为 100%／0。比例、固定长边上限、较小输入不放大和 320×180 保守缩小边界保持一致，设置仍需重新以画面增强启动。公共设置不会加入实时 NR 修订或 NR 画质预设。

公共入口只降采样一次，将已解释 crop／flip 的源图写入私有、非线性编码的 RGBA16F 图像；mutable-SRGB 来源沿用先等尺寸复制到 UNORM、再缩小的编码字节契约。NR 与 SR 接收同一小尺寸 Source，所有 NR 模型／Look 资源以此尺寸运行，普通启动不再叠加旧 NR 推理缩放／总变化量重建。NR／SR 都关闭时，将该输入等比放回原 viewport；只启用 FG 或全部增强关闭也能应用输入缩放。窗口／交换链输出尺寸不变，FG 仍按最终呈现尺寸运行，不宣称降低了 FG 模型尺寸或一定提升帧率。文字和细线也参与公共缩放，画质语义与旧 NR 专用变化量重建不同。

硬件光流继续分析原始、未经处理的连续 Present 对；公共阶段在光流提交后准备输入，保留光流可用性标记，消费原有等待信号量后不再重复传递。完成 fence 后才交给 NR／SR；资源重建／源映射身份变化使消费者重置，保留原 viewport 作为运动坐标基准。没有跨帧资源环，新增等待和带宽成本需用户实测。缩放资源在交换链／设备退休前完成并显式释放；尺寸／格式准备失败报告实际原尺寸和错误，不冒充已应用。

公共启动器参数为 `--input-scale`／`--input-max-edge`，由独立 `NS_EMU_INPUT_SCALING_V1` 标记保护；它属于 SDK 接入而不依赖 `native-nr` feature 或 NR 模型安装。工具箱清理继承环境变量，在 NR 参数块外传递公共参数；NR 关闭时保存的集中提交实验不再阻止独立缩放启动。旧 `--nr-inference-*` 保留供原隔离诊断，不与普通公共缩放混用。状态增加 `inputScalingSupported`／`inputScale`，公开实际原图、处理输入、输出尺寸和实际消费者。

组件 host／Windows 全 feature 全 target 检查、额外的仅 `sdk-bridge` 组件编译、工具箱 host／Windows 检查、前端 app／node 类型检查和修改组件 ESLint 均通过，零错误／警告。本轮未执行游戏、GPU／NGX 或功能测试；旧 v9 的 NR 专用缩放观察不用于验收此公共阶段。

已完成 Vite、组件／工具箱 Release 和 NSIS 构建，交付 v10 安装包与便携 ZIP，目录为 `output/NsEmuTools-0.6.3-input-v10-20261006/`。完整离线组件仍为 23 个文件；嵌入版本／全部组件哈希、23 个安装资源目标路径、便携 ZIP 的 27 个文件名称／CRC／SHA-256 和公共输入缩放 DLL 标记均通过核对。v9 交付目录、旧组件与指针备份保留，本机 Release 的 Eden／Ryujinx 已切换到 `local-e0d4acf90eaf-e5fedaae0199-nr3108-v10`；模型和配套稳定运行库未更换，未公开发布。

| v10 文件 | SHA-256 |
| --- | --- |
| `NsEmuTools.exe` | `53c1f01b31c7444b4f08036a091553a8a4320e2b719dc9a9bc7d015c1acfa509` |
| `streamline_probe_layer.dll` | `e5fedaae019953c2863cd9e819ec0aca7faf075d181cb1825e47a7a82d8c3a70` |
| `NsEmuTools-0.6.3-input-v10-windows-x64-setup.exe` | `4bbd92088441f3f7f266d2eaaad7732bfc275c15a8f69500fbd70cb92a573c7a` |
| `NsEmuTools-0.6.3-input-v10-windows-x64-portable.zip` | `9a6558bdb883473ca99c617f2d20034d30a594c8fb8a741c0f62bea40ac509d8` |

完整文件清单和公共缩放行为／验证边界见交付目录的 `package-verification.json`。本轮未进行功能测试，旧游戏证据继续归属原构建。

## 输入尺寸实时调整（2026-10-06）

v11 去掉 v10 公共输入设置只能启动时读取的限制。工具箱先保存配置，再向当前专用会话发送完整 `inputSizing` 与单调递增 `inputScaleRevision`；组件用 `inputScalingLiveSupported` 声明支持，旧会话需要更新组件并重启一次。后续比例、固定长边上限、模式切换与恢复原尺寸均在下一帧应用，无需重新加载游戏。暂停时保留请求，恢复画面后再确认。

组件工作线程验证完整数值范围，拒绝过期、不完整或非法修订；渲染线程只读取原子一致的帧控制快照，不改进程环境，也不做文件 I/O。帧边界先完成上帧提交和设备任务，再按消费者先于源图的顺序释放 SR、NR、公共缩放资源并重建；同步清除历史、失败重试状态，重置 NR／SR／FG 与光流。只调整输入，不改效果开关、SR 倍率或输出交换链尺寸；无需安装 NR 模型即可实时缩放。

`inputScale.appliedRevision`、快照参数和实际尺寸用于界面确认。缩放准备失败明确报告错误及回退原尺寸；“已发送”不视为“已生效”。界面允许重新发送相同保存值以重试，并隔离更换模拟器目标时的异步回复。本轮功能、GPU、画质和性能测试由用户执行，不复用旧构建的游戏观察作验收结论。

已执行组件与工具箱的 `cargo fmt`、host／Windows target 全 target 检查，以及仅 `sdk-bridge` 组件检查，零错误／警告；前端 app／node 类型检查、修改文件 ESLint、Vite、Release／NSIS 构建通过。v11 的 23 个离线组件、嵌入版本／哈希、NSIS 资源目标路径、两个输入缩放 DLL 标记和便携 ZIP 的 27 个文件名称／CRC／SHA-256 已核对。本机 Release 中 Eden／Ryujinx 指针已更新为 `local-cc8949e46572-8022e54e0d1b-nr3108-v11`，旧 v10 与指针备份保留；运行中的旧会话未被改动，模型和稳定运行库未变。

交付目录为 `output/NsEmuTools-0.6.3-input-v11-20261006/`；完整验证边界见 `package-verification.json`，代码与交付记录追加到原证据的 `dynamic_input_scaling_followup`，不覆盖旧验收记录。

| v11 文件 | SHA-256 |
| --- | --- |
| `NsEmuTools.exe` | `427487287bd0f97c9c6191dec3bc5f2053f927825722753156db04de4f20bfb6` |
| `streamline_probe_layer.dll` | `8022e54e0d1bbd1c2c43eb5e5890d60c605a3b9e6f7dc88e715e1d3ff84807f3` |
| `NsEmuTools-0.6.3-input-v11-windows-x64-setup.exe` | `6e88bc3ccf1a04b352f1687f9574359e9f6d55dfb889cfb43260a1ac9b4b2244` |
| `NsEmuTools-0.6.3-input-v11-windows-x64-portable.zip` | `25cc07e755a34251c6150c1eecd29f1353d8c71ebd11d4a564725532553474e1` |

## 显卡菜单与选项门槛（2026-10-06）

工具箱通过 Windows DXGI 枚举硬件适配器，跳过软件适配器，按 NVIDIA 的 PCI vendor ID 判定是否展示“图形增强”菜单。枚举全部显卡，避免核显排第一的双显卡电脑误判；检测完成前、失败或无 NVIDIA 时不展示入口，直接进入页面也不实例化增强面板，失败可重新检测。不调用外部 shell、`nvidia-smi` 或模拟器进行检测。

按已知 GeForce 型号限定 SR／DLAA 为 RTX 20／30／40／50、NR（DLSS 5）为 RTX 50、FG 2× 为 RTX 40／50，FG 3×～6×／动态模式为 RTX 50。依据 [NVIDIA DLSS 5 说明](https://www.nvidia.com/en-us/geforce/news/dlss-5-3d-guided-neural-rendering/) 和 [DLSS 4.5 帧生成说明](https://www.nvidia.com/en-gb/geforce/news/dlss-4-5-dynamic-multi-frame-generation-6x-mode-released/)。未知型号不猜测能力，专业卡名称的 RTX 4000／5000 不误识别为消费级 40／50 系列。不支持的效果开关、高级配置、倍率条目与动态模式置灰并注明门槛。实时会话报告的 `maximumGenerated` 进一步收紧本机最大倍率，0 不作为无限制；断开或无新帧时回到本机初筛。

初筛表示本机存在支持的显卡，并非模拟器当前选用设备的保证；页面明确提示多显卡时选择支持的 NVIDIA 显卡，运行库仍判定实际设备能力。旧保存配置超出支持范围时阻止增强启动，用户可一键关闭不支持的效果、降低倍率与改回固定模式；不自动重写偏好。工具箱启动后端同时检查显卡系列，避免缓存配置或绕过界面启动不支持的效果。组件保持 v11，不更新模型或稳定运行库，保留实时输入尺寸功能。

工具箱已执行 `cargo fmt` 与 host／Windows target 全 target 检查，零警告／错误；前端 app／node 类型检查、图形相关修改文件 ESLint、Vite 与 Release／NSIS 构建通过。AppDrawer 修复未使用导入和可改 const 的错误，保留原有无关格式警告；本轮没有显卡检测实机或模拟器功能测试，按用户要求由用户执行。v12 打包验证嵌入组件清单／全部哈希、23 个 NSIS 资源目标路径及便携 ZIP 全部 27 个文件 CRC／SHA-256，证据追加在 `graphics_gpu_ui_followup`。

交付目录：`output/NsEmuTools-0.6.3-gpu-v12-20261006/`。v11 交付目录保留，组件与已安装指针仍是 `local-cc8949e46572-8022e54e0d1b-nr3108-v11`。

| v12 文件 | SHA-256 |
| --- | --- |
| `NsEmuTools.exe` | `b8b2f9bc8b40e48cc35c3c926eade73fa1ca549f7d66fcdc5077ee79e0e30210` |
| `NsEmuTools-0.6.3-gpu-v12-windows-x64-setup.exe` | `c5b6120dcab846b4cdbaa9486729f51dba94f6427c66392d3c7ccc473ee3e508` |
| `NsEmuTools-0.6.3-gpu-v12-windows-x64-portable.zip` | `8163e32f8a627a9cae7f3bc31d84b1ca554ca6cd7f3144796a86b3ebe36fc44f` |

## 显卡名称去重与直接 Release 交付（2026-10-06）

显卡型号显示先统一空白再按名称去重，同名 NVIDIA 适配器只显示一次；底层枚举和能力判断保留全部适配器。前端类型检查、修改组件 ESLint 与 Vite 构建通过，未执行游戏或 GPU 测试。

按用户新要求使用 Tauri `--no-bundle`，直接更新 `src-tauri/target/release/NsEmuTools.exe`，与旁边 `streamline-fg-package` 目录一起交付；不使用新的 `output` 目录，也不为本次交付生成安装包或 ZIP。该偏好已写入根 `AGENTS.md`，旧交付历史保留。Release 构建成功，23 个组件文件哈希与 EXE 嵌入清单校验通过；组件仍是 `local-cc8949e46572-8022e54e0d1b-nr3108-v11`。EXE SHA-256 为 `15b363adb8fc15f52a2635d7390da08c0ab77c0ccce5edddd9488167ffa4a8f8`，核对记录在 Release 目录的 `release-verification.json`，原证据追加 `gpu_name_dedup_release_followup`。

## 显卡初筛检查状态修复（2026-10-06）

预检列表原先固定显示“显卡与驱动：待确认”，未接入菜单已使用的显卡检测。现改为“显卡型号初筛”，使用同一 DXGI 枚举：检测到 NVIDIA 硬件显卡显示通过，未检测到显示阻止，仅检测出错时显示待确认并提供原因。详情统一空白、去重显卡名称，列出型号允许的 SR／DLAA、NR 与 FG 选项。初筛通过不代表驱动或模拟器实际设备已通过运行验证，`runtime_state` 仍由专用启动后的运行状态确认；现有检查用例移除固定 GPU 待确认断言，保留未运行时不能宣称运行确认的约束。

已执行 `cargo fmt`、host 与 `x86_64-pc-windows-msvc` 的 `cargo check --locked --all-targets`，零错误／警告。前端本轮未变，沿用上一轮构建。Tauri `--no-bundle` Release 构建成功，直接更新 `src-tauri/target/release/NsEmuTools.exe`；SHA-256 为 `5107a266ba7dc32c02ebef09161a5d7fb735314400c396d631561aed2a374101`。23 个组件文件与 EXE 嵌入清单／全部组件哈希校验通过，配套组件仍为 v11；未生成安装包、ZIP 或新的 output 交付目录，未执行游戏或 GPU 功能测试。记录更新到 Release 的 `release-verification.json`，原证据追加 `gpu_preflight_release_followup`。

## 本地组件包检查状态修复（2026-10-06）

原“组件小包版本”只有安装版本匹配最新清单时才显示通过，本地配套包完整可用但尚未安装也显示待确认。现将本地构建该项改为“本地组件包可用性”：沿用预检的完整文件哈希校验结果，可用即通过，详情明确提示尚需安装到当前模拟器；已安装版本匹配且校验通过也可通过。可用性不冒充安装完成，安装／更新状态与操作继续独立保留。缺少或损坏本地包时，版本清单不再覆盖真实可用性，显示不满足并保留不可用状态。在线组件的版本检查逻辑不变。

`cargo fmt`、host 与 Windows target 的 `cargo check --locked --all-targets` 均通过，零错误／警告；前端未变。Tauri `--no-bundle` 成功更新 `src-tauri/target/release/NsEmuTools.exe`，SHA-256 为 `a4701157704b79b2b61e07e359a9016b3426ca22b894fbaebfb5baf88b1b1a95`。23 个配套组件文件及 EXE 嵌入清单／哈希校验通过，组件仍为 v11。未生成安装包、ZIP 或新的 output 目录，未执行游戏或 GPU 功能测试；核对记录在 Release 的 `release-verification.json`，原证据追加 `component_preflight_release_followup`。

## 移除两项预检提示（2026-10-06）

按用户要求移除预检列表的“画面输入”（`source`）与“其他图形组件”（`layers`），删除仅为后者提供文案的目标目录冲突文件枚举及对应未使用导入，移除现有用例对输入提示的旧断言。启动路径、运行时检查和增强组件不变。

`cargo fmt`、host／Windows target 的 `cargo check --locked --all-targets` 均通过，零错误／警告；前端未变，未执行游戏或 GPU 功能测试。Tauri `--no-bundle` 构建成功，直接更新 `src-tauri/target/release/NsEmuTools.exe`；SHA-256 为 `5dbd3937ae9e63d10bc30a033d62be6f23518a7ab93ff4d7362e36733718bfbd`。23 个组件文件与 EXE 嵌入清单／哈希校验通过，组件仍为 v11；不创建安装包、ZIP 或新的 output 目录。记录更新到 Release 的 `release-verification.json`，原证据追加 `preflight_prompt_removal_followup`。

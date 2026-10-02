# DLSS5 原生 Vulkan 集成计划

日期：2026-09-30。

状态：待实施。已完成现有代码、上游实现及本地 NR DLL 导出表的静态核查；尚未完成直接 Vulkan NR 的 GPU 验证。

目标是在工具现有 Rust Vulkan 层中直接调用 DLSS5 Neural Rendering（NR），由工具管理运行库、启动与实时控制，解除运行时对 ReShade、Feeder 和 RenoDX addon 的依赖。优先验证原生 Vulkan NR；只有这条路径出现明确阻塞时，才评估自有 Vulkan 与 D3D12 桥接。

## 范围与实现基线

- 首期限定 Windows x64、NVIDIA GPU、SDR、Vulkan、单个主交换链，以现有 Ryujinx 测试构建和游戏作为首个验证组合。
- NR、SR（超分辨率或 DLAA）和 FG（插帧）分别管理请求状态与实际运行状态。NR 首版只提供单次处理、开关和必要的强度控制。
- 图形代码运行在模拟器进程内的自有 Vulkan 层中；工具进程负责安装、校验、启动和控制。仅在 Tauri 进程加载 NR DLL 无法访问模拟器的 Vulkan 资源。
- 实现以当前工作区已有的源纹理识别、NVOF、SR、FG 改动为基线。开始实施时记录实际 Git 状态和构建摘要，保留已有未提交改动；部分旧 README 的能力描述已落后于实现。
- 后端与诊断先行，主要改动位于 `src-tauri`。完整前端入口在后端验收后单独实施，沿用已有图形增强和实时控制交互。
- OpenGL、Linux/macOS、HDR、跨 GPU、多次 NR、游戏原生深度与运动矢量识别不进入首版验收范围。

本计划是历史 [ReShade 接入计划](reshade-dlss-integration-plan.md) 的独立后续路线。ReShade/Feeder/RenoDX 旧路线已于 2026-10-01 从工具中移除；已有组件的所有权与文件不变，不自动卸载用户现有组件。

## 已核实的接入方式

工具此前安装的 ReShade 路线为（现已移除）：

```text
ReShade 提供画面与深度
  -> Lumenite Kernel 和 DLSS5 Feed shader 提供估算运动等输入
  -> Feeder 经 Vulkan 与 D3D12 共享资源构造 DLAA 调用
  -> RenoDX addon 拦截 NGX 调用并执行 feature 18 NR
  -> 处理结果返回模拟器
```

Feeder 提供输入和传输，NR 模型实际位于 `nvngx_dlssnr.dll`。ReShade 的捕获、shader、配置和生命周期工作可以由自己的图形层替代。

| 已核实事实 | 证据与适用边界 |
| --- | --- |
| 工具已有 Rust Vulkan 层、NVOF、SR 和 FG | 当前 `streamline-fg` 源码；这些模块可以复用，但没有直接 NR 阶段 |
| 本地两份 NR 310.8.0 DLL 均导出 Vulkan NR 入口 | 只解析 PE 导出表，没有加载或执行 DLL；导出存在不证明初始化成功 |
| 有独立 Rust Vulkan NR 实现 | `bevy_dlss5` 直接调用 feature 18；上游报告 Windows Vulkan / RTX 4090 验证，不能外推为当前模拟器已兼容 |
| AIO 可以直接调用 NR snippet | `nr-standalone.cpp` 和 `nvngx-bridge.cpp` 使用 D3D12 NR 入口；其捕获与界面仍依赖 ReShade |
| 当前 NR 契约需要固定版本 | 调研时 NVIDIA 公开头文件仍将 18 定义为 `Reserved18`；社区使用 `DLSSNR.*` 参数，不应视作稳定公开 API |
| 现有 Feeder 测试不能作为直接 NR 的验收结果 | [本地实测记录](../../src-tauri/src/services/graphics_components/feeder/README.md)记录成功 NR evaluate、深度为零，以及尺寸重建时的异常和退出 |

本地导出表样本如下。同一版本号对应不同摘要，实施时必须按具体文件校验。

| 样本 | 文件版本 | SHA256 |
| --- | --- | --- |
| `src-tauri/target/release/graphics-components/cache/aio-v1/runtimes/nvngx_dlssnr.dll` | `310,8,0,0` | `e16bcf15e16e13f527491cdf7845b2fe6521a738d8f7c9c721866a8496e1fc8e` |
| `D:/ryubing-dlss5-win-x64/nvngx_dlssnr.dll` | `310,8,0,0` | `4b8d19bc3eff58a084f5eca7489c921501c203450169fb82ff4f649a4482ba05` |

两份样本均包含 `NVSDK_NGX_VULKAN_Init_Ext2`、`CreateFeature1`、`EvaluateFeature`、`PopulateParameters_Impl`、`ReleaseFeature` 和 `Shutdown1`。摘要仅标识本次检查样本，不代表来源认证、再分发许可或兼容性认证。

## 路线选择与帧处理顺序

| 路线 | 实现方式 | 启动条件 |
| --- | --- | --- |
| 原生 Vulkan NR | 自有 Vulkan 层直接绑定 NR 输入和输出，调用 Vulkan snippet | 首选；先通过独立 GPU 试验确认 ABI、初始化和连续输出 |
| 自有 Vulkan 与 D3D12 桥接 | 按 Vulkan GPU 的 LUID 创建 D3D12 设备，使用共享纹理和 fence 调用 D3D12 NR | 原生路径被明确证据阻塞，且最小桥接试验成功后再选择 |

拟验证的默认顺序为：

```text
取得源画面与呈现映射
  -> 在未处理的相邻真实帧上估算 NVOF 运动
  -> NR 单次处理，保持输入分辨率
  -> 可选 SR 或 DLAA
  -> 输出映射到交换链
  -> FG 标签与代理 Present
```

这一顺序是拟选设计，需要验证画质与资源契约。SR 或 FG 关闭时 NR 仍应工作；现有启动器和帧处理逻辑对 FG 呈现集成的依赖需明确拆分，不能为了启用 NR 强制开启实际插帧。NVOF 不分析 NR 输出或生成帧，以免神经处理变化污染运动估计。

## 现有代码接入点

| 文件或模块 | 复用或修改内容 |
| --- | --- |
| `src-tauri/crates/streamline-fg/src/lib.rs` | Vulkan 层入口和分派，接入 NR 模块 |
| `src-tauri/crates/streamline-fg/src/target_runtime.rs` | 汇总 NR 所需扩展和特性，管理设备初始化与销毁顺序 |
| `src-tauri/crates/streamline-fg/src/source_auto.rs`、`source_model.rs`、`scale_copy.rs` | 复用源画面选择、crop、翻转与呈现映射；不把选出的颜色纹理称为游戏原生 TAA 输入 |
| `src-tauri/crates/streamline-fg/src/target_nvof.rs` | 复用硬件光流；当前分析交换链画面，需适配 NR 源画面或严格转换坐标 |
| `src-tauri/crates/streamline-fg/src/target_sr.rs` | 复用资源创建、颜色拷贝、尺寸处理和计时方法，使 SR 能消费 NR 输出 |
| `src-tauri/crates/streamline-fg/src/target_fg.rs` | 编排 NVOF、NR、SR、FG 与 Present 的同步关系 |
| `src-tauri/crates/streamline-fg/src/live.rs`、`launcher.rs`、`session_verify.rs` | 独立 NR 请求、状态、启动配置与运行证据 |
| `src-tauri/src/services/graphics_components/streamline_install.rs`、`streamline-package.json` | 增加可选 NR 组件校验、部署记录和恢复 |
| `src-tauri/src/commands/graphics_components.rs`、`config.rs` | 提供后端设置与诊断接口，保持旧配置迁移兼容 |

拟新增 `nr_api.rs`（NGX 与 NR ABI）、`target_nr.rs`（帧资源与评估）、`diagnostics/sdk_nr.rs`（独立 GPU 试验）。若必须增加调用方 DLL，则在 `src-tauri/crates` 下建立最小 Rust `cdylib`，只封装已核实的调用边界。以上名称为建议，实施时按现有模块职责调整。

## P0 核实 ABI 与独立 GPU 执行

先在工具拥有的 Vulkan 设备、纹理和命令缓冲上验证 NR，不立即接入真实模拟器。

1. 固定 SDK 头文件、NR DLL、调用桥和驱动版本，记录摘要、架构、来源、导出以及 GPU 标识。首轮采用与本机 GPU 匹配的运行库。
2. 核对 NGX 参数对象获取和写入方式、`Resource_VK` 布局、调用约定、成功码、feature 创建和释放函数，以及 Vulkan 分派指针来源。NGX feature 18 不可直接当成 Streamline feature ID 加入其列表。
3. 查询并验证设备能力。上游使用 `VK_NVX_binary_import`、`VK_NVX_image_view_handle`、`VK_KHR_push_descriptor`、`VK_KHR_maintenance4` 和 buffer device address；需核实当前 DLL 的实际要求与核心版本替代关系，不能照抄后无条件启用。
4. 验证 NGX core 与 NR snippet 初始化。上游对 `nvngx.dll` 的限制有不同描述：AIO 保留调用地址位于该 DLL 的边界，Bevy 路线要求进程映像名。需要分别记录直接调用和最小调用桥的结果，不能提前认定 Vulkan 分支可复用 D3D12 桥。
5. 如使用调用桥，检查优化后的真实调用边界和返回地址，避免尾调用使校验失效。首版不以重命名模拟器 EXE 或修改 NVIDIA DLL 二进制为前提。
6. 用固定尺寸的静态及移动测试图完成创建、连续评估和读回，明确颜色、深度、运动单位及 history reset。使用 NR 关闭、强度为零和非零强度对照，排除仅原样拷贝或陈旧输出。
7. 在同一设备上增加现有 Streamline 消费者，核对 NGX 会话所有权、重复初始化、参数隔离和 shutdown 顺序。NR 停止不能销毁仍在使用的 SR 或 FG 全局状态。
8. 独立 GPU 试验启用 `VK_LAYER_KHRONOS_validation` 的核心及同步验证，覆盖创建、连续评估、读回、重复释放和 Streamline 共存。保存验证层版本、启用配置与完整消息，检查图像布局、访问依赖、队列同步和资源生命周期。验证层不可用或验证运行失败时标记为未完成，不以无验证层的连续输出替代。

P0 通过条件：固定组合下初始化成功，至少连续 300 帧有效输出，运动序列没有停帧；NR 输出变化可归因于 NR；重复创建与释放无资源生命周期错误；与 Streamline 的初始化和退出顺序得到验证；上述核心及同步验证无错误，警告逐项核查并保留结论，未解释的同步或生命周期警告阻止通过。

如果原生初始化、必要设备能力或会话共存出现阻塞，保存精确错误、失败阶段和最小复现。仅对已定位原因评估 D3D12 路线；若两条路径都受运行库限制，保留实验状态和证据，不进入安装或 GUI 开放阶段。

## P1 接入模拟器中的独立 NR

### 输入契约

- 颜色使用已识别的源画面，保留 crop、flip、有效区域与格式信息；无法确认源图时，明确报告使用最终呈现画面的降级路径。
- NR 输入需匹配运行库期望的 gamma 编码和显示域颜色。根据实际 UNORM/SRGB 视图处理转换，复用现有 SR 对编码字节的保护，避免双重编码或解码。
- 首版可用常量深度证明执行链路，但状态和证据明确标记为合成深度。其通过不代表真实几何引导或画质兼容性通过。
- 现有 NVOF 输出是当前帧到上一帧的 UV 位移。NR 的 `MVecScale`、方向、尺寸和原点必须独立核对；源图与交换链存在裁剪、黑边、翻转或尺寸差异时，转换运动或重新计算源图光流。
- 尺寸、源图身份、裁剪、翻转、暂停恢复或不连续帧变化时重置历史。NVOF 不可用时使用明确的零运动降级，不能沿用旧帧运动，也不能将缺失运动视为已确认的静止画面。
- 零运动降级必须同时固定 NR 的时间历史策略。P0/P1 对照验证零运动下逐帧 reset 的输出和稳定性；现有 SR 在缺少运动输入时逐帧 reset，仅作为参照，不推定 NR 契约相同。首版只有在该策略通过验证后才继续零运动 NR；否则暂停 NR 并报告原因，回退到已验证路径。有效运动转为零运动时清除不适用的历史；恢复 NVOF 时重新建立光流帧对，并在第一帧恢复使用有效运动的 NR 评估中 reset。记录运动有效性、实际降级策略、reset 原因和生效帧。

### 资源与失败处理

为每个设备和交换链维护 NR 参数对象、feature handle、输入输出资源及历史状态，避免与 SR 共享可变参数。固定输入尺寸先完成连续处理，再支持尺寸稳定后的重建。

梳理原始应用等待、NVOF 完成、NR 完成、SR 完成和 Present 的依赖。二进制 semaphore 只消费一次；资源在 GPU 完成使用后才能复用、释放或重建。首版可采用有界 fence 等待，先保证正确性，再按性能证据优化并行。

初始化失败或尚未提交 GPU 工作的评估失败应关闭 NR 并保留普通呈现或已验证的 SR/FG。提交后的未知状态、设备丢失或不确定同步按现有错误策略处理，不能在未证明安全时继续读取输出。超时必须有明确状态和诊断，避免渲染线程无限等待。

P1 验收时关闭实际 SR 和 FG，确认 NR 独立运行、连续运动有效、开关后恢复原画面；再验证 resize、最小化、失焦、暂停与正常退出。静止与移动序列都需覆盖“有效运动 → 零运动 → 恢复有效运动”，核对实际降级策略、历史重置和连续输出，检查陈旧内容累积与残影；没有通过零运动策略验证时，应实际暂停 NR。

## P2 验证 NR 与 SR 及 FG 共存

分别测试全部关闭、NR、SR、FG、NR 加 SR、NR 加 FG、SR 加 FG、三者全部开启。请求状态、实际活动状态和暂停原因分别记录；后台导致 FG 暂停的样本不能用作组合性能结论。

确认 SR 消费本帧 NR 输出，FG 消费本帧最终处理后的颜色，运动始终来自相应的未处理真实帧。处理 NR 与 SR 尺寸不同、黑边映射、翻转及源纹理变化时的 reset 和资源重建。

NR 开关、强度变化及可安全执行的失败回退统一在帧边界生效，每个真实帧使用同一份已应用配置。即使尺寸和源纹理未变，这些变化也会改变下游颜色契约：首版每次应用不同的强度值都触发 reset，不依赖尚未验证的突变阈值；NR 启用、恢复或强度变化时重置自身历史，并向所有受影响的 SR/FG 消费者传递 reset；关闭或失败回退时，SR/FG 在首次消费回退画面的帧重置。暂停中的消费者保留待重置信号，到下一次实际消费时应用，不能只清除 NR 自身状态。单纯 NR 外观变化不要求重建仍然连续有效的原始画面 NVOF 历史。状态与证据记录切换原因、生效帧及各消费者的 reset 应用结果。

组合切换测试分别保持 SR、FG、SR 加 FG 持续开启，再切换 NR、改变强度并注入可安全回退的 NR 失败。检查切换帧及后续帧的历史重置、画面连续性与残影；FG 因后台等原因暂停的样本不计作持续活动消费者的验收，并另测其恢复后的待重置信号。

性能记录包含应用帧率、实际呈现帧率、NR GPU 时间、颜色准备与拷贝、NVOF、SR、CPU 记录和 fence 等待。CPU 等待与 GPU 工作可能重叠，统计时不相加。现有 [SR GPU 测量](../../src-tauri/crates/streamline-fg/docs/SR-gpu-profile-20260926.md)可复用测量方法，其数值不能作为 NR 性能。

正确性验证与性能采样分别运行：P1/P2 在核心及同步验证开启时覆盖组合切换、降级恢复和 resize；性能采样关闭验证层，保持相同构建、运行库、场景和功能配置，并记录验证开关，不能混用两类样本得出性能结论。

采用固定场景、相同源分辨率和档位对照，包含静止、平移、人物运动、文字与细密几何。去除 ReShade 是减少依赖和简化编排，不承诺提升帧率；NR 增加的 GPU 工作需单独量化。

## P3 工具管理与实时控制

- NR 组件清单独立于 SR/FG，固定运行库和调用桥摘要、GPU 架构、依赖及经过验证的组合。NR-only 所需文件以 P0 结果为准，不因为 Feeder 曾需要 SR DLL 就继续强制安装。
- 优先支持选择本地 NR 运行库；自动下载和打包前核实可用来源与分发条件，技术可调用不等于具备分发许可。
- 复用现有下载、校验、事务、所有权、修复和卸载逻辑。未知或修改过的组件不静默覆盖；模拟器运行时不替换已加载文件。
- 启动器只对目标子进程启用自有层，记录实际 layer 链，并排除本次会话中的 ReShade 与竞争 NR 消费者。保留用户全局注册与其他安装。
- 配置默认 NR 关闭；旧配置缺少 NR 字段时保持原行为。NR 能单独启停，不与 SR 档位、FG 倍率混为一个开关。
- 实时状态至少包含请求值、实际活动、运行库摘要、输入输出尺寸、源图类型、运动来源、深度类型、初始化或评估错误、历史重置及性能信息。
- 首版通过诊断入口和 Tauri 后端接口完成验证，再将独立 NR 开关及必要的强度设置接入现有前端。

## D3D12 备选路线

若 P0 明确原生 Vulkan NR 不可用，先完成独立共享资源试验，再决定是否移植 AIO 的直接 NR 调用契约。

按 Vulkan GPU 的 LUID 选择同一 D3D12 适配器；由 D3D12 创建共享颜色、引导、输出纹理及 fence，导入到 Vulkan，验证双向颜色拷贝和 timeline 同步。显卡匹配失败、外部句柄或格式不支持时明确失败。

此路线依然由自有层负责捕获和呈现，不恢复 AIO 的 ReShade 事件、独立呈现窗口或整套安装器。必须验证共享资源生命周期、超时、resize、颜色空间以及额外等待成本。原生路线已经通过时不同时维护两套首版运行时。

## 验证与最终验收

自动化检查聚焦实际契约：ABI 布局、运动坐标与单位转换、源映射和 history reset、NR 状态与强度变化的下游 reset 传播、暂停消费者的待重置信号、运动降级与恢复状态、尺寸重建状态、运行库摘要、旧配置迁移及文件事务恢复。真实 NR 创建、评估、画面和性能需要 GPU 测试，编译或 mock 不能代替。

Rust 代码修改后执行 `cargo fmt`、宿主 `cargo check` 和 `cargo check --target x86_64-pc-windows-msvc`，应用与独立 `streamline-fg` crate 分别检查其实际构建特性，解决全部错误和警告。若后续修改前端，再执行现有类型检查与构建。本文本身的落盘不触发 Rust 编译要求。

验收证据保存在 `src-tauri/crates/streamline-fg/evidence` 或对应文档中，记录 EXE、游戏版本、场景、GPU、驱动、SDK、层及运行库摘要、实际功能状态和原始会话位置。大体积日志与临时上游源码保留在忽略的 `target` 目录。

- [ ] 独立 Vulkan NR 在固定 GPU 组合通过连续帧、输出对照和重复释放试验。
- [ ] P0 及 P1/P2 的组合切换、降级恢复和 resize 通过 Vulkan 核心及同步验证；无错误，警告有逐项核查结论，保存验证层版本、配置及原始消息。性能采样独立运行并关闭验证层。
- [ ] 真实模拟器连续运行至少 10 分钟，运动画面持续更新，无黑屏、死锁或未报告的降级。
- [ ] ReShade 和 Feeder 未加载，RenoDX addon 未加载，NR 仍实际评估并产生输出。
- [ ] NR 能在实际 SR 和 FG 均关闭时运行；八种开关组合的活动状态与呈现行为正确。
- [ ] 至少完成 10 次 NR 开关切换和 10 次窗口尺寸切换，历史重置与资源重建正确。
- [ ] 分别在 SR、FG、SR 加 FG 持续活动时切换 NR、改变强度及触发安全失败回退，受影响消费者在正确帧 reset；暂停消费者恢复后应用待重置信号，画面无旧历史污染。
- [ ] 静止与移动序列通过“有效运动 → 零运动 → 恢复有效运动”测试，NR 使用经验证的零运动历史策略或明确暂停；运动恢复时重建帧对并 reset，实际状态和证据一致。
- [ ] 最小化、失焦、暂停恢复、正常退出和下一次启动没有残留状态或资源使用错误。
- [ ] 缺失、不匹配或校验失败的 DLL 有明确诊断，旧 SR/FG 和普通启动路径通过回归。
- [ ] 明确记录真实或合成深度、光流质量、文字和 UI 变化；功能执行与画质兼容性分别验收。
- [ ] 对照测试报告 NR 独立成本与组合成本，不使用请求开关或叠加帧率计数代替实际运行证据。
- [ ] 安装、更新、修复与卸载保留用户已有文件、配置及全局 Vulkan 注册。

## 调研来源

- [DLSS5 Feeder 的 Vulkan 路线](https://github.com/jlrouzies-fr/DLSS5-Feeder#the-vulkan-path)。本地安装基线为 `v1.16.0-beta.6`，不能将最新 README 的全部能力视作该版本能力。
- [AIO 直接 NR 实现](https://github.com/kibblerz/DLSS5-Reshade-AIO/blob/09301f5528e619e8b9ec17c257d167e2985f53b0/addon/src/nr-standalone.cpp)。
- [AIO 的 NGX 调用桥](https://github.com/kibblerz/DLSS5-Reshade-AIO/blob/09301f5528e619e8b9ec17c257d167e2985f53b0/addon/src/nvngx-bridge.cpp)。
- [Rust Vulkan NR ABI 与调用](https://github.com/AlrikOlson/bevy_dlss5/blob/ed5ea6264338b2e19d81dcecbed1e06ab22a01ed/src/ngx.rs)。
- [Rust NR 颜色转换](https://github.com/AlrikOlson/bevy_dlss5/blob/ed5ea6264338b2e19d81dcecbed1e06ab22a01ed/src/bridge.rs)。
- [NVIDIA 公开 NGX 定义](https://github.com/NVIDIA/DLSS/blob/main/include/nvsdk_ngx_defs.h)，调研日期为 2026-09-30；实施时重新固定 SDK 提交。
- [现有 NVOF 契约](../../src-tauri/crates/streamline-fg/docs/NVOF.md)、[参考 SR 实现核查](../../src-tauri/tools/streamline-sdk-audit/SR-review.md)。

上游实现的许可和 NVIDIA SDK、模型 DLL 的条件分别核实。参考调用设计时保留来源；直接复用源码时记录适用许可与必要声明。

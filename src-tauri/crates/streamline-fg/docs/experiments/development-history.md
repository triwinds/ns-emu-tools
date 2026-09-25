> 历史实验记录：当前目录、构建命令和运行状态以 [运行组件说明](../../README.md) 为准；部分原始日志已清理。

# Streamline / Vulkan 调用链诊断

P0 的可运行实验，不是可供 Ryubing 使用的插帧组件。包含独立 Rust `cdylib` 诊断层和 Windows x64 测试宿主；不加入 Tauri 默认构建。

## 构建与运行

在仓库根目录执行：

```powershell
cargo fmt --manifest-path src-tauri/crates/streamline-fg/Cargo.toml
cargo check --locked --manifest-path src-tauri/crates/streamline-fg/Cargo.toml
cargo check --locked --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --target x86_64-pc-windows-msvc
cargo test --locked --manifest-path src-tauri/crates/streamline-fg/Cargo.toml
cargo build --locked --manifest-path src-tauri/crates/streamline-fg/Cargo.toml

& D:\py\ns-emu-tools\src-tauri\crates\streamline-fg\target\debug\streamline-layer-probe.exe `
  --layer D:\py\ns-emu-tools\src-tauri\crates\streamline-fg\target\debug\streamline_probe_layer.dll `
  --interposer D:\ryubing-dlss5-win-x64\dlss\sl.interposer.dll `
  --session D:\py\ns-emu-tools\src-tauri\target\streamline-probe-new
```

输入使用绝对路径，session 必须尚不存在。需 NVIDIA Vulkan graphics/present 队列及支持 SDR/TRANSFER_DST 的 Win32 surface。原始参考目录只读；将通过 P0 SHA-256 校验的 interposer 复制到独立会话目录后加载。Loader 也需匹配已冻结哈希，变更时重新审核，不自动放宽。

宿主创建隐藏测试窗口。基线子进程和诊断层子进程各完成两轮 instance/device/surface/swapchain 创建、三帧清屏呈现和销毁。实际呈现成功不是有效显示帧或视觉效果验收；隐藏窗口不用于显示质量测量。

父进程生成会话专用显式 layer manifest，只有测试宿主在 `VkInstanceCreateInfo` 里启用该层。层内校验预期可执行文件路径。环境变量仅传给子进程，不写注册表、不改模拟器、不替换系统 Vulkan DLL。不是生产级目标身份/防篡改机制。

本机有多个 ReShade 隐式注册，因此实验明确在两个子进程的 `VK_LOADER_LAYERS_DISABLE` 中追加 `VK_LAYER_reshade`；保留已有禁用项、记录原环境和实际层链。其他驱动隐式层仍存在，不声称隔离了所有层。遇到继承的强制启用/allow 配置会拒绝启动。父进程设 40 秒超时，仅结束自己创建的测试子进程。

## 默认模式实验边界

- 诊断层只透明转发和记录，绝不调用 Streamline。
- 宿主加载固定 interposer，仅调用其 GIPA/GDPA 及已核对为透传的函数；不调用 `slInit`、`slSetVulkanInfo` 或需要初始化的 SDK proxy。
- SDK 的 GetDeviceQueue 透传分别在主线程和宿主创建的工作线程调用。该工作线程不是 SDK 自己的异步 FG 线程。
- 对比 system、SDK、next-layer 地址及其 DLL 归属，记录 surface 的直接 SDK 导出与 GIPA 取址差异。
- 单独调用保存的 next GetDeviceQueue，确认不会再次进入本层。线程 phase=1 原生，2 SDK 透传，3 直接下一层，4 工作线程 SDK 透传；phase=0 为启动。
- 覆盖 GetDeviceQueue2、AcquireNextImage2KHR 原生路径，保留原始 Vulkan 参数/返回值。没有修改 pNext 或 PresentInfo；只按 Loader 协议临时推进 Loader 自己的 link 信息，调用完成后还原。
- 分发表按 dispatch key 保存，锁在调用下层前释放。每轮销毁后检查映射数量为零。

输出 `inputs.json`、`baseline.json`、`layered.json`、`layer.jsonl`、`result.json` 及 Loader stderr/stdout 日志。自动断言检查完整两轮、六次层内原生呈现、线程差异、next 调用没有重入、地址关系、基线一致性和清理结果。失败退出码 1；实验完成退出码 0 **只表示此诊断实验通过**，结果中的 `p0_passed` 始终为 false。

没有安装或启用 Khronos validation layer，不声称 Vulkan 验证错误为零；`validation_layer_enabled=false` 明确记录未启用验证层。当前设备/线程/入口覆盖只适用于这个最小宿主，不能当作原版 Ryubing 的 P1 验收。

本次结果及下一步见 [EXPERIMENT.md](EXPERIMENT.md)。

## ABI 来源

Loader ABI 按 [Vulkan-Headers v1.4.341 vk_layer.h](https://github.com/KhronosGroup/Vulkan-Headers/blob/v1.4.341/include/vulkan/vk_layer.h) 定义，协商接口 v2；应用 Vulkan ABI 使用固定 Cargo.lock 中的 ash。x64 测试核对 Loader 结构大小、关键偏移和 link 查找。调用链与显式层发现参考 [Vulkan-Loader v1.4.341](https://github.com/KhronosGroup/Vulkan-Loader/blob/v1.4.341/docs/LoaderLayerInterface.md)。
## 初始化 SDK 的诊断模式

可选 sdk-bridge feature 在宿主中静态链接一个最小 C++ C ABI shim。默认诊断层只转发和记录；显式目标 SDK 实验会加载 SDK。shim 按固定头文件构造 Preferences、FeatureRequirements、VulkanInfo、AdapterInfo；Rust 只传递标量、函数地址及自己拥有的定长字符串副本，不重建 SDK C++ 继承布局。

需要 MSVC C++ 工具链。构建脚本默认使用 src-tauri/target/streamline-sdk-v2.12.0/include 和 src-tauri/target/vulkan-headers-v1.4.341/include；可用 STREAMLINE_SDK_DIR（SDK 根目录）、VULKAN_HEADERS_DIR（include 目录）覆盖。固定来源：

- Streamline v2.12.0，commit e8aaa6eaac968711fb62473d4ae8256dde20919b。
- Vulkan-Headers v1.4.341，commit b5c8f996196ba4aa6d8f97e52b5d3b6e70f7e4e2。

缺少时，在仓库根目录克隆到以上 target 目录；已有目录不要覆盖：

    git clone --depth 1 --branch v2.12.0 https://github.com/NVIDIA-RTX/Streamline.git src-tauri/target/streamline-sdk-v2.12.0
    git clone --depth 1 --branch v1.4.341 https://github.com/KhronosGroup/Vulkan-Headers.git src-tauri/target/vulkan-headers-v1.4.341
    cargo build --locked --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --features sdk-bridge

构建先按清单校验 LF 规范化 SHA-256，再复制头文件到 OUT_DIR 编译；哈希不匹配直接失败，不从未核验的原始 include 目录编译。

在上方运行命令的 --layer 前添加以下一个参数：

- --sdk-requirements：隔离子进程加载七个冻结 DLL，调用 slInit，查询 DLSS-G/Reflex/PCL 需求，随后 slShutdown；不创建 Vulkan 对象。
- --sdk-device：同样先查询需求，再创建 Vulkan 1.3 实例/设备，核验扩展、特性、额外队列和 SDK adapter support，调用 slSetVulkanInfo。仅执行初始化后的 SDK DeviceWaitIdle 代理，记录主线程及宿主工作线程重入，先 slShutdown 后销毁设备/实例；不创建交换链或启用 FG。

SDK 模式也必须指定新的 session 和 layer 绝对路径。七个 DLL 从参考目录校验、复制、再次校验后使用。固定项目 ID 是诊断宿主自己的标识，不借用参考分支的应用身份。OTA 下载及加载开关关闭，但这不代表拦截了 SDK/驱动自身的全部网络行为。

--sdk-device 使用 family 内互不重叠的宿主、SDK graphics、SDK compute 队列；根据固定 manual-hooking 文档选择 optical-flow interop，useNativeOpticalFlowMode=false。未知特性名或不满足的需求直接失败。phase 5=创建，6=SetVulkanInfo，7=主线程 SDK idle，8=宿主工作线程 SDK idle，9=关闭。

输出 sdk-init、sdk-requirements、sdk-adapter、sdk-device-plan、sdk-set-vulkan、sdk-proxy-calls、sdk-shutdown、sdk-device-result 等 JSON，以及完整 SDK/Loader 日志。requirements 文件记录创建实例前的阶段状态。退出 0 仅表示该模式实验通过；仍不代表 P0 通过。运行时 SDK 警告单独保留，不能据此宣称完整功能可用。

验证命令与前文相同，增加 --all-features 检查并测试 C ABI 诊断模式。最新结果见 [SDK-EXPERIMENT.md](SDK-EXPERIMENT.md)。
## 初始化取址路由实验

新增 --sdk-idle-capture 模式，需要 sdk-bridge feature；参数与 --sdk-device 相同。它只测试 slSetVulkanInfo 期间临时向 SDK 提供 next idle 地址的候选方案。最终实测该方案没有避开本层：直接层 GDPA 对照有效，系统 GDPA 仍返回本层，SDK 初始化期间未进入选择分支。详见 [ROUTING-EXPERIMENT.md](ROUTING-EXPERIMENT.md)。该模式完成不代表路由方案通过。

源码适配审查与只读 Rust 审计入口见 [SOURCE-ROUTING-REVIEW.md](SOURCE-ROUTING-REVIEW.md)：显式 next resolver 需要覆盖 interposer、计算层和两条 NGX 初始化路径。后续构建与运行实验见下文，P0 仍未通过。

后续已完成首版源码补丁、隔离 SDK 构建和回调契约测试，见 [SDK 路由构建实验](sdk-route/README.md)。新 DLL 的运行实验见下文，FG 关闭、P0 未通过。

新增实验模式 --sdk-route：独立清单校验补丁运行时，显式注册下游回调，修正 physicalDevice 的 Loader 层级，并验证实际 SDK idle 无重入。使用方法、失败分析和边界见 [RUNTIME-EXPERIMENT.md](sdk-route/RUNTIME-EXPERIMENT.md)。

--sdk-route 现包含 Loader 队列／命令缓冲初始化和宿主通过 SDK 接口完成两轮空命令提交与释放的实验。只使用宿主队列 0；host_command_lifecycle_verified 不代表 SDK 自有线程或完整 FG 路由通过。详见上述运行实验文档。


--sdk-route 进一步覆盖 FG 关闭时的 SDK surface/交换链创建、两种 acquire、呈现、oldSwapchain 重建和销毁，并与应用路径对照。该模式会短暂显示诊断窗口；SDK 日志出现 error 时拒绝验收通过。两次通过的真实运行与首次隐藏窗口失败边界见 [交换链证据](evidence/sdk-route-swapchain-2026-09-25/README.md)。实际 resize 的后续验证见下文；SDK 后台线程和插帧输出尚未验证。


--sdk-route 现进一步验证实际窗口扩大后的重建，以及销毁交换链后在同一设备/surface 上恢复呈现。[两次实机证据](evidence/sdk-route-resize-2026-09-25/README.md)包含尺寸、时序与路由记录。这不代表 SDK 完整重初始化通过；最小化/恢复和 FG 仍待验证。


## 同进程生命周期批量验证

新增 --sdk-route-repeat：同一子进程尝试 20 轮完整 SDK 初始化、Vulkan 生命周期及关闭，逐轮保存证据并在首个失败处停止。--sdk-route 仍为单轮，现包含最小化暂停与恢复。

v2 的 NGX 日志回调悬空问题已通过 v3 稳定回调修复；两批各 20 轮完整 SDK 生命周期通过，共 40 轮、720 次呈现，common 确实卸载重载并发生地址变化。见 [修复与完整批量证据](evidence/sdk-route-v3-2026-09-25/README.md)。旧 [失败证据](evidence/sdk-route-repeat-2026-09-25/README.md) 保留。FG 继续关闭；本测试不替代原版游戏、SDK 自有线程和真实显示验收。


## 交换链失效恢复与呈现资源退休

诊断宿主现处理 OUT_OF_DATE / SUBOPTIMAL 重建，并分别注入 acquire 前、present 后失效验证恢复。另修复仅凭 queue idle 释放呈现资源的缺口：基线使用呈现 fence，SDK 路径通过显式下游适配器等待原生呈现 fence，并要求同线程同步完成。该适配器只适用于当前 FG 关闭、单交换链诊断。两批各 20 轮全部通过，共 160 次注入恢复、800 次呈现；见 [实现、失败修复及批量证据](evidence/sdk-recovery-2026-09-25/README.md)。这不代表驱动真实 OUT_OF_DATE、零尺寸暂停、SDK 异步 FG 或游戏验收通过。


## 2× FG 激活实验

`--sdk-fg` 在独立诊断窗口启用 FG；窗口需点击进入前台。两轮各 600 应用帧观察到 SDK 异步工作线程的额外原生呈现、输入 timeline 推进，以及 Off 后关闭归零。见 [FG 激活证据与边界](evidence/sdk-fg-2026-09-25/README.md)。这是诊断宿主接通，尚不是 Ryujinx 游戏内 FG 或显示效果验收。

`--target-probe --layer <绝对 DLL 路径> --session <新目录> [--game <游戏绝对路径>]` 为用户指定的 `D:/Ryujinx_test/Ryujinx.exe` 加载透明诊断层，按 `target-profile.json` 核验 EXE 哈希，仅对该子进程禁用 ReShade。该入口不启用 SDK/FG；游戏结束后正常关闭程序，启动器记录退出码。


## 原版 Ryujinx 游戏内 FG 诊断

用户指定 Canary 1.3.351、`D:/Ryujinx_test/Ryujinx.exe`，并选择《王国之泪》。`--target-probe --sdk-off --runtime <绝对运行时目录>` 验证游戏 SDK 代理路径；改为 `--fg` 启用有界 2× 实验，其余 `--layer`、`--session`、`--game` 参数同上。运行时必须是冻结的 route-v3 七个 DLL。

FG 模式要求固定窗口在前台且创建至少 10 秒，最多请求 600 个 FG 应用帧；失焦或窗口变更请求后停用，本次交换链不自动重启 FG。采用常量深度、零矢量和合成相机参数，只提供真实可见的 Present 标记。窗口 CBT guard 在 FG 活跃时取消变更，呈现线程随后 Off+flush；被取消的操作需要用户重试。这是固定窗口诊断，不是完整窗口生命周期实现。暂不支持同进程第二次游戏设备生命周期、AcquireNextImage2 代理或未知设备特性链。

首轮游戏内接通和完整证据见 [游戏 FG 记录](evidence/ryujinx-fg-2026-09-25/README.md)。启动器正常退出后自动校验 SDK 状态、额外底层呈现、输入完成及清理；也可使用 `--target-probe --verify <已有 session 绝对路径>` 重新判定。退出 0 不代表有效中间画面、显示节奏、延迟或 P4 通过。归档 trace 为 gzip；复查时在独立目录解压为 `layer.jsonl`，保持其他文件相对路径。

### 单变量限帧复验

目标 `--fg` 可额外指定 `--reflex-ab`：同一进程的 600 个 On 帧自动分成 0 / 16667 / 0 μs 限帧各 200 帧，其他代码路径不变。普通运行始终使用 0，不从父进程继承此实验开关。只有始终保持前台才能完成三段，失焦仍按原规则永久停用。日志分别记录 Sleep 所用的上一帧选项和当前新选项。此实验的原生呈现速率不是最终显示帧率；详见游戏证据目录中的 `passed-005-reflex-ab`。


### Target build compatibility

`--target-probe --target <absolute EXE path>` selects another installation. The frozen SHA-256 identifies a verified build, not an exclusive version requirement. A different valid x64 PE EXE requires the explicit `--allow-unverified-target` flag; this flag does not bypass runtime SDK, queue, surface or size checks. Invalid PE, DLL, x86 and ARM64 targets are rejected. Unknown builds are recorded with unknown version metadata and their own hash. Changing the target during preparation aborts launch.

The manager preflight uses the same classification policy. Its trial checkbox resets whenever the target, graphics API, or detection result changes. Installation and launch integration are still pending; selecting the checkbox alone does not launch anything.

## 本地安装闭环（2026-09-25）

普通 `--target-probe --fg` 不再设置 600 个 On 帧上限。`--bounded` 显式启用 600 帧诊断，`--reflex-ab` 自动启用该上限。普通模式失焦暂停、恢复前台时重置帧历史并恢复；窗口操作、SDK 状态等门控仍然保留；无上限不代表窗口生命周期已经通过 P4 验收。

运行 `package-local.ps1` 构建本地实验包并更新工具箱编译内置的 SHA-256 清单，然后重新构建工具箱。SDK 运行库仍按 `sdk-route/runtime-v3.json` 验证。该脚本仅打包本机已有且验证过的组件，不下载或发布 NVIDIA 二进制。源码检出缺少组件时，界面禁用安装。发布构建从工具箱 EXE 同级的 `streamline-fg-package` 读取同一固定清单；自动发行与再分发许可整理仍待完成。

工具箱安装目录为配置目录下 `graphics/streamline-fg/<目标路径摘要>/local-<组件摘要>`。先写临时目录、校验、再重命名提交。安装记录绑定目标路径与哈希。启动再次验证目标、组件及未验证构建的本次授权；`--expected-target-sha256` 防止准备期间目标被替换。每次运行使用独立 session 副本，不全局注册 Vulkan，不改模拟器文件。卸载仅删除校验一致的已登记文件；额外或被修改文件会阻止删除。游戏会话副本和记录保留。

界面“已安装”只表示文件状态，“以 FG 启动”只发出请求；当前界面不宣称实时插帧成功。若窗口操作触发终止停用，关闭模拟器后重新专用启动。不同构建使用独立内容版本目录；旧版本与会话记录保留，不自动清理。运行日志保留在会话路径。

测试：安装/卸载与失败回滚测试、前端 IPC 交互测试。显式 GPU 测试可设置 `FG_SMOKE_TARGET` 和 `FG_SMOKE_GAME`，运行主 crate 的 ignored `local_game_launch` 测试；会实际安装本地组件并启动所选游戏，请勿加入无人值守 CI。

### 参考参数 A/B

`--target-probe --fg --reference-params` 将 cameraFar/FOV/motionVectorsInvalidValue 对齐到参考构建的 10000/1.0/0，并使用 R16G16_SFLOAT 零运动矢量资源（分配、资源标签和 DLSSGOptions 格式同步调整）。显式填写参考构建相同的 orthographicProjection=false、motionVectorsDilated=false、motionVectorsJittered=false、minRelativeLinearDepthObjectSeparation=40。默认不带开关仍保留原参数，启动器会显式清除继承的参数模式，记录 target-inputs.json/reference_parameters。

该模式只对齐上述 FG 常量/引导格式；不会接入参考分支的 DLSS/NR 缩放、原始输入序列识别或不同引导分辨率。倍率仍为2×，Reflex低延迟、无限帧数、失焦暂停恢复保持相同，画质结论需用户同场景比较。

参考参数首轮实际只启用了 44 帧，随后窗口事件的全局 STOP 标志残留到了重建的交换链，导致新链全程关闭，不能用于画质判断。修复后，仅在旧交换链完成销毁、设备空闲、引导资源退休之后清除普通模式的窗口停止标志；新链仍需独立预热并重置历史。bounded 模式保留终止标志。增加对应回归测试。

### 额外运动估计原型（opt-in）

`--target-probe --fg --reference-params --estimate-motion` 启用独立 GPU 块匹配运动估计，默认关闭，不改变已安装组件。它不是参考分支的完整 VORT 实现，也不提供游戏原生运动矢量或真实深度。

在呈现前消费应用原有等待信号量，将原始呈现图像缩小到最多 180 像素高的颜色历史，执行粗搜索加局部细化，把 current-to-previous UV 位移写入 RG32F 引导纹理。随后用单独信号量交给 SDK 呈现，原信号量不会被重复消费。估计/输入资源复用等待本帧 SDK 输入完成，销毁等待设备空闲。首次启用、失焦恢复及新交换链清除时间历史；高比例匹配失败触发 FG 历史重置。每 8×8 输出像素共享一个位移，低置信度块回退零，仍可能出现块状伪影或误匹配。

`target_motion` 记录非零块数量、拒绝数量、位移幅度和 CPU 端复制/计算/等待总耗时（不是纯 GPU 时间）。Shader 源文件和 SPIR-V 用 `shaders/motion.json` 固定配对，使用 glslangValidator `-V --target-env vulkan1.2` 编译并以 spirv-val 校验。合成 GPU 测试 `gpu_translation_static_and_reset -- --ignored --nocapture` 验证静止、reset 和已知平移（9216/9216 个内部像素方向/幅度匹配），需本机 Vulkan 离散 GPU。

首次游戏启动失败发生在 Vulkan 之前：原版配置 graphics_backend=2 不受 Canary 1.3.351 支持。已备份到 target/ryujinx-fg-motion-001/config-before-backend-repair.json，只修回 Vulkan=0，保留用户当前 2× 分辨率设置。重试会话 ryujinx-fg-motion-002。


### Toolbox live controls and measured rates (2026-09-25)

Toolbox-managed launches now use reference constants and zero motion vectors (the optional motion estimator remains off). Each session has its own control mailbox and one-second telemetry snapshot; the frontend cannot select arbitrary IPC paths. The toolbox remembers the latest session for each target so returning to the graphics page or restarting the toolbox reconnects to it. Previously launched DLLs need a restart to acquire this protocol.

The FG toggle is applied at a presentation boundary, acknowledges a monotonically increasing command revision, resets frame history on resume, and keeps a manual Off across focus changes. Focus gating still applies. The existing window-operation terminal guard is retained: the UI reports that stop and directs the user to restart. SR remains explicitly unavailable.

Telemetry keeps 60 samples. Application FPS counts completed application presentation calls. Native presentation rate counts successful/suboptimal native presents whose retirement fences completed, including the generated frames. Neither the SDK multiplier nor simulator status-bar FPS is substituted for this second counter. These counters do not establish display scanout or visual quality. Sampling uses a monotonic clock; stale renderer data creates gaps, and stale heartbeat or process exit disables control. File I/O runs on a background thread.


### Local release directory deployment

After `package-local.ps1` updates the pinned manifest, rebuild the toolbox, then run `stage-local.ps1 -OutputDirectory <toolbox-output-directory>` (defaults to `src-tauri/target/release`). This verifies every source artifact and the executable's embedded package version/hashes before staging the nine files into `streamline-fg-package` beside `NsEmuTools.exe`. A different existing package is never overwritten. Distribute this directory together with the EXE; copying the EXE alone does not include the experimental SDK package. This is local deployment, not an automated public release or redistribution workflow.

Missing or damaged packages now report the expected directory and validation error in the installation UI instead of only showing a disabled button. After staging files, use “重新检查”; restarting the toolbox is unnecessary.

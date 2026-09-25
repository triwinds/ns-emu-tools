# P0：固定版本接入审查与阻断记录

日期：2026-09-25。对应 [实施计划](../../../../docs/plan/ryubing-streamline-vulkan-layer-implementation-plan.md) 的 P0。

**结论：P0 未通过，不能进入 P2/P3 或工具箱集成。** 本次完成静态调用链补查、证据冻结和可重复运行的 Rust 文件校验工具。没有创建注入 DLL，没有启动参考程序或原版模拟器，没有进行 GPU 实验。

当前 stock SDK 的直接代理方案存在 Loader 重入风险；允许范围内的窗口操作前停用路径也尚未成立。这是当前方案的实施阻断，不是“所有独立层都不可能”的证明，更不能据此直接要求修改模拟器。下面分别记录已知事实、推论和待验证项。

## 1. 冻结材料

| 材料 | 标识与边界 |
| --- | --- |
| 参考分支 | `D:\ryubing-dlss5-win-x64\Ryujinx.exe`，日志构建 `2026.911.47` |
| EXE SHA-256 | `bb518e2ec059d46dfad7df2dcdb3acc608cc8b77bde38e3ac0852ec3ceb103ed`，与既有调研一致 |
| SDK | 官方 tag `v2.12.0`，commit `e8aaa6eaac968711fb62473d4ae8256dde20919b` |
| 本机 Loader | `C:\Windows\System32\vulkan-1.dll`，文件版本 `1.4.341.0` |
| 哈希清单 | [baseline.json](baseline.json)，参考 EXE + 7 个 FG 相关 DLL、24 个 SDK 头文件、8 个 SDK 源码/文档、Loader，共 41 项 |
| 原版模拟器 | 尚未选定；不能用参考分支的身份代替原版身份 |

DLL 清单包含 interposer、common、DLSS-G、Reflex、PCL、`nvngx_dlssg.dll` 和 `NvLowLatencyVk.dll`。未将 SR/NR、ReShade 或其安装器纳入首版依赖。清单不包含运行库文件本身，不授权重新分发；版本号和哈希也不证明这些 DLL 是对应公开源码的原样构建。

本地 SDK 位于 `src-tauri/target/streamline-sdk-v2.12.0`；反编译产物位于 `src-tauri/target/ryubing-inspect`，清理 target 后会丢失。除已有材料外，本次用同一 ILSpyCmd `11.1.0.9782` 定向补取：

```powershell
dotnet src-tauri/target/ryubing-inspect/ilspy/tools/net10.0/any/ilspycmd.dll `
  -t Ryujinx.Graphics.Vulkan.Dlss.Streamline `
  src-tauri/target/ryubing-inspect/Ryujinx.Graphics.Vulkan.dll
```

另外两个类型是 `Ryujinx.Graphics.Vulkan.VulkanRenderer` 和 `Ryujinx.Graphics.Vulkan.VulkanInitialization`。行号对应本次输出，类型/方法名是重新定位依据。反编译输出有未解析类型，不能把其 C# 布局或每个反编译条件表达式视为可靠 FFI 定义。

## 2. 分支初始化、队列与销毁

`VulkanRenderer.cs:695–758` 的已读顺序为：原生创建 instance → surface/物理设备选择 → 原生创建设备 → 获取应用队列/加载特性 → `InitializeFrameGeneration` → `Streamline.Initialize` → `SetVulkanInfo(..., queueFamilyIndex, 2)` → 支持查询 → 安装六个交换链代理。

`Dlss.Streamline.cs:287–325`：FG 模式请求 feature IDs `0,1000,3,4`，对应 DLSS、DLSS-G、Reflex、PCL；偏好值为 `1 | 4 | 128`，即本版本的禁用命令列表状态跟踪、手动 hook 和按帧资源标签。新原型不应顺带启用 SR；FG/Reflex/PCL 及依赖要逐项查询。

| 队列 | 分支观察 | 独立层的要求 |
| --- | --- | --- |
| 应用主队列 | `VulkanRenderer.cs:726`：索引 0 | 保留应用请求与索引 |
| 应用后台队列 | `VulkanRenderer.cs:445–448`：有容量时索引 1 | 保留原创建标志、优先级 |
| SDK graphics | `Dlss.Streamline.cs:495–496`：同族、起点 2 | 按实际功能需求追加，验证容量 |
| SDK compute | `Dlss.Streamline.cs:497–498`：同族、起点 2 | 不假定与 graphics 共用合法；需求合并规则须查明 |
| optical flow | 同样传起点 2，native 标志默认关闭 | 首版候选为 interop；native 路径不开放 |

`VulkanInitialization.cs:542–555`：FG 开关打开时不再把该族队列数限制为 2，创建设备时请求传入的队列数量。`:799–850`：尝试附加实际支持的三个扩展 `VK_NVX_binary_import`、`VK_NVX_image_view_handle`、`VK_EXT_private_data`；private-data 特性进入链；设备创建有去掉 GPL/FG 扩展重试的分支。`Dlss.Streamline` 未声明或调用 `slGetFeatureRequirements`。

这些事实不能证明 SDK 队列需求满足。固定 SDK 要求先 `slInit`，在创建 instance/device 前为每个启用功能查询扩展、特性和额外队列；分支的已读顺序不能作为合规接入模板。当前未动态调用 SDK，所以实际需求数量、容量不足路径和最终分配表仍未获得，不能填造常量。

`VulkanRenderer.cs:1328–1334`：先销毁 surface，再调用 FG/DLSS 的 device-destroy 清理和 `Streamline.Shutdown`，最后销毁 Vulkan device。`Dlss.Streamline.cs:539` 的 shutdown 释放日志/feature 指针并卸载 interposer。独立层还需明确 surface 代理寿命及进程级重初始化约束；本次没有验证同进程第二次游戏启动。

依据：[固定 SDK 手动接入说明](https://github.com/NVIDIA-RTX/Streamline/blob/e8aaa6eaac968711fb62473d4ae8256dde20919b/docs/ProgrammingGuideManualHooking.md)。

## 3. 转发拓扑：直接接代理为何尚不能实现

官方 `source/core/sl.interposer/vulkan/wrapper.cpp:44–95` 的 `processVulkanInterface` 加载系统 `vulkan-1.dll`，取得其 GIPA/GDPA，随后构建 SDK 的 instance/device 表。`layer.h:347` 的 Present 函数从该 GDPA 查询。`slSetVulkanInfo` 调用上述逻辑后初始化插件。

```text
应用 → Vulkan Loader → 本层 Present → SDK vkQueuePresentKHR
                                      ↓
                         SDK 缓存的系统 GDPA 查询结果
                                      ↓
                       可能再次指向本层 Present → …
```

这是源码支持的重入风险推论，尚未用目标 DLL 动态验证。`wrapper.cpp:2178` 在 before hooks 没有接管调用时走 `s_ddt.QueuePresentKHR`；开启 FG 后插件还可能异步呈现，因此单纯在当前线程套一个递归标志不足以证明 SDK 工作线程也能正确区分来源。

| 接口组 | SDK 路径与尚缺的证明 |
| --- | --- |
| instance/device 创建 | SDK 代理再次使用系统 Loader；不能把其当成本层 next-GIPA/next-GDPA 直接调用 |
| swapchain 创建/枚举/acquire/present/destroy、device idle | SDK hook + 缓存分发表；必须证明返回给 SDK 的 next 指针绕过本层而仍保持合法对象/层链 |
| surface 创建/销毁 | `wrapper.cpp:2239/2274` 有 hook 包装，`exports.def:248–249` 有导出；但 GIPA 的 `SL_INTERCEPT` 列表没有这两个名字，需要显式核查导出取址和 HWND 关联 |
| `vkAcquireNextImage2KHR` 等变体 | SDK 已读代理选择列表没有等价覆盖证明，不能透传代理对象后就视作支持 |
| SDK/NGX 内部调用 | common 初始化 NGX 的位置仍传空的 GIPA/GDPA 参数；仅修改公开 interposer 的一张表不足以证明所有路径 |

公开 `VulkanInfo` v3 只有设备、实例、队列等信息，没有注入下一层 GIPA/GDPA 的字段。SDK 内部参数或全局变量不属于稳定 ABI，不能从 Rust 随意写入。

可研究的解除路径是让 SDK 明确接受下一层分发表，或证明特定取址路由同时覆盖 SDK 初始化、异步线程、surface 和 NGX；必须有最小宿主与 Loader 验证证据。修改 SDK interposer 将产生新的二进制与哈希，也超出“薄 C ABI shim”的工作量，不能声称现有 stock DLL 已满足。

源码依据：[wrapper.cpp](https://github.com/NVIDIA-RTX/Streamline/blob/e8aaa6eaac968711fb62473d4ae8256dde20919b/source/core/sl.interposer/vulkan/wrapper.cpp)、[layer.h](https://github.com/NVIDIA-RTX/Streamline/blob/e8aaa6eaac968711fb62473d4ae8256dde20919b/source/core/sl.interposer/vulkan/layer.h)、[commonEntry.cpp](https://github.com/NVIDIA-RTX/Streamline/blob/e8aaa6eaac968711fb62473d4ae8256dde20919b/source/plugins/sl.common/commonEntry.cpp)。

## 4. ABI 边界

| 类型 | 固定头文件版本 | 已核对语义 |
| --- | --- | --- |
| `BaseStructure` | `sl_struct.h` | `next`、GUID、`size_t structVersion`；不是可直接照抄的 C# 结构 |
| `Preferences` | v1 | feature 数组及偏好由 SDK C++ 类型初始化 |
| `FeatureRequirements` | v2 | 实例/设备扩展、特性和额外队列需求 |
| `VulkanInfo` | v3 | 队列起点、族、native OF 标志、创建标志；无函数指针注入 |
| `DLSSGOptions` | v5 | 2× 对应 `numFramesToGenerate=1`；不是设置为 2 |
| `DLSSGState` | v4 | 最小尺寸、生成上限、状态、资源处理完成 fence/value |
| `ReflexOptions` | v1 | SDK 定义的低延迟选项 |

`DLSSGState::numFramesActuallyPresented` 是自上次状态查询以来的呈现计数，不能直接解释成单个应用帧的倍率。查询会影响计数区间，诊断和 UI 不得各自独立消费再拼接倍率。

SDK 要求在 present 线程取得资源处理完成 fence/value；非 presenting 队列复用输入时要等待，使用 `eBlockNoClientQueues` 时所有队列都要等待。实际 Vulkan fence/semaphore 类型及调用适配还需确认，当前不能把 `void*` 直接转换为猜测的 Rust Vulkan 类型。

若继续，最小 C ABI shim 的范围应限于：使用固定头文件构造 SDK 类型、以显式宽度 POD 传递结果、拷贝 SDK 返回的需求数组、暴露初始化/查询/清理调用；用 Windows x64 C++ 编译器验证尺寸、偏移、版本和调用约定。不能让异常跨 C ABI，也不能让 SDK 临时数组逃逸。它不负责另造呈现运行时，且本身不能解决 Loader 重入问题。本次未引入 shim 或未经验证的 Rust FFI，因此 ABI 编译验收仍未完成。

依据：[sl_helpers_vk.h](https://github.com/NVIDIA-RTX/Streamline/blob/e8aaa6eaac968711fb62473d4ae8256dde20919b/include/sl_helpers_vk.h)、[sl_dlss_g.h](https://github.com/NVIDIA-RTX/Streamline/blob/e8aaa6eaac968711fb62473d4ae8256dde20919b/include/sl_dlss_g.h)。

## 5. 窗口、资源与帧阶段

`Window.cs:1118` 的 `SetSize` 只设置 dirty 标志。交换链 teardown 路径在 `Window.cs:205/1143` 调用 `OnSwapchainTeardown`，之后 device-idle；`StreamlineFrameGen.cs:552–641` 发现输入尺寸变化后停用，并等待 30 次稳定计数。以上均不证明已在窗口操作前停用。

固定 SDK 接入清单要求在 resize、最大化/最小化、全屏切换等窗口操作前关闭 FG；options/state API 又非线程安全。仅记录 surface 创建/销毁、收到 out-of-date 或在 Present 检测尺寸，都没有建立前置通知与完成确认。

目前没有覆盖全部允许操作的机制：

- 仅监听 `WM_SIZE` 无法作为前置通知；只处理拖动进入消息也不能覆盖程序化尺寸变化、最大化和模式切换。
- 由窗口回调请求 present 线程关闭并同步等待，可能形成窗口线程与 SDK 异步呈现之间的等待环。
- 直接在窗口回调调用 options，会与 present 线程 SDK 操作交叉；用全局锁串行化仍需证明不会引入等待环。

因此当前不能给出计划要求的窗口停用路径，保持实施阻断。P2/P3 必须验证“前置通知 → 在合法线程提交 eOff → 必要完成确认 → 窗口操作 → generation 退休 → 状态/尺寸重查 → 预热”，并记录线程、时间、generation 与完成信号。

| 资源/信息 | 所有者或可见性 | 本次结论 |
| --- | --- | --- |
| 应用 instance/device/queue | 应用创建，本层只追踪下一层信息 | 不得私自替换或编造队列 |
| SDK 全局状态 | 进程级 SDK | 销毁顺序与重复初始化尚待实验 |
| 代理交换链和图像 | SDK 代理路径 | 创建/获取/呈现/销毁必须同路 |
| 常量深度与零 MV | 将由本层按 generation/frame slot 管理 | 尚未分配；未证明 GPU 完成前不得退休 |
| frame token | 应用呈现帧，不含 SDK 生成帧 | 外部层须防止重入重复记帧 |
| 游戏逻辑阶段/缩放前颜色/HUD-less | 外部 Vulkan 层不能可靠获得 | 不伪造原生数据；输出尺寸契约须独立验证 |

额外发现：`StreamlineFrameGen.OnFrameEvaluated` 成功后连续发 simulation start/end、render-submit start；`BeforePresent` 发 render-submit end/present start；`AfterPresent` 发 present end 并 sleep。它们是分支的实际调用位置，不证明接近真实逻辑阶段，也不能替代 SDK 对外部层近似标记的接受性验证。

依据：[固定 SDK DLSS-G 接入清单](https://github.com/NVIDIA-RTX/Streamline/blob/e8aaa6eaac968711fb62473d4ae8256dde20919b/docs/ProgrammingGuideDLSS_G.md#170-dlss-g-integration-checklist-details)。

## 6. 交付与继续条件

已交付独立 Rust 校验工具、冻结清单和本报告。实测 41 项匹配；哈希变化、缺失文件和非目标宿主会报告失败，匹配也始终报告 P0 阻断。测试覆盖二进制摘要、文本换行、缺失/篡改、完整结果保留、参数拒绝及不将匹配当作 FG 可用。详见 [构建与运行说明](README.md)。

后续工作先解除以下门槛，而不是继续写下载/UI 或宣称已接入：

1. 为固定 SDK 建立可验证的下一层分发表路径，包括异步工作线程和 surface。
2. 给出允许窗口操作的前置通知、SDK 串行化和停用完成机制，证明无等待环。
3. 编译验证 C ABI shim，实际采集逐功能 requirements，形成容量验证及 queue allocation 表。
4. 选定原版 Ryubing 的精确身份，核验其 Loader 入口变体和冲突层，再实现/验收 P1 透明层。

不把“没有找到安全路径”写成必须修改模拟器的最终结论；若以上只能通过宿主私有能力解决，届时再提供相应证据与范围决策。当前 P0 的通过条件仍未满足，P1–P6 均未验收。

## 7. 后续动态实验（2026-09-25）

本文件前六节保留首批静态审查的历史范围。后续已新增独立诊断层和最小 Vulkan 宿主，完成真实原生呈现与两轮生命周期实验。SDK 透传重新进入当前层链已取得动态证据；SDK 初始化后的代理路径仍未执行。详见 [调用链实验报告](../streamline-layer-probe/EXPERIMENT.md)。P0 仍未通过，本次实验不替代原版 Ryubing 的 P1 验收。
## 8. 初始化后的 SDK 分发表实测（2026-09-25）

已加入固定官方头文件的最小 C ABI shim，实测 slInit → 各功能需求查询 → 合规 Vulkan 1.3 实例/设备 → slSetVulkanInfo → SDK idle proxy → slShutdown/销毁成功。SDK 主线程及宿主工作线程的 idle proxy 均重入诊断层，确认初始化后的路由问题。实际返回 DLSS-G 附加 graphics=1、compute=2，实验为宿主/SDK 分配互不重叠的四条队列。

详见 [SDK-EXPERIMENT.md](../streamline-layer-probe/SDK-EXPERIMENT.md) 及其原始证据。SDK 运行时警告、交换链生命周期、SDK 自有线程路由、窗口前置停用和原版构建身份仍未闭环，P0 保持未通过。

## 9. 初始化取址候选的反证

新增诊断模式 --sdk-idle-capture 已完成正对照和双线程 SDK idle 实测。直接本层 GDPA 可返回 next，但系统 GDPA 仍返回本层入口；slSetVulkanInfo 期间没有触发本层 idle 选择分支，SDK 调用依然重入。排除仅在初始化期切换 GDPA 返回值的候选方案。详见 [路由实验](../streamline-layer-probe/ROUTING-EXPERIMENT.md)。P0 继续保持未通过，下一步需要审查显式下游 resolver 的源码适配可行性及 NGX/插件覆盖范围。

## 10. 显式下游 resolver 源码适配审查

[源码适配审查](../streamline-layer-probe/SOURCE-ROUTING-REVIEW.md) 确认只改 interposer 无法覆盖两条未传 resolver 的 NGX 初始化路径，插件计算层也会复制 resolver 后重建函数表。新增 sdk-route-audit 冻结 13 个源码文件并报告关键位置。尚未构建实验 SDK 或证明闭源路由，FG 关闭，P0 未通过。

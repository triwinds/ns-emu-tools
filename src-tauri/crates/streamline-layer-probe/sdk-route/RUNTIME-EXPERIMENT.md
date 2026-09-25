# 实验 SDK 实际运行验证（2026-09-25）

已完成诊断宿主 --sdk-route 模式；FG 关闭，P0 未通过。此模式验证初始化和 idle 下游路由，不验证完整 FG/交换链路由。

## 运行与身份

先以 sdk-bridge feature 构建宿主。用 stage-runtime.ps1 -Destination <新绝对目录> 从已记录构建产物和冻结参考目录准备七 DLL，再运行：

    streamline-layer-probe.exe --sdk-route --layer <诊断层绝对路径> --interposer <实验目录/sl.interposer.dll> --session <新绝对目录>

[runtime-v2.json](runtime-v2.json) 固定替换 interposer/common 的 SHA-256；其余五 DLL 与原基线相同。宿主编译内嵌该清单，复制前、复制后以及子进程加载前都校验。默认模式继续使用原七 DLL 基线。两个模式互用错误版本均在创建 session 前拒绝；不提供动态接受新哈希的开关。没有修改 SDK 安全加载实现。

Layer 只提供 next 回调和记录，不加载 SDK。宿主在 slInit 后创建设备，通过 C++ 桥注册 48 字节私有结构，再 slSetVulkanInfo；实验模式的 adapter support 查询移至建表后。回调按精确 live instance/device 身份查表，释放锁后进入下游，不依赖 TLS 初始化窗口。

## 物理设备句柄边界修正

首次运行 failed-001 的注册成功，但 slSetVulkanInfo 返回 24（eErrorExceptionHandler）。SDK dump 的首个异常是 0xc0000005，发生于系统 vulkan-1.dll+0x45fb8。使用本地匹配 PDB 解析内存中的返回地址 sl.common.dll+0x5b8d5，对应 Vulkan::init 的 vulkan.cpp:1180，即 GetPhysicalDeviceMemoryProperties 调用后的源码位置。这是 dump 地址与局部源码定位，不是完整符号化堆栈。

应用层枚举返回的 physicalDevice 与诊断层 vkCreateDevice 收到的 physicalDevice 不相同。下游实例函数必须配套该层边界的句柄。层现在在成功创建设备时保存其 physicalDevice；宿主按设备取得该身份，只用于实验 SDK 的注册、VulkanInfo 和 adapter support，应用自己的对象保持原值。sdk-route-objects.json 记录二者，未靠猜测内存布局解包 Loader 对象。

修正后 passed-002、passed-003 两个独立子进程均通过。SDK shutdown 失败时子进程 abort，避免继续销毁 Vulkan 对象或卸载回调 DLL；失败会话因此以 0xc0000409 结束，不能误记为正常 teardown。成功路径 join 宿主工作线程、shutdown 成功后才销毁 device/instance，最终映射为零。

## 动态证据

完整日志和 JSON 位于 [evidence/sdk-route-runtime-2026-09-25](../evidence/sdk-route-runtime-2026-09-25)。

| 检查 | 两次修正后的运行结果 |
| --- | --- |
| slInit / 路由注册 / slSetVulkanInfo / slShutdown | 全部 0 |
| DLSS-G / Reflex / PCL adapter support | 全部 0 |
| 应用 DeviceWaitIdle 对照 phase 14 | 进入诊断层一次 |
| SDK 主线程 idle phase 7 | 成功，层重入 0 次 |
| SDK 宿主工作线程 idle phase 8 | 成功，层重入 0 次 |
| SDK idle / 系统 idle / next idle 地址归属 | interposer / 诊断层 / nvoglv64 |
| callback 建表查询 | 观察到 GIPA 和 GDPA，下游 idle 非空 |
| 销毁后 dispatch 映射 | 0 |

旧冻结 SDK device 回归仍观察到主线程/工作线程各一次 layer 重入。默认呈现回归通过两轮生命周期、baseline/layered 共 12 次原生呈现。编译检查覆盖默认和 all-features 的 host、显式 Windows target；零编译警告/错误，10 项测试通过，cargo fmt 通过。

SDK 运行日志仍有 slInit 前 API 提示、debug utils 未启用和三个 command hooks 不支持的警告，与旧实验一致，未屏蔽。没有启用 Khronos validation layer。

## 下一步

宿主拥有的 queue/command-buffer 路径已完成下述实验；下一步扩展 SDK 交换链生命周期。NGX 是否在完整 FG 路径中遵循 resolver、SDK 自有异步线程、同进程重复初始化和 teardown 后后台访问均未证明。当前 worker 是宿主创建的；idle_routing_verified=true，但 runtime_routing_verified、sdk_owned_worker_tested、p0_passed 保持 false。

该结果仅适用于单 instance/device 的隔离诊断宿主，不能直接部署到 Ryubing。

## 队列和命令缓冲实验（同日续）

已完成宿主拥有的 queue/command-buffer 路径验证。诊断层从设备创建链保存 VK_LOADER_DATA_CALLBACK，不修改应用 pNext。私有 probeRouteGdpa 对 GetDeviceQueue、GetDeviceQueue2、AllocateCommandBuffers 提供适配：调用 next 后，用 Loader 提供的 SetDeviceLoaderData 初始化对象，核对 dispatch key 与设备一致。只覆盖该私有 GDPA 路径；GIPA、NGX、SDK 内部对象创建路径还需扩展验证。SDK 补丁与七 DLL 哈希未变。

层不拥有队列，不替 SDK 缓存或释放命令缓冲。缺少 callback 时分配失败；部分初始化失败会释放本批命令缓冲并清空输出。void 队列接口无法返回错误，初始化失败时终止隔离子进程。

phase 15 使用应用函数表作对照，phase 16 使用 SDK GDPA 函数表。各使用宿主 queue index 0，执行两轮 pool 创建、primary command buffer 分配、空命令录制、QueueSubmit、fence 等待、QueueWaitIdle、释放 command buffer、销毁 fence/pool。GetDeviceQueue 和 GetDeviceQueue2 返回同一队列。提交或等待失败时终止子进程，避免在队列未确认静止时正常 teardown；fence 等待上限五秒，父进程保留整体超时。

两个独立会话均通过：
- 应用对照的七个观测入口次数符合预期；SDK 路径这七个入口层重入为零。
- 两次队列初始化、两次命令缓冲初始化均成功，dispatch key 与设备一致。
- 两轮 fence 完成、命令缓冲释放；slShutdown 成功，销毁后设备／实例映射归零。
- SDK 主线程及宿主工作线程 idle 仍无重入。

完整证据见 [sdk-route-commands-2026-09-25](../evidence/sdk-route-commands-2026-09-25)。驱动复用了已释放的命令缓冲地址，不要求两轮句柄值不同。host_command_lifecycle_verified=true 仅覆盖宿主拥有的空命令；资源读写、SDK 自有 worker/command buffer、同进程 SDK 重初始化和交换链尚未验证。runtime_routing_verified=false、p0_passed=false，FG 关闭。

cargo fmt、默认/all-features 的 all-targets host 和显式 x86_64-pc-windows-msvc cargo check 通过，零编译警告／错误；11 项测试通过。新增检查覆盖 Loader callback 链查找，以及初始化失败证据、SDK 重入、缺失应用对照的拒绝。旧冻结 --sdk-device 和默认 baseline/layered 共 12 次呈现回归通过。未启用 validation layer，SDK 运行警告保留。

下一步扩展 SDK 交换链生命周期，并验证 SDK 内部对象创建与退出顺序。


## 交换链生命周期实验（2026-09-25 续）

--sdk-route 现在在既有空命令测试后运行应用路径（phase 17）与 SDK 路径（phase 18）的交换链对照。SDK 路径使用其 GIPA/GDPA 构建函数表，并使用 layer 捕获的下游 physical handle；应用路径仍使用 Loader 的 physical handle。实例扩展补齐 surface/win32_surface，设备扩展补齐 swapchain，仅影响实验模式。

每条路径创建自己的 Win32 surface，使用宿主保留的 queue index 0。创建两代 SDR/FIFO 交换链，第二代通过 oldSwapchain 替换第一代；每代获取图像、清色并呈现三帧，覆盖 acquire/acquire2。旧交换链在上一代队列等待完成后退役，最终销毁交换链、surface 和窗口，再执行原有 SDK shutdown、device/instance 销毁及 dispatch map 清空检查。发生运行失败时直接终止隔离子进程，避免对可能仍在执行的资源进行展开清理。

首次隐藏窗口运行（hidden-window-001）虽完成 Vulkan 调用，但 SDK 产生两条 Could not find a window corresponding to this application 错误。该运行原始 JSON 的成功标记不作为验收证据。探针改为短暂显示不抢焦点的诊断窗口，并在输出最终成功结果前拒绝 SDK 日志中的任何 [error]。后续 passed-002/003 两次真实 GPU 运行均通过此门禁。原有 SDK pre-slInit、debug utils、未支持 hook 等 warnings 仍保留；这些并未被修复或隐藏。

两次通过的运行各完成应用 6 次及 SDK 6 次呈现、各两次 create/destroy。应用路径拦截计数作为正对照，SDK 路径 surface/swapchain/acquire/present/command/submit/idle 拦截均为零。phase 18 观察到 5 次 QUEUE 与 30 次 COMMAND_BUFFER 的 Loader 初始化，全部返回成功且 dispatch 匹配。数量超过宿主显式创建的 1 次 queue 与 6 个 command buffer，说明本次交换链 hook 触发了额外对象路径；不能仅凭数量证明 SDK 内部所有路径或后台线程正确。该阶段 trace 全部来自宿主线程。

持久证据：[sdk-route-swapchain-2026-09-25](../evidence/sdk-route-swapchain-2026-09-25/README.md)。新增实际 trace 回放与反例测试，总计 12 项测试通过；默认/全特性、host/显式 Windows target 的 all-targets cargo check 全部无警告无错误。冻结 SDK 的 device 模式及未初始化 baseline/layered 模式回归通过。

限制：这是宿主调用 SDK 的 FG-off 生命周期测试，没有开启 FG、提交引导资源或验证生成帧；窗口短暂显示仅用于 SDK 窗口识别，不是显示节奏验收。重建尺寸不变（实测 client extent 304×201），尚未覆盖真实 resize/minimize、窗口线程协调、同进程 SDK 重新初始化、SDK-owned worker quiescence 或 validation layer。完整 runtime routing 与 P0 仍为 false。下一步扩展实际窗口尺寸变化和停止/重新启动生命周期。


## 实际缩放与呈现恢复（2026-09-25 续）

交换链探针现运行三代：初次呈现三帧；宿主在前一代队列等待完成后记录 before_resize，再调用 SetWindowPos 扩大窗口，重新查询 surface capabilities，使用新 extent 和 oldSwapchain 创建第二代并呈现三帧；销毁第二代后，以 null oldSwapchain 创建第三代，恢复呈现三帧。最后销毁交换链、surface 和窗口，执行既有 SDK shutdown 与设备/实例映射归零检查。窗口操作、代际完成时间和宿主线程写入 sdk-swapchain-calls.json；若尺寸未变化或为零，实验拒绝通过。

两次实机运行均通过。应用路径与 SDK 路径分别从 304×201 变为 624×441，各完成九次呈现、三次交换链创建/销毁。SDK 观测入口重入为零，phase 18 的五次 QUEUE 和 33 次 COMMAND_BUFFER Loader 初始化全部成功且 dispatch 匹配，SDK 日志无 error。12 项测试通过；默认/全特性、host/显式 Windows target 的 all-targets cargo check 无警告无错误。SDK 补丁与运行时 DLL 未变。

持久证据：[sdk-route-resize-2026-09-25](../evidence/sdk-route-resize-2026-09-25/README.md)。当前恢复仅指同一设备和 surface 上停止/恢复呈现，不能代替同进程 SDK 重新初始化或第二次启动游戏的验证。前置通知由诊断宿主主动发出，尚未证明外部层可捕获任意游戏窗口操作；最小化/恢复、零尺寸暂停、OUT_OF_DATE 恢复及 SDK 自有线程仍待验证。FG 关闭，完整 runtime routing 和 P0 仍未通过。下一步验证完整 SDK 关闭/重初始化及最小化生命周期。


## 同进程重初始化阻断（2026-09-25）

实际执行 --sdk-route-repeat 后，多个会话分别在第二、第七、第十三、第九轮 slInit 失败。第九轮异常地址根据模块快照与匹配 PDB 精确定位到上一轮 sl.common 的 sl::ngx::ngxLog（偏移 0x40310），新一轮 common 已重定位。只保持 common 加载会导致插件 JSON 状态缺失，试验已撤回。需要稳定 NGX 回调入口并保持插件重置契约后才能继续验收。单轮最小化恢复与冻结 SDK／默认基线回归通过；详见 [证据](../evidence/sdk-route-repeat-2026-09-25/README.md)。


## v3 已修复重初始化阻断（2026-09-25）

NGX 日志回调迁至常驻 interposer，直接写入诊断 stderr；common 与功能插件仍正常卸载。两批各 20 轮完整 SDK 生命周期通过，common 分别出现 4、3 个加载地址，覆盖此前悬空回调触发条件。共 720 次呈现，各轮 shutdown 成功、映射清零、SDK 日志无 error。冻结 SDK 与无 SDK 基线回归通过。详见 [v3 完整记录](../evidence/sdk-route-v3-2026-09-25/README.md)。FG、完整 routing、P0 仍不因本项通过而开启。

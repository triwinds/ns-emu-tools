# SDK 下游路由实验：物理设备归属适配

2026-09-25。当前第三版继承第二版物理设备路由，并修复跨 SDK 生命周期的 NGX 日志回调悬空。两批各 20 轮 SDK 重初始化通过，见 [v3 验证记录](../evidence/sdk-route-v3-2026-09-25/README.md)。
已通过隔离诊断宿主的初始化和 idle 路由测试，见 [实际运行证据](RUNTIME-EXPERIMENT.md)。该补丁不是完整可用的 Vulkan layer 集成。FG 关闭，P0 未通过。

## 实现

- [downstream-v3.patch](downstream-v3.patch) 固定应用到 SDK commit e8aaa6eaac968711fb62473d4ae8256dde20919b。
- 新增 slRegisterVulkanLayerRouteV1，48 字节 Windows x64 私有结构包含 size/version、instance/physicalDevice/device 和 GIPA/GDPA；不修改官方 VulkanInfo 或 VkTable 布局。
- slInit 后注册，slSetVulkanInfo 建表前消费绑定。拒绝空值、版本/大小不符、重复绑定、对象不符及重复消费；此实验版缺失注册时直接失败。
- resolver 查询自身时返回受控入口，未知对象/空实例/空名称返回 nullptr，其他命令交给显式下游。回调前释放锁。
- interposer 发布该表，原有 compute 初始化复制表和 resolver 的路径随之获得新入口。
- sl.common 的 ProjectID 和 AppID 两条 NGX Vulkan 初始化分支均传入表中的 resolver；启动时校验表和设备/实例归属。
- slShutdown 在插件卸载完成后清除绑定。宿主负责串行化初始化/关闭，等待自己的任务结束，并保持回调模块与对象存活。
- Packman bootstrap URL 改为 HTTPS；后续依赖仍使用原版 Packman 配置，本实验未声称整个下载链路均为 HTTPS。

## 复现

在项目根目录运行：

~~~powershell
& ./src-tauri/crates/streamline-layer-probe/sdk-route/build.ps1
# 已有实验 checkout 时，校验补丁文件后重建：
& ./src-tauri/crates/streamline-layer-probe/sdk-route/build.ps1 -Resume
~~~

要求已有冻结 SDK checkout 和 VS 2022 C++ 工具链。脚本从固定 commit 克隆到 src-tauri/target/streamline-sdk-route-repro-v3，拒绝直接覆盖已有目录。
依赖缓存在 src-tauri/target/streamline-packman-cache，PM_PACKAGES_ROOT 仅设置在当前进程并恢复，不使用 setx。
固定 project.xml 包版本；依赖缓存本身不是额外的独立密码学基线。

脚本检查 [patched-files-v3.json](patched-files-v3.json) 中 9 个修改文件的 LF 哈希、其他已跟踪修改及非预期源码，生成 VS2022 工程，再依次构建 interposer、compute、common。
使用 Develop|x64、MSVC v143。NVAPI 原版头文件包含 CP1252 字节，故使用 /source-charset:.1252 /execution-charset:utf-8，并保留 warnings-as-errors。
原先直接构建 common 会缺少先行生成的 gitVersion.h/compute 库；因此显式维护三个工程的构建顺序。

源码与 DLL 留在 target 下；原有 SDK 和七 DLL 冻结校验规则不变。没有安装、注入模拟器或修改系统 Vulkan 注册。
此处 sdk_rebuilt 只表示重建上述三个开源目标，不表示闭源 DLSS-G 被重建。

## 第二版历史验证证据

[build-evidence.json](../evidence/sdk-build-v2-2026-09-25/build-evidence.json) 记录补丁、构建脚本、测试源码和三个产物的 SHA-256。
同目录保留四份构建日志，最终均为 0 warning / 0 error；从第二个干净 checkout 应用补丁和完整构建成功，-Resume 校验也通过。
DLL 导出表确认包含 slRegisterVulkanLayerRouteV1、slSetVulkanInfo、slShutdown。

[route-contract.cpp](route-contract.cpp) 编译为独立 MSVC /W4 /WX 测试程序，使用伪句柄与伪回调，不加载 Vulkan 或 SDK DLL。
覆盖 9 类无效注册、缺失/重复绑定、对象不匹配、重复消费、self-query、未知命令/对象、回调同步重入、工作线程查询、清除后失效及第二次完整生命周期。
412 次下游查询全部符合预期。该工作线程是测试程序创建的，不能算 SDK 自有工作线程证据。

## 尚未验证与下一步

1. 已适配 wrapper 的 vkGetPhysicalDeviceToolProperties 和宿主 LUID 查询。getStaticVKMethods / 光流查询只在已审阅的 SDK 自建探测实例调用链中保留系统入口；代理创建实例路径、闭源插件内部调用仍不属于已验证范围。
2. 当前 resolver 拒绝 null instance 的全局查询；若 NGX 使用这种合法查询，必须明确扩展全局命令契约并加入证据，不能静默回 Loader。
3. 此实现只控制按注册实例/设备取址，返回命令指针本身不封装句柄归属；多设备、多实例和代理 vkCreateInstance 路径不属于已验证范围。
4. 已新增独立实验 DLL 清单、宿主开关和显式注册。混合运行时的初始化和 idle 路径通过，不能推断完整 FG 兼容性。
5. NGX 是否遵循回调、SDK 自有线程、队列/命令缓冲 dispatch 初始化、交换链和 teardown 后无后台访问仍需动态验证。绑定清理本身不证明 SDK 工作已经全部结束。
6. 后续宿主必须验证先完成 SDK 工作、slShutdown 成功，然后销毁 native device/instance；注册后初始化失败也走有序清理。未成功关闭时不能卸载回调模块。
7. 不修改安全加载实现，不放宽原有基线，不启用 FG。运行时路由通过之前 runtime_routing_verified 和 p0_passed 保持 false。

## 第二版的物理设备契约

- 私有参数 sl.layer_route.v1.physicalResolver 跨模块发布物理设备 resolver，不改变官方 VkTable 布局。
- 仅允许已消费绑定中的 physicalDevice 查询 vkGetPhysicalDeviceProperties2 / vkGetPhysicalDeviceToolProperties；通过该设备所属的已注册 instance 查询下游。空对象、未知对象、其他命令及 shutdown 清理后均返回空。
- getLUIDFromDevice 改为必须传入显式 properties 函数；findAdapter 获取上述 resolver 并检查失败结果，取消隐式系统入口。实验模式下，在 slSetVulkanInfo 建表之前使用 Vulkan AdapterInfo 查询支持情况返回 eErrorInvalidState；其他物理设备返回 eErrorInvalidParameter。这是有意收紧的实验限制。
- 工具属性入口没有匹配绑定或下游函数时返回 VK_ERROR_INITIALIZATION_FAILED，不解引用空函数指针。
- SDK 自建探测实例的创建、物理设备枚举、光流能力查询和销毁继续属于系统 Loader 路径，不借用宿主 instance 的下游表。当前未新增探测实例的动态生命周期证据。
- 契约测试增加物理设备归属、命令白名单、有效返回、下游缺少命令和关闭后失效；仍是 mock 测试。
- downstream-v1.patch 和旧 evidence/sdk-build-2026-09-25 保留作首版历史证据；当前脚本与哈希清单对应 v3，旧构建目录不覆盖。

诊断宿主的真实 SDK idle 已避开 layer 重入，物理设备句柄层级修正、回归结果和后续工作见 [RUNTIME-EXPERIMENT.md](RUNTIME-EXPERIMENT.md)。完整运行时验证前不启用 FG。


## 第三版 NGX 回调

slInit 在插件加载前发布 interposer 内的强类型日志回调，并用 GetModuleHandleExW(PIN) 保证其代码存活到进程终止；common 仅取得回调指针，不再将自己的 ngxLog 注册给 NGX。日志写入子进程捕获的 stderr，避开已销毁的 SDK logger。common 和功能插件继续正常卸载，不能通过常驻所有插件绕过状态重置。当前不支持进程内热卸载 interposer。

构建使用 downstream-v3.patch 与 patched-files-v3.json；stage-runtime.ps1 默认读取 runtime-v3.json，将固定的两份重建 DLL 与五份参考 DLL 放入新目录。既有 v2 补丁、清单、运行库与失败证据保留。

> 历史实验记录：当前目录、构建命令和运行状态以 [运行组件说明](../../README.md) 为准；部分原始日志已清理。

# P0：显式下游 resolver 源码适配审查

日期：2026-09-25。Streamline v2.12.0，commit e8aaa6eaac968711fb62473d4ae8256dde20919b。
结论：存在可实施的源码适配入口，但只改 interposer 不足以证明完整路由成立。
本轮新增只读 Rust 审计工具；尚未修改或重建 SDK，FG 关闭，P0 未通过。

## 可复核证据

[sdk-route-audit.json](evidence/source-route-2026-09-25/sdk-route-audit.json) 保存 13 个文件的 LF 归一化 SHA-256、关键语句及一基行号、构建材料存在性盘点。
规则位于 ../../../../tools/streamline-sdk-audit/route-baseline.json；工具不会加载 DLL、运行 SDK setup 或修改 SDK。
哈希匹配只说明审查对象相同，不证明闭源运行库行为，也不是构建就绪判定。

| 路径 | 固定源码依据 | 适配影响 |
| --- | --- | --- |
| 手动注册 | sl.api/sl.cpp:469 的 slSetVulkanInfo 先 processVulkanInterface，再 setVulkanDevice / initializePlugins | 必须在建表及插件启动前安装 resolver |
| interposer | vulkan/wrapper.cpp:57 起从系统模块取 GIPA/GDPA，建 s_vk 表，再复制到 s_idt/s_ddt 并发布 kVulkanTable | 提供显式 next 回调入口；同步所有表，不能只替换 idle |
| resolver 再查询 | vulkan/layer.h:96、178 查询 GIPA/GDPA 本身 | 回调对自身名称也必须返回受控 resolver，防止再落回 Loader |
| 插件计算层 | sl.chi/vulkan.cpp:1076–1092 新建 VkTable，复制 resolver，再建表；命令上下文还复制 device 表 | 后改 interposer 的表无法修复已有副本；回调须贯穿整个 SDK 生命周期 |
| NGX ProjectID | sl.common/commonEntry.cpp:1516 两个 resolver 参数均为 nullptr | 必须将明确的回调传到此处 |
| NGX AppID | 同文件:1556 同样传 nullptr | 两条分支都要适配，不只修当前诊断宿主使用的 ProjectID |
| NGX ABI | external/ngx-sdk/include/nvsdk_ngx_vk.h:182、258 支持 GIPA/GDPA 参数 | 有公开入参可用；闭源 NGX / DLSS-G 是否完全遵循它仍需动态验证 |
| 系统导出旁路 | wrapper.cpp:1893 直接查 vkGetPhysicalDeviceToolProperties；sl.chi/vulkan.cpp:2780 起直接查实例及物理设备命令 | 按对象归属审查，不能将全部系统入口替换成一个应用实例的 next |
| SDK 辅助实例 | commonEntry.cpp:923 调用 createInstanceAndFindPhysicalDevice；sl.chi/vulkan.cpp:2804 创建独立实例，后续有销毁、LUID、光流能力查询 | SDK 自建探测实例与应用实例需要不同路由和完整生命周期记录 |

上一轮 [初始化期间捕获实验](ROUTING-EXPERIMENT.md) 已否定仅在 slSetVulkanInfo 期间改变本层 GDPA 返回值的方案。
本轮明确源码覆盖范围，没有证明运行时路由已经打通。

## 最小实验适配契约

建议新增实验专用、带版本与结构大小的 C ABI 注册入口，绑定实际 instance / physicalDevice / device 与两个 VKAPI_CALL resolver。
不改变公开 VulkanInfo v3 的大小，也不改变插件共享的 VkTable 布局。该入口尚未实现，名字和版本号在补丁落地时确定。

1. slInit 后、slSetVulkanInfo 前注册；只接受完整的非空对象与回调对。首轮限制一个活动 device，拒绝覆盖活动绑定和未注册对象。实验模式缺失回调直接失败，不能静默回系统 Loader。
2. processVulkanInterface 在建表前消费绑定。覆盖 resolver 本身的查询、实例命令、设备命令和已支持扩展入口；保持未知或未启用命令返回规则，不能把非空当作能力支持证明。
3. 回调从本层按对象保存的 next dispatch 中取址，调用前释放映射锁。不得用线程 ID、调用模块、TLS 或全局开关判断“来自 SDK”。应用路径始终进入本层需接管的入口，SDK 路径使用专用回调。
4. sl.common 在两条 NGX 初始化分支取得已发布的 VkTable resolver 并显式传入，先检查表和回调存在。需要重建 sl.common 及其 sl.compute 静态依赖。
5. 逐个审计直接系统导出：应用对象使用其下游表；SDK 自建探测实例保留它自己的 Loader 链，不套用应用 instance/device。进程身份授权无法区分同一进程内这两类实例。
6. 绑定保持到 slShutdown 返回及 SDK 所有工作完成后，再清除并销毁应用 device/instance；错误初始化也执行有序清理。仍需验证关闭是否结束闭源工作线程，不能从源码推断已保证。
7. 对 SDK 创建的队列/命令缓冲等 dispatchable 对象，核查后续层/Loader 所需的分发表初始化契约。直达下一层能执行 idle，不证明全部对象的创建与使用都合法。

独立实验目录保留源码补丁、原始及修改文件哈希、构建参数、工具链和运行 DLL 哈希。
新宿主模式明确选择实验基线，原有七 DLL 校验不放宽。官方冻结版本和已有实验目录保持不变。

## 构建范围和约束

premake.lua 提供 sl.interposer、sl.compute、sl.common 目标；sl.common 链接 nvsdk_ngx_d.lib、NVAPI 等。
project.xml 固定 premake 5.0.0-beta1+nv1、VulkanSDK 1.3.231.1-ext 等依赖；它与 Rust 探针的 Vulkan-Headers 1.4.341 是两个基线，不能默默互换。
setup.bat 会通过 Packman 获取依赖并生成工程；本轮未执行下载或构建。

- 本次盘点缺少 tools/premake5/premake5.exe、external/vulkan/Include/vulkan/vulkan.h、external/nvapi/amd64/nvapi64.lib、external/slang/bin/slangc.exe。
- NGX 静态库及 NvLowLatencyVk.lib 存在；存在不代表已验证兼容。
- source/plugins/sl.dlss_g 不存在，实验仍依赖冻结的闭源 DLSS-G 插件，不能声称完成全 SDK 源码构建。
- 本机 vswhere 检测到 VS 2022 Community 17.14 和 VS 18 BuildTools 18.1。build.bat 的 VS2022 分支限定 [17,18)，Rust 探针能用 VS18 编译不代表 SDK 原始构建脚本能用它；VS2022 所需组件尚未验证。
- license.txt 与 external/ngx-sdk/license.txt 已加入审计基线。后续分发实验版需逐组件保留适用许可证和通知；本轮未作分发资格结论。
- 自建 interposer/common 与冻结 DLSS-G、Reflex、PCL 的加载及 ABI 兼容性未验证。遇到加载或签名限制应记录失败，不移除校验冒充成功。

## 下一步与验收

下一步在独立源码副本准备依赖及最小补丁，先验证能构建 interposer/common，再扩展诊断宿主显式注册 resolver。
验收至少包括：

- 应用 idle 仍经本层；SDK 主线程和宿主工作线程 idle 成功且不重入本层。
- 记录插件建表和两种 NGX 初始化分支的 resolver 输入及实际查询，不以非空或初始化成功代替路由证据。
- SDK 自有线程证据单独记录；宿主启动的工作线程不算 SDK 自有线程。
- 逐步验证创建/销毁、队列与命令对象、surface/交换链，覆盖别名入口与直接系统导出。
- 重复初始化、失败回滚、关闭后不访问旧表、透明呈现回归和进程隔离。

闭源路径仍重入或证据不足时，runtime_routing_verified 与 p0_passed 保持 false。

## 复现

~~~powershell
cargo build --locked --manifest-path src-tauri/tools/streamline-sdk-audit/Cargo.toml --bin sdk-route-audit
& src-tauri/tools/streamline-sdk-audit/target/debug/sdk-route-audit.exe --sdk-dir D:\py\ns-emu-tools\src-tauri\target\streamline-sdk-v2.12.0
~~~

退出码 2 表示源码证据匹配但运行时路由未验证；1 表示输入或证据错误。JSON 保留全部检查结果。
本轮 cargo fmt、6 项测试（新增 2 项）、host 与显式 Windows x64 的 all-targets cargo check 均通过，无编译警告。
后续进展：首版注册/NGX 补丁与隔离构建已完成，见 [构建实验](sdk-route/README.md)；本报告上文保留源码审查当时的状态。直接系统入口与运行时验证仍未完成。

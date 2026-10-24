> 历史实验记录：当前目录、构建命令和运行状态以 [运行组件说明](../../README.md) 为准；部分原始日志已清理。

# P0：初始化期间选择 next 地址的反证实验

日期：2026-09-25。最终会话 streamline-idle-capture-002。实验完成，候选路由没有生效；FG 关闭，P0 未通过。

## 实现和范围

新增 --sdk-idle-capture 模式（要求 sdk-bridge feature）。复用原有 SDK 初始化、需求查询和原生设备创建，只在调用 slSetVulkanInfo 的线程上临时启用诊断层 GDPA 的 idle 取址分支。分支仅针对 vkDeviceWaitIdle 返回保存的 next 地址，其余命令保持透明诊断行为。作用域退出时通过 RAII 恢复，包括错误返回。

这不是调用时的递归保护，也不把 TLS 当作异步重入解决方案。实验要检验的假设是：SDK 初始化时会重新查询本层 GDPA，因而可在这时向其提供独立的 next 函数缓存。没有修改 Loader 分发表、冻结 SDK 二进制或系统注册。

## 对照和结果

文本证据位于 [evidence/idle-capture-2026-09-25](evidence/idle-capture-2026-09-25/idle-capture-result.json)。inputs.json 记录本次宿主、层和运行库哈希；layer.jsonl 和完整 sl.log 保留原始路径与时序。

| 检查 | 实测 |
| --- | --- |
| phase 12：直接调用本层 GDPA，开启选择分支 | 返回 nvoglv64.dll 中的 next idle 地址 |
| phase 12：同一作用域内调用系统 GDPA | 仍返回 streamline_probe_layer.dll 的 idle 入口 |
| phase 6：slSetVulkanInfo 期间本层 idle 选择分支命中次数 | 0 |
| phase 7：主线程调用 SDK idle | VK_SUCCESS，本层重入 1 次 |
| phase 8：宿主工作线程调用 SDK idle | VK_SUCCESS，本层重入 1 次 |
| SDK 插件行为 | sl.log 记录两次 slHookVkDeviceWaitIdle / flushAll |
| SDK 关闭及 Vulkan 销毁 | 成功，层分发表存活对象数为 0 |

固定 SDK source/core/sl.interposer/vulkan/wrapper.cpp 的 processVulkanInterface 从系统 vulkan-1.dll 取得 GIPA/GDPA，再由 layer.h 的 mapVulkanDeviceAPI 构建 SDK 表。实测说明在当前冻结 Loader/SDK 组合下，系统 GDPA 的 idle 返回值没有随本层临时取址策略改变。设备创建期间已发生 idle 取址，初始化期却没有再次进入该分支；这与 Loader 使用此前建立的设备分发表相符。内部缓存细节属于解释，直接观测的地址差异和命中次数才是本实验结论。

**因此，不能采用“只在 slSetVulkanInfo 期间向 SDK 返回 next”的方案。** 把选择分支扩大到所有命令也不能凭此宣称可行，更不能在设备创建时永久返回 next，因为那会同时让应用路径绕过需接管的入口。

## 后续路线

下一步审查明确提供 SDK 下游 GIPA/GDPA 的源码适配方案：区分 SDK 内部缓存、插件持有的 VkTable、NGX 取址和直接系统导出，明确每条路径能否使用层保存的 next；同时审查是否需要重新构建 interposer 及其构建/分发约束。若需要新二进制，必须建立独立版本与哈希基线，不能覆盖本次官方冻结二进制的证据。

公开 VulkanInfo 没有下游 resolver 字段；当前没有实现或验证上述源码适配，也没有证明独立层不可能。不得把调用者模块猜测、线程名或全局开关用于生产路由。SDK 自有工作线程、NGX、surface 和交换链生命周期仍未验证。先明确路由再扩展交换链实验，不进入工具箱 UI。

## 复现与验证

在仓库根目录运行（SDK/Vulkan 头文件准备方法见 README）：

~~~powershell
cargo build --locked --all-features --manifest-path src-tauri/crates/streamline-fg/Cargo.toml
$probeRoot = Join-Path $PWD 'src-tauri/crates/streamline-fg/target/debug'
& "$probeRoot/streamline-layer-probe.exe" --sdk-idle-capture --layer "$probeRoot/streamline_probe_layer.dll" --interposer D:\ryubing-dlss5-win-x64\dlss\sl.interposer.dll --session D:\py\ns-emu-tools\src-tauri\target\streamline-idle-capture-new
~~~

会话目录必须不存在。experiment_completed 表示观察和对照完整，不表示候选路由成功；判断候选结果应读取 scoped_idle_capture_avoided_reentry。routing_solution_verified 和 p0_passed 保持 false。

- cargo fmt / fmt --check 通过。
- 默认、all-features 的 host 及显式 Windows x64 cargo check 均无警告/错误。
- 8 项测试通过；新增 TLS 隔离/命令范围和捕获证据完整性校验。
- 最终捕获实验 002 完成；SDK 原有模式 004、默认呈现实验 006 回归通过。
- SDK 原有运行警告仍保留，未启用 Vulkan validation layer。没有提交渲染工作或启用 FG。

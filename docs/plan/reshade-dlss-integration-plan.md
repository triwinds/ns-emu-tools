# ReShade / DLSS 实验性功能接入计划

状态：待实施。本文只定义实现方案，尚未接入 GUI 或安装组件。

## 目标与范围

- 在 GUI「实验性功能」分组新增「ReShade / DLSS」页面入口，集中管理模拟器的图形增强组件。
- 首期支持 Windows x64 模拟器：优先验证 Vulkan，同时支持 OpenGL 的 ReShade 安装、检测、更新和卸载恢复。
- 第二阶段接入 DLSS5 Feeder；只有完成对应模拟器、图形 API、GPU 和驱动组合的实测后，才标记为可用。
- 安装对象是宿主模拟器进程及其可执行文件目录，不是 Switch ROM、固件或游戏 mod 目录。同一模拟器安装下的多个游戏可能共用配置，页面必须说明作用范围。
- Linux/macOS 保留页面说明，但禁用本计划的 Windows 安装操作。32 位辅助进程、DX9 转译、原生 DLSS Tool/Bridge/ShortFuse、驱动配置修改后续另行评估。

## 已核对的项目接入点

| 现有文件 | 计划用途 |
| --- | --- |
| `frontend/src/layouts/AppDrawer.vue` | 在 `v-list-group value="experiment"` 中增加入口，与金手指管理、存档备份并列 |
| `frontend/src/router/index.ts` | 已使用基于 `pages/*.vue` 的自动路由，沿用现有机制 |
| `frontend/src/utils/tauri.ts` | 增加图形组件管理的类型与 invoke 封装 |
| `src-tauri/src/services/network.rs` | 复用 HTTP、代理、GitHub API 和下载镜像处理 |
| `src-tauri/src/services/downloader/` | 复用下载管理器、进度与取消能力，避免新增专用下载器 |
| `src-tauri/src/services/installer.rs` | 复用 `InstallReporter` 和安装阶段事件 |
| `frontend/src/components/ProgressDialog.vue`、`frontend/src/stores/ProgressStore.ts` | 实施时核对并复用现有进度交互 |

## GUI 页面设计

新增 `frontend/src/pages/reshadeDlss.vue`，路由为 `/reshadeDlss`。由自动路由流程生成类型，不手工维护生成文件。该页面不写入只接受 Yuzu/Ryujinx 的 `lastOpenEmuPage`。

页面从上到下包含：

1. **目标模拟器**：从已有配置列出 Yuzu 衍生系列、Ryujinx 的安装；允许手动选择模拟器 exe。显示实际路径、位数和图形 API，识别不确定时允许用户选择 Vulkan/OpenGL。
2. **ReShade**：显示渠道、可安装版本、当前版本、来源、安装状态；提供安装、更新、卸载、打开配置目录。首期使用支持 addon 的稳定版，后续再扩展 Nightly/自定义版本。
3. **DLSS Feeder**：独立开关和安装操作，展示组件版本、依赖状态与兼容性结果。未完成支持验证时显示“尚未验证”，不得用“文件已安装”代替“效果可用”。
4. **诊断与恢复**：重新检测、打开日志、查看安装清单、恢复原文件。操作结果包括部分失败的文件和可继续执行的恢复动作。

交互要求：

- 打开页面只读取状态，不自动下载、安装或修改 Vulkan 注册信息。
- 安装前展示目标目录、组件和冲突摘要；只有覆盖外部组件或需要系统提权时追加相应确认。
- 同一目标的写入操作串行执行；模拟器运行时阻止替换已加载组件，并提示关闭进程。
- 下载、解包、备份、部署、配置、验证使用现有进度 UI；下载可取消，开始修改文件后必须完成当前原子操作并恢复到一致状态。
- 未安装、已安装、可更新、外部安装、安装不完整、不支持、错误分别展示。

## ReShade 安装实现

### 下载与缓存

参考 RHI：解析官方站点的 addon 安装包链接，下载时携带必要的 Referer；从安装器附加归档提取 DLL，不运行安装向导。

实施前验证项目现有解包能力能否处理 PE 尾部 ZIP；不支持时再选择可维护的提取方案。至少校验下载响应、归档结构和 DLL 架构，不能仅靠 MZ 文件头判断下载成功。有可信上游摘要时校验摘要；本地计算的哈希用于恢复和变更检测，不冒充来源认证。

按组件、版本、架构建立缓存。通过现有网络与下载层传递请求头；如果发现请求头或取消能力缺失，再补齐统一接口，不直接绕过下载器。

### OpenGL

- 将匹配位数的 ReShade DLL 部署为模拟器 exe 同目录的 `opengl32.dll`。
- 检测同名文件归属，外部安装或未知 DLL 不静默覆盖。
- 使用组件专属 preset 和 shader 目录；合并必要的 ReShade 配置项，保留用户已有配置。

### Vulkan

- 单独实现 Vulkan implicit layer 部署与注册，不能将 OpenGL/DXGI 的 DLL 重命名方案套用到 Vulkan。
- 实施前核实当前 ReShade layer manifest、注册位置、启用范围及提权需求；优先采用上游支持的按目标启用机制。若只能全局启用，页面明确显示全局影响，不宣称仅影响选中模拟器。
- 记录 layer DLL、JSON、注册项及其原状态；同一个全局 layer 被多个目标使用时维护引用关系，卸载单个目标不能移除其他目标仍依赖的 layer。
- 提权限定在确实需要权限的注册/清理步骤，避免整个 GUI 常驻管理员权限。

## DLSS Feeder 接入

### 先做兼容性验证

Feeder 依赖 ReShade 提供的画面、有效深度和估算运动向量，构造 DLAA 调用供神经渲染消费者处理。它不是模拟器原生接入 DLSS 超分辨率；不得承诺提升帧率或支持所有游戏。

先选一个 Windows x64 Vulkan 模拟器组合实测：ReShade 加载、深度可用、运动向量 shader 编译、消费者运行、画面持续更新，以及 UI/性能影响。OpenGL 独立验证。无法取得有效深度时阻止“一键启用成功”的结论，保留普通 ReShade 功能。

### 组件与配置

拟部署 Feeder addon、兼容的神经渲染消费者、DLSS SR/NR DLL，以及所需 motion-vector/feed shaders。RHI 调研时使用的组合为：

- `dlss5-feed.addon64`
- `renodx-dlss5.addon64`
- `nvngx_dlss.dll`、`nvngx_dlssnr.dll`
- `DLSS5_Feed.fx`、`lumenite_Kernel.fx`

建立经过验证的版本组合清单，明确来源、架构、依赖和兼容条件。安装事务冻结所有组件版本，避免中途解析 latest 得到不同组合；升级失败保留旧组合。上游发生变化时重新验证，不能把独立组件的“最新版本”视为天然兼容。

用专属 preset 配置执行顺序和运动向量 provider；只有当前 shader 版本确认支持时才使用 RHI 调研中的 `DLSS5_MV_PROVIDER=3`。不覆盖用户 `ReShadePreset.ini`，不删除共享 shader/include 目录。

RHI 的 DLL 清单指向第三方镜像，接入前核实来源、再分发条件和版本可获得性；无法提供可靠自动下载时，明确保留用户选择本地组件的路径。

## Rust 结构与命令（拟新增）

业务集中在 Rust，前端只负责状态和交互。以下为拟新增路径及接口，实施时按现有模块注册方式接入：

- `src-tauri/src/repositories/graphics_components.rs`：版本、来源和资产解析。
- `src-tauri/src/services/graphics_components/`：目标检测、缓存、ReShade 部署、Vulkan layer、Feeder 配置、事务与恢复。
- `src-tauri/src/commands/graphics_components.rs`：Tauri 命令入口。
- 模型：目标（exe/API/架构）、组件版本、兼容性报告、安装状态、变更计划、安装记录。

建议接口：

| 命令 | 职责 |
| --- | --- |
| `detect_graphics_components` | 只读检测目标和组件状态 |
| `get_graphics_component_versions` | 查询可用版本和已验证组合 |
| `plan_graphics_component_install` | 解析依赖、检查冲突并返回具体变更清单 |
| `install_graphics_components` | 重新校验目标状态后执行已解析计划并上报进度 |
| `uninstall_graphics_components` | 按所有权卸载并恢复原文件/配置/注册信息 |
| `repair_graphics_components` | 从未完成事务记录恢复一致状态 |

不能信任前端提供的任意下载地址和文件写入列表；安装命令接受受控组件标识与目标，后端自行解析并限制路径。

## 安装记录与恢复

采用独立的、带 schema 版本的结构化安装记录，不借用 RHI 的 `.original` 文件作为本工具所有权证明。

- 记录目标 exe、API、组件版本、源 URL、文件相对路径、部署哈希、原文件备份、配置项前后值，以及全局 layer 的使用关系。
- 下载解包完成后再修改目标；检查归档路径穿越、目标路径和符号链接/目录联接，所有实际写入必须限定在声明范围内。
- 首次备份不可在升级时覆盖；事务日志记录各阶段，支持崩溃或中断后的修复。
- 卸载只删除本工具拥有且未被外部修改的文件；用户后来修改的文件保留并报告冲突，配置恢复不覆盖后续用户编辑。
- Feeder 和 ReShade 分别跟踪依赖；卸载 Feeder 保留普通 ReShade，移除被依赖的 ReShade 时明确列出影响。
- 不照搬 RHI 调研中覆盖 preset、清理共享目录、仅记录错误后继续的处理方式。关键依赖缺失必须返回失败，不能显示安装成功。

## 实施顺序

1. 新增实验性功能页面和入口，完成目标选择、只读检测及不支持平台状态。
2. 实现组件来源、缓存、安装记录和失败恢复；验证 ReShade 官方安装包的解包方式。
3. 完成 OpenGL ReShade 的安装/更新/卸载闭环，再完成 Vulkan layer 及多目标引用管理。
4. 对 Vulkan/OpenGL 模拟器实测 ReShade 加载和深度能力，保存兼容性结果。
5. 接入经验证的 Feeder 组件组合、专属 preset、诊断与恢复，开放对应组合的 GUI 操作。
6. 回归旧功能和安装失败路径，补充用户说明与已知限制。

## 验证与验收

自动化测试聚焦版本解析、架构筛选、路径约束、冲突检测、重复安装幂等性、保留首次备份、取消/失败恢复、用户修改保护、共享 layer 引用和旧记录迁移。文件操作测试使用临时目录，注册操作使用可替换接口，不触碰真实模拟器安装。

实现 Rust 改动后执行 `cargo fmt`、宿主 `cargo check`、Windows target `cargo check --target x86_64-pc-windows-msvc`，解决全部错误和警告；从 Windows 验证 macOS 时按仓库要求使用 `cargo zigbuild`。前端执行现有 `type-check` 和 `build-only` 脚本。

验收清单：

- [ ] 「实验性功能 → ReShade / DLSS」入口可见，页面、直接路由和刷新正常。
- [ ] 正确区分多个模拟器安装、图形 API 和组件归属。
- [ ] ReShade 能在选定 Vulkan/OpenGL 模拟器中真实加载。
- [ ] 外部 DLL、已有 preset 和用户 shader 不被静默覆盖或删除。
- [ ] 更新、取消、网络失败、文件锁定、提权拒绝后状态可诊断且可恢复。
- [ ] 卸载单个目标不会破坏其他目标的 Vulkan layer。
- [ ] Feeder 的“文件已部署”和“运行验证通过”分别展示；没有兼容性证据时不标记支持。
- [ ] 卸载后恢复原文件和受管理配置，旧模拟器下载/更新功能通过回归。

## 调研参考

以下为前一轮源码调研入口，均为可变的 main 分支或上游页面。实施时记录实际提交与版本，并重新核实下载及兼容条件。

- [RHI ReShade 下载与解包](https://github.com/RankFTW/RHI/blob/main/RenoDXCommander/Services/ReShadeUpdateService.cs)
- [RHI ReShade 部署](https://github.com/RankFTW/RHI/blob/main/RenoDXCommander/Services/AuxInstallService.Install.cs)
- [RHI 神经渲染安装编排](https://github.com/RankFTW/RHI/blob/main/RenoDXCommander/DetailPanelBuilder.NeuralRendering.cs)
- [RHI addon 下载与恢复](https://github.com/RankFTW/RHI/blob/main/RenoDXCommander/Services/Renodx5AddonService.cs)
- [RHI DLSS 版本清单](https://github.com/RankFTW/RHI/blob/main/dlss_manifest.json)
- [RHI 组件来源清单](https://github.com/RankFTW/RHI/blob/main/manifest.json)
- [DLSS5 Feeder 原理、安装与限制](https://github.com/jlrouzies-fr/DLSS5-Feeder)

采用独立 Rust 实现；若要直接移植 RHI 代码，先核对其 GPL-3.0 与本项目许可及分发方式是否兼容。

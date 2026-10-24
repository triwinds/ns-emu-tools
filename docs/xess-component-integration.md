# 独立 XeSS 组件与工具界面（2026-10-17）

2026-10-18 三分包调整：当前交付改为公共启动器与两个独立外部运行库包。以下 `xess-component.exe`、`xess-package` 和单 ZIP 发布描述保留作历史记录；当前目录、下载校验及构建命令以[三分包说明](../src-tauri/tools/graphics-components/README.md)为准。XeSS 图形层仍独立编译。

已将 Native AA 与 FG 合并为独立 XeSS 组件，支持分别启用和同时启用。
没有加入 XeSS SR，也没有接入 DLSS、Streamline SDK 或外部光流。

2026-10-18 补充：以上说明仍适用于独立 XeSS 组件。另有显式
`hybrid-xess-fg` 构建特性，在 DLSS 页的 FG 高级设置中选择 XeSS FG，
可将 DLSS SR／DLAA、可选 NR 的输出交给现有 XeSS FG。混合模式使用两个
已校验组件包，固定 2×，保留 XeSS 的零运动向量与固定深度，关闭 DLSS FG
和 Reflex，使用 XeLL；切换引擎后须从 DLSS 页重新启动。独立 XeSS 组件的
构建隔离和配置不变。架构、当前 GPU 限制及实测范围见
[图形层 README](../src-tauri/crates/streamline-fg/README.md#experimental-dlss-sr--nr-with-xess-fg)。

## 用户入口

工具的“实验性功能 → 图形增强 → XeSS”提供：

- 独立组件校验、安装、卸载与 Vulkan x64 目标检查。
- Native AA、FG 的下次启动设置；效果区前的独立输入尺寸配置。
- 可选游戏文件，以及“以 XeSS 启动”按钮。
- 本次实际处理状态、源帧/SDK 提交速率、独立暂停和恢复。

图形增强菜单与目标选择不再以 NVIDIA 显卡作为入口条件。既有
“DLSS / NVIDIA 光流”页签仍由原 NVIDIA 能力检查控制，其组件与配置不变。
界面沿用 DLSS 面板的标题、三步流程、组件状态、启动区和纵向效果分区，
检测结果与版本放在“检测与安装详情”，更新和卸载放在“组件维护”。
Native AA、FG 与 DLSS 侧的 NR / SR / FG 开关均采用标准 Vuetify `v-switch`，
不使用 `inset` 胶囊样式。
安装状态、已保存设置、启动过程、实际 AA/FG 结果分别显示。
未在本次启动时加载的功能不能中途初始化；修改下次启动设置不会伪报为已生效。

## 组件与存储

本地配套目录：`src-tauri/target/release/xess-package`，13 个校验成员加清单。
当前本地组件版本 `local-xess-4667e3b88b6fc8f2`。

2026-10-18 退出状态修复：独立 XeSS 组件默认使用正常启动模式，界面和
smoke helper 同时显式传入 `--graphics-launch`，不再于模拟器退出后执行
仅识别 Streamline 帧事件的严格验收。正常关闭模拟器不会再触发
`no game presents`。退出状态优先采用模拟器自己的记录，启动器异常单独
报告；启动器提前退出不能证明模拟器已退出。默认/显式模式分别验证了
目标退出码 0 和非零场景，均通过；验证结果与本地交付哈希见
`src-tauri/tools/xess-component/evidence/exit-status-fix.json`。

2026-10-18：组件接入运行库仓库的独立 Release
`xess-7504af9236e203ac-v1`。点击安装时，优先校验已安装版本、EXE 旁配套
目录与独立下载缓存；缺少有效本地组件时自动下载。沿用工具配置的下载器、
代理、GitHub 镜像和进度/取消入口。主程序固定版本、下载 URL、归档 SHA-256
及逐文件 SHA-256；全部校验通过后才发布缓存并安装。失败或取消不发布半成品，
被修改的旧缓存保留为 `.damaged-*`，不影响 DLSS 的组件与设置。

`src-tauri/tools/xess-component/release.py` 在 `package.py` 之后生成发布资产，
同时更新主程序的 `xess-release.json`。发布包含运行库 ZIP、对应源码 ZIP、
清单、许可证与 SHA256SUMS。本地仍按原约定保留 EXE 旁的配套组件目录。

```text
NsEmuTools.exe
xess-package/
  xess-component.exe
  xess-layer.dll
  libxess.dll
  bin/libxess_fg.dll
  bin/libxell.dll
  D3D12/{D3D12Core.dll,d3d12SDKLayers.dll,D3D12StateObjectCompiler.dll}
  licenses/{Intel、Microsoft、项目许可证与第三方声明}
  package.json
streamline-fg-package/       原配套组件保持原样
```

安装后使用工具配置目录下的 `graphics/xess/runtime/<版本>`；设置位于
`graphics/xess/settings.json`，会话记录位于 `graphics/xess/sessions`，
会话指针位于 `graphics/xess/live`。不复用 DLSS 的存储目录或用户设置。
默认 Native AA 开、FG 关，输入为原始尺寸（100%、长边上限 0）。
旧设置只迁移 AA/FG 开关，退休的 `fgMaxEdge` 不再施加第二次缩小。

### 与 DLSS 对齐的输入缩放

两边共用 `InputSizingControls.vue`：按输入比例 50～100%、固定长边上限
320～8192、恢复原始输入尺寸，以及实时状态与错误提示。XeSS 的设置与
控制文件独立；没有复用 DLSS 的配置存储、Streamline 或光流运行时。

Vulkan 尺寸计算也共用 `input_dimensions::resized`，保持宽高比、不放大、
缩小结果至少 320 × 180。XeSS 链路是先缩小输入，再以该尺寸执行 Native AA，
最后恢复到原始窗口尺寸交给 FG。Native AA 仍使用同尺寸输入/输出，普通
线性缩放负责恢复窗口尺寸，未加入 XeSS SR。关闭 AA 后也可单独缩放。
FG 接收完整窗口尺寸，没有额外的 640／2560 限制。

保存后可实时应用：新版会话通过独立的 `xess-input-control.json` 传递配置
和递增 revision，GPU 完成切换后在实际状态中反馈尺寸及 appliedRevision。
无运行会话时保存供下次启动使用；旧会话须重新启动新版组件一次。
尺寸不可用或过小时保留上次可用输入，并持续反馈错误，直到配置恢复有效。

主程序内嵌可信清单，安装、启动、卸载均校验成员和哈希；目录中的额外文件、
被修改的文件和链接路径会阻止操作。运行中的启动器独立持有组件文件的
只读共享句柄，工具退出后仍保持保护。卸载先取得所有成员的删除访问权限，
再删除已确认归属的文件；保留设置、会话和其他组件。

## 执行路径

`xess-component` Cargo 特性组合 Native AA 与 FG，并禁止与 `sdk-bridge`
混合。启动宿主来自原通用 layer 工程，交付文件已使用独立 XeSS 名称。
启动 EXE 负责目标校验、进程级 Vulkan manifest 和生命周期；算法在模拟器
进程中的 layer 与 Intel DLL 内执行。没有全局注册 Vulkan layer。

- 仅 AA：Vulkan 交换链 → 可选输入缩放 → XeSS Native AA → 恢复窗口尺寸 → 原生 present。
- 仅 FG：Vulkan 完成帧 → 可选输入缩放/恢复窗口尺寸 → 同显卡共享纹理/fence → D3D12 XeSS FG → 原窗口。
- AA + FG：先缩小输入并执行 AA，恢复窗口尺寸后把处理后的颜色送入 FG；原始 present
  等待信号量由 AA 消费一次，FG 不重复等待，隐藏原生交换链完成帧回收。

联合创建设备时，保留模拟器与 Intel SDK 所需的 Vulkan 特性，再添加 FG
的外部内存和 timeline 要求。Intel 的 mutable descriptor 特性节点使用已知
结构大小复制；未知节点继续拒绝，避免按错误大小复制。

启动环境清除继承的图形增强变量，单独配置 XeSS；不传入 Streamline、
NVOF、DLSS SR/FG 选项。运行状态通过本会话的 `xess-status.json` 更新，
5 秒未更新便不视为已确认运行。控制文件仅定位到工具登记的 XeSS 会话。

## 实测与验证

AMD Radeon 610M，Nextendo 1.9.4，《异度神剑 3》实际版本 1.0.0：

- `build/xess-component-20261017/combined-002`：进入可操作战斗场景，
  验证 AA+FG、仅 FG、两项暂停、仅 AA、恢复 AA+FG。记录 7895 次 AA
  实际处理成功、8252 次 FG 返回成功且提交 2 帧；包含菜单与切换阶段，
  不是固定场景性能基准。无 AA/FG failure 事件，正常退出 code 0。
  此轮组件版本为 `local-xess-a8f920ce66f3f4d1`。
- 工具界面实际选择游戏并启动 AA-only，会话
  `game-1c9b26ae-842f-4b0f-b889-1049ef6ab622`：页面显示 AA 正在处理、
  FG 未插帧；界面暂停/恢复 AA 与后台结果一致。保存下次启用 FG 后，
  当前会话仍报告 FG 关闭。关闭工具后游戏继续处理，随后游戏正常退出。
- `fg-only-final`：AA 未加载，FG 成功提交生成帧，原窗口正常出图，
  正常退出 code 0，设备和实例剩余数量为 0。此项取样止于启动提示阶段。
- 最终组件 `abi-final`：Vulkan 回调明确使用 `extern "system"`，以 Vulkan
  PFN 类型常量在编译期约束 ABI；AMD 同时执行 AA 与 FG，状态均确认生效，
  原窗口出图且正常退出 code 0。此项取样止于启动提示阶段。
- 联合模式模块清单只有 `xess-layer.dll`、`libxess.dll`、`libxess_fg.dll`、
  `libxell.dll`；FG-only 清单只有 layer、FG、XeLL，无 Streamline/NGX/NVOF。
- 新版最终主程序与配套组件、已部署 runtime 的清单和 13 个文件哈希一致。
  原始输入为默认设置；旧配置兼容迁移。界面操作由用户按 Esc 停止，未继续最终界面复核。
  模拟器临时 AMD 选择已恢复为 `null`，正式 XeSS 启动不写入显卡型号。
- 新版 `input-sizing-both`（组件 `local-xess-7504af9236e203ac`）：AMD 启动提示阶段验证长边 640、75% 比例、恢复 100% 原始输入及独立 AA/FG 暂停。窗口从 1200 × 656 变化至 2560 × 1391 后，FG 均保持对应完整窗口尺寸；缩小输入低于 320 × 180 时拒绝并保留前一有效尺寸。正常退出 code 0，无 AA/FG failure 事件。尺寸与 revision 状态保存在该会话 `state-*.json` 中。
- 原 `streamline-fg-package` 的 23 个成员全部匹配原清单，0 差异。

后端安装管理 7 项测试通过（篡改/额外文件保留、路径隔离、活动文件保护、
设置拒绝 SR 等），统一 layer 105 项单测通过。主程序、统一组件和原
native-nr 路径的宿主/显式 Windows target cargo check 均通过，无警告；
cargo fmt、前端类型检查、新增面板/图形页的 ESLint 和 Tauri `--no-bundle`
构建通过。

最终 `NsEmuTools.exe` 和配套成员哈希记录在
`src-tauri/tools/xess-component/evidence/integration.json`；
完整日志保留在上述 build 会话和本地工具的 XeSS 会话目录。

## 复现构建

先按现有 `xess-vulkan-diagnostics/prepare-sdk.ps1` 与
`xess-fg-diagnostics/prepare-sdk.ps1` 准备固定哈希的 Intel / Agility 文件。
随后从仓库根目录执行：

```powershell
cargo build --locked --release --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --target-dir src-tauri/target/xess-component --features xess-component --lib --bin streamline-layer-probe
python src-tauri/tools/xess-component/package.py
# package.py 会更新主程序内嵌清单，必须在主程序构建前运行。
Set-Location src-tauri
bun ../frontend/node_modules/@tauri-apps/cli/tauri.js build --no-bundle -- --offline
```

组件脚本校验 Intel / Agility 哈希和独立构建标记，保留许可证；活动文件会
阻止覆盖。正常交付保留 EXE 旁的 `xess-package` 及原图形组件目录，
不生成安装器、ZIP 或 `output` 下的新交付目录。

## 当前边界

Native AA 无 jitter、零运动向量、逐帧重置历史，不能等同完整时域 AA。
FG 使用零运动向量、深度 0.5、单位矩阵和无 UI mask，尚未验收运动画质。
前置输入缩放和恢复窗口尺寸属于普通线性缩放，不是 XeSS SR；FG 始终使用
完整窗口尺寸。输入比例同时影响文字和细线，实际画质仍需按场景判断。

XeLL 只是 FG 必需依赖；present hook 的 marker 不构成真实游戏输入/模拟
低延迟接入。SDK 提交 FPS 不是实测屏幕扫描帧率。当前仍限单交换链 present、
SDR RGBA8/BGRA8、图形队列，缺少完整设备丢失恢复。本轮未对游戏 layer
开启 Vulkan validation，也没有声称 D3D12 debug layer 验证通过。

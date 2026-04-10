# 应用自更新改造计划（ZIP 优先 + macOS 便携更新）

本文档用于规划 NS Emu Tools 当前自研 portable 更新器的下一轮改造，目标是在保留 Windows 现有 portable 更新模式的前提下，为 macOS 增加一套类似的应用自更新方案，并同步调整 CI / release 产物。

## 1. 背景与现状

当前仓库的应用自更新链路具有以下特征：

- `src-tauri/src/services/updater.rs` 明确按 portable 模式设计，只完整支持 Windows。
- `src-tauri/src/repositories/app_info.rs` 与 `src-tauri/src/services/updater.rs` 的 release 资产选择逻辑目前只识别 Windows `.exe`。
- `src-tauri/src/services/updater.rs` 当前安装逻辑只支持 `.zip` / `.7z` / `.exe`，非 Windows 平台直接返回“不支持”。
- `ci-build.yaml` 目前只发布 Windows 裸 `exe` 和 macOS `.app` 的 zip 包。
- `manual-build.yml` 目前只产出 Windows 裸 `exe`。

这意味着：

- Windows 已有“下载更新包 -> 解压/替换 -> 重启”的 portable 更新模型。
- macOS 目前只有 release 资产，没有“应用自身更新”的安装闭环。
- release 资产没有统一的“ZIP 优先，平台原生包兜底”规则。

## 2. 目标

本轮计划目标如下：

1. 保留现有自研 portable 更新器，不切换到 Tauri 官方 updater。
2. 为 macOS 增加一套类似 Windows 的应用自更新方案。
3. 调整 release 产物策略：
   - Windows 同时上传 portable zip 和裸 `exe`。
   - macOS 同时上传 zip 包和 app bundle 兜底包。
4. 更新时按当前平台优先下载 zip 包。
5. 如果 release 中没有当前平台对应的 zip 包，再退回下载平台原生兜底资产：
   - Windows: `exe`
   - macOS: app bundle 归档包
6. 尽量保持前端 API 不变，把资产优先级和安装差异收敛在 Rust 后端。

## 3. 非目标

本轮不做以下事情：

- 不接入 Tauri 官方 updater。
- 不改成安装器分发模型。
- 不引入 Windows MSI / NSIS。
- 不把 DMG 纳入应用自更新主链路。
- 不在本轮处理 Linux 自更新。

## 4. 发布资产约定

### 4.1 目标资产矩阵

| 平台 | 资产类型 | 用途 | 优先级 |
| ------ | ---------- | ------ | -------- |
| Windows | `NsEmuTools-windows-portable.zip` | 自更新主资产 | 1 |
| Windows | `NsEmuTools.exe` | 自更新兜底资产 | 2 |
| macOS | `NS-Emu-Tools-macos-app.zip` | 自更新主资产 | 1 |
| macOS | `NS-Emu-Tools-macos-app.tar.gz` | 自更新兜底资产 | 2 |

### 4.2 资产内容约定

Windows zip：

- 应包含完整 portable 分发目录，而不只是单个 `exe`。
- 如果当前发布布局只有 `NsEmuTools.exe`，也允许 zip 只包含该文件，但计划上应保留扩展空间，以兼容未来可能出现的 `_internal`、资源目录或 sidecar 文件。

macOS zip：

- 应包含 `NS Emu Tools.app` bundle。
- 保持当前 `ditto -c -k --sequesterRsrc --keepParent` 的压缩方式。

macOS app 兜底包：

- GitHub Release 无法直接上传目录，因此“app 包”在 release 侧应实现为包含 `.app` bundle 的单文件归档。
- 计划默认采用 `.app.tar.gz` 作为 app bundle 兜底资产，以保留 bundle 结构和 Unix 权限。

## 5. 更新选择规则

### 5.1 平台优先级

Windows：

1. 匹配 Windows portable zip。
2. 如果不存在，再匹配 Windows `exe`。

macOS：

1. 匹配 macOS zip。
2. 如果不存在，再匹配 macOS app bundle 归档包。

### 5.2 选择原则

- 只在当前平台内做优先级选择，不允许误选其他平台资产。
- 优先用明确命名匹配，保留后缀和关键词兜底匹配，避免历史 release 命名差异导致完全失效。
- “找不到 zip 再回退”是资产选择阶段的规则，不在本轮强制实现“zip 下载失败后自动重试 app/exe”。

### 5.3 建议的数据结构调整

建议把当前只面向 Windows 的资产查找逻辑升级为通用的“自更新资产分类器”，例如：

- `ReleaseAssetKind`
  - `WindowsPortableZip`
  - `WindowsExe`
  - `MacosZip`
  - `MacosAppArchive`
  - `Unknown`
- `SelfUpdateAssetSelection`
  - `primary`
  - `fallback`

这样 `check_update()` 和 `update_self_by_tag()` 可以共用同一套选择逻辑，而不是各自硬编码平台分支。

## 6. 后端改造计划

### 6.1 `src-tauri/src/models/release.rs`

计划改造：

- 保留 `ReleaseAsset` / `ReleaseInfo` 基础结构。
- 新增通用自更新资产分类方法，替代当前 `find_windows_asset()` 的单平台实现。
- 为当前平台提供统一入口，例如：
  - `find_self_update_asset_for_current_platform()`
  - `find_self_update_fallback_asset_for_current_platform()`
  - 或一次性返回 primary / fallback 的方法。

建议兼容的匹配特征：

- Windows zip：名称包含 `windows`、`portable`、`.zip`
- Windows exe：名称等于或近似 `NsEmuTools.exe`
- macOS zip：名称包含 `macos`、`.zip`
- macOS app 兜底包：名称包含 `macos` 且后缀为 `.tar.gz`

### 6.2 `src-tauri/src/repositories/app_info.rs`

计划改造：

- `check_update()` 不再调用 `find_windows_asset()`。
- 改为根据当前平台拿到主资产和可选兜底资产。
- `UpdateCheckResult` 建议增加以下字段，便于日志和前端展示：
  - `download_asset_name`
  - `download_asset_type`
  - `fallback_download_url`
  - `fallback_asset_name`

兼容策略：

- 若不想立即改前端，可先保留 `download_url`，让其始终指向“当前平台的首选资产”。
- fallback 信息可以先只在后端内部使用，后续再决定是否透出给前端。

### 6.3 `src-tauri/src/services/updater.rs`

计划改造：

- `install_update()` 扩展为跨平台安装入口，不再只有 Windows 分支。
- `update_self_by_tag()` 与 `download_update()` 使用统一的资产选择逻辑（当前两处均硬编码 `find_windows_asset()`，需统一改为平台感知选择）。
- 根据平台和资产格式走不同安装路径：

Windows：

1. `.zip`：解压到 staging 目录。
2. 查找新版本 portable 根目录或 `exe`。
3. 复用现有 `update.bat` 风格脚本替换当前目录内容。
4. 如果 zip 不存在，则回退到 `exe` 直装逻辑。

macOS：

1. `.zip`：解压到 staging 目录，递归找到 `.app`。
2. `.tar.gz`：解压到 staging 目录，递归找到 `.app`。
3. 识别当前运行中的 `.app` bundle 路径。
4. 生成独立 shell 脚本，在主进程退出后执行替换。
5. 替换完成后执行：
   - `xattr -r -d com.apple.quarantine`
   - `chmod 755 <app>`
   - `chmod +x <app>/Contents/MacOS/<binary>`
6. 使用 `open` 重启新版本应用。

建议新增内部辅助函数：

- `install_update_windows_zip()`
- `install_update_windows_exe()`
- `install_update_macos_zip()`
- `install_update_macos_app_archive()`
- `create_macos_update_script()`
- `find_current_macos_app_bundle()`

### 6.4 `src-tauri/src/utils/archive.rs`

现有可复用能力：

- `extract_zip()`
- `extract_tar_gz()`
- `extract_7z()`
- `extract_tar_xz()`

计划改造：

- 抽出“从目录中递归查找 `.app`”的通用公开 helper，避免 updater 重新实现一份搜索逻辑。
- updater 路径优先使用“解压到 staging，再统一替换”的方式，而不是直接把 tar.gz 解到目标目录，这样更接近 Windows 当前的 staged update 模型。

### 6.5 `src-tauri/src/utils/platform.rs`

现有可复用能力：

- `get_macos_bundle_executable_path()`
- `finalize_macos_app_install()`

计划改造：

- 增加“由当前进程反推 `.app` 根目录”的 helper。
- 将 shell 脚本和 Rust 侧都需要的 macOS bundle 后处理逻辑抽成更通用的步骤定义，避免权限/去隔离逻辑出现两套实现长期漂移。

## 7. macOS 更新流程设计

### 7.1 路径识别

当前 macOS 更新不能沿用 `current_exe.parent()` 作为安装根目录，因为该路径位于 `.app/Contents/MacOS/` 内部。

计划改为：

1. 从 `std::env::current_exe()` 出发向上遍历祖先目录。
2. 找到第一个以 `.app` 结尾的目录作为当前应用 bundle 根目录。
3. 以该 bundle 的父目录作为替换目标目录。

### 7.2 替换步骤

建议流程：

1. 下载更新包到现有 `download/upgrade_files` 目录。
2. 解压到 `download/upgrade_files_extracted` staging 目录。
3. 在 staging 中递归找到新的 `NS Emu Tools.app`。
4. 生成 `update.sh` 到临时目录或应用数据目录。
5. `update.sh` 等待主进程退出后：
   - 备份旧 `.app` 为 `.app.bak`
   - 使用 `ditto` 复制新 `.app` 到目标目录（ditto 默认保留资源分支和扩展属性）
   - 执行 `xattr` / `chmod`
   - 清理 staging、下载目录和旧备份
   - 使用 `open` 启动新应用
6. Rust 侧触发脚本后退出当前进程。

### 7.3 失败处理

若遇到以下场景，脚本应输出明确错误并停止：

- 当前 `.app` 根目录无法识别
- 新版本 `.app` 未找到
- 目标目录没有写权限
- `ditto` 复制失败
- 重启失败

建议保留旧版本 `.app.bak`，只有在新版本成功启动后再考虑清理；首版实现可以先在“复制成功后立即清理旧备份”和“保留一次备份”之间二选一，优先选择更安全的保留策略。

## 8. CI / Release 改造计划

### 8.1 `/.github/workflows/ci-build.yaml`

Windows job：

- 保留现有 `NsEmuTools.exe` 构建。
- 新增 portable zip 打包步骤。
- 上传两个 artifact：
  - `NsEmuTools-windows-exe`
  - `NsEmuTools-windows-zip`

macOS job：

- 保留现有 `.app` 构建。
- 保留当前 zip 打包步骤。
- 新增 app bundle 兜底归档步骤，建议产出：
  - `NS-Emu-Tools-macos-app.tar.gz`
- 上传两个 artifact：
  - `NsEmuTools-macos-zip`
  - `NsEmuTools-macos-app-archive`

release job：

- 在 tag release 时统一上传以下资产：
  - Windows zip
  - Windows exe
  - macOS zip
  - macOS app bundle 归档

### 8.2 `/.github/workflows/manual-build.yml`

计划改造：

- Windows 手动构建也补齐 zip 打包。
- 保持当前 workflow 只跑 Windows 也可以，但产物要与正式 release 的 Windows 资产契约一致。
- 如果后续需要人工验证 macOS 自更新，再考虑增加 `macos-latest` 手动构建矩阵。

### 8.3 命名与兼容策略

建议：

- 尽量保持现有 macOS zip 名称不变，降低历史兼容成本。
- 新增资产时使用稳定、可预测的名称，避免选择器依赖过多模糊匹配。
- 更新器内部仍应保留后缀和关键词兜底匹配，兼容旧 release。

## 9. 前端影响

前端目标是最小化修改。

计划原则：

- `frontend/src/stores/ConfigStore.ts`
- `frontend/src/components/NewVersionDialog.vue`
- `frontend/src/utils/tauri.ts`

尽量不改交互流程，只改日志或展示信息：

- 如后端新增 `download_asset_name` / `download_asset_type`，前端可在调试日志中打印。
- “自动更新”按钮仍然只调用现有后端命令。

## 10. 测试计划

### 10.1 Rust 单元测试

新增或扩展测试覆盖：

- Release 资产分类
- 平台优先级选择
- 找不到 zip 时的 fallback 选择
- macOS `.app` 路径识别
- macOS update shell 脚本内容生成
- zip / tar.gz 解压后 `.app` 定位

### 10.2 手工验证矩阵

Windows：

- 仅存在 zip 资产
- zip 缺失，仅存在 exe
- zip 中包含单个 exe
- zip 中包含额外资源目录

macOS：

- 仅存在 zip 资产
- zip 缺失，仅存在 app bundle 归档
- 应用位于用户目录
- 应用位于 `/Applications`
- bundle 带 quarantine 属性
- 更新后可正常重启

### 10.3 回归点

- 旧版本 release 只有 `exe` 时，Windows 更新不应退化。
- 现有 release 只有 macOS zip 时，macOS 新链路应可直接消费。
- `effective_config_dir()` 现有配置读写行为不应被本轮更新改造破坏。

## 11. 分阶段实施建议

### Phase 1：资产契约与选择逻辑

- 新增通用资产分类器
- `check_update()` / `update_self_by_tag()` 改为平台感知选择
- 先打通“ZIP 优先，fallback 次之”的选择链路

### Phase 2：Windows zip 正式化

- CI 产出 Windows zip
- updater 明确把 Windows zip 视为主路径
- 保留 exe fallback

### Phase 3：macOS staged update

- 新增 `.app` 路径识别
- 新增 shell 脚本替换流程
- 支持 macOS zip 和 `.app.tar.gz`

### Phase 4：CI / release 收口

- 更新 `ci-build.yaml`
- 更新 `manual-build.yml`
- 校验 release 资产命名和上传列表

### Phase 5：测试与发布验证

- 补充单元测试
- 在真实 macOS 环境验证一次从旧版本到新版本的更新

## 12. 风险与注意事项

1. GitHub Release 不能直接上传目录，因此 macOS “app 包”必须落成单文件归档，而不是裸 `.app` 目录。
2. macOS 如果应用位于无写权限目录，自动更新会失败，需要给出明确提示。
3. 如果用户从带 quarantine 的 zip 直接启动旧版本，可能出现 App Translocation；需要把这一场景纳入验证。
4. Windows zip 的内容边界必须明确，否则未来新增 sidecar 文件时可能出现“zip 更新后比 exe 兜底反而不完整”的问题。
5. 更新器不应依赖前端传入平台类型，平台判断应始终由 Rust 后端完成。

## 13. 建议的首批实施文件

首批预计涉及：

- `src-tauri/src/models/release.rs`
- `src-tauri/src/repositories/app_info.rs`
- `src-tauri/src/services/updater.rs`
- `src-tauri/src/utils/archive.rs`
- `src-tauri/src/utils/platform.rs`
- `.github/workflows/ci-build.yaml`
- `.github/workflows/manual-build.yml`

前端是否需要跟进字段展示，等 Rust 侧接口定稿后再决定。

# 为 Citron / Eden 下载安装添加 macOS 支持（实现规划）

## 背景与现状

- 当前 Citron / Eden 的下载安装流程主要按 Windows 资源包/可执行文件组织方式实现（如 `*.exe`、Windows zip/7z 资源筛选、MSVC 运行库检查等）。
- Rust(Tauri) 侧 `YuzuConfig.yuzu_path` 已将 macOS 默认值设为 `~/yuzu`，但实际下载资源筛选、解压与安装、版本检测等流程仍偏向 Windows 逻辑。

## 目标

- 在 macOS 上支持 Citron 与 Eden 的：
  - 自动匹配正确的 Release 资源（按 OS/CPU 架构选择）。
  - 下载、解压/挂载、安装到指定位置（默认路径可用且可落地）。
  - 安装后的基础可用性校验（至少能定位到 `.app` 或可执行入口）。

## 非目标（本规划不强制覆盖）

- 不承诺实现应用签名/公证（notarization）流程；仅保证从上游下载的产物能被正确落盘与启动（受 Gatekeeper 影响时提供提示/降级方案）。

## 关键决策点

1. **安装目标路径与权限**
   - 推荐默认安装到 `~/yuzu`（用户主目录下），避免 `/Applications` 需要管理员权限导致安装失败。
   - 如果用户明确选择 `/Applications`，需要在 UI/日志里提示可能需要授权（或让用户改选可写目录）。
   - 若用户自定义路径写入失败，提示用户检查权限或选择其他目录。

2. **资源包格式**
   - Eden 提供 `tar.gz` 格式（解压后包含 `.app`）。
   - Citron 提供 `dmg` 格式（挂载后复制 `.app`）。
   - 方案：需同时支持 `tar.gz` 解压和 `dmg` 挂载两种安装方式。

3. **多架构支持（Apple Silicon / Intel）**
   - 当前上游 Eden 和 Citron 的 macOS 包均为通用包，不区分架构。
   - 无需在筛选时处理架构匹配逻辑。

4. **多分支并存（Citron 与 Eden 同时安装）**
   - 当前逻辑存在“删除所有可执行文件”的倾向；macOS 上不应误删另一个 `.app`。
   - 方案：改为“只替换目标分支对应的 App 包”（例如 `Eden.app` / `Citron.app`），不动其他应用。

## 技术方案（Rust/Tauri 主链路）

### 1) 平台识别

- 新增统一的 `Platform` 结构：`os`（windows/macos/linux）、`arch`（x86_64/aarch64）。
- 使用 `std::env::consts::{OS, ARCH}` 或 `cfg!(target_os = "...")` 进行判定。

### 2) Release 资源筛选规则

在 `src-tauri/src/services/yuzu.rs` 的下载资源选择处引入"按平台匹配"的筛选器：

- **Eden 筛选规则**：
  - 查找文件名包含 `macOS`（区分大小写）的资源
  - 预期格式为 `.tar.gz`
  - 示例匹配：`Eden-macOS-v0.0.4-rc3.tar.gz`

- **Citron 筛选规则**：
  - 查找文件名包含 `macOS`（区分大小写）的资源
  - 预期格式为 `.dmg`
  - 示例匹配：`Citron-macOS-stable-01c042048.dmg`
  - 若无匹配资源，返回明确错误提示（部分旧版本可能没有 macOS 包）

> 注：Eden 来自 GitHub Release，使用 `macOS` 关键字匹配；Citron 来自 Forgejo Release，同样使用 `macOS` 关键字匹配。

### 3) 解压/挂载与安装流程

#### 3.1 tar.gz（Eden 使用此格式）

- 复用现有 `uncompress()`，解压到临时目录。
- 在临时目录中查找目标 `.app`（如 `Eden.app`）。
- 复制到目标目录：
  - 建议用外部命令 `ditto`（更可靠地保留 macOS bundle 的权限/资源 fork/扩展属性），或引入专门的目录复制实现以保留权限与符号链接。
  - 采用"临时目录 -> 原子替换"的策略：先复制到 `Target.app.tmp`，再替换 `Target.app`，避免半安装状态。

#### 3.2 dmg（Citron 使用此格式）

- 在 `src-tauri/src/utils/archive.rs` 增加 `extract_dmg()`（或新增 `src-tauri/src/services/macos_dmg.rs`）：
  - `hdiutil attach <dmg> -nobrowse -readonly -mountpoint <tmp_mount>`
  - 扫描 `<tmp_mount>` 内的 `.app`，复制到目标目录（同上）。
  - `hdiutil detach <tmp_mount>`
- 对失败场景做清理兜底：确保 detach 与临时目录删除在 `drop/cleanup` 路径执行。

#### 3.3 Gatekeeper/quarantine 处理（可选但建议）

- 若用户反馈“已损坏/无法打开”，可在安装完成后提供可选操作：
  - `xattr -dr com.apple.quarantine <Target.app>`
  - 或在 UI 提示用户到系统设置允许（不默认自动执行，避免安全争议）。

### 4) 配置与 UI 交互调整

- `yuzu_path` 在 macOS 上语义从"exe 所在目录"转为"安装目录（容纳 *.app）"仍可行，但需要：
  - UI 允许选择目录（而非单个文件）。
  - 后端安装时以 `yuzu_path/{Eden.app|Citron.app}` 作为最终目标。
- 推荐默认路径为 `~/yuzu`；若使用 `/Applications` 需考虑权限/授权提示。

### 5) 运行环境检查步骤改造

- `check_and_install_msvc()` 仅 Windows 适用：
  - macOS 下将该步骤改为“跳过（Success）”或替换为轻量校验（例如确认 `.app` 存在/可读）。

## 任务拆分与里程碑

1. **资源筛选器落地**
   - 实现 `select_macos_asset(release_info, branch)` 函数
   - Eden：匹配包含 `macOS` 且以 `.tar.gz` 结尾的文件
   - Citron：匹配包含 `macOS` 且以 `.dmg` 结尾的文件
   - 为筛选器添加离线单元测试（使用本地 fixture JSON，避免测试依赖网络）。
2. **macOS 安装链路**
   - tar.gz（Eden）：解压 -> 定位 `.app` -> 复制/替换 -> 清理。
   - dmg（Citron）：挂载 -> 定位 `.app` -> 复制/替换 -> 卸载 -> 清理。
3. **路径配置**
   - 默认路径设为 `~/yuzu`，在前端和配置中正确显示。
4. **回归与验证**
   - macOS 上安装 Eden/Citron 各 1 个版本：首次安装、覆盖安装、路径不可写场景。
   -（可选）补充 GitHub Actions `macos-latest` 的最小编译/单测任务，避免后续回归。

## 代码改动点（预估）

- `src-tauri/src/services/yuzu.rs`：平台识别、资源筛选、macOS 安装分支、替换 MSVC 检查逻辑。
- `src-tauri/src/utils/archive.rs`：新增 dmg 处理入口（或新模块实现挂载/卸载）。
- `src-tauri/src/config.rs`：macOS 默认路径设为 `~/yuzu`。
- `frontend/src/pages/yuzu.vue`（如需要）：路径选择与提示文案适配 macOS。

## 风险与应对

- 上游资源命名变更：采用"关键词评分 + 容错"而非固定前缀，降低脆弱性。
- 路径权限问题：默认使用 `~/yuzu`（用户目录），避免系统目录权限问题。
- `.app` 复制不完整导致无法运行：优先使用 `ditto`，并用结构校验（`Contents/Info.plist`）验证复制结果。
- Gatekeeper 拦截：提供明确提示与可选的 quarantine 清理操作（默认不自动执行）。

## 附录：Release 获取接口说明

### Eden（GitHub Release）

- **API 地址**: `https://api.github.com/repos/eden-emulator/Releases/releases`
- **获取指定版本**: `https://api.github.com/repos/eden-emulator/Releases/releases/tags/{version}`
- **解析方式**: 使用 `ReleaseInfo::from_github_api()` 解析

**macOS 资源命名规则**:
- 格式: `Eden-macOS-{version}.tar.gz`
- 示例: `Eden-macOS-v0.0.4-rc3.tar.gz`
- 特点: 不区分架构，只有一个通用包

**其他平台命名参考**:
- Windows: `Eden-Windows-{version}-{arch}-{compiler}.zip`（如 `Eden-Windows-v0.0.4-rc3-amd64-msvc-standard.zip`）
- Linux: `Eden-Linux-{version}-{arch}-{compiler}.AppImage`
- Android: `Eden-Android-{version}-{variant}.apk`

### Citron（Forgejo Release）

- **API 地址**: `https://git.citron-emu.org/api/v1/repos/Citron/Emulator/releases`
- **获取指定版本**: `https://git.citron-emu.org/api/v1/repos/Citron/Emulator/releases/tags/{version}`
- **解析方式**: 使用 `ReleaseInfo::from_forgejo_api()` 解析

**macOS 资源命名规则**:
- 格式: `Citron-macOS-stable-{commit_hash}.dmg`
- 示例: `Citron-macOS-stable-01c042048.dmg`
- 特点: 不区分架构，只有一个通用包（dmg 格式）
- 注意: 部分旧版本可能没有 macOS 资源

**其他平台命名参考**:
- Windows: `Citron-windows-{variant}-{commit_hash}-x64.zip`（如 `Citron-windows-stable-01c042048-x64.zip`）
- Linux: `citron_stable-{commit_hash}-linux-{arch}.AppImage`
- Android: `app-mainline-release.apk`

### macOS 资源筛选策略

根据实际 Release 资源分析，macOS 筛选逻辑如下：

**Eden**:
1. 查找文件名包含 `macOS`（区分大小写）的资源
2. 预期格式为 `.tar.gz`
3. 当前无需处理架构区分（上游只提供单一通用包）

**Citron**:
1. 查找文件名包含 `macOS`（区分大小写）的资源
2. 预期格式为 `.dmg`
3. 当前无需处理架构区分（上游只提供单一通用包）
4. 需处理部分版本无 macOS 资源的情况（返回明确错误提示）

**通用排除规则**:
- 排除包含 `windows`、`Windows`、`linux`、`Linux`、`android`、`Android` 的文件
- 排除 `.zsync`、`.txt` 等非安装包文件


## 实现步骤

**重要**: 每实现一个步骤都需要在此文档中更新，标记相应步骤已完成，并简单描述实现方案。

### 第一阶段：平台识别与资源筛选

#### 步骤 1.1：创建平台识别工具模块 ✅ 已完成

**文件**: `src-tauri/src/utils/platform.rs` (新建)

**实现方案**: 已创建 platform.rs 模块，实现了 Platform 结构体及其相关方法，用于识别当前运行平台（OS 和 架构）。已在 utils/mod.rs 中导出。

```rust
//! 平台识别工具模块

use std::env::consts::{ARCH, OS};

/// 当前平台信息
#[derive(Debug, Clone, PartialEq)]
pub struct Platform {
    pub os: PlatformOS,
    pub arch: PlatformArch,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlatformOS {
    Windows,
    MacOS,
    Linux,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlatformArch {
    X86_64,
    Aarch64,
    Other(String),
}

impl Platform {
    /// 获取当前运行平台
    pub fn current() -> Self {
        let os = match OS {
            "windows" => PlatformOS::Windows,
            "macos" => PlatformOS::MacOS,
            "linux" => PlatformOS::Linux,
            _ => PlatformOS::Linux, // fallback
        };

        let arch = match ARCH {
            "x86_64" => PlatformArch::X86_64,
            "aarch64" => PlatformArch::Aarch64,
            other => PlatformArch::Other(other.to_string()),
        };

        Self { os, arch }
    }

    pub fn is_macos(&self) -> bool {
        matches!(self.os, PlatformOS::MacOS)
    }

    pub fn is_windows(&self) -> bool {
        matches!(self.os, PlatformOS::Windows)
    }
}
```

**操作**:
1. 创建 `src-tauri/src/utils/platform.rs`
2. 在 `src-tauri/src/utils/mod.rs` 中添加 `pub mod platform;`

---

#### 步骤 1.2：实现 macOS 资源筛选器 ✅ 已完成

**文件**: `src-tauri/src/services/yuzu.rs` (修改)

**实现方案**: 已实现 `select_macos_asset` 函数，支持 Eden (macOS + tar.gz) 和 Citron (macOS + dmg) 的资源筛选。已在 `download_yuzu` 函数中集成平台判断逻辑（使用 `cfg!(target_os = "macos")`）。

在 `download_yuzu` 函数中添加 macOS 资源筛选逻辑：

```rust
/// 选择 macOS 下载资源
fn select_macos_asset(release_info: &ReleaseInfo, branch: &str) -> Option<String> {
    for asset in &release_info.assets {
        let name = &asset.name;

        // 排除其他平台的文件
        let name_lower = name.to_lowercase();
        if name_lower.contains("windows") || name_lower.contains("linux") || name_lower.contains("android") {
            continue;
        }

        // Eden: 匹配 macOS + .tar.gz
        if branch == "eden" && name_lower.contains("macos") && name_lower.ends_with(".tar.gz") {
            return Some(asset.download_url.clone());
        }

        // Citron: 匹配 macOS + .dmg
        if branch == "citron" && name_lower.contains("macos") && name_lower.ends_with(".dmg") {
            return Some(asset.download_url.clone());
        }
    }
    None
}
```

**修改 `download_yuzu` 函数**:

```rust
pub async fn download_yuzu<F>(...) -> AppResult<PathBuf> {
    // ... 现有代码 ...

    // 查找下载 URL - 根据平台选择
    let download_url: Option<String> = if cfg!(target_os = "macos") {
        select_macos_asset(&release_info, branch)
    } else {
        // 现有的 Windows 筛选逻辑
        let mut url = None;
        for asset in &release_info.assets {
            // ... 现有 Windows 逻辑 ...
        }
        url
    };

    // ... 后续代码 ...
}
```

**操作**:
1. 在 `yuzu.rs` 中添加 `select_macos_asset` 函数
2. 修改 `download_yuzu` 添加平台判断

---

### 第二阶段：DMG 挂载与解压功能

#### 步骤 2.1：实现 DMG 挂载/解压模块 ✅ 已完成

**文件**: `src-tauri/src/utils/archive.rs` (修改)

**实现方案**: 已在 archive.rs 中添加三个 macOS 专用函数：
- `extract_dmg()`: 挂载 DMG 文件，查找并复制 .app 到目标目录
- `extract_and_install_app_from_tar_gz()`: 从 tar.gz 提取 .app 并安装
- `find_app_recursive()` 和 `find_app_in_dir()`: 查找 .app 的辅助函数
使用 `hdiutil` 命令挂载/卸载 DMG，使用 `ditto` 命令复制 .app 以保留权限和扩展属性。

添加 DMG 处理函数：

```rust
/// 挂载并提取 DMG 文件中的 .app (仅 macOS)
#[cfg(target_os = "macos")]
pub fn extract_dmg(dmg_path: &Path, target_path: &Path) -> AppResult<PathBuf> {
    use std::process::Command;

    info!("挂载 DMG: {} -> {}", dmg_path.display(), target_path.display());

    // 创建临时挂载点
    let mount_point = std::env::temp_dir().join(format!("dmg_mount_{}", std::process::id()));
    std::fs::create_dir_all(&mount_point)?;

    // 挂载 DMG
    let mount_result = Command::new("hdiutil")
        .args(["attach"])
        .arg(dmg_path)
        .args(["-nobrowse", "-readonly", "-mountpoint"])
        .arg(&mount_point)
        .output()?;

    if !mount_result.status.success() {
        let _ = std::fs::remove_dir_all(&mount_point);
        return Err(AppError::Extract(format!(
            "DMG 挂载失败: {}",
            String::from_utf8_lossy(&mount_result.stderr)
        )));
    }

    // 查找 .app（建议优先按预期 App 名称匹配，其次再兜底“找到第一个 .app”）
    let app_path = find_app_in_dir(&mount_point)?;
    let app_name = app_path.file_name()
        .ok_or_else(|| AppError::Extract("无法获取 .app 名称".to_string()))?;

    // 确保目标目录存在
    std::fs::create_dir_all(target_path)?;

    let target_app = target_path.join(app_name);

    // 使用 ditto 复制 .app（保留权限和扩展属性）
    let copy_result = Command::new("ditto")
        .args(["--rsrc", "--extattr"])
        .arg(&app_path)
        .arg(&target_app)
        .output()?;

    // 卸载 DMG（无论复制是否成功都要卸载）
    let _ = Command::new("hdiutil")
        .args(["detach", "-quiet"])
        .arg(&mount_point)
        .output();

    // 清理挂载点目录
    let _ = std::fs::remove_dir_all(&mount_point);

    if !copy_result.status.success() {
        return Err(AppError::Extract(format!(
            "复制 .app 失败: {}",
            String::from_utf8_lossy(&copy_result.stderr)
        )));
    }

    info!("DMG 提取完成: {}", target_app.display());
    Ok(target_app)
}

/// 在目录中查找 .app 包
#[cfg(target_os = "macos")]
fn find_app_in_dir(dir: &Path) -> AppResult<PathBuf> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name() {
                if name.to_string_lossy().ends_with(".app") {
                    return Ok(path);
                }
            }
        }
    }
    Err(AppError::Extract("DMG 中未找到 .app 文件".to_string()))
}

/// 从 tar.gz 中提取 .app 并安装 (仅 macOS)
#[cfg(target_os = "macos")]
pub fn extract_and_install_app_from_tar_gz(
    tar_gz_path: &Path,
    target_path: &Path,
    app_name: &str, // 例如 "Eden.app"
) -> AppResult<PathBuf> {
    use std::process::Command;

    info!("从 tar.gz 提取 .app: {}", tar_gz_path.display());

    // 解压到临时目录
    let tmp_dir = std::env::temp_dir().join(format!("app_extract_{}", std::process::id()));
    if tmp_dir.exists() {
        std::fs::remove_dir_all(&tmp_dir)?;
    }
    std::fs::create_dir_all(&tmp_dir)?;

    // 使用现有的 extract_tar_gz
    extract_tar_gz(tar_gz_path, &tmp_dir)?;

    // 在解压目录中递归查找目标 .app
    let app_path = find_app_recursive(&tmp_dir, app_name)?;

    // 确保目标目录存在
    std::fs::create_dir_all(target_path)?;

    let target_app = target_path.join(app_name);

    // 如果目标已存在，先删除（更稳妥的方式是复制到临时目录后原子替换）
    if target_app.exists() {
        std::fs::remove_dir_all(&target_app)?;
    }

    // 使用 ditto 复制（保留权限和扩展属性）
    let copy_result = Command::new("ditto")
        .args(["--rsrc", "--extattr"])
        .arg(&app_path)
        .arg(&target_app)
        .output()?;

    // 清理临时目录
    let _ = std::fs::remove_dir_all(&tmp_dir);

    if !copy_result.status.success() {
        return Err(AppError::Extract(format!(
            "复制 .app 失败: {}",
            String::from_utf8_lossy(&copy_result.stderr)
        )));
    }

    // 验证安装结果
    let info_plist = target_app.join("Contents/Info.plist");
    if !info_plist.exists() {
        return Err(AppError::Extract(
            ".app 安装验证失败: Contents/Info.plist 不存在".to_string()
        ));
    }

    info!(".app 安装完成: {}", target_app.display());
    Ok(target_app)
}

/// 递归查找指定名称的 .app
#[cfg(target_os = "macos")]
fn find_app_recursive(dir: &Path, app_name: &str) -> AppResult<PathBuf> {
    // 首先在当前目录查找
    let direct_path = dir.join(app_name);
    if direct_path.exists() && direct_path.is_dir() {
        return Ok(direct_path);
    }

    // 递归查找子目录
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string());
            if name.as_deref() == Some(app_name) {
                return Ok(path);
            }
            // 继续在子目录中查找
            if let Ok(found) = find_app_recursive(&path, app_name) {
                return Ok(found);
            }
        }
    }

    Err(AppError::Extract(format!("未找到 {}", app_name)))
}
```

**操作**:
1. 在 `archive.rs` 中添加上述 macOS 专用函数

---

### 第三阶段：安装流程改造

#### 步骤 3.1：修改 install_eden 支持 macOS ✅ 已完成

**文件**: `src-tauri/src/services/yuzu.rs` (修改)

**实现方案**:
- 使用 `#[cfg(target_os = "macos")]` 和 `#[cfg(not(target_os = "macos"))]` 区分平台
- macOS 平台：调用 `extract_and_install_app_from_tar_gz` 直接从 tar.gz 提取并安装 Eden.app
- Windows 平台：保持原有的解压到临时目录再复制的逻辑
- macOS 检查运行环境时跳过 MSVC 检查，直接返回成功；Windows 保持原有 MSVC 检查逻辑

**实现位置**: src-tauri/src/services/yuzu.rs:340-397 (macOS 分支), 399-602 (Windows 分支)

```rust
pub async fn install_eden<F>(target_version: &str, on_event: F) -> AppResult<()>
where
    F: Fn(ProgressEvent) + Send + Sync + 'static + Clone,
{
    // ... 获取版本信息、下载步骤保持不变 ...

    // 解压/安装 - 根据平台区分处理
    #[cfg(target_os = "macos")]
    {
        // macOS: tar.gz -> 提取 .app
        on_event(ProgressEvent::StepUpdate { step: /* 安装中 */ });

        let app_name = "Eden.app";
        let installed_app = crate::utils::archive::extract_and_install_app_from_tar_gz(
            &package_path,
            &yuzu_path,
            app_name,
        )?;

        info!("Eden.app 已安装到: {}", installed_app.display());
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Windows: 现有的解压和复制逻辑
        // ... 保持现有代码 ...
    }

    // 检查运行环境 - macOS 跳过 MSVC 检查
    #[cfg(target_os = "macos")]
    {
        on_event(ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "check_env".to_string(),
                title: "检查运行环境".to_string(),
                status: ProgressStatus::Success,
                // ...
            }
        });
        // macOS 无需 MSVC，直接成功
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Windows: 执行 MSVC 检查
        if let Err(e) = check_and_install_msvc().await {
            // ...
        }
    }

    // ...
}
```

#### 步骤 3.2：修改 install_citron 支持 macOS ✅ 已完成

**文件**: `src-tauri/src/services/yuzu.rs` (修改)

**实现方案**:
- 使用 `#[cfg(target_os = "macos")]` 和 `#[cfg(not(target_os = "macos"))]` 区分平台
- macOS 平台：调用 `extract_dmg` 直接挂载 DMG 并复制 Citron.app 到目标目录
- Windows 平台：保持原有的解压到临时目录、处理顶层目录、再复制的逻辑
- macOS 检查运行环境时跳过 MSVC 检查，直接返回成功；Windows 保持原有 MSVC 检查逻辑

**实现位置**: src-tauri/src/services/yuzu.rs:752-806 (macOS 分支), 808-1122 (Windows 分支)

```rust
pub async fn install_citron<F>(target_version: &str, on_event: F) -> AppResult<()>
where
    F: Fn(ProgressEvent) + Send + Sync + 'static + Clone,
{
    // ... 获取版本信息、下载步骤保持不变 ...

    // 解压/安装 - 根据平台区分处理
    #[cfg(target_os = "macos")]
    {
        // macOS: DMG -> 挂载 -> 复制 .app
        on_event(ProgressEvent::StepUpdate { step: /* 安装中 */ });

        let installed_app = crate::utils::archive::extract_dmg(
            &package_path,
            &yuzu_path,
        )?;

        info!("Citron.app 已安装到: {}", installed_app.display());
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Windows: 现有的解压和复制逻辑
        // ... 保持现有代码 ...
    }

    // 检查运行环境 - macOS 跳过 MSVC 检查（同 Eden）
    // ...
}
```

---

#### 步骤 3.3-3.5：修改可执行文件检测和删除逻辑 ✅ 已完成

**文件**: `src-tauri/src/services/yuzu.rs` (修改)

**实现方案**:
- **步骤 3.3**: 修改 `DETECT_EXE_LIST` 常量，macOS 版本使用 `.app` 后缀，Windows 版本使用 `.exe` 后缀
- **步骤 3.4**: 修改 `get_yuzu_exe_path()` 函数，macOS 查找 .app 并返回内部可执行文件路径（`Contents/MacOS/` 下）
- **步骤 3.5**: 新增 `remove_target_app(branch)` 函数，只删除当前分支对应的应用；保留 `remove_all_executable_file()` 为 deprecated，避免误删其他分支的应用

**实现位置**:
- DETECT_EXE_LIST: src-tauri/src/services/yuzu.rs:18-23
- get_yuzu_exe_path: src-tauri/src/services/yuzu.rs:1343-1390
- remove_target_app: src-tauri/src/services/yuzu.rs:1127-1162

```rust
/// 支持的模拟器可执行文件/应用列表
#[cfg(target_os = "macos")]
const DETECT_EXE_LIST: &[&str] = &["Eden.app", "Citron.app", "yuzu.app"];

#[cfg(not(target_os = "macos"))]
const DETECT_EXE_LIST: &[&str] = &["yuzu.exe", "eden.exe", "citron.exe", "suzu.exe", "cemu.exe"];
```

#### 步骤 3.4：修改 get_yuzu_exe_path 支持 macOS

```rust
/// 获取 Yuzu 可执行文件路径
pub fn get_yuzu_exe_path() -> PathBuf {
    let config = get_config();
    let yuzu_path = PathBuf::from(&config.yuzu.yuzu_path);

    #[cfg(target_os = "macos")]
    {
        // macOS: 查找 .app 并返回其内部的可执行文件
        for app_name in &["Eden.app", "Citron.app"] {
            let app_path = yuzu_path.join(app_name);
            if app_path.exists() {
                // 更稳妥：在 .app/Contents/MacOS 下找第一个可执行文件（或读取 Info.plist 的 CFBundleExecutable）
                let macos_bin_dir = app_path.join("Contents/MacOS");
                if let Ok(entries) = std::fs::read_dir(&macos_bin_dir) {
                    for entry in entries.flatten() {
                        let exe_path = entry.path();
                        if exe_path.is_file() {
                            return exe_path;
                        }
                    }
                }
            }
        }
        // 默认返回 Eden.app 路径
        yuzu_path.join("Eden.app/Contents/MacOS/Eden")
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Windows: 现有逻辑
        // ...
    }
}
```

#### 步骤 3.5：修改 remove_all_executable_file 支持 macOS

```rust
/// 删除旧的模拟器（仅删除当前分支对应的）
pub fn remove_target_app(branch: &str) -> AppResult<()> {
    let config = get_config();
    let yuzu_path = PathBuf::from(&config.yuzu.yuzu_path);

    #[cfg(target_os = "macos")]
    {
        let app_name = match branch {
            "eden" => "Eden.app",
            "citron" => "Citron.app",
            _ => return Ok(()),
        };
        let app_path = yuzu_path.join(app_name);
        if app_path.exists() {
            info!("删除旧应用: {}", app_path.display());
            std::fs::remove_dir_all(&app_path)?;
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Windows: 删除对应的 .exe
        let exe_name = match branch {
            "eden" => "eden.exe",
            "citron" => "citron.exe",
            _ => return Ok(()),
        };
        let exe_path = yuzu_path.join(exe_name);
        if exe_path.exists() {
            info!("删除: {}", exe_path.display());
            std::fs::remove_file(&exe_path)?;
        }
    }

    Ok(())
}
```

**注意**：
- `install_yuzu()` 里当前是无条件调用 `remove_all_executable_file()`；落地本步骤时需要把调用改为 `remove_target_app(branch)`（或将 `remove_all_executable_file()` 改为接收 `branch` 并按分支删除），否则 macOS 下会误用“删除文件”的逻辑处理 `.app` 目录。

---

### 第四阶段：配置路径更新

#### 步骤 4.1：更新默认路径 ✅ 已完成

**文件**: `src-tauri/src/config.rs` (已完成)

**实现方案**: macOS 默认路径已设置为 `~/yuzu`，避免 `/Applications` 的权限/授权问题；`/Applications` 作为可选安装位置保留。

---

### 第五阶段：用户数据目录支持

#### 步骤 5.1：修改 get_yuzu_user_path 支持 macOS ✅ 已完成

**文件**: `src-tauri/src/services/yuzu.rs` (修改)

**实现方案**:
- macOS: 用户数据在 `~/Library/Application Support/<app_name>` (eden/citron/yuzu)
- Windows: 保持原有逻辑，优先使用本地 user 目录，其次检查 AppData 目录
- 按优先级检查已存在的目录，默认返回基于当前分支的路径

**实现位置**: src-tauri/src/services/yuzu.rs:1574-1624

```rust
/// 获取 Yuzu 用户数据目录
pub fn get_yuzu_user_path() -> PathBuf {
    let config = get_config();

    #[cfg(target_os = "macos")]
    {
        // macOS: 用户数据在 ~/Library/Application Support/<app_name>
        if let Ok(home) = std::env::var("HOME") {
            let app_support = PathBuf::from(home).join("Library/Application Support");

            // 按优先级检查
            for name in &["eden", "citron", "yuzu"] {
                let path = app_support.join(name);
                if path.exists() {
                    return path;
                }
            }

            // 默认返回基于当前分支的路径
            return app_support.join(&config.yuzu.branch);
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Windows: 现有逻辑
        let yuzu_path = PathBuf::from(&config.yuzu.yuzu_path);

        // 优先使用本地 user 目录
        let local_user = yuzu_path.join("user");
        if local_user.exists() {
            return local_user;
        }

        // 检查 AppData 目录
        if let Ok(appdata) = std::env::var("APPDATA") {
            let appdata_path = PathBuf::from(appdata);
            for name in &["yuzu", "eden", "citron"] {
                let path = appdata_path.join(name);
                if path.exists() {
                    return path;
                }
            }
        }

        return local_user;
    }

    // Fallback
    PathBuf::from(&config.yuzu.yuzu_path).join("user")
}
```

---

### 第六阶段：测试与验证

#### 步骤 6.1：添加单元测试 ✅ 已完成

**文件**: `src-tauri/src/services/yuzu.rs` (修改)

**实现方案**:
- 添加了 `test_select_macos_asset_eden` 测试：验证 Eden 的 macOS tar.gz 资源筛选
- 添加了 `test_select_macos_asset_citron` 测试：验证 Citron 的 macOS dmg 资源筛选
- 添加了 `test_select_macos_asset_no_macos_build` 测试：验证无 macOS 资源时返回 None
- 添加了 `test_select_macos_asset_excludes_other_platforms` 测试：验证正确排除其他平台资源
- 修复了 `utils/archive.rs` 中缺少 `PathBuf` 导入的编译错误

**测试结果**: 所有 4 个测试通过 ✅

**实现位置**: src-tauri/src/services/yuzu.rs:2009-2149

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::release::{ReleaseAsset, ReleaseInfo};

    #[test]
    fn test_select_macos_asset_eden() {
        let release = ReleaseInfo {
            name: "v0.0.4-rc3".to_string(),
            tag_name: "v0.0.4-rc3".to_string(),
            description: "".to_string(),
            published_at: None,
            prerelease: false,
            html_url: None,
            assets: vec![
                ReleaseAsset {
                    name: "Eden-Windows-v0.0.4-rc3-amd64-msvc-standard.zip".to_string(),
                    download_url: "https://example.com/windows.zip".to_string(),
                    size: 0,
                    content_type: None,
                },
                ReleaseAsset {
                    name: "Eden-macOS-v0.0.4-rc3.tar.gz".to_string(),
                    download_url: "https://example.com/macos.tar.gz".to_string(),
                    size: 0,
                    content_type: None,
                },
            ],
        };

        let url = select_macos_asset(&release, "eden");
        assert_eq!(url, Some("https://example.com/macos.tar.gz".to_string()));
    }

    #[test]
    fn test_select_macos_asset_citron() {
        let release = ReleaseInfo {
            name: "stable".to_string(),
            tag_name: "stable-01c042048".to_string(),
            description: "".to_string(),
            published_at: None,
            prerelease: false,
            html_url: None,
            assets: vec![
                ReleaseAsset {
                    name: "Citron-windows-stable-01c042048-x64.zip".to_string(),
                    download_url: "https://example.com/windows.zip".to_string(),
                    size: 0,
                    content_type: None,
                },
                ReleaseAsset {
                    name: "Citron-macOS-stable-01c042048.dmg".to_string(),
                    download_url: "https://example.com/macos.dmg".to_string(),
                    size: 0,
                    content_type: None,
                },
            ],
        };

        let url = select_macos_asset(&release, "citron");
        assert_eq!(url, Some("https://example.com/macos.dmg".to_string()));
    }

    #[test]
    fn test_select_macos_asset_no_macos_build() {
        let release = ReleaseInfo {
            name: "old-version".to_string(),
            tag_name: "v0.0.1".to_string(),
            description: "".to_string(),
            published_at: None,
            prerelease: false,
            html_url: None,
            assets: vec![
                ReleaseAsset {
                    name: "Citron-windows-stable.zip".to_string(),
                    download_url: "https://example.com/windows.zip".to_string(),
                    size: 0,
                    content_type: None,
                },
            ],
        };

        let url = select_macos_asset(&release, "citron");
        assert_eq!(url, None);
    }
}
```

---

### 实现顺序总结

| 阶段 | 文件 | 改动类型 | 描述 |
|------|------|----------|------|
| 1.1 | `utils/platform.rs` | 新建 | 平台识别工具 |
| 1.2 | `services/yuzu.rs` | 修改 | 添加 `select_macos_asset` 函数 |
| 2.1 | `utils/archive.rs` | 修改 | 添加 DMG 挂载、tar.gz 提取 .app 功能 |
| 3.1 | `services/yuzu.rs` | 修改 | `install_eden` 添加 macOS 分支 |
| 3.2 | `services/yuzu.rs` | 修改 | `install_citron` 添加 macOS 分支 |
| 3.3 | `services/yuzu.rs` | 修改 | 更新检测列表（macOS 下 `DETECT_EXE_LIST` 包含 `.app`） |
| 3.4 | `services/yuzu.rs` | 修改 | `get_yuzu_exe_path` macOS 支持 |
| 3.5 | `services/yuzu.rs` | 修改 | `remove_target_app` 替换旧逻辑 |
| 5.1 | `services/yuzu.rs` | 修改 | `get_yuzu_user_path` macOS 支持 |
| 6.1 | `services/yuzu.rs` | 修改 | 添加单元测试 |

---

### 验证清单

- [ ] `cargo build --target aarch64-apple-darwin` 编译通过
- [ ] `cargo test` 单元测试通过
- [ ] macOS 上安装 Eden (tar.gz) 成功
- [ ] macOS 上安装 Citron (dmg) 成功
- [ ] 覆盖安装（已有版本）正常工作
- [ ] 安装目录权限异常时提示友好
- [ ] Windows 功能无回归

---

## 实现总结

### 已完成的功能

1. **平台识别与资源筛选** ✅
   - 创建了 `utils/platform.rs` 模块用于平台识别
   - 实现了 `select_macos_asset` 函数，支持 Eden (tar.gz) 和 Citron (dmg) 的资源筛选
   - 在 `download_yuzu` 中集成了平台判断逻辑

2. **DMG 挂载与解压功能** ✅
   - 在 `utils/archive.rs` 中实现了 `extract_dmg` 函数
   - 实现了 `extract_and_install_app_from_tar_gz` 函数
   - 使用 `hdiutil` 挂载/卸载 DMG，使用 `ditto` 复制 .app

3. **安装流程改造** ✅
   - 修改了 `install_eden` 支持 macOS (tar.gz 提取)
   - 修改了 `install_citron` 支持 macOS (DMG 挂载)
   - 更新了 `DETECT_EXE_LIST` 常量，macOS 使用 .app 后缀
   - 修改了 `get_yuzu_exe_path` 支持 macOS .app 路径
   - 实现了 `remove_target_app` 函数，避免误删其他分支的应用

4. **配置路径更新** ✅
   - macOS 默认路径设置为 `~/yuzu`

5. **用户数据目录支持** ✅
   - 修改了 `get_yuzu_user_path` 支持 macOS (`~/Library/Application Support/<app_name>`)

### 待完成的任务

1. **实际测试验证** ⚠️
   - 需要在 macOS 上实际测试 Eden 和 Citron 的安装流程
   - 验证覆盖安装、权限异常等场景
   - 验证 Windows 功能无回归

### 下一步行动

1. 在 macOS 上进行实际安装测试（Eden 和 Citron）
2. 验证覆盖安装场景
3. 验证路径权限异常场景的错误提示
4. 在 Windows 上回归测试，确保现有功能正常

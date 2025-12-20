# Eel to Tauri 迁移文档

## 概述

本文档记录了从 Python Eel 到 Rust + Tauri 的前端调用迁移计划。

**当前进度：9/18 文件已完成迁移 (50%)**

## 已完成迁移

### ✅ 已迁移文件

1. **frontend/src/App.vue**
   - 移除了 `window.eel.update_window_size()` 调用
   - 移除了 `setupWebsocketConnectivityCheck()` (Tauri 不需要 WebSocket 检查)
   - 使用 `getCurrentWindow()` 获取窗口信息

2. **frontend/src/main.ts**
   - 移除了 `window.eel: any` 类型声明

3. **frontend/src/layouts/AppDrawer.vue**
   - `window.eel.update_last_open_emu_page()` → `updateLastOpenEmuPage()`

4. **frontend/src/layouts/AppBar.vue**
   - `window.eel.update_dark_state()` → `updateDarkState()`

5. **frontend/src/stores/ConfigStore.ts**
   - `window.eel.get_config()` → `getConfig()`
   - `window.eel.get_current_version()` → `getAppVersion()`
   - `window.eel.check_update()` → `checkUpdate()`
   - 完全移除 Eel 兼容代码，统一使用 Tauri API

6. **frontend/src/pages/settings.vue**
   - `window.eel.update_setting()` → `updateSetting()`
   - `window.eel.get_available_firmware_sources()` → `getAvailableFirmwareSources()`
   - `window.eel.get_github_mirrors()` → `getGithubMirrors()`

7. **frontend/src/pages/keys.vue**
   - `window.eel.open_yuzu_keys_folder()` → `openYuzuKeysFolder()`
   - `window.eel.open_ryujinx_keys_folder()` → `openRyujinxKeysFolder()`

8. **frontend/src/pages/about.vue**
   - `window.eel.load_change_log()` → `loadChangeLog()`

9. **frontend/src/utils/common.ts**
   - `window.eel.open_url_in_default_browser()` → `openUrl()`
   - `window.eel.get_game_data()` → `getGameData()`

### 🎯 新增的 Tauri 命令 (本次迁移)

**后端命令 (src-tauri/src/commands/common.rs)**:
- `check_update` - 检查应用更新
- `load_change_log` - 加载变更日志
- `get_available_firmware_sources` - 获取固件下载源列表
- `get_github_mirrors` - 获取 GitHub 镜像列表
- `get_game_data` - 获取游戏数据映射

**前端 API (frontend/src/utils/tauri.ts)**:
- `checkUpdate(includePrerelease)` - 检查更新
- `loadChangeLog()` - 加载变更日志
- `getAvailableFirmwareSources()` - 获取固件源
- `getGithubMirrors()` - 获取镜像列表
- `getGameData()` - 获取游戏数据
- `openRyujinxKeysFolder()` - 打开 Ryujinx keys 文件夹

**新增模块**:
- `src-tauri/src/repositories/config_data.rs` - 配置数据仓库

## 待迁移文件清单

### 1. Stores (1 file)

#### ~~1.1 frontend/src/stores/ConfigStore.ts~~ ✅ 已完成

~~**需要迁移的调用：**~~

~~| Eel 方法 | Tauri 替代 | 状态 | 备注 |~~
~~|---------|-----------|------|------|~~
~~| `window.eel.get_config()` | `getConfig()` | ✅ 已有 | 已在 tauri.ts 中定义 |~~
~~| `window.eel.get_current_version()` | `getAppVersion()` | ✅ 已有 | 已在 tauri.ts 中定义 |~~
~~| `window.eel.check_update()` | 需要新增 | ❌ 待实现 | 需要添加 `checkUpdate()` |~~

#### 1.2 frontend/src/stores/app.ts

**需要迁移的调用：**

| Eel 方法 | Tauri 替代 | 状态 | 备注 |
|---------|-----------|------|------|
| `window.eel.get_available_firmware_infos()` | 需要新增 | ❌ 待实现 | 获取可用固件列表 |
| `window.eel.get_game_data()` | 需要新增 | ❌ 待实现 | 获取游戏数据映射 |

---

### 2. Components (4 files)

#### 2.1 frontend/src/components/ConsoleDialog.vue

**需要迁移的调用：**

| Eel 方法 | Tauri 替代 | 状态 | 备注 |
|---------|-----------|------|------|
| `window.eel.stop_download()` | 需要新增 | ❌ 待实现 | 停止下载 |
| `window.eel.pause_download()` | 需要新增 | ❌ 待实现 | 暂停下载 |

#### 2.2 frontend/src/components/NewVersionDialog.vue

**需要迁移的调用：**

| Eel 方法 | Tauri 替代 | 状态 | 备注 |
|---------|-----------|------|------|
| `window.eel.load_change_log()` | 需要新增 | ❌ 待实现 | 加载更新日志 |
| `window.eel.download_net_by_tag()` | 需要新增 | ❌ 待实现 | 下载 ns emt tools |
| `window.eel.update_net_by_tag()` | 需要新增 | ❌ 待实现 | 更新 ns emt tools |

#### 2.3 frontend/src/components/YuzuSaveCommonPart.vue

**需要迁移的调用：**

| Eel 方法 | Tauri 替代 | 状态 | 备注 |
|---------|-----------|------|------|
| `window.eel.get_users_in_save()` | 需要新增 | ❌ 待实现 | 获取 Yuzu 存档用户列表 |
| `window.eel.ask_and_update_yuzu_save_backup_folder()` | 需要新增 | ❌ 待实现 | 选择备份文件夹 |
| `window.eel.get_storage()` | `getStorage()` | ✅ 已有 | 已在 tauri.ts 中定义 |
| `window.eel.open_yuzu_save_backup_folder()` | 需要新增 | ❌ 待实现 | 打开备份文件夹 |

#### 2.4 frontend/src/components/YuzuSaveRestoreTab.vue

**需要迁移的调用：**

| Eel 方法 | Tauri 替代 | 状态 | 备注 |
|---------|-----------|------|------|
| `window.eel.list_all_yuzu_backups()` | 需要新增 | ❌ 待实现 | 列出所有备份 |
| `window.eel.restore_yuzu_save_from_backup()` | 需要新增 | ❌ 待实现 | 从备份恢复存档 |
| `window.eel.delete_path()` | 需要新增 | ❌ 待实现 | 删除路径 |

---

### 3. Pages (4 files)

#### ~~3.1 frontend/src/pages/keys.vue~~ ✅ 已完成

#### ~~3.2 frontend/src/pages/about.vue~~ ✅ 已完成

#### 3.3 frontend/src/pages/yuzuSaveManagement.vue

**需要迁移的调用：**

| Eel 方法 | Tauri 替代 | 状态 | 备注 |
|---------|-----------|------|------|
| `window.eel.list_all_games_by_user_folder()` | 需要新增 | ❌ 待实现 | 列出用户文件夹下的游戏 |
| `window.eel.backup_yuzu_save_folder()` | 需要新增 | ❌ 待实现 | 备份 Yuzu 存档 |

#### 3.4 frontend/src/pages/yuzuCheatsManagement.vue

**需要迁移的调用：**

| Eel 方法 | Tauri 替代 | 状态 | 备注 |
|---------|-----------|------|------|
| `window.eel.scan_all_cheats_folder()` | 需要新增 | ❌ 待实现 | 扫描金手指文件夹 |
| `window.eel.list_all_cheat_files_from_folder()` | 需要新增 | ❌ 待实现 | 列出文件夹下的金手指文件 |
| `window.eel.load_cheat_chunk_info()` | 需要新增 | ❌ 待实现 | 加载金手指块信息 |
| `window.eel.update_current_cheats()` | 需要新增 | ❌ 待实现 | 更新当前金手指 |
| `window.eel.open_cheat_mod_folder()` | 需要新增 | ❌ 待实现 | 打开金手指 MOD 文件夹 |

#### ~~3.5 frontend/src/pages/settings.vue~~ ✅ 已完成

#### 3.6 frontend/src/pages/ryujinx.vue

**需要迁移的调用：**

| Eel 方法 | Tauri 替代 | 状态 | 备注 |
|---------|-----------|------|------|
| `window.eel.update_last_open_emu_page()` | `updateLastOpenEmuPage()` | ✅ 已有 | 已在 tauri.ts 中定义 |
| `window.eel.get_ryujinx_release_infos()` | 需要新增 | ❌ 待实现 | 获取 Ryujinx 版本信息 |
| `window.eel.load_history_path()` | 需要新增 | ❌ 待实现 | 加载历史路径 |
| `window.eel.update_ryujinx_path()` | 需要新增 | ❌ 待实现 | 更新 Ryujinx 路径 |
| `window.eel.delete_history_path()` | `deleteHistoryPath()` | ✅ 已有 | 已在 tauri.ts 中定义 |
| `window.eel.detect_ryujinx_version()` | 需要新增 | ❌ 待实现 | 检测 Ryujinx 版本 |
| `window.eel.install_ryujinx()` | 需要新增 | ❌ 待实现 | 安装 Ryujinx |
| `window.eel.install_ryujinx_firmware()` | 需要新增 | ❌ 待实现 | 安装 Ryujinx 固件 |
| `window.eel.ask_and_update_ryujinx_path()` | 需要新增 | ❌ 待实现 | 选择并更新 Ryujinx 路径 |
| `window.eel.start_ryujinx()` | 需要新增 | ❌ 待实现 | 启动 Ryujinx |
| `window.eel.detect_firmware_version()` | 需要新增 | ❌ 待实现 | 检测固件版本 |
| `window.eel.switch_ryujinx_branch()` | 需要新增 | ❌ 待实现 | 切换 Ryujinx 分支 |
| `window.eel.load_ryujinx_change_log()` | 需要新增 | ❌ 待实现 | 加载 Ryujinx 更新日志 |

#### 3.7 frontend/src/pages/yuzu.vue

**状态：** ✅ 已完成（该文件已不使用 eel）

---

### 4. Utils (0 files)

#### ~~4.1 frontend/src/utils/common.ts~~ ✅ 已完成

---

## 需要添加的 Tauri 命令

### ✅ 已完成的命令 (本次迁移新增)

1. **版本管理相关**
   - ✅ `check_update` - 检查应用更新
   - ✅ `load_change_log` - 加载变更日志

2. **配置数据**
   - ✅ `get_available_firmware_sources` - 获取固件下载源列表
   - ✅ `get_github_mirrors` - 获取 GitHub 镜像列表

3. **游戏数据**
   - ✅ `get_game_data` - 获取游戏数据映射

### 高优先级（核心功能）

1. **版本管理相关**
   - ~~`check_update` - 检查应用更新~~ ✅ 已完成
   - `get_available_firmware_infos` - 获取可用固件列表
   - `detect_firmware_version` - 检测固件版本

2. **Ryujinx 核心功能**
   - `get_ryujinx_release_infos` - 获取 Ryujinx 版本列表
   - `install_ryujinx` - 安装 Ryujinx
   - `detect_ryujinx_version` - 检测 Ryujinx 版本
   - `start_ryujinx` - 启动 Ryujinx
   - `update_ryujinx_path` - 更新 Ryujinx 路径
   - `ask_and_update_ryujinx_path` - 选择并更新路径
   - `switch_ryujinx_branch` - 切换分支
   - `install_ryujinx_firmware` - 安装固件
   - ~~`open_ryujinx_keys_folder` - 打开 keys 文件夹~~ ✅ 已完成

3. **下载管理**
   - `stop_download` - 停止下载
   - `pause_download` - 暂停下载

### 中优先级（常用功能）

4. **设置相关**
   - ~~`get_available_firmware_sources` - 获取固件下载源列表~~ ✅ 已完成
   - ~~`get_github_mirrors` - 获取 GitHub 镜像列表~~ ✅ 已完成
   - `load_history_path` - 加载历史路径列表

5. **游戏数据**
   - ~~`get_game_data` - 获取游戏标题 ID 映射~~ ✅ 已完成

6. **更新日志**
   - ~~`load_change_log` - 加载应用更新日志~~ ✅ 已完成
   - `load_ryujinx_change_log` - 加载 Ryujinx 更新日志

### 低优先级（实验性功能）

7. **Yuzu 存档管理**
   - `get_users_in_save` - 获取存档用户列表
   - `list_all_games_by_user_folder` - 列出用户游戏
   - `backup_yuzu_save_folder` - 备份存档
   - `list_all_yuzu_backups` - 列出所有备份
   - `restore_yuzu_save_from_backup` - 恢复备份
   - `ask_and_update_yuzu_save_backup_folder` - 选择备份文件夹
   - `open_yuzu_save_backup_folder` - 打开备份文件夹

8. **Yuzu 金手指管理**
   - `scan_all_cheats_folder` - 扫描金手指文件夹
   - `list_all_cheat_files_from_folder` - 列出金手指文件
   - `load_cheat_chunk_info` - 加载金手指信息
   - `update_current_cheats` - 更新金手指
   - `open_cheat_mod_folder` - 打开 MOD 文件夹

9. **文件操作**
   - `delete_path` - 删除路径

10. **.NET Runtime 管理**
    - `download_net_by_tag` - 下载 .NET
    - `update_net_by_tag` - 更新 .NET

---

## 迁移优先级建议

### Phase 1: 核心功能（必须）
- ✅ 窗口管理和基础 UI (已完成)
- ConfigStore 基础方法
- Ryujinx 核心功能
- 下载管理
- 设置页面

### Phase 2: 常用功能
- 版本检查和更新
- 游戏数据加载
- 历史路径管理
- Keys 管理

### Phase 3: 实验性功能
- Yuzu 存档备份与恢复
- Yuzu 金手指管理
- .NET Runtime 管理

---

## 迁移模式

### 模式 1: 已有 Tauri API（直接替换）

```typescript
// 之前
window.eel.method_name(arg)((resp: CommonResponse) => {
  // handle response
})

// 之后
import { methodName } from "@/utils/tauri";

const resp = await methodName(arg)
// handle response
```

### 模式 2: 需要新增 Tauri 命令

#### 后端 (Rust)
```rust
// src-tauri/src/commands/xxx.rs
#[tauri::command]
pub fn method_name(arg: String) -> Result<ApiResponse<DataType>, String> {
    // implementation
    Ok(ApiResponse::success(data))
}

// 在 main.rs 中注册
.invoke_handler(tauri::generate_handler![
    // ...
    method_name,
])
```

#### 前端 (TypeScript)
```typescript
// frontend/src/utils/tauri.ts
export async function methodName(arg: string) {
  return invokeCommand<ApiResponse<DataType>>('method_name', { arg })
}

// 在组件中使用
import { methodName } from "@/utils/tauri";

const resp = await methodName(arg)
if (resp.code === 0) {
  // handle success
}
```

### 模式 3: 保留兼容性检查（过渡期）

```typescript
import { isTauri } from "@/utils/tauri";

if (isTauri()) {
  // 使用 Tauri API
  await methodName(arg)
} else {
  // 使用 Eel API（后续删除）
  window.eel.method_name(arg)()
}
```

> **注意：** 根据项目目标，应该完全迁移到 Tauri，不需要保留 Eel 兼容性。

---

## 注意事项

1. **异步调用方式变化**
   - Eel: `window.eel.method()(callback)` 或 `await window.eel.method()()`
   - Tauri: `await invokeCommand('method')`

2. **响应格式保持一致**
   ```typescript
   interface ApiResponse<T> {
     code: number
     data?: T
     msg?: string
   }
   ```

3. **事件监听**
   - Eel: 通过回调或自定义事件
   - Tauri: 使用 `listen()` API，已在 `tauri.ts` 中封装

4. **错误处理**
   - 所有 Tauri 调用应包含 try-catch
   - 使用统一的错误提示机制（ConsoleDialog）

5. **类型安全**
   - 为所有新增的 Tauri 命令添加 TypeScript 类型定义
   - 在 `frontend/src/utils/tauri.ts` 中维护类型

---

## 进度跟踪

- [x] App.vue
- [x] main.ts
- [x] layouts/AppDrawer.vue
- [x] layouts/AppBar.vue
- [x] stores/ConfigStore.ts ✨ 新增: `check_update`, `load_change_log`
- [ ] stores/app.ts
- [ ] components/ConsoleDialog.vue
- [ ] components/NewVersionDialog.vue
- [ ] components/YuzuSaveCommonPart.vue
- [ ] components/YuzuSaveRestoreTab.vue
- [x] pages/keys.vue ✨ 使用已有: `open_ryujinx_keys_folder_command`
- [x] pages/about.vue ✨ 使用已有: `load_change_log`
- [ ] pages/yuzuSaveManagement.vue
- [ ] pages/yuzuCheatsManagement.vue
- [x] pages/settings.vue ✨ 新增: `get_available_firmware_sources`, `get_github_mirrors`
- [ ] pages/ryujinx.vue
- [x] pages/yuzu.vue (无需迁移)
- [x] utils/common.ts ✨ 新增: `get_game_data`

**总计：** 9/18 完成 (50%)

---

## 参考资料

- [Tauri Command API](https://tauri.app/v1/guides/features/command)
- [Tauri Event System](https://tauri.app/v1/guides/features/events)
- [项目 Tauri API 封装](../../frontend/src/utils/tauri.ts)
- [Rust Tauri 重构计划](./rust-tauri-refactoring-plan.md)

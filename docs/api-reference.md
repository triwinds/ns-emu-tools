# API 参考文档

本文档详细描述了 NS Emu Tools 后端暴露给前端的所有 API 接口。

## API 设计原则

### 统一响应格式

所有 API 返回统一的 JSON 格式:

**成功响应**:
```json
{
  "success": true,
  "data": <返回数据>,
  "message": <可选的成功消息>
}
```

**错误响应**:
```json
{
  "success": false,
  "error": <错误信息>,
  "traceback": <详细堆栈跟踪>
}
```

### 调用方式

前端通过 Eel 桥接调用 Python 函数:

```javascript
// 异步调用
const result = await window.eel.function_name(param1, param2)()

// 带回调
window.eel.function_name(param1, param2)((result) => {
  console.log(result)
})
```

## API 模块

### 1. Common API (common_api.py)

通用功能 API，提供配置管理、系统检查等基础功能。

#### get_config()

获取当前应用配置。

**参数**: 无

**返回**:
```json
{
  "success": true,
  "data": {
    "yuzu": {
      "yuzu_path": "D:\\Yuzu",
      "yuzu_version": "1.0.0",
      "yuzu_firmware": "16.0.0",
      "branch": "eden"
    },
    "ryujinx": {
      "path": "D:\\Ryujinx",
      "version": "1.1.0",
      "firmware": "16.0.0",
      "branch": "mainline"
    },
    "setting": {
      "ui": {
        "lastOpenEmuPage": "ryujinx",
        "dark": true,
        "mode": "auto",
        "width": 1300,
        "height": 850
      },
      "network": {
        "firmwareDownloadSource": "github",
        "githubApiMode": "direct",
        "githubDownloadMirror": "cloudflare_load_balance",
        "useDoh": true,
        "proxy": "system"
      },
      "download": {
        "autoDeleteAfterInstall": true,
        "disableAria2Ipv6": true,
        "removeOldAria2LogFile": true,
        "verifyFirmwareMd5": true
      }
    }
  }
}
```

**示例**:
```javascript
const config = await window.eel.get_config()()
console.log(config.data.ryujinx.version)
```

---

#### update_config(setting)

更新应用配置。

**参数**:
- `setting` (Object): 新的配置对象

**返回**:
```json
{
  "success": true,
  "data": null
}
```

**示例**:
```javascript
await window.eel.update_config({
  ui: { dark: false },
  network: { useDoh: true }
})()
```

---

#### open_folder(path)

在文件管理器中打开指定文件夹。

**参数**:
- `path` (String): 文件夹路径

**返回**:
```json
{
  "success": true,
  "data": null
}
```

**示例**:
```javascript
await window.eel.open_folder('D:\\Ryujinx')()
```

---

#### check_msvc()

检查并安装 MSVC 运行库。

**参数**: 无

**返回**:
```json
{
  "success": true,
  "data": {
    "installed": true,
    "version": "14.0"
  }
}
```

**示例**:
```javascript
const result = await window.eel.check_msvc()()
if (!result.data.installed) {
  console.log('需要安装 MSVC 运行库')
}
```

---

#### get_window_size()

获取当前窗口尺寸 (仅 WebView 模式)。

**参数**: 无

**返回**:
```json
{
  "success": true,
  "data": {
    "width": 1300,
    "height": 850
  }
}
```

---

### 2. Updater API (updater_api.py)

应用自更新相关 API。

#### check_update()

检查是否有新版本可用。

**参数**: 无

**返回**:
```json
{
  "success": true,
  "data": {
    "has_update": true,
    "latest_version": "0.5.10",
    "current_version": "0.5.9",
    "release_notes": "## 更新内容\n- 新功能...",
    "download_url": "https://github.com/..."
  }
}
```

**示例**:
```javascript
const update = await window.eel.check_update()()
if (update.data.has_update) {
  console.log(`发现新版本: ${update.data.latest_version}`)
}
```

---

#### download_net_by_tag(tag)

下载指定版本的更新包。

**参数**:
- `tag` (String): 版本标签 (如 "v0.5.10")

**返回**:
```json
{
  "success": true,
  "data": {
    "file_path": "download/NsEmuTools-0.5.10.zip",
    "size": 52428800
  }
}
```

**示例**:
```javascript
await window.eel.download_net_by_tag('v0.5.10')()
```

---

#### update_net_by_tag(tag)

更新到指定版本。

**参数**:
- `tag` (String): 版本标签

**返回**:
```json
{
  "success": true,
  "data": {
    "message": "更新脚本已生成，应用将重启"
  }
}
```

**注意**: 此函数会触发应用重启。

**示例**:
```javascript
await window.eel.update_net_by_tag('v0.5.10')()
// 应用将自动重启
```

---

#### load_change_log()

加载更新日志。

**参数**: 无

**返回**:
```json
{
  "success": true,
  "data": "## v0.5.9\n- 修复 bug\n\n## v0.5.8\n- 新功能..."
}
```

---

### 3. Ryujinx API (ryujinx_api.py)

Ryujinx 模拟器管理 API。

#### get_ryujinx_version(branch)

获取 Ryujinx 版本信息。

**参数**:
- `branch` (String): 分支名称 ("mainline" 或 "canary")

**返回**:
```json
{
  "success": true,
  "data": {
    "local_version": "1.1.1000",
    "remote_version": "1.1.1050",
    "has_update": true,
    "release_date": "2025-12-18"
  }
}
```

**示例**:
```javascript
const version = await window.eel.get_ryujinx_version('mainline')()
console.log(`本地版本: ${version.data.local_version}`)
```

---

#### install_ryujinx(branch, version)

安装 Ryujinx 模拟器。

**参数**:
- `branch` (String): 分支名称
- `version` (String, 可选): 指定版本，不传则安装最新版

**返回**:
```json
{
  "success": true,
  "data": {
    "installed_version": "1.1.1050",
    "install_path": "D:\\Ryujinx"
  }
}
```

**示例**:
```javascript
// 安装最新版
await window.eel.install_ryujinx('mainline')()

// 安装指定版本
await window.eel.install_ryujinx('mainline', '1.1.1000')()
```

---

#### update_ryujinx(branch)

更新 Ryujinx 到最新版本。

**参数**:
- `branch` (String): 分支名称

**返回**:
```json
{
  "success": true,
  "data": {
    "old_version": "1.1.1000",
    "new_version": "1.1.1050"
  }
}
```

---

#### install_firmware(emulator, firmware_version)

安装 NS 固件到模拟器。

**参数**:
- `emulator` (String): 模拟器名称 ("ryujinx" 或 "yuzu")
- `firmware_version` (String, 可选): 固件版本

**返回**:
```json
{
  "success": true,
  "data": {
    "firmware_version": "16.0.0",
    "install_path": "D:\\Ryujinx\\firmware"
  }
}
```

**示例**:
```javascript
await window.eel.install_firmware('ryujinx', '16.0.0')()
```

---

#### get_firmware_list()

获取可用固件列表。

**参数**: 无

**返回**:
```json
{
  "success": true,
  "data": [
    {
      "version": "16.0.0",
      "release_date": "2023-04-13",
      "size": "536870912"
    },
    {
      "version": "15.0.1",
      "release_date": "2022-12-06",
      "size": "520093696"
    }
  ]
}
```

---

#### check_firmware_version(emulator)

检查模拟器当前固件版本。

**参数**:
- `emulator` (String): 模拟器名称

**返回**:
```json
{
  "success": true,
  "data": {
    "current_version": "15.0.1",
    "latest_version": "16.0.0",
    "has_update": true
  }
}
```

---

#### load_ryujinx_change_log(branch)

加载 Ryujinx 更新日志。

**参数**:
- `branch` (String): 分支名称

**返回**:
```json
{
  "success": true,
  "data": "## 更新内容\n- 修复了...\n- 新增了..."
}
```

---

### 4. Yuzu API (yuzu_api.py)

Yuzu/Eden/Citron 模拟器管理 API。

#### get_yuzu_version(branch)

获取 Yuzu 系列模拟器版本信息。

**参数**:
- `branch` (String): 分支名称 ("eden", "citron")

**返回**:
```json
{
  "success": true,
  "data": {
    "local_version": "1.0.0",
    "remote_version": "1.0.5",
    "has_update": true
  }
}
```

---

#### install_yuzu(branch, version)

安装 Yuzu 系列模拟器。

**参数**:
- `branch` (String): 分支名称
- `version` (String, 可选): 指定版本

**返回**:
```json
{
  "success": true,
  "data": {
    "installed_version": "1.0.5",
    "install_path": "D:\\Yuzu"
  }
}
```

---

#### update_yuzu(branch)

更新 Yuzu 系列模拟器。

**参数**:
- `branch` (String): 分支名称

**返回**:
```json
{
  "success": true,
  "data": {
    "old_version": "1.0.0",
    "new_version": "1.0.5"
  }
}
```

---

### 5. Save Manager API (save_manager_api.py)

存档管理 API。

#### list_saves(emulator)

列出模拟器的所有存档。

**参数**:
- `emulator` (String): 模拟器名称

**返回**:
```json
{
  "success": true,
  "data": [
    {
      "game_id": "0100000000010000",
      "game_name": "Super Mario Odyssey",
      "save_path": "D:\\Yuzu\\nand\\user\\save\\0000000000000000\\0100000000010000",
      "size": 1048576,
      "last_modified": "2025-12-18T10:30:00"
    }
  ]
}
```

---

#### backup_save(emulator, game_id, backup_path)

备份游戏存档。

**参数**:
- `emulator` (String): 模拟器名称
- `game_id` (String): 游戏 ID
- `backup_path` (String): 备份路径

**返回**:
```json
{
  "success": true,
  "data": {
    "backup_file": "D:\\Backups\\save_20251218_103000.zip",
    "size": 1048576
  }
}
```

---

#### restore_save(emulator, game_id, backup_file)

恢复游戏存档。

**参数**:
- `emulator` (String): 模拟器名称
- `game_id` (String): 游戏 ID
- `backup_file` (String): 备份文件路径

**返回**:
```json
{
  "success": true,
  "data": {
    "restored": true
  }
}
```

---

### 6. Cheats API (cheats_api.py)

金手指管理 API (仅 Yuzu)。

#### list_cheats(game_id)

列出游戏的所有金手指。

**参数**:
- `game_id` (String): 游戏 ID

**返回**:
```json
{
  "success": true,
  "data": [
    {
      "name": "无限金币",
      "enabled": true,
      "codes": ["04000000 00000000 3B9ACA00"]
    }
  ]
}
```

---

#### enable_cheat(game_id, cheat_name, enabled)

启用/禁用金手指。

**参数**:
- `game_id` (String): 游戏 ID
- `cheat_name` (String): 金手指名称
- `enabled` (Boolean): 是否启用

**返回**:
```json
{
  "success": true,
  "data": null
}
```

---

#### add_cheat(game_id, cheat_name, codes)

添加新金手指。

**参数**:
- `game_id` (String): 游戏 ID
- `cheat_name` (String): 金手指名称
- `codes` (Array): 金手指代码数组

**返回**:
```json
{
  "success": true,
  "data": null
}
```

---

#### delete_cheat(game_id, cheat_name)

删除金手指。

**参数**:
- `game_id` (String): 游戏 ID
- `cheat_name` (String): 金手指名称

**返回**:
```json
{
  "success": true,
  "data": null
}
```

---

## 消息通知 API

### 前端监听后端消息

后端通过 `msg_notifier.py` 发送实时消息到前端。

#### update_message(message)

接收后端消息更新。

**参数**:
- `message` (String): 消息内容

**前端实现**:
```javascript
// 在前端暴露函数
window.eel.expose(updateMessage, 'update_message')

function updateMessage(message) {
  // 处理消息
  if (message.startsWith('^')) {
    // 进度更新
    const progress = message.substring(1)
    console.log('进度:', progress)
  } else {
    // 普通消息
    console.log('消息:', message)
  }
}
```

**消息格式**:
- 普通消息: `"正在下载..."`
- 进度消息: `"^50%|████████░░░░░░░░|[2m30s, 5MB/s]"`

---

## 错误处理

### 错误类型

所有 API 错误都会返回统一格式:

```json
{
  "success": false,
  "error": "错误描述",
  "traceback": "详细堆栈跟踪"
}
```

### 常见错误

#### DownloadException
下载相关错误:
- 网络连接失败
- 下载中断
- 文件校验失败

#### InstallException
安装相关错误:
- 解压失败
- 文件权限不足
- 磁盘空间不足

#### CommonException
通用错误:
- 配置文件损坏
- 路径不存在
- 参数错误

### 错误处理示例

```javascript
try {
  const result = await window.eel.install_ryujinx('mainline')()
  if (result.success) {
    console.log('安装成功')
  } else {
    console.error('安装失败:', result.error)
    // 显示错误给用户
  }
} catch (error) {
  console.error('调用失败:', error)
}
```

---

## API 使用最佳实践

### 1. 错误处理

始终检查 `success` 字段:

```javascript
const result = await window.eel.some_api()()
if (!result.success) {
  // 处理错误
  showError(result.error)
  return
}
// 使用 result.data
```

### 2. 加载状态

长时间操作显示加载状态:

```javascript
loading.value = true
try {
  await window.eel.install_ryujinx('mainline')()
} finally {
  loading.value = false
}
```

### 3. 消息监听

监听后端消息更新 UI:

```javascript
window.eel.expose(updateProgress, 'update_message')

function updateProgress(message) {
  if (message.startsWith('^')) {
    progressText.value = message.substring(1)
  }
}
```

### 4. 配置更新

更新配置后刷新 UI:

```javascript
await window.eel.update_config(newConfig)()
await loadConfig() // 重新加载配置
```

---

## 版本兼容性

### API 版本

当前 API 版本: **1.0**

### 向后兼容

- 新增 API 不会破坏现有功能
- 废弃的 API 会保留至少一个大版本
- 破坏性变更会在主版本号中体现

### 变更日志

#### v1.0 (2025-12-18)
- 初始 API 版本
- 支持 Ryujinx, Yuzu, Eden, Citron
- 固件管理
- 存档管理
- 金手指管理

---

## 附录

### A. 数据类型定义

#### ReleaseInfo
```typescript
interface ReleaseInfo {
  tag_name: string
  name: string
  description: string
  published_at: string
  assets: Asset[]
}
```

#### Asset
```typescript
interface Asset {
  name: string
  browser_download_url: string
  size: number
}
```

#### Config
```typescript
interface Config {
  yuzu: YuzuConfig
  ryujinx: RyujinxConfig
  setting: CommonSetting
}
```

### B. 常量定义

#### 模拟器分支
- Ryujinx: `"mainline"`, `"canary"`
- Yuzu: `"eden"`, `"citron"`

#### 下载源
- GitHub: `"github"`
- darthsternie: `"darthsternie"`

---

**文档版本**: 1.0
**最后更新**: 2025-12-18

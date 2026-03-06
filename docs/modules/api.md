# api 目录模块文档

`api` 目录是项目的 Web API 层，基于 Eel 框架提供前端调用的接口。所有 API 函数都通过 `@eel.expose` 装饰器暴露给前端 JavaScript 调用。

## 目录结构

```
api/
├── __init__.py             # 模块初始化，导出所有 API 模块
├── common_response.py      # 统一响应格式和异常处理
├── common_api.py           # 通用 API 接口
├── yuzu_api.py             # Yuzu 模拟器相关 API
├── ryujinx_api.py          # Ryujinx 模拟器相关 API
├── cheats_api.py           # 金手指管理 API
├── save_manager_api.py     # 存档管理 API
└── updater_api.py          # 程序更新 API
```

---

## 响应格式规范

### common_response.py - 统一响应模块

**功能概述：** 定义统一的 API 响应格式和异常处理机制。

#### 响应格式

```python
# 成功响应
{
    "code": 0,
    "data": <返回数据>,
    "msg": <可选消息>
}

# 错误响应
{
    "code": <错误码>,
    "msg": <错误消息>
}
```

#### 主要函数

| 函数名 | 功能说明 |
|--------|----------|
| `success_response(data, msg)` | 构建成功响应 |
| `error_response(code, msg)` | 构建错误响应 |
| `exception_response(ex)` | 将异常转换为响应 |
| `@generic_api` | 装饰器，自动处理函数的响应和异常 |

#### 错误码定义

| 错误码 | 含义 |
|--------|------|
| 0 | 成功 |
| 100 | 用户取消操作 |
| 404 | 版本未找到 |
| 501 | MD5 不匹配 |
| 601 | 下载被终止 |
| 602 | 下载被暂停 |
| 603 | 下载未完成 |
| 701 | 文件复制失败 |
| 801 | 忽略的异常 |
| 999 | 未知错误 |

#### 异常处理映射

```python
exception_handler_map = {
    VersionNotFoundException: version_not_found_handler,
    Md5NotMatchException: md5_not_found_handler,
    DownloadInterrupted: download_interrupted_handler,
    DownloadPaused: download_paused_handler,
    DownloadNotCompleted: download_not_completed_handler,
    FailToCopyFiles: fail_to_copy_files_handler,
    IgnoredException: ignored_exception_handler,
    ConnectionError: connection_error_handler,
}
```

---

## 各 API 模块详细说明

### common_api.py - 通用 API

**功能概述：** 提供通用功能的 API 接口，包括配置管理、下载控制、版本信息等。

#### API 列表

| API 名称 | 功能说明 | 参数 |
|----------|----------|------|
| `get_current_version()` | 获取当前程序版本 | 无 |
| `get_config()` | 获取当前配置 | 无 |
| `update_setting(setting)` | 更新设置 | `setting: Dict` |
| `get_available_firmware_infos()` | 获取可用固件列表 | 无 |
| `get_available_firmware_sources()` | 获取固件下载源列表 | 无 |
| `detect_firmware_version(emu_type)` | 检测固件版本 | `emu_type: str` ('yuzu'/'ryujinx') |
| `stop_download()` | 停止下载 | 无 |
| `pause_download()` | 暂停下载 | 无 |
| `load_history_path(emu_type)` | 加载历史路径 | `emu_type: str` |
| `delete_history_path(emu_type, path)` | 删除历史路径 | `emu_type: str`, `path: str` |
| `get_github_mirrors()` | 获取 GitHub 镜像列表 | 无 |
| `update_window_size(width, height)` | 更新窗口尺寸 | `width: int`, `height: int` |
| `update_last_open_emu_page(page)` | 更新最后打开的模拟器页面 | `page: str` |
| `update_dark_state(dark)` | 更新深色模式状态 | `dark: bool` |
| `open_url_in_default_browser(url)` | 在默认浏览器打开 URL | `url: str` |
| `get_net_release_info_by_tag(tag)` | 获取指定版本的发布信息 | `tag: str` |
| `get_storage()` | 获取存储数据 | 无 |
| `delete_path(path)` | 删除路径 | `path: str` |

---

### yuzu_api.py - Yuzu 模拟器 API

**功能概述：** 提供 Yuzu 系列模拟器 (Eden, Citron) 管理的 API 接口。

#### API 列表

| API 名称 | 功能说明 | 参数 |
|----------|----------|------|
| `get_yuzu_config()` | 获取 Yuzu 配置 | 无 |
| `ask_and_update_yuzu_path()` | 弹窗选择并更新路径 | 无 |
| `update_yuzu_path(folder)` | 更新 Yuzu 路径 | `folder: str` |
| `detect_yuzu_version()` | 检测 Yuzu 版本 | 无 |
| `start_yuzu()` | 启动 Yuzu | 无 |
| `install_yuzu(version, branch)` | 安装指定版本 | `version: str`, `branch: str` |
| `install_yuzu_firmware(version)` | 安装固件 | `version: str` |
| `switch_yuzu_branch(branch)` | 切换分支 | `branch: str` |
| `get_all_yuzu_release_versions()` | 获取所有可用版本 | 无 |
| `open_yuzu_keys_folder()` | 打开密钥文件夹 | 无 |
| `get_yuzu_change_logs()` | 获取更新日志 | 无 |

#### 支持的分支

- `ea`: Early Access (原 Yuzu EA)
- `mainline`: 主线版本
- `eden`: Eden 模拟器
- `citron`: Citron 模拟器

---

### ryujinx_api.py - Ryujinx 模拟器 API

**功能概述：** 提供 Ryujinx 模拟器管理的 API 接口。

#### API 列表

| API 名称 | 功能说明 | 参数 |
|----------|----------|------|
| `get_ryujinx_config()` | 获取 Ryujinx 配置 | 无 |
| `ask_and_update_ryujinx_path()` | 弹窗选择并更新路径 | 无 |
| `update_ryujinx_path(folder)` | 更新路径 | `folder: str` |
| `get_ryujinx_release_infos()` | 获取发布版本信息 | 无 |
| `detect_ryujinx_version()` | 检测版本 | 无 |
| `start_ryujinx()` | 启动 Ryujinx | 无 |
| `install_ryujinx(version, branch)` | 安装指定版本 | `version: str`, `branch: str` |
| `install_ryujinx_firmware(version)` | 安装固件 | `version: str` |
| `switch_ryujinx_branch(branch)` | 切换分支 | `branch: str` |
| `open_ryujinx_keys_folder()` | 打开密钥文件夹 | 无 |
| `load_ryujinx_change_log()` | 加载更新日志 | 无 |

#### 支持的分支

- `mainline`: 主线版本
- `canary`: 金丝雀版本

---

### cheats_api.py - 金手指管理 API

**功能概述：** 提供金手指文件扫描、解析和管理的 API 接口。

#### API 列表

| API 名称 | 功能说明 | 参数 |
|----------|----------|------|
| `scan_all_cheats_folder()` | 扫描所有金手指文件夹 | 无 |
| `list_all_cheat_files_from_folder(folder_path)` | 列出文件夹中的金手指文件 | `folder_path: str` |
| `load_cheat_chunk_info(cheat_file_path)` | 加载金手指条目信息 | `cheat_file_path: str` |
| `update_current_cheats(enable_titles, cheat_file_path)` | 更新启用的金手指 | `enable_titles: List[str]`, `cheat_file_path: str` |
| `open_cheat_mod_folder(folder_path)` | 打开金手指文件夹 | `folder_path: str` |
| `get_game_data()` | 获取游戏数据 | 无 |

#### 返回数据格式

**scan_all_cheats_folder 返回：**
```python
[
    {
        "game_id": "0100F2C0115B6000",
        "cheats_path": "D:\\citron\\user\\load\\0100F2C0115B6000\\...\\cheats"
    },
    ...
]
```

**load_cheat_chunk_info 返回：**
```python
[
    {
        "title": "60 FPS",
        "enable": True
    },
    ...
]
```

---

### save_manager_api.py - 存档管理 API

**功能概述：** 提供 Yuzu 存档备份和还原的 API 接口。

#### API 列表

| API 名称 | 功能说明 | 参数 |
|----------|----------|------|
| `get_users_in_save()` | 获取存档中的用户列表 | 无 |
| `list_all_games_by_user_folder(folder)` | 列出用户的所有游戏存档 | `folder: str` |
| `ask_and_update_yuzu_save_backup_folder()` | 选择备份文件夹 | 无 |
| `backup_yuzu_save_folder(folder)` | 备份存档文件夹 | `folder: str` |
| `open_yuzu_save_backup_folder()` | 打开备份文件夹 | 无 |
| `list_all_yuzu_backups()` | 列出所有备份 | 无 |
| `restore_yuzu_save_from_backup(user_folder_name, backup_path)` | 还原存档 | `user_folder_name: str`, `backup_path: str` |

#### 返回数据格式

**get_users_in_save 返回：**
```python
[
    {
        "user_id": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
        "folder": "97A1DAE861CD445AB9645267B3AB99BE"
    },
    ...
]
```

**list_all_yuzu_backups 返回：**
```python
[
    {
        "filename": "yuzu_0100F2C0115B6000_1685114415.7z",
        "path": "D:\\yuzu_save_backup\\yuzu_0100F2C0115B6000_1685114415.7z",
        "title_id": "0100F2C0115B6000",
        "bak_time": 1685114415000
    },
    ...
]
```

---

### updater_api.py - 程序更新 API

**功能概述：** 提供程序自动更新的 API 接口。

#### API 列表

| API 名称 | 功能说明 | 参数 |
|----------|----------|------|
| `check_update()` | 检查更新 | 无 |
| `download_net_by_tag(tag)` | 下载指定版本 | `tag: str` |
| `update_net_by_tag(tag)` | 更新到指定版本 | `tag: str` |
| `load_change_log()` | 加载更新日志 | 无 |

#### 返回数据格式

**check_update 返回：**
```python
{
    "code": 0,
    "data": True,  # 是否有更新
    "msg": "0.5.10"  # 最新版本号
}
```

---

## API 调用示例

### 前端 JavaScript 调用

```javascript
// 获取配置
const config = await eel.get_config()();

// 安装 Yuzu
const result = await eel.install_yuzu("0.1.0", "citron")();
if (result.code === 0) {
    console.log("安装成功");
} else {
    console.error("安装失败:", result.msg);
}

// 检查更新
const updateInfo = await eel.check_update()();
if (updateInfo.data) {
    console.log("有新版本:", updateInfo.msg);
}
```

### 使用 @generic_api 装饰器

```python
from api.common_response import generic_api

@generic_api
def my_api_function(param):
    # 业务逻辑
    result = do_something(param)
    return result  # 自动包装为 success_response
    # 异常自动被捕获并转换为 exception_response
```

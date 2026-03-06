# module 目录模块文档

`module` 目录是项目的核心业务逻辑层，包含了与 Nintendo Switch 模拟器管理相关的各种功能模块。

## 目录结构

```
module/
├── cheats/                 # 金手指(作弊码)管理子模块
│   ├── __init__.py
│   ├── cheats.py           # 金手指核心功能
│   ├── cheats_types.py     # 金手指数据类型定义
│   └── cheats_yuzu_parser.py  # Yuzu 格式金手指解析器
├── external/
│   └── bat_scripts.py      # 外部批处理脚本生成
├── common.py               # 通用功能模块
├── dialogs.py              # 文件对话框模块
├── downloader.py           # 下载器模块
├── firmware.py             # 固件管理模块
├── hosts.py                # hosts 文件管理模块
├── msg_notifier.py         # 消息通知模块
├── network.py              # 网络请求模块
├── nsz_wrapper.py          # NSZ 库封装模块
├── ryujinx.py              # Ryujinx 模拟器管理模块
├── save_manager.py         # 存档管理模块
├── sentry.py               # Sentry 错误追踪模块
├── updater.py              # 程序自动更新模块
└── yuzu.py                 # Yuzu 系列模拟器管理模块
```

---

## 各模块详细说明

### common.py - 通用功能模块

**功能概述：** 提供项目中共用的基础功能函数。

**主要函数：**

| 函数名 | 功能说明 |
|--------|----------|
| `check_and_install_msvc()` | 检查并安装 MSVC 运行库，模拟器运行依赖 |
| `delete_path(path)` | 安全删除指定路径的文件或目录 |

**依赖关系：**
- 依赖 `msg_notifier` 发送通知消息
- 依赖 `downloader` 下载 MSVC 安装包
- 依赖 `network` 获取下载链接

---

### msg_notifier.py - 消息通知模块

**功能概述：** 提供统一的消息通知机制，支持多种通知模式。

**主要函数：**

| 函数名 | 功能说明 |
|--------|----------|
| `send_notify(msg)` | 发送通知消息 |
| `update_notifier(mode)` | 切换通知模式 ('eel', 'eel-console', 'dummy') |

**通知模式：**
- `eel`: 通过 eel 框架更新顶部状态栏
- `eel-console`: 通过 eel 框架追加控制台消息
- `dummy`: 空操作，不发送任何通知

---

### downloader.py - 下载器模块

**功能概述：** 基于 aria2 的文件下载管理器，支持多线程下载、断点续传等功能。

**主要函数：**

| 函数名 | 功能说明 |
|--------|----------|
| `download(url, save_dir, options, download_in_background)` | 下载文件，支持后台下载 |
| `init_aria2()` | 初始化 aria2 下载守护进程 |
| `stop_download()` | 停止所有下载任务 |
| `pause_download()` | 暂停所有下载任务 |

**特性：**
- 自动启动 aria2 守护进程
- 支持代理配置
- 支持 IPv6 (可配置禁用)
- 支持 DNS over HTTPS
- 带有进度条显示 (MyTqdm)

---

### network.py - 网络请求模块

**功能概述：** 封装网络请求相关功能，包括代理管理、镜像源切换、GitHub API 请求等。

**主要函数：**

| 函数名 | 功能说明 |
|--------|----------|
| `get_finial_url(origin_url)` | 根据配置返回最终的下载 URL |
| `get_github_download_url(origin_url)` | 获取 GitHub 下载的镜像 URL |
| `request_github_api(url)` | 请求 GitHub API，支持自动 CDN 回退 |
| `get_proxies()` | 获取当前代理配置 |
| `is_using_proxy()` | 检查是否正在使用代理 |
| `get_github_mirrors()` | 获取可用的 GitHub 镜像列表 |
| `get_available_port()` | 获取可用的网络端口 |

**镜像源支持：**
- 多个 Cloudflare CDN 镜像
- 自建代理服务器 (nsarchive.e6ex.com)
- 支持自动负载均衡

---

### yuzu.py - Yuzu 系列模拟器管理模块

**功能概述：** 管理 Yuzu 系列模拟器 (Yuzu, Eden, Citron, Suzu) 的安装、更新和配置。

**主要函数：**

| 函数名 | 功能说明 |
|--------|----------|
| `install_yuzu(target_version, branch)` | 安装指定版本的模拟器 |
| `download_yuzu(target_version, branch)` | 下载模拟器安装包 |
| `detect_yuzu_version()` | 检测当前安装的模拟器版本 |
| `start_yuzu()` | 启动模拟器 |
| `install_firmware_to_yuzu(firmware_version)` | 安装固件到模拟器 |
| `get_yuzu_user_path()` | 获取用户数据目录 |
| `get_yuzu_nand_path()` | 获取 NAND 目录 |
| `get_yuzu_load_path()` | 获取 mod 加载目录 |
| `open_yuzu_keys_folder()` | 打开密钥文件夹 |
| `update_yuzu_path(new_path)` | 更新模拟器安装路径 |
| `get_yuzu_change_logs()` | 获取更新日志 |

**支持的分支：**
- `eden`: Eden 模拟器
- `citron`: Citron 模拟器

---

### ryujinx.py - Ryujinx 模拟器管理模块

**功能概述：** 管理 Ryujinx 模拟器的安装、更新和配置。

**主要函数：**

| 函数名 | 功能说明 |
|--------|----------|
| `install_ryujinx_by_version(target_version, branch)` | 安装指定版本的 Ryujinx |
| `detect_ryujinx_version()` | 检测当前安装的版本 |
| `start_ryujinx()` | 启动 Ryujinx |
| `install_firmware_to_ryujinx(firmware_version)` | 安装固件 |
| `get_ryujinx_user_folder()` | 获取用户数据目录 |
| `open_ryujinx_keys_folder()` | 打开密钥文件夹 |
| `update_ryujinx_path(new_path)` | 更新安装路径 |
| `detect_current_branch()` | 检测当前分支 (ava/mainline) |

**支持的分支：**
- `mainline`: 主线版本
- `canary`: 金丝雀版本 (测试版)
- `ldn`: LDN 版本 (局域网联机)

---

### firmware.py - 固件管理模块

**功能概述：** 管理 Nintendo Switch 固件的下载、安装和版本检测。

**主要函数：**

| 函数名 | 功能说明 |
|--------|----------|
| `get_firmware_infos()` | 获取可用固件列表 |
| `install_firmware(firmware_version, target_path)` | 安装固件到指定路径 |
| `detect_firmware_version(emu_type)` | 检测当前安装的固件版本 |
| `check_file_md5(file, target_md5)` | 校验文件 MD5 |
| `get_available_firmware_sources()` | 获取可用的固件下载源 |

**固件来源：**
- GitHub (THZoria/NX_Firmware)
- nsarchive (darthsternie.net)

---

### save_manager.py - 存档管理模块

**功能概述：** 管理 Yuzu 模拟器的游戏存档备份和还原。

**主要函数：**

| 函数名 | 功能说明 |
|--------|----------|
| `get_users_in_save()` | 获取存档中的用户列表 |
| `list_all_games_by_user_folder(user_folder)` | 列出指定用户的所有游戏存档 |
| `backup_folder(folder_path)` | 备份指定存档文件夹 |
| `restore_yuzu_save_from_backup(user_folder, backup_path)` | 从备份还原存档 |
| `list_all_yuzu_backups()` | 列出所有备份 |
| `open_yuzu_save_backup_folder()` | 打开备份文件夹 |

---

### updater.py - 程序自动更新模块

**功能概述：** 管理 NsEmuTools 工具本身的版本检查和自动更新。

**主要函数：**

| 函数名 | 功能说明 |
|--------|----------|
| `check_update(prerelease)` | 检查是否有新版本 |
| `download_net_by_tag(tag)` | 下载指定版本 |
| `update_self_by_tag(tag)` | 更新到指定版本 |

**更新流程：**
1. 下载新版本压缩包
2. 生成更新批处理脚本
3. 执行脚本完成文件替换
4. 重启程序

---

### hosts.py - hosts 文件管理模块

**功能概述：** 提供 hosts 文件的读取、解析和写入功能。

**主要类：**

| 类名 | 功能说明 |
|------|----------|
| `HostsEntry` | hosts 文件条目的表示 |
| `Hosts` | hosts 文件的管理类 |

**功能：**
- 支持 IPv4/IPv6 地址
- 支持注释和空行
- 支持从 URL 导入
- 自动去重

---

### cheats 子模块 - 金手指管理

#### cheats/cheats.py

**功能概述：** 金手指文件的扫描、解析和管理。

**主要函数：**

| 函数名 | 功能说明 |
|--------|----------|
| `scan_all_cheats_folder(mod_path)` | 扫描指定目录下的所有金手指文件夹 |
| `list_all_cheat_files_from_folder(folder_path)` | 列出文件夹中的金手指文件 |
| `load_cheat_chunk_info(cheat_file_path)` | 加载金手指条目信息 |
| `update_current_cheats(enable_titles, cheat_file_path)` | 更新启用的金手指 |
| `get_game_data()` | 获取游戏数据 (游戏ID到名称的映射) |

#### cheats/cheats_types.py

**数据类型定义：**
- `CheatEntry`: 单个金手指条目
- `CheatFile`: 金手指文件
- `CheatParseError`: 解析错误异常

#### cheats/cheats_yuzu_parser.py

**功能：** 解析 Yuzu/Citron 格式的金手指文件。

**主要函数：**

| 函数名 | 功能说明 |
|--------|----------|
| `parse_text(text)` | 解析金手指文本 |
| `parse_file(path)` | 解析金手指文件 |
| `serialize(model)` | 序列化金手指模型为文本 |

---

### dialogs.py - 文件对话框模块

**功能概述：** 基于 tkinter 的跨平台文件选择对话框。

**主要函数：**

| 函数名 | 功能说明 |
|--------|----------|
| `ask_file(file_type)` | 选择单个文件 |
| `ask_files()` | 选择多个文件 |
| `ask_folder()` | 选择文件夹 |
| `ask_file_save_location(file_type)` | 选择保存位置 |

---

### nsz_wrapper.py - NSZ 库封装

**功能概述：** 封装 nsz 库的功能，用于解析 NCA 文件。

**主要函数：**

| 函数名 | 功能说明 |
|--------|----------|
| `reload_key(key_path)` | 加载密钥文件 |
| `parse_nca_header(nca_path)` | 解析 NCA 文件头 |
| `read_firmware_version_from_nca(nca_path)` | 从 NCA 文件读取固件版本 |

---

### sentry.py - 错误追踪模块

**功能概述：** 集成 Sentry SDK 进行错误追踪和监控。

**主要函数：**

| 函数名 | 功能说明 |
|--------|----------|
| `init_sentry()` | 初始化 Sentry SDK |
| `sampler(sample_data)` | 采样函数，控制追踪比例 |

---

### external/bat_scripts.py - 批处理脚本生成

**功能概述：** 生成辅助批处理脚本。

**主要函数：**

| 函数名 | 功能说明 |
|--------|----------|
| `create_scripts()` | 创建 UI 启动模式切换脚本 |

**生成的脚本：**
- `切换 UI 启动模式.bat`: 用于切换 webview/浏览器启动模式

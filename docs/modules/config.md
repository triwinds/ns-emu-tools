# config 和 storage 模块文档

`config.py` 和 `storage.py` 是项目的配置管理模块，负责应用程序配置和持久化数据的管理。

## 文件结构

```
根目录/
├── config.py       # 应用配置管理
├── config.json     # 配置文件 (运行时生成)
├── storage.py      # 持久化存储管理
└── storage.json    # 存储文件 (运行时生成)
```

---

## config.py - 应用配置管理

### 功能概述

管理应用程序的所有配置项，包括模拟器路径、网络设置、下载设置、UI 设置等。

### 全局变量

| 变量名 | 类型 | 说明 |
|--------|------|------|
| `current_version` | `str` | 当前程序版本号 |
| `user_agent` | `str` | HTTP 请求的 User-Agent |
| `config` | `Config` | 全局配置对象 |
| `shared` | `dict` | 运行时共享数据 |
| `config_path` | `Path` | 配置文件路径 |

### 数据类定义

#### YuzuConfig - Yuzu 模拟器配置

```python
@dataclass
class YuzuConfig:
    yuzu_path: str = 'D:\\Yuzu'      # 模拟器安装路径
    yuzu_version: str = None          # 当前版本
    yuzu_firmware: str = None         # 固件版本
    branch: str = 'eden'              # 分支 (eden/citron/ea/mainline)
```

#### RyujinxConfig - Ryujinx 模拟器配置

```python
@dataclass
class RyujinxConfig:
    path: str = 'D:\\Ryujinx'         # 模拟器安装路径
    version: str = None               # 当前版本
    firmware: str = None              # 固件版本
    branch: str = 'mainline'          # 分支 (mainline/canary/ldn)
```

#### NetworkSetting - 网络设置

```python
@dataclass
class NetworkSetting:
    firmwareDownloadSource: str = 'github'           # 固件下载源 (github/nsarchive)
    githubApiMode: str = 'direct'                    # GitHub API 模式 (direct/cdn/auto-detect)
    githubDownloadMirror: str = 'cloudflare_load_balance'  # GitHub 下载镜像
    ryujinxGitLabDownloadMirror: str = 'direct'      # Ryujinx GitLab 镜像
    useDoh: bool = True                              # 是否使用 DNS over HTTPS
    proxy: str = 'system'                            # 代理设置 (system/空/代理地址)
```

#### DownloadSetting - 下载设置

```python
@dataclass
class DownloadSetting:
    autoDeleteAfterInstall: bool = True    # 安装后自动删除安装包
    disableAria2Ipv6: bool = True          # 禁用 aria2 IPv6
    removeOldAria2LogFile: bool = True     # 删除旧的 aria2 日志
    verifyFirmwareMd5: bool = True         # 验证固件 MD5
```

#### UiSetting - UI 设置

```python
@dataclass
class UiSetting:
    lastOpenEmuPage: str = 'ryujinx'       # 最后打开的模拟器页面
    dark: bool = True                       # 深色模式
    mode: str = 'auto'                      # UI 模式 (auto/webview/browser/chrome/edge)
    width: int = 1300                       # 窗口宽度
    height: int = 850                       # 窗口高度
```

#### OtherSetting - 其他设置

```python
@dataclass
class OtherSetting:
    rename_yuzu_to_cemu: bool = False      # 将 yuzu.exe 重命名为 cemu.exe
```

#### CommonSetting - 通用设置容器

```python
@dataclass
class CommonSetting:
    ui: UiSetting
    network: NetworkSetting
    download: DownloadSetting
    other: OtherSetting
```

#### Config - 主配置类

```python
@dataclass
class Config:
    yuzu: YuzuConfig
    ryujinx: RyujinxConfig
    setting: CommonSetting
```

### 主要函数

| 函数名 | 功能说明 |
|--------|----------|
| `dump_config()` | 保存配置到文件 |
| `update_last_open_emu_page(page)` | 更新最后打开的模拟器页面 |
| `update_dark_state(dark)` | 更新深色模式状态 |
| `update_setting(setting)` | 更新设置 |
| `log_versions()` | 记录系统和程序版本信息 |

### 配置文件格式

`config.json` 示例：

```json
{
  "yuzu": {
    "yuzu_path": "D:\\Yuzu",
    "yuzu_version": "0.1.0",
    "yuzu_firmware": "18.1.0",
    "branch": "citron"
  },
  "ryujinx": {
    "path": "D:\\Ryujinx",
    "version": "1.2.0",
    "firmware": "18.1.0",
    "branch": "mainline"
  },
  "setting": {
    "ui": {
      "lastOpenEmuPage": "yuzu",
      "dark": true,
      "mode": "webview",
      "width": 1300,
      "height": 850
    },
    "network": {
      "firmwareDownloadSource": "github",
      "githubApiMode": "direct",
      "githubDownloadMirror": "cloudflare_load_balance",
      "ryujinxGitLabDownloadMirror": "direct",
      "useDoh": true,
      "proxy": "system"
    },
    "download": {
      "autoDeleteAfterInstall": true,
      "disableAria2Ipv6": true,
      "removeOldAria2LogFile": true,
      "verifyFirmwareMd5": true
    },
    "other": {
      "rename_yuzu_to_cemu": false
    }
  }
}
```

### 日志配置

模块初始化时配置日志系统：

- 日志级别: DEBUG
- 日志格式: `时间|级别|模块|文件:行号|函数|消息`
- 输出目标: 控制台 + 文件 (`ns-emu-tools.log`)
- 文件轮转: 最大 10MB，保留 10 个备份

---

## storage.py - 持久化存储管理

### 功能概述

管理需要持久化但不属于配置的数据，如历史路径记录、备份路径等。

### 数据类定义

#### Storage - 存储类

```python
@dataclass
class Storage:
    yuzu_history: Dict[str, YuzuConfig] = {}       # Yuzu 路径历史
    ryujinx_history: Dict[str, RyujinxConfig] = {} # Ryujinx 路径历史
    yuzu_save_backup_path: str = 'D:\\yuzu_save_backup'  # 存档备份路径
```

### 主要函数

| 函数名 | 功能说明 | 参数 |
|--------|----------|------|
| `dump_storage()` | 保存存储到文件 | 无 |
| `add_yuzu_history(yuzu_config, dump)` | 添加 Yuzu 路径历史 | `yuzu_config: YuzuConfig`, `dump: bool` |
| `add_ryujinx_history(ryujinx_config, dump)` | 添加 Ryujinx 路径历史 | `ryujinx_config: RyujinxConfig`, `dump: bool` |
| `delete_history_path(emu_type, path_to_delete)` | 删除历史路径 | `emu_type: str`, `path_to_delete: str` |

### 存储文件格式

`storage.json` 示例：

```json
{
  "yuzu_history": {
    "D:\\Yuzu": {
      "yuzu_path": "D:\\Yuzu",
      "yuzu_version": "0.1.0",
      "yuzu_firmware": "18.1.0",
      "branch": "citron"
    },
    "E:\\Games\\Yuzu": {
      "yuzu_path": "E:\\Games\\Yuzu",
      "yuzu_version": "0.0.9",
      "yuzu_firmware": "17.0.0",
      "branch": "eden"
    }
  },
  "ryujinx_history": {
    "D:\\Ryujinx": {
      "path": "D:\\Ryujinx",
      "version": "1.2.0",
      "firmware": "18.1.0",
      "branch": "mainline"
    }
  },
  "yuzu_save_backup_path": "D:\\yuzu_save_backup"
}
```

### 使用场景

1. **路径历史**: 当用户切换模拟器安装路径时，旧路径的配置会保存到历史记录中，方便用户切换回之前的配置。

2. **存档备份路径**: 存储用户选择的存档备份目录。

---

## 使用示例

### 读取配置

```python
from config import config

# 获取 Yuzu 路径
yuzu_path = config.yuzu.yuzu_path

# 获取网络设置
use_doh = config.setting.network.useDoh
proxy = config.setting.network.proxy

# 获取 UI 设置
is_dark = config.setting.ui.dark
```

### 修改配置

```python
from config import config, dump_config

# 修改配置
config.yuzu.yuzu_version = '0.2.0'
config.setting.ui.dark = False

# 保存配置
dump_config()
```

### 管理历史路径

```python
from storage import storage, add_yuzu_history, delete_history_path
from config import YuzuConfig

# 添加历史
new_config = YuzuConfig(yuzu_path='E:\\NewYuzu')
add_yuzu_history(new_config)

# 获取历史
for path, cfg in storage.yuzu_history.items():
    print(f"{path}: {cfg.yuzu_version}")

# 删除历史
delete_history_path('yuzu', 'E:\\OldYuzu')
```

---

## 序列化说明

两个模块都使用 `dataclasses_json` 库进行 JSON 序列化：

- `@dataclass_json` 装饰器自动添加 `to_json()`, `to_dict()`, `from_json()`, `from_dict()` 方法
- `undefined=Undefined.EXCLUDE` 选项忽略未知字段，保证向后兼容性

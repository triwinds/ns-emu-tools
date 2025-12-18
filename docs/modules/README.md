# NsEmuTools 模块文档

本目录包含 NsEmuTools 项目各模块的详细文档。

## 项目概述

NsEmuTools 是一个 Nintendo Switch 模拟器管理工具，支持 Yuzu 系列 (Eden, Citron) 和 Ryujinx 模拟器的安装、更新、固件管理、存档备份等功能。

## 架构概览

```
┌─────────────────────────────────────────────────────────────┐
│                        前端 (Web UI)                         │
│                    HTML/CSS/JavaScript                       │
└─────────────────────────────────────────────────────────────┘
                              │
                              │ Eel Framework
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      API 层 (api/)                           │
│  common_api │ yuzu_api │ ryujinx_api │ cheats_api │ ...     │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    业务逻辑层 (module/)                       │
│  yuzu │ ryujinx │ firmware │ downloader │ cheats │ ...      │
└─────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│ 数据访问层       │ │ 工具层          │ │ 配置层          │
│ (repository/)   │ │ (utils/)        │ │ (config.py)     │
│                 │ │                 │ │ (storage.py)    │
└─────────────────┘ └─────────────────┘ └─────────────────┘
```

## 文档目录

| 文档 | 说明 |
|------|------|
| [module.md](module.md) | 核心业务逻辑模块 |
| [api.md](api.md) | Web API 接口层 |
| [repository.md](repository.md) | 数据访问层 |
| [utils.md](utils.md) | 工具函数模块 |
| [config.md](config.md) | 配置和存储管理 |
| [hooks.md](hooks.md) | PyInstaller 打包钩子 |

## 模块依赖关系

```
api/
 ├── 依赖 module/          (业务逻辑)
 ├── 依赖 repository/      (数据获取)
 └── 依赖 config           (配置读取)

module/
 ├── 依赖 repository/      (版本信息)
 ├── 依赖 utils/           (工具函数)
 ├── 依赖 config           (配置读取)
 └── 依赖 storage          (持久化数据)

repository/
 └── 依赖 module/network   (网络请求)

utils/
 └── 依赖 config           (配置读取)
```

## 主要功能模块

### 模拟器管理

| 模块 | 功能 |
|------|------|
| `module/yuzu.py` | Yuzu/Eden/Citron 模拟器安装、更新、版本检测 |
| `module/ryujinx.py` | Ryujinx 模拟器安装、更新、版本检测 |
| `module/firmware.py` | 固件下载、安装、版本检测 |

### 下载管理

| 模块 | 功能 |
|------|------|
| `module/downloader.py` | 基于 aria2 的文件下载 |
| `module/network.py` | 网络请求、代理、镜像源管理 |
| `utils/doh.py` | DNS over HTTPS 支持 |

### 数据管理

| 模块 | 功能 |
|------|------|
| `module/save_manager.py` | 游戏存档备份和还原 |
| `module/cheats/` | 金手指文件管理 |
| `config.py` | 应用配置管理 |
| `storage.py` | 持久化数据存储 |

### 版本信息

| 模块 | 功能 |
|------|------|
| `repository/yuzu.py` | Yuzu 系列版本信息获取 |
| `repository/ryujinx.py` | Ryujinx 版本信息获取 |
| `repository/my_info.py` | NsEmuTools 自身版本信息 |

## 技术栈

- **后端**: Python 3.x
- **前端框架**: Eel (Python-JavaScript 桥接)
- **UI 渲染**: WebView2 / 浏览器
- **下载引擎**: aria2
- **数据序列化**: dataclasses-json
- **网络请求**: requests, requests-cache, httpx
- **DNS**: dnspython (DoH 支持)

## 配置文件

| 文件 | 说明 |
|------|------|
| `config.json` | 应用配置 (模拟器路径、网络设置等) |
| `storage.json` | 持久化数据 (历史路径、备份路径等) |
| `ns-emu-tools.log` | 应用日志 |
| `aria2.log` | 下载日志 |

## 开发说明

### 添加新的 API

1. 在 `api/` 目录下创建新的 API 模块
2. 使用 `@eel.expose` 装饰器暴露函数
3. 使用 `@generic_api` 装饰器自动处理响应和异常
4. 在 `api/__init__.py` 的 `__all__` 中添加模块名

### 添加新的模拟器支持

1. 在 `repository/` 下添加版本信息获取模块
2. 在 `module/` 下添加模拟器管理模块
3. 在 `api/` 下添加对应的 API 模块
4. 在 `config.py` 中添加配置数据类

### 日志记录

```python
import logging
logger = logging.getLogger(__name__)

logger.debug('调试信息')
logger.info('一般信息')
logger.warning('警告信息')
logger.error('错误信息')
```

### 消息通知

```python
from module.msg_notifier import send_notify

send_notify('操作完成')  # 发送通知到前端
```

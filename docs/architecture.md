# NS Emu Tools 架构文档

## 项目概述

NS Emu Tools 是一个用于安装和更新 Nintendo Switch 模拟器的桌面应用程序。该项目采用前后端分离架构，使用 Python 作为后端，Vue.js 作为前端，通过 Eel/Webview 实现桌面应用界面。

**版本**: 0.5.9
**Python 版本要求**: >= 3.11
**Node.js 版本要求**: 20
**许可证**: AGPL-3.0

## 核心功能

- 支持安装和更新多个 NS 模拟器：
  - Ryujinx (正式版/Canary版)
  - Eden 模拟器
  - Citron 模拟器
  - ~~Yuzu (已停止开发)~~
- 自动检测并安装 MSVC 运行库
- NS 固件安装和更新
- 固件版本检测
- 模拟器密钥管理
- Yuzu 金手指管理
- Aria2 多线程下载支持

## 整体架构

### 架构图

```
┌─────────────────────────────────────────────────────────────┐
│                        用户界面层                              │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Frontend (Vue.js + Vuetify)                         │   │
│  │  - 组件化 UI                                          │   │
│  │  - Pinia 状态管理                                     │   │
│  │  - Vue Router 路由                                    │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
                            ↕ (Eel/Webview Bridge)
┌─────────────────────────────────────────────────────────────┐
│                        API 层                                 │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  API Modules (@eel.expose)                           │   │
│  │  - updater_api.py                                    │   │
│  │  - ryujinx_api.py                                    │   │
│  │  - yuzu_api.py                                       │   │
│  │  - save_manager_api.py                               │   │
│  │  - cheats_api.py                                     │   │
│  │  - common_api.py                                     │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
                            ↕
┌─────────────────────────────────────────────────────────────┐
│                      业务逻辑层 (Module)                       │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  - updater.py (自更新逻辑)                            │   │
│  │  - ryujinx.py (Ryujinx 管理)                         │   │
│  │  - yuzu.py (Yuzu 管理)                               │   │
│  │  - firmware.py (固件管理)                             │   │
│  │  - downloader.py (Aria2 下载)                        │   │
│  │  - save_manager.py (存档管理)                         │   │
│  │  - network.py (网络工具)                              │   │
│  │  - common.py (通用工具)                               │   │
│  │  - msg_notifier.py (消息通知)                         │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
                            ↕
┌─────────────────────────────────────────────────────────────┐
│                    数据访问层 (Repository)                     │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  - ryujinx.py (GitLab API)                           │   │
│  │  - yuzu.py (GitHub API)                              │   │
│  │  - my_info.py (项目自身信息)                          │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
                            ↕
┌─────────────────────────────────────────────────────────────┐
│                      外部服务                                 │
│  - GitHub API (发布信息)                                      │
│  - GitLab API (Ryujinx 发布)                                 │
│  - 固件下载源                                                 │
│  - Aria2 下载引擎                                             │
└─────────────────────────────────────────────────────────────┘
```

## 目录结构

```
ns-emu-tools/
├── api/                    # API 层 - 暴露给前端的接口
│   ├── __init__.py        # 自动导入所有 API 模块
│   ├── common_api.py      # 通用 API
│   ├── updater_api.py     # 自更新 API
│   ├── ryujinx_api.py     # Ryujinx 相关 API
│   ├── yuzu_api.py        # Yuzu 相关 API
│   ├── save_manager_api.py # 存档管理 API
│   ├── cheats_api.py      # 金手指 API
│   └── common_response.py # 统一响应格式
│
├── module/                 # 业务逻辑层
│   ├── updater.py         # 自更新逻辑
│   ├── ryujinx.py         # Ryujinx 安装/更新
│   ├── yuzu.py            # Yuzu 安装/更新
│   ├── firmware.py        # 固件管理
│   ├── downloader.py      # Aria2 下载管理
│   ├── save_manager.py    # 存档管理
│   ├── network.py         # 网络工具
│   ├── common.py          # 通用工具函数
│   ├── msg_notifier.py    # 消息通知系统
│   ├── nsz_wrapper.py     # NSZ 固件解析
│   ├── dialogs.py         # 对话框工具
│   ├── hosts.py           # Hosts 文件管理
│   ├── sentry.py          # 错误追踪
│   └── external/          # 外部脚本
│       └── bat_scripts.py # Windows 批处理脚本
│
├── repository/             # 数据访问层
│   ├── ryujinx.py         # Ryujinx GitLab API
│   ├── yuzu.py            # Yuzu GitHub API
│   ├── my_info.py         # 项目自身信息
│   └── domain/            # 数据模型
│       └── release_info.py
│
├── utils/                  # 工具类
│   ├── admin.py           # 管理员权限
│   ├── hardware.py        # 硬件信息
│   ├── string_util.py     # 字符串工具
│   ├── doh.py             # DNS over HTTPS
│   ├── webview2.py        # WebView2 组件
│   ├── package.py         # 包管理
│   └── common.py          # 通用工具
│
├── exception/              # 异常定义
│   ├── common_exception.py
│   ├── download_exception.py
│   └── install_exception.py
│
├── frontend/               # 前端项目
│   ├── src/
│   │   ├── components/    # Vue 组件
│   │   ├── layouts/       # 布局组件
│   │   ├── pages/         # 页面组件
│   │   ├── stores/        # Pinia 状态管理
│   │   ├── plugins/       # Vue 插件
│   │   ├── router/        # 路由配置
│   │   ├── types/         # TypeScript 类型
│   │   └── utils/         # 前端工具
│   ├── package.json
│   └── vite.config.ts
│
├── build_tools/            # 构建工具
│   └── zip_files.py
│
├── hooks/                  # Git hooks
│
├── web/                    # 编译后的前端资源
│
├── config.py              # 配置管理
├── ui.py                  # 浏览器模式入口
├── ui_webview.py          # WebView 模式入口
├── main.py                # 主入口
├── pyproject.toml         # Python 项目配置
└── README.md

```

## 核心模块详解

### 1. 入口层 (Entry Points)

#### ui.py - 浏览器模式
- 使用 Eel 框架启动 Web 服务器
- 支持 Chrome、Edge 或默认浏览器
- 自动检测可用浏览器并选择最佳模式
- 使用 gevent 进行异步处理

#### ui_webview.py - WebView 模式
- 使用 pywebview 创建原生窗口
- 集成 WebView2 组件
- 提供更好的桌面应用体验
- 支持窗口最大化和尺寸管理

#### 启动流程
```python
1. 导入 API 模块 (import_api_modules)
2. 初始化 Eel (eel.init)
3. 获取可用端口
4. 创建窗口/启动浏览器
5. 启动 Eel 服务器
```

### 2. API 层

#### 设计模式
- 使用 `@eel.expose` 装饰器暴露 Python 函数给前端
- 统一的响应格式 (success_response / exception_response)
- 自动模块导入机制 (`api/__init__.py`)

#### 主要 API 模块

**updater_api.py** - 自更新
```python
- check_update()           # 检查更新
- download_net_by_tag()    # 下载指定版本
- update_net_by_tag()      # 更新到指定版本
- load_change_log()        # 加载更新日志
```

**ryujinx_api.py** - Ryujinx 管理
```python
- get_ryujinx_version()    # 获取版本信息
- install_ryujinx()        # 安装模拟器
- update_ryujinx()         # 更新模拟器
- install_firmware()       # 安装固件
```

**common_api.py** - 通用功能
```python
- get_config()             # 获取配置
- update_config()          # 更新配置
- open_folder()            # 打开文件夹
- check_msvc()             # 检查 MSVC 运行库
```

#### 响应格式
```python
# 成功响应
{
    "success": True,
    "data": <返回数据>,
    "message": <可选消息>
}

# 错误响应
{
    "success": False,
    "error": <错误信息>,
    "traceback": <堆栈跟踪>
}
```

### 3. 业务逻辑层 (Module)

#### downloader.py - 下载管理
- **核心功能**:
  - 集成 Aria2 多线程下载
  - 自定义进度条 (MyTqdm)
  - 实时下载速度和 ETA 显示
  - 支持代理配置
  - 自动重试机制

- **关键类**:
  ```python
  class MyTqdm(tqdm):
      # 自定义进度条，集成 aria2p.Download
      # 实时更新下载进度到前端
  ```

#### updater.py - 自更新系统
- **更新流程**:
  1. 从 GitHub API 获取最新版本
  2. 版本比较 (支持预发布版本)
  3. 下载更新包
  4. 生成更新脚本 (Windows batch)
  5. 优雅关闭当前进程
  6. 执行更新并重启

- **更新脚本特性**:
  - 优雅关闭 (taskkill)
  - 强制终止备份
  - 文件备份 (.bak)
  - 清理临时文件
  - 自动重启

#### ryujinx.py / yuzu.py - 模拟器管理
- **安装流程**:
  1. 检查本地版本
  2. 获取远程版本信息
  3. 下载安装包
  4. 解压到目标目录
  5. 验证安装
  6. 更新配置

- **版本检测**:
  - 读取本地版本文件
  - 对比远程版本
  - 支持多分支 (mainline/canary)

#### firmware.py - 固件管理
- **固件来源**:
  - GitHub (THZoria/NX_Firmware)
  - darthsternie.net
- **解析工具**: NSZ (nicoboss/nsz)
- **功能**:
  - 固件下载
  - 版本检测
  - MD5 校验
  - 自动安装到模拟器

#### network.py - 网络工具
- **功能**:
  - 端口可用性检测
  - GitHub 镜像源支持
  - DoH (DNS over HTTPS)
  - 代理配置
  - 请求缓存 (requests-cache)

#### msg_notifier.py - 消息通知
- **通知机制**:
  - 通过 Eel 发送实时消息到前端
  - 支持进度更新
  - 错误通知
  - 状态变更通知

### 4. 数据访问层 (Repository)

#### ryujinx.py
- **数据源**: GitLab API (git.ryujinx.app)
- **API 端点**:
  - Mainline: `/api/v4/projects/1/releases`
  - Canary: `/api/v4/projects/68/releases`
- **数据转换**: GitLab API → ReleaseInfo 模型

#### yuzu.py
- **数据源**: GitHub API
- **支持分支**:
  - Eden (eden-emu.dev)
  - Citron (citron-emu.org)
- **功能**: 获取发布信息、下载链接

#### my_info.py
- **功能**: 管理项目自身的发布信息
- **数据源**: GitHub Releases API

### 5. 配置管理 (config.py)

#### 配置结构
```python
@dataclass
class Config:
    yuzu: YuzuConfig          # Yuzu 配置
    ryujinx: RyujinxConfig    # Ryujinx 配置
    setting: CommonSetting    # 通用设置

@dataclass
class CommonSetting:
    ui: UiSetting             # UI 设置
    network: NetworkSetting   # 网络设置
    download: DownloadSetting # 下载设置
    other: OtherSetting       # 其他设置
```

#### 配置持久化
- 文件格式: JSON
- 位置: `config.json`
- 使用 dataclasses-json 进行序列化

#### 日志系统
- 使用 Python logging 模块
- RotatingFileHandler (10MB, 10 个备份)
- 日志文件: `ns-emu-tools.log`
- 日志级别: DEBUG

### 6. 前端架构 (Frontend)

#### 技术栈
- **框架**: Vue 3 (Composition API)
- **UI 库**: Vuetify 3
- **状态管理**: Pinia
- **路由**: Vue Router 4
- **构建工具**: Vite
- **语言**: TypeScript

#### 目录结构
```
frontend/src/
├── components/          # 可复用组件
│   ├── ChangeLogDialog.vue
│   ├── ConsoleDialog.vue
│   ├── NewVersionDialog.vue
│   └── ...
├── layouts/            # 布局组件
│   ├── default.vue
│   ├── AppBar.vue
│   └── AppDrawer.vue
├── pages/              # 页面组件
│   ├── index.vue
│   ├── ryujinx.vue
│   ├── yuzu.vue
│   ├── settings.vue
│   └── ...
├── stores/             # Pinia 状态管理
│   ├── app.ts
│   ├── ConfigStore.ts
│   ├── ConsoleDialogStore.ts
│   └── YuzuSaveStore.ts
├── plugins/            # Vue 插件
│   ├── vuetify.ts
│   └── mitt.ts
├── router/             # 路由配置
│   └── index.ts
└── utils/              # 工具函数
    ├── common.ts
    └── markdown.ts
```

#### 状态管理 (Pinia Stores)

**ConfigStore** - 配置管理
```typescript
- config: 应用配置
- loadConfig(): 加载配置
- updateConfig(): 更新配置
```

**ConsoleDialogStore** - 控制台对话框
```typescript
- messages: 消息列表
- addMessage(): 添加消息
- clear(): 清空消息
```

**YuzuSaveStore** - 存档管理
```typescript
- saves: 存档列表
- loadSaves(): 加载存档
- backupSave(): 备份存档
```

#### 前后端通信

**Eel Bridge**
```typescript
// 调用 Python 函数
await window.eel.check_update()()

// Python 调用前端
eel.update_progress(progress)
```

**消息通知**
```typescript
// 监听后端消息
window.eel.expose(handleMessage, 'handle_message')
```

### 7. 工具类 (Utils)

#### admin.py - 管理员权限
- 检查是否以管理员运行
- 请求管理员权限提升
- UAC 对话框处理

#### hardware.py - 硬件信息
- CPU 信息检测
- 内存信息
- 磁盘空间

#### doh.py - DNS over HTTPS
- 使用 dnspython 实现
- 支持多个 DoH 提供商
- 提高 DNS 解析速度和安全性

#### webview2.py - WebView2 管理
- 检查 WebView2 运行时
- 自动下载和安装
- 版本检测

### 8. 异常处理 (Exception)

#### 异常层次结构
```python
exception/
├── common_exception.py      # 通用异常
├── download_exception.py    # 下载相关异常
└── install_exception.py     # 安装相关异常
```

#### 异常处理策略
- API 层捕获所有异常
- 转换为统一的错误响应
- 记录详细的堆栈跟踪
- 发送到 Sentry (可选)

## 数据流

### 典型操作流程: 安装 Ryujinx

```
1. 用户点击"安装 Ryujinx"按钮
   ↓
2. Frontend 调用 window.eel.install_ryujinx()
   ↓
3. API 层 (ryujinx_api.py) 接收请求
   ↓
4. 调用 Module 层 (ryujinx.py) 的安装逻辑
   ↓
5. Repository 层 (ryujinx.py) 获取最新版本信息
   ↓
6. Module 层 (downloader.py) 下载安装包
   ↓ (实时进度通知)
7. Module 层 (ryujinx.py) 解压并安装
   ↓
8. 更新配置 (config.py)
   ↓
9. 返回成功响应到 Frontend
   ↓
10. Frontend 更新 UI 状态
```

### 消息通知流程

```
Backend (Module)
    ↓ send_notify(message)
msg_notifier.py
    ↓ eel.update_message(message)
Frontend (JavaScript)
    ↓ window.eel.update_message
ConsoleDialogStore
    ↓ addMessage(message)
UI 更新
```

## 构建和部署

### 开发环境

#### 后端设置
```bash
# 使用 uv (推荐)
uv sync

# 或使用 pip
python -m venv venv
venv\Scripts\activate
pip install -r requirements.txt
```

#### 前端设置
```bash
cd frontend
bun install
bun build
```

### 运行模式

#### 开发模式
```bash
# 后端
uv run python ui.py

# 前端 (另一个终端)
cd frontend
bun dev
```

#### 生产模式
```bash
# 浏览器模式
python ui.py

# WebView 模式
python ui_webview.py
```

### 打包发布

#### PyInstaller 配置
- 使用 PyInstaller 打包为独立 exe
- 两个版本:
  - `NsEmuTools.exe` (无控制台窗口)
  - `NsEmuTools-console.exe` (带控制台窗口)

#### CI/CD
- GitHub Actions 自动构建
- 自动发布到 GitHub Releases
- 支持预发布版本

## 依赖管理

### Python 依赖 (pyproject.toml)

**核心依赖**:
- `eel>=0.18.0` - Python/JavaScript 桥接
- `pywebview>=4.2.2` - 原生窗口
- `aria2p>=0.11.3` - 下载管理
- `requests>=2.31.0` - HTTP 请求
- `py7zr>=0.20.6` - 7z 解压
- `beautifulsoup4>=4.12.2` - HTML 解析
- `dnspython[doh]>=2.4.2` - DNS over HTTPS
- `sentry-sdk>=1.29.2` - 错误追踪
- `dataclasses-json>=0.5.14` - 数据类序列化
- `nsz` - NS 固件解析 (自定义 fork)

### 前端依赖 (package.json)

**核心依赖**:
- `vue@^3.4.31` - 前端框架
- `vuetify@^3.6.14` - UI 组件库
- `pinia@^2.1.7` - 状态管理
- `vue-router@^4.4.0` - 路由
- `mitt@^3.0.1` - 事件总线
- `marked@^15.0.7` - Markdown 渲染

**开发依赖**:
- `vite@^5.4.10` - 构建工具
- `typescript@~5.6.3` - 类型检查
- `eslint@^9.14.0` - 代码检查

## 网络架构

### GitHub 镜像源
- 支持多个 GitHub 下载镜像
- 自动选择最快的镜像
- 镜像列表可配置

### DNS over HTTPS
- 提高 DNS 解析速度
- 绕过 DNS 污染
- 支持 IPv6

### 代理支持
- 系统代理
- 自定义代理
- Aria2 代理配置

## 安全性

### 权限管理
- 最小权限原则
- 仅在需要时请求管理员权限
- UAC 提示

### 数据安全
- 本地配置加密 (可选)
- 不存储敏感信息
- HTTPS 通信

### 错误追踪
- Sentry 集成
- 匿名错误报告
- 用户可选择退出

## 性能优化

### 下载优化
- Aria2 多线程下载
- 断点续传
- 自动重试

### 缓存策略
- HTTP 请求缓存 (requests-cache)
- 15 分钟缓存过期
- 减少 API 调用

### 异步处理
- gevent 协程
- 非阻塞 I/O
- 后台任务

## 扩展性

### 添加新模拟器

1. **创建 Repository 模块**
   ```python
   # repository/new_emulator.py
   def get_all_releases():
       # 实现获取发布信息
       pass
   ```

2. **创建 Module 模块**
   ```python
   # module/new_emulator.py
   def install_emulator():
       # 实现安装逻辑
       pass
   ```

3. **创建 API 模块**
   ```python
   # api/new_emulator_api.py
   @eel.expose
   def install_new_emulator():
       # 暴露 API
       pass
   ```

4. **添加前端页面**
   ```vue
   <!-- frontend/src/pages/new-emulator.vue -->
   <template>
     <!-- UI 实现 -->
   </template>
   ```

### 插件系统
- 目前不支持插件
- 未来可考虑添加插件机制

## 测试策略

### 单元测试
- 目前缺少单元测试
- 建议使用 pytest

### 集成测试
- 手动测试
- CI/CD 自动构建测试

### 端到端测试
- 手动测试主要流程
- 建议添加自动化 E2E 测试

## 已知问题和限制

1. **Yuzu 停止开发**: Yuzu 模拟器已停止开发，相关功能保留但不再更新
2. **Windows 专用**: 主要针对 Windows 平台，其他平台支持有限
3. **WebView2 依赖**: WebView 模式需要 WebView2 运行时
4. **网络依赖**: 需要稳定的网络连接访问 GitHub/GitLab

## 未来规划

### 短期目标
- 添加更多模拟器支持
- 改进错误处理
- 添加单元测试

### 长期目标
- 跨平台支持 (Linux, macOS)
- 插件系统
- 自动化测试
- 性能监控

## 贡献指南

### 代码风格
- Python: PEP 8
- TypeScript: ESLint 配置
- 提交信息: Conventional Commits

### 开发流程
1. Fork 项目
2. 创建功能分支
3. 提交代码
4. 创建 Pull Request

### 调试技巧

#### Python 调试
- 不建议在 Eel 启动时调试 (gevent 冲突)
- 使用 IDE 的 main 方法调试
- 查看 `ns-emu-tools.log` 日志

#### 前端调试
- 使用浏览器开发者工具
- Vue DevTools
- 查看控制台输出

## 参考资源

### 官方文档
- [Eel 文档](https://github.com/python-eel/Eel)
- [Vue 3 文档](https://vuejs.org/)
- [Vuetify 3 文档](https://vuetifyjs.com/)
- [Aria2 文档](https://aria2.github.io/)

### 相关项目
- [Ryujinx](https://ryujinx.app/)
- [Eden](https://eden-emu.dev/)
- [Citron](https://citron-emu.org/)
- [NSZ](https://github.com/nicoboss/nsz)

## 许可证

本项目采用 AGPL-3.0 许可证。详见 [LICENSE](../LICENSE) 文件。

## 联系方式

- **GitHub**: [triwinds/ns-emu-tools](https://github.com/triwinds/ns-emu-tools)
- **Telegram**: [讨论组](https://t.me/+mxI34BRClLUwZDcx)
- **Issues**: [GitHub Issues](https://github.com/triwinds/ns-emu-tools/issues)

---

**文档版本**: 1.0
**最后更新**: 2025-12-18
**维护者**: triwinds

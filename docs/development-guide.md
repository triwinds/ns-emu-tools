# 开发指南

本文档为 NS Emu Tools 项目的开发者提供详细的开发指导。

## 目录

- [环境准备](#环境准备)
- [项目结构](#项目结构)
- [开发流程](#开发流程)
- [代码规范](#代码规范)
- [调试技巧](#调试技巧)
- [测试](#测试)
- [常见问题](#常见问题)

## 环境准备

### 系统要求

- **操作系统**: Windows 10/11 (主要开发平台)
- **Python**: 3.11 或更高版本
- **Node.js**: 20.x
- **包管理器**:
  - Python: uv (推荐) 或 pip
  - Node.js: bun (推荐) 或 npm

### 安装开发工具

#### 1. 安装 Python

```bash
# 下载并安装 Python 3.11+
# https://www.python.org/downloads/

# 验证安装
python --version
```

#### 2. 安装 uv (推荐)

```bash
# Windows (PowerShell)
irm https://astral.sh/uv/install.ps1 | iex

# 验证安装
uv --version
```

#### 3. 安装 Node.js 和 bun

```bash
# 下载并安装 Node.js 20
# https://nodejs.org/

# 安装 bun
npm install -g bun

# 验证安装
node --version
bun --version
```

#### 4. 安装 Git

```bash
# 下载并安装 Git
# https://git-scm.com/

# 验证安装
git --version
```

### 克隆项目

```bash
# 克隆仓库
git clone https://github.com/triwinds/ns-emu-tools.git
cd ns-emu-tools
```

### 安装依赖

#### 后端依赖

```bash
# 使用 uv (推荐)
uv sync

# 或使用 pip
python -m venv venv
venv\Scripts\activate
pip install -e .
```

#### 前端依赖

```bash
cd frontend
bun install
# 或使用 npm
npm install
```

### 构建前端

```bash
cd frontend
bun build
# 或
npm run build
```

构建完成后，前端资源会输出到 `web/` 目录。

## 项目结构

详细的项目结构请参考 [架构文档](architecture.md#目录结构)。

### 关键文件说明

```
ns-emu-tools/
├── config.py              # 配置管理和日志设置
├── ui.py                  # 浏览器模式入口
├── ui_webview.py          # WebView 模式入口
├── main.py                # 主入口 (选择启动模式)
├── pyproject.toml         # Python 项目配置
├── requirements.txt       # Python 依赖列表
├── .gitignore            # Git 忽略文件
└── README.md             # 项目说明
```

## 开发流程

### 1. 创建功能分支

```bash
# 从 main 分支创建新分支
git checkout -b feature/your-feature-name
```

### 2. 开发模式运行

#### 方式一: 前后端分离开发 (推荐)

**终端 1 - 启动后端**:
```bash
# 激活虚拟环境 (如果使用 venv)
venv\Scripts\activate

# 启动后端服务
uv run python ui.py
# 或
python ui.py
```

**终端 2 - 启动前端开发服务器**:
```bash
cd frontend
bun dev
# 或
npm run dev
```

前端开发服务器会在 `http://localhost:5173` 启动，支持热重载。

#### 方式二: 生产模式运行

```bash
# 先构建前端
cd frontend
bun build

# 返回根目录
cd ..

# 启动应用
python ui.py
# 或
python ui_webview.py
```

### 3. 代码修改

#### 后端开发

1. 在 `module/` 中添加业务逻辑
2. 在 `api/` 中暴露 API 接口
3. 在 `repository/` 中添加数据访问逻辑
4. 更新 `config.py` (如需新配置)

**示例: 添加新模拟器支持**

```python
# 1. repository/new_emulator.py
from module.network import session

def get_all_releases():
    """获取所有发布版本"""
    resp = session.get('https://api.example.com/releases').json()
    return resp

# 2. module/new_emulator.py
from repository.new_emulator import get_all_releases
from module.downloader import download
import py7zr

def install_emulator(version=None):
    """安装模拟器"""
    releases = get_all_releases()
    target_release = releases[0] if not version else find_version(releases, version)

    # 下载
    file_path = download(target_release['download_url'])

    # 解压
    with py7zr.SevenZipFile(file_path, 'r') as archive:
        archive.extractall(config.new_emulator.path)

    return True

# 3. api/new_emulator_api.py
import eel
from api.common_response import success_response, exception_response

@eel.expose
def install_new_emulator(version=None):
    from module.new_emulator import install_emulator
    try:
        result = install_emulator(version)
        return success_response(result)
    except Exception as e:
        return exception_response(e)
```

#### 前端开发

1. 在 `frontend/src/pages/` 中添加页面
2. 在 `frontend/src/components/` 中添加组件
3. 在 `frontend/src/stores/` 中添加状态管理
4. 更新路由配置 (如需要)

**示例: 添加新页面**

```vue
<!-- frontend/src/pages/new-emulator.vue -->
<template>
  <v-container>
    <v-card>
      <v-card-title>新模拟器管理</v-card-title>
      <v-card-text>
        <v-btn @click="install" :loading="loading">
          安装模拟器
        </v-btn>
      </v-card-text>
    </v-card>
  </v-container>
</template>

<script setup lang="ts">
import { ref } from 'vue'

const loading = ref(false)

async function install() {
  loading.value = true
  try {
    const result = await window.eel.install_new_emulator()()
    if (result.success) {
      console.log('安装成功')
    } else {
      console.error('安装失败:', result.error)
    }
  } finally {
    loading.value = false
  }
}
</script>
```

### 4. 提交代码

```bash
# 添加修改的文件
git add .

# 提交 (使用 Conventional Commits 格式)
git commit -m "feat: 添加新模拟器支持"

# 推送到远程
git push origin feature/your-feature-name
```

### 5. 创建 Pull Request

1. 在 GitHub 上创建 Pull Request
2. 填写 PR 描述
3. 等待代码审查
4. 根据反馈修改代码
5. 合并到 main 分支

## 代码规范

### Python 代码规范

遵循 [PEP 8](https://pep8.org/) 规范:

```python
# 好的示例
def install_emulator(version: str, branch: str = 'mainline') -> bool:
    """
    安装模拟器

    Args:
        version: 版本号
        branch: 分支名称

    Returns:
        是否安装成功
    """
    logger.info(f'Installing emulator version {version}')
    # 实现...
    return True

# 避免
def InstallEmulator(version,branch='mainline'):
    print('installing')
    return True
```

**关键点**:
- 使用 4 空格缩进
- 函数名使用 snake_case
- 类名使用 PascalCase
- 常量使用 UPPER_CASE
- 添加类型注解
- 编写文档字符串

### TypeScript 代码规范

遵循项目的 ESLint 配置:

```typescript
// 好的示例
interface EmulatorConfig {
  path: string
  version: string
  branch: 'mainline' | 'canary'
}

async function installEmulator(config: EmulatorConfig): Promise<boolean> {
  const result = await window.eel.install_emulator(config.version)()
  return result.success
}

// 避免
function InstallEmulator(config) {
  window.eel.install_emulator(config.version)()
}
```

**关键点**:
- 使用 2 空格缩进
- 使用 camelCase 命名
- 添加类型注解
- 使用 async/await
- 避免 any 类型

### Git Commit 规范

使用 [Conventional Commits](https://www.conventionalcommits.org/):

```bash
# 格式
<type>(<scope>): <subject>

# 类型
feat:     新功能
fix:      修复 bug
docs:     文档更新
style:    代码格式 (不影响功能)
refactor: 重构
test:     测试
chore:    构建/工具

# 示例
feat(ryujinx): 添加 Canary 分支支持
fix(downloader): 修复下载进度显示错误
docs(api): 更新 API 文档
refactor(config): 重构配置管理模块
```

## 调试技巧

### Python 调试

#### 1. 使用日志

```python
import logging
logger = logging.getLogger(__name__)

def some_function():
    logger.debug('调试信息')
    logger.info('普通信息')
    logger.warning('警告信息')
    logger.error('错误信息')
```

日志文件位置: `ns-emu-tools.log`

#### 2. 使用 IDE 调试器

**PyCharm**:
1. 设置断点
2. 右键 `ui.py` → Debug
3. 注意: gevent 可能与调试器冲突

**VS Code**:
```json
// .vscode/launch.json
{
  "version": "0.2.0",
  "configurations": [
    {
      "name": "Python: UI",
      "type": "python",
      "request": "launch",
      "program": "${workspaceFolder}/ui.py",
      "console": "integratedTerminal"
    }
  ]
}
```

#### 3. 单元测试调试

```python
# 在模块中添加 main 函数
if __name__ == '__main__':
    # 测试代码
    from module.ryujinx import install_ryujinx
    install_ryujinx('mainline')
```

### 前端调试

#### 1. 浏览器开发者工具

- F12 打开开发者工具
- Console: 查看日志和错误
- Network: 查看网络请求
- Sources: 设置断点调试

#### 2. Vue DevTools

安装 [Vue DevTools](https://devtools.vuejs.org/) 浏览器扩展:
- 查看组件树
- 检查组件状态
- 查看 Pinia store
- 时间旅行调试

#### 3. 调试 Eel 通信

```javascript
// 在前端添加日志
const originalEel = window.eel
window.eel = new Proxy(originalEel, {
  get(target, prop) {
    return (...args) => {
      console.log(`Calling eel.${prop}`, args)
      return target[prop](...args)
    }
  }
})
```

### 常见调试场景

#### 下载失败

1. 检查 `aria2.log` 文件
2. 检查网络连接
3. 检查代理设置
4. 验证下载 URL

#### 安装失败

1. 检查磁盘空间
2. 检查文件权限
3. 检查目标路径
4. 查看详细错误日志

#### 前端无响应

1. 检查后端是否启动
2. 检查端口是否被占用
3. 检查浏览器控制台错误
4. 验证 Eel 连接

## 测试

### 单元测试

目前项目缺少单元测试，建议添加:

```python
# tests/test_downloader.py
import pytest
from module.downloader import download

def test_download():
    url = 'https://example.com/file.zip'
    result = download(url)
    assert result.exists()
```

运行测试:
```bash
pytest tests/
```

### 集成测试

手动测试主要流程:

1. **安装模拟器**
   - 测试 Ryujinx mainline 安装
   - 测试 Ryujinx canary 安装
   - 测试 Eden 安装
   - 测试 Citron 安装

2. **更新模拟器**
   - 测试版本检测
   - 测试更新流程
   - 验证更新后版本

3. **固件管理**
   - 测试固件下载
   - 测试固件安装
   - 验证固件版本

4. **自更新**
   - 测试更新检测
   - 测试更新下载
   - 测试更新安装

### 端到端测试

建议使用 Playwright 或 Selenium 进行自动化测试:

```python
# tests/e2e/test_install.py
from playwright.sync_api import sync_playwright

def test_install_ryujinx():
    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page()
        page.goto('http://localhost:8888')

        # 点击安装按钮
        page.click('text=安装 Ryujinx')

        # 等待安装完成
        page.wait_for_selector('text=安装成功')

        browser.close()
```

## 常见问题

### Q: gevent 与调试器冲突怎么办?

A: 不要在 Eel 启动时调试。创建独立的测试脚本:

```python
# test_module.py
if __name__ == '__main__':
    from module.ryujinx import install_ryujinx
    install_ryujinx('mainline')
```

### Q: 前端修改不生效?

A: 确保使用开发模式:
```bash
cd frontend
bun dev  # 启动开发服务器
```

### Q: Aria2 启动失败?

A: 检查:
1. `module/aria2c.exe` 是否存在
2. 端口是否被占用
3. 防火墙设置

### Q: WebView2 组件缺失?

A: 运行 `ui_webview.py` 会自动检测并提示安装。

### Q: 如何添加新的配置项?

A: 修改 `config.py`:

```python
@dataclass
class CommonSetting:
    ui: UiSetting = field(default_factory=UiSetting)
    network: NetworkSetting = field(default_factory=NetworkSetting)
    download: DownloadSetting = field(default_factory=DownloadSetting)
    new_setting: NewSetting = field(default_factory=NewSetting)  # 新增
```

### Q: 如何添加新的 API?

A: 在 `api/` 目录创建新文件:

```python
# api/new_api.py
import eel
from api.common_response import success_response

@eel.expose
def new_function():
    return success_response('Hello')
```

API 会自动被 `api/__init__.py` 导入。

### Q: 如何处理大文件下载?

A: 使用 Aria2:

```python
from module.downloader import download

file_path = download(
    url='https://example.com/large-file.zip',
    filename='large-file.zip'
)
```

### Q: 如何添加新的下载源?

A: 修改 `module/network.py`:

```python
def get_download_url(original_url):
    if config.setting.network.githubDownloadMirror == 'new_mirror':
        return original_url.replace('github.com', 'new-mirror.com')
    return original_url
```

## 性能优化

### 后端优化

1. **使用缓存**
```python
from requests_cache import CachedSession

session = CachedSession(
    'cache',
    expire_after=900  # 15 分钟
)
```

2. **异步处理**
```python
import gevent

def long_task():
    # 长时间任务
    pass

gevent.spawn(long_task)
```

3. **减少日志输出**
```python
# 生产环境使用 INFO 级别
logging.basicConfig(level=logging.INFO)
```

### 前端优化

1. **懒加载组件**
```typescript
const HeavyComponent = defineAsyncComponent(() =>
  import('./components/HeavyComponent.vue')
)
```

2. **使用 computed 缓存**
```typescript
const expensiveValue = computed(() => {
  return heavyCalculation(data.value)
})
```

3. **避免不必要的响应式**
```typescript
const staticData = markRaw(largeObject)
```

## 贡献指南

### 提交 PR 前检查清单

- [ ] 代码遵循项目规范
- [ ] 添加必要的注释和文档
- [ ] 测试所有修改的功能
- [ ] 更新相关文档
- [ ] Commit 信息符合规范
- [ ] 没有引入新的警告或错误

### 代码审查

PR 会经过以下审查:
1. 代码质量
2. 功能完整性
3. 测试覆盖
4. 文档完整性
5. 性能影响

## 资源链接

### 官方文档
- [Python 文档](https://docs.python.org/3/)
- [Vue 3 文档](https://vuejs.org/)
- [Vuetify 3 文档](https://vuetifyjs.com/)
- [Eel 文档](https://github.com/python-eel/Eel)

### 工具
- [uv](https://github.com/astral-sh/uv)
- [bun](https://bun.sh/)
- [PyCharm](https://www.jetbrains.com/pycharm/)
- [VS Code](https://code.visualstudio.com/)

### 社区
- [GitHub Issues](https://github.com/triwinds/ns-emu-tools/issues)
- [Telegram 讨论组](https://t.me/+mxI34BRClLUwZDcx)

---

**文档版本**: 1.0
**最后更新**: 2025-12-18

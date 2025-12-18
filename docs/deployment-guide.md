# 部署指南

本文档详细说明如何构建和部署 NS Emu Tools 应用程序。

## 目录

- [构建准备](#构建准备)
- [本地构建](#本地构建)
- [CI/CD 自动构建](#cicd-自动构建)
- [发布流程](#发布流程)
- [分发策略](#分发策略)
- [故障排查](#故障排查)

## 构建准备

### 环境要求

- **操作系统**: Windows 10/11
- **Python**: 3.11+
- **Node.js**: 20.x
- **磁盘空间**: 至少 2GB 可用空间

### 安装构建工具

```bash
# 安装 Python 依赖
uv sync

# 安装前端依赖
cd frontend
bun install
```

## 本地构建

### 1. 构建前端

```bash
cd frontend

# 安装依赖 (如果还没安装)
bun install

# 构建生产版本
bun build

# 输出目录: ../web/
```

**构建配置** (`frontend/vite.config.ts`):
```typescript
export default defineConfig({
  build: {
    outDir: '../web',
    emptyOutDir: true,
    rollupOptions: {
      output: {
        manualChunks: {
          'vue-vendor': ['vue', 'vue-router', 'pinia'],
          'vuetify': ['vuetify']
        }
      }
    }
  }
})
```

### 2. 构建后端可执行文件

#### 使用 PyInstaller

项目使用 PyInstaller 将 Python 应用打包为独立的 exe 文件。

**构建脚本** (`build.spec`):
```python
# -*- mode: python ; coding: utf-8 -*-

block_cipher = None

a = Analysis(
    ['ui.py'],
    pathex=[],
    binaries=[],
    datas=[
        ('web', 'web'),
        ('module/aria2c.exe', 'module'),
        ('module/storage.json', 'module'),
    ],
    hiddenimports=[
        'api',
        'module',
        'repository',
        'utils',
        'exception',
    ],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],
    win_no_prefer_redirects=False,
    win_private_assemblies=False,
    cipher=block_cipher,
    noarchive=False,
)

pyz = PYZ(a.pure, a.zipped_data, cipher=block_cipher)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name='NsEmuTools',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    console=False,  # 无控制台窗口
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon='icon.ico'
)

coll = COLLECT(
    exe,
    a.binaries,
    a.zipfiles,
    a.datas,
    strip=False,
    upx=True,
    upx_exclude=[],
    name='NsEmuTools',
)
```

**执行构建**:

```bash
# 方式 1: 使用 spec 文件
pyinstaller build.spec

# 方式 2: 命令行参数
pyinstaller --name=NsEmuTools ^
    --windowed ^
    --onedir ^
    --add-data="web;web" ^
    --add-data="module/aria2c.exe;module" ^
    --add-data="module/storage.json;module" ^
    --hidden-import=api ^
    --hidden-import=module ^
    --hidden-import=repository ^
    --icon=icon.ico ^
    ui.py
```

**构建两个版本**:

1. **无控制台版本** (`NsEmuTools.exe`):
   ```bash
   pyinstaller --windowed ui.py
   ```

2. **带控制台版本** (`NsEmuTools-console.exe`):
   ```bash
   pyinstaller --console ui.py
   ```

**输出目录**:
```
dist/
└── NsEmuTools/
    ├── NsEmuTools.exe
    ├── _internal/
    │   ├── Python DLLs
    │   ├── 依赖库
    │   └── ...
    └── web/
        └── 前端资源
```

### 3. 打包发布文件

```bash
# 使用 build_tools/zip_files.py
python build_tools/zip_files.py

# 或手动打包
cd dist
7z a NsEmuTools-v0.5.9-windows-x64.zip NsEmuTools/
```

**打包内容**:
- `NsEmuTools.exe` - 主程序
- `NsEmuTools-console.exe` - 带控制台版本
- `_internal/` - 依赖文件
- `web/` - 前端资源
- `README.md` - 使用说明
- `LICENSE` - 许可证

## CI/CD 自动构建

### GitHub Actions 工作流

项目使用 GitHub Actions 进行自动构建和发布。

**工作流文件** (`.github/workflows/ci-build.yaml`):

```yaml
name: CI Build

on:
  push:
    branches: [ main ]
  pull_request:
    branches: [ main ]
  release:
    types: [ created ]

jobs:
  build:
    runs-on: windows-latest

    steps:
    - name: Checkout code
      uses: actions/checkout@v3

    - name: Setup Python
      uses: actions/setup-python@v4
      with:
        python-version: '3.11'

    - name: Setup Node.js
      uses: actions/setup-node@v3
      with:
        node-version: '20'

    - name: Install Python dependencies
      run: |
        pip install uv
        uv sync

    - name: Install frontend dependencies
      run: |
        cd frontend
        npm install -g bun
        bun install

    - name: Build frontend
      run: |
        cd frontend
        bun build

    - name: Build executable
      run: |
        pyinstaller build.spec
        pyinstaller build-console.spec

    - name: Package release
      run: |
        python build_tools/zip_files.py

    - name: Upload artifact
      uses: actions/upload-artifact@v3
      with:
        name: NsEmuTools-windows-x64
        path: dist/*.zip

    - name: Upload to release
      if: github.event_name == 'release'
      uses: actions/upload-release-asset@v1
      env:
        GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
      with:
        upload_url: ${{ github.event.release.upload_url }}
        asset_path: dist/NsEmuTools-${{ github.event.release.tag_name }}-windows-x64.zip
        asset_name: NsEmuTools-${{ github.event.release.tag_name }}-windows-x64.zip
        asset_content_type: application/zip
```

### 触发构建

1. **推送到 main 分支**:
   ```bash
   git push origin main
   ```
   自动触发构建，生成 artifact

2. **创建 Pull Request**:
   自动触发构建，验证代码

3. **创建 Release**:
   ```bash
   git tag v0.5.10
   git push origin v0.5.10
   ```
   自动构建并上传到 GitHub Releases

### 查看构建结果

1. 访问 [GitHub Actions](https://github.com/triwinds/ns-emu-tools/actions)
2. 查看最新的工作流运行
3. 下载构建的 artifact

## 发布流程

### 1. 准备发布

#### 更新版本号

**config.py**:
```python
current_version = '0.5.10'  # 更新版本号
```

**pyproject.toml**:
```toml
[project]
version = "0.5.10"
```

**frontend/package.json**:
```json
{
  "version": "0.5.10"
}
```

#### 更新 CHANGELOG

创建或更新 `CHANGELOG.md`:
```markdown
## [0.5.10] - 2025-12-18

### Added
- 新增 XXX 功能

### Fixed
- 修复 XXX 问题

### Changed
- 改进 XXX 性能
```

### 2. 创建 Git Tag

```bash
# 创建带注释的 tag
git tag -a v0.5.10 -m "Release v0.5.10"

# 推送 tag
git push origin v0.5.10
```

### 3. 创建 GitHub Release

#### 方式 1: 通过 GitHub Web 界面

1. 访问 [Releases 页面](https://github.com/triwinds/ns-emu-tools/releases)
2. 点击 "Draft a new release"
3. 选择 tag: `v0.5.10`
4. 填写 Release 标题: `v0.5.10`
5. 填写 Release 说明 (从 CHANGELOG 复制)
6. 勾选 "This is a pre-release" (如果是预发布版本)
7. 点击 "Publish release"

#### 方式 2: 使用 GitHub CLI

```bash
# 安装 gh CLI
# https://cli.github.com/

# 创建 release
gh release create v0.5.10 \
  --title "v0.5.10" \
  --notes-file CHANGELOG.md \
  dist/*.zip
```

### 4. 自动构建和上传

GitHub Actions 会自动:
1. 检测到新的 release
2. 触发构建流程
3. 构建 Windows 可执行文件
4. 上传到 release assets

### 5. 验证发布

1. 检查 [Releases 页面](https://github.com/triwinds/ns-emu-tools/releases)
2. 确认文件已上传:
   - `NsEmuTools-v0.5.10-windows-x64.zip`
3. 下载并测试
4. 检查自动更新功能

## 分发策略

### 发布渠道

1. **GitHub Releases** (主要渠道)
   - 稳定版本
   - 预发布版本
   - 源代码

2. **GitHub Actions Artifacts** (开发版本)
   - 每次提交的构建
   - 用于测试

3. **镜像站** (可选)
   - 国内镜像加速
   - 提高下载速度

### 版本命名规范

```
v<major>.<minor>.<patch>[-<prerelease>]

示例:
v0.5.9          # 稳定版本
v0.5.10-beta.1  # 预发布版本
v0.5.10-rc.1    # 候选版本
```

### 发布频率

- **稳定版本**: 每月 1-2 次
- **预发布版本**: 根据需要
- **热修复版本**: 紧急 bug 修复

## 更新机制

### 自动更新流程

1. **检查更新** (`module/updater.py`):
   ```python
   def check_update(prerelease=False):
       # 从 GitHub API 获取最新版本
       release_infos = get_all_release()
       remote_version = release_infos[0]['tag_name']

       # 比较版本
       if remote_version > current_version:
           return True, remote_version
       return False, None
   ```

2. **下载更新**:
   ```python
   def download_net_by_tag(tag):
       # 获取下载 URL
       release_info = get_release_by_tag(tag)
       download_url = release_info['assets'][0]['browser_download_url']

       # 下载文件
       file_path = download(download_url)
       return file_path
   ```

3. **安装更新**:
   ```python
   def update_self_by_tag(tag):
       # 下载更新包
       file_path = download_net_by_tag(tag)

       # 解压
       extract_path = 'download/upgrade_files'
       with py7zr.SevenZipFile(file_path, 'r') as archive:
           archive.extractall(extract_path)

       # 生成更新脚本
       script = generate_update_script()
       with open('update.bat', 'w') as f:
           f.write(script)

       # 执行更新脚本
       subprocess.Popen('update.bat', shell=True)
       sys.exit(0)
   ```

### 更新脚本

**update.bat**:
```batch
@echo off
chcp>nul 2>nul 65001
echo 开始准备更新

echo 尝试优雅关闭程序...
taskkill /IM NsEmuTools* >nul 2>nul
timeout /t 3 /nobreak

echo 检查是否还有残留进程...
tasklist /FI "IMAGENAME eq NsEmuTools*" 2>nul | find /I "NsEmuTools" >nul
if %ERRORLEVEL% equ 0 (
  echo 程序未能正常退出，强制终止...
  taskkill /F /IM NsEmuTools* >nul 2>nul
  timeout /t 3 /nobreak
)

echo 备份原文件
if exist "NsEmuTools.exe" (
  move /Y "NsEmuTools.exe" "NsEmuTools.exe.bak"
)

echo 复制新文件
robocopy "download/upgrade_files" . /MOVE /E /NFL /NDL /NC

echo 清理临时文件
rmdir /s /q "download/upgrade_files"

echo 启动新版本
start /b "NsEmuTools" "NsEmuTools.exe"

DEL "%~f0"
```

## 故障排查

### 构建失败

#### 问题: PyInstaller 找不到模块

**解决方案**:
```python
# 在 build.spec 中添加 hiddenimports
hiddenimports=[
    'api',
    'module',
    'repository',
    'missing_module',  # 添加缺失的模块
]
```

#### 问题: 前端构建失败

**解决方案**:
```bash
# 清理缓存
cd frontend
rm -rf node_modules
rm -rf dist
bun install
bun build
```

#### 问题: 打包后文件过大

**解决方案**:
1. 启用 UPX 压缩:
   ```python
   exe = EXE(
       ...
       upx=True,
   )
   ```

2. 排除不必要的模块:
   ```python
   excludes=[
       'tkinter',
       'matplotlib',
       'numpy',
   ]
   ```

### 运行时错误

#### 问题: 找不到 web 资源

**解决方案**:
确保 `web/` 目录被正确打包:
```python
datas=[
    ('web', 'web'),
]
```

#### 问题: Aria2 启动失败

**解决方案**:
确保 `aria2c.exe` 被打包:
```python
datas=[
    ('module/aria2c.exe', 'module'),
]
```

#### 问题: 配置文件丢失

**解决方案**:
打包默认配置:
```python
datas=[
    ('module/storage.json', 'module'),
]
```

### 更新失败

#### 问题: 更新脚本无法执行

**解决方案**:
1. 检查脚本权限
2. 使用管理员权限运行
3. 检查防病毒软件

#### 问题: 文件被占用

**解决方案**:
```batch
# 在更新脚本中添加强制终止
taskkill /F /IM NsEmuTools.exe
timeout /t 5
```

## 性能优化

### 减小包体积

1. **使用 UPX 压缩**:
   ```python
   upx=True
   ```

2. **排除不必要的依赖**:
   ```python
   excludes=['tkinter', 'test', 'unittest']
   ```

3. **使用 --onefile** (可选):
   ```bash
   pyinstaller --onefile ui.py
   ```
   注意: 启动速度会变慢

### 加快启动速度

1. **使用 --onedir** (推荐):
   ```bash
   pyinstaller --onedir ui.py
   ```

2. **延迟导入**:
   ```python
   def some_function():
       import heavy_module  # 延迟导入
       heavy_module.do_something()
   ```

3. **预编译 Python 文件**:
   ```bash
   python -m compileall .
   ```

## 安全考虑

### 代码签名

为了避免 Windows SmartScreen 警告，建议对 exe 文件进行代码签名:

```bash
# 使用 signtool
signtool sign /f certificate.pfx /p password /t http://timestamp.digicert.com NsEmuTools.exe
```

### 病毒扫描

在发布前进行病毒扫描:
1. 使用 VirusTotal 扫描
2. 提交到主要杀毒软件厂商
3. 添加白名单申请

### 完整性校验

提供文件校验和:
```bash
# 生成 SHA256
certutil -hashfile NsEmuTools-v0.5.10-windows-x64.zip SHA256

# 在 Release 说明中添加
SHA256: abc123...
```

## 监控和分析

### 错误追踪

使用 Sentry 追踪生产环境错误:

```python
# module/sentry.py
import sentry_sdk

sentry_sdk.init(
    dsn="your-sentry-dsn",
    traces_sample_rate=1.0,
    release=f"ns-emu-tools@{current_version}"
)
```

### 使用统计

收集匿名使用统计 (可选):
- 版本分布
- 功能使用频率
- 错误率

## 回滚策略

### 发布回滚

如果发现严重问题:

1. **标记 Release 为 Pre-release**:
   ```bash
   gh release edit v0.5.10 --prerelease
   ```

2. **发布热修复版本**:
   ```bash
   git tag v0.5.11
   git push origin v0.5.11
   ```

3. **通知用户**:
   - 更新 Release 说明
   - 发送通知 (Telegram)

## 文档更新

发布新版本时更新:
- [ ] README.md
- [ ] CHANGELOG.md
- [ ] 架构文档
- [ ] API 文档
- [ ] 用户手册

## 检查清单

### 发布前检查

- [ ] 更新版本号 (config.py, pyproject.toml, package.json)
- [ ] 更新 CHANGELOG
- [ ] 运行所有测试
- [ ] 本地构建测试
- [ ] 检查依赖更新
- [ ] 更新文档
- [ ] 创建 Git tag
- [ ] 推送到 GitHub

### 发布后检查

- [ ] 验证 GitHub Release
- [ ] 下载并测试发布文件
- [ ] 测试自动更新
- [ ] 检查错误追踪
- [ ] 通知用户
- [ ] 更新网站/文档

## 联系方式

如有构建或部署问题:
- **GitHub Issues**: [提交问题](https://github.com/triwinds/ns-emu-tools/issues)
- **Telegram**: [讨论组](https://t.me/+mxI34BRClLUwZDcx)

---

**文档版本**: 1.0
**最后更新**: 2025-12-18

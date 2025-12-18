# repository 目录模块文档

`repository` 目录是项目的数据访问层，负责从远程 API (GitHub, GitLab, Forgejo) 获取模拟器和程序的版本发布信息。

## 目录结构

```
repository/
├── domain/
│   └── release_info.py     # 发布信息数据模型
├── my_info.py              # NsEmuTools 自身版本信息
├── ryujinx.py              # Ryujinx 版本信息
└── yuzu.py                 # Yuzu 系列版本信息
```

---

## 数据模型

### domain/release_info.py - 发布信息模型

**功能概述：** 定义统一的发布信息数据结构，并提供从不同 API 格式转换的函数。

#### 数据类

```python
@dataclass
class ReleaseAsset:
    name: str           # 资源文件名
    download_url: str   # 下载链接

@dataclass
class ReleaseInfo:
    name: str                   # 发布名称
    tag_name: str               # 版本标签
    description: str            # 发布说明
    assets: List[ReleaseAsset]  # 资源文件列表
```

#### 转换函数

| 函数名 | 功能说明 | 数据来源 |
|--------|----------|----------|
| `from_github_api(release_info)` | 从 GitHub API 响应转换 | GitHub |
| `from_gitlab_api(release_info)` | 从 GitLab API 响应转换 | GitLab (Ryujinx) |
| `from_forgejo_api(release_info)` | 从 Forgejo API 响应转换 | Forgejo (Citron) |

#### API 响应格式差异

**GitHub API:**
```json
{
    "name": "Release Name",
    "tag_name": "v1.0.0",
    "body": "Release description",
    "assets": [
        {"name": "file.zip", "browser_download_url": "https://..."}
    ]
}
```

**GitLab API:**
```json
{
    "name": "Release Name",
    "tag_name": "v1.0.0",
    "description": "Release description",
    "assets": {
        "links": [
            {"name": "file.zip", "url": "https://..."}
        ]
    }
}
```

**Forgejo API:**
```json
{
    "name": "Release Name",
    "tag_name": "v1.0.0",
    "body": "Release description",
    "assets": [
        {"name": "file.zip", "browser_download_url": "https://..."}
    ]
}
```

---

## 各模块详细说明

### my_info.py - NsEmuTools 版本信息

**功能概述：** 获取 NsEmuTools 工具自身的版本发布信息。

**数据来源：** GitHub API (`triwinds/ns-emu-tools`)

#### 主要函数

| 函数名 | 功能说明 | 返回值 |
|--------|----------|--------|
| `get_all_release()` | 获取所有发布版本 | `List[Dict]` |
| `get_latest_release(prerelease)` | 获取最新版本 | `Dict` |
| `get_release_info_by_tag(tag)` | 获取指定版本信息 | `Dict` |
| `load_change_log()` | 加载更新日志 | `str` (Markdown) |

#### 使用示例

```python
from repository.my_info import get_all_release, get_latest_release

# 获取所有版本
releases = get_all_release()

# 获取最新稳定版
latest = get_latest_release(prerelease=False)
print(f"最新版本: {latest['tag_name']}")

# 获取最新版本（包括预发布）
latest_with_pre = get_latest_release(prerelease=True)
```

---

### ryujinx.py - Ryujinx 版本信息

**功能概述：** 获取 Ryujinx 模拟器的版本发布信息。

**数据来源：**
- GitLab API (`git.ryujinx.app/ryubing/ryujinx`)
- 主线版本: Project ID 1
- Canary 版本: Project ID 68

#### 主要函数

| 函数名 | 功能说明 | 返回值 |
|--------|----------|--------|
| `get_all_ryujinx_release_infos(branch)` | 获取所有版本信息 | `List[ReleaseInfo]` |
| `get_all_canary_ryujinx_release_infos()` | 获取 Canary 版本信息 | `List[ReleaseInfo]` |
| `get_latest_ryujinx_release_info()` | 获取最新版本 | `ReleaseInfo` |
| `get_ryujinx_release_info_by_version(version, branch)` | 获取指定版本 | `ReleaseInfo` |
| `load_ryujinx_change_log(branch)` | 加载更新日志 | `str` |

#### 支持的分支

| 分支 | 说明 | API 端点 |
|------|------|----------|
| `mainline` | 主线版本 | `/api/v4/projects/1/releases` |
| `canary` | 金丝雀版本 | `/api/v4/projects/68/releases` |

#### 使用示例

```python
from repository.ryujinx import get_all_ryujinx_release_infos, get_ryujinx_release_info_by_version

# 获取主线版本列表
mainline_releases = get_all_ryujinx_release_infos('mainline')

# 获取 Canary 版本列表
canary_releases = get_all_ryujinx_release_infos('canary')

# 获取指定版本
release = get_ryujinx_release_info_by_version('1.2.0', 'mainline')
print(f"版本: {release.tag_name}")
for asset in release.assets:
    print(f"  - {asset.name}: {asset.download_url}")
```

---

### yuzu.py - Yuzu 系列版本信息

**功能概述：** 获取 Yuzu 系列模拟器 (Eden, Citron) 的版本发布信息。

**数据来源：**
- Eden: GitHub API (`eden-emulator/Releases`)
- Citron: Forgejo API (`git.citron-emu.org/Citron/Emulator`)

#### 主要函数

| 函数名 | 功能说明 | 返回值 |
|--------|----------|--------|
| `get_all_yuzu_release_versions(branch)` | 获取所有版本号列表 | `List[str]` |
| `get_yuzu_release_info_by_version(version, branch)` | 获取指定版本信息 | `ReleaseInfo` |
| `get_yuzu_all_release_info(branch)` | 获取所有版本详细信息 | `List[ReleaseInfo]` |
| `get_latest_change_log(branch)` | 获取最新更新日志 | `str` |

#### Eden 专用函数

| 函数名 | 功能说明 |
|--------|----------|
| `get_eden_all_release_info()` | 获取 Eden 所有版本 |
| `get_eden_all_release_versions()` | 获取 Eden 版本号列表 |
| `get_eden_release_info_by_version(version)` | 获取 Eden 指定版本 |

#### Citron 专用函数

| 函数名 | 功能说明 |
|--------|----------|
| `get_citron_all_release_info()` | 获取 Citron 所有版本 |
| `get_citron_all_release_versions()` | 获取 Citron 版本号列表 |
| `get_citron_release_info_by_version(version)` | 获取 Citron 指定版本 |

#### 支持的分支

| 分支 | 说明 | API 端点 |
|------|------|----------|
| `eden` | Eden 模拟器 | GitHub: `eden-emulator/Releases` |
| `citron` | Citron 模拟器 | Forgejo: `git.citron-emu.org` |

#### 使用示例

```python
from repository.yuzu import get_all_yuzu_release_versions, get_yuzu_release_info_by_version

# 获取 Eden 版本列表
eden_versions = get_all_yuzu_release_versions('eden')
print(f"Eden 版本: {eden_versions[:5]}")

# 获取 Citron 版本列表
citron_versions = get_all_yuzu_release_versions('citron')
print(f"Citron 版本: {citron_versions[:5]}")

# 获取指定版本信息
release = get_yuzu_release_info_by_version('0.11.0', 'citron')
print(f"版本: {release.tag_name}")
print(f"说明: {release.description[:100]}...")
```

---

## 网络请求说明

所有 repository 模块都依赖 `module.network` 模块进行网络请求：

- `session`: 带缓存的 requests 会话
- `request_github_api()`: GitHub API 请求，支持 CDN 回退
- `get_finial_url()`: 获取最终 URL (可能经过镜像替换)

### 缓存机制

- 使用 `requests_cache` 进行请求缓存
- 支持内存缓存和持久化缓存
- `get_all_release()` 等函数禁用缓存以获取最新数据

### 错误处理

- 版本不存在时抛出 `VersionNotFoundException`
- API 响应异常时抛出 `IgnoredException`
- 网络错误由上层 API 统一处理

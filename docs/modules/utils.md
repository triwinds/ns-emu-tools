# utils 目录模块文档

`utils` 目录包含项目中使用的各种工具函数和辅助模块。

## 目录结构

```
utils/
├── admin.py        # 管理员权限相关
├── common.py       # 通用工具函数
├── doh.py          # DNS over HTTPS 实现
├── hardware.py     # 硬件信息获取
├── package.py      # 压缩包处理
├── string_util.py  # 字符串工具
└── webview2.py     # WebView2 运行时检测和安装
```

---

## 各模块详细说明

### common.py - 通用工具函数

**功能概述：** 提供 Windows 系统相关的通用工具函数。

#### 主要函数

| 函数名 | 功能说明 | 参数 | 返回值 |
|--------|----------|------|--------|
| `get_all_window_name()` | 获取所有窗口标题 | 无 | `List[str]` |
| `decode_yuzu_path(raw_path)` | 解码 Yuzu 配置中的路径 | `raw_path: str` | `str` |
| `find_all_instances(process_name, exe_path)` | 查找所有匹配的进程 | `process_name: str`, `exe_path: Path` | `List[Process]` |
| `kill_all_instances(process_name, exe_path)` | 终止所有匹配的进程 | `process_name: str`, `exe_path: Path` | 无 |
| `is_path_in_use(file_path)` | 检查路径是否被占用 | `file_path: str/Path` | `bool` |
| `get_installed_software()` | 获取已安装软件列表 | 无 | `List[Dict]` |
| `find_installed_software(name_pattern)` | 按名称模式查找软件 | `name_pattern: str` | `List[Dict]` |
| `is_newer_version(min_version, current_version)` | 版本比较 | `min_version: str`, `current_version: str` | `bool` |

#### 使用示例

```python
from utils.common import find_all_instances, kill_all_instances, is_path_in_use

# 查找 Yuzu 进程
yuzu_processes = find_all_instances('yuzu.exe')
for p in yuzu_processes:
    print(f"PID: {p.pid}, Name: {p.name()}")

# 终止所有 Yuzu 进程
kill_all_instances('yuzu.exe')

# 检查路径是否被占用
if is_path_in_use('D:\\Yuzu\\user'):
    print("路径正在使用中")
```

#### 路径解码说明

Yuzu 配置文件中的路径可能包含 Unicode 转义序列，如 `\x65b0\x5efa`，`decode_yuzu_path` 函数将其转换为正常的 Unicode 字符。

---

### package.py - 压缩包处理

**功能概述：** 提供压缩和解压缩功能，支持 ZIP、7z、tar.xz 格式。

#### 主要函数

| 函数名 | 功能说明 | 参数 |
|--------|----------|------|
| `uncompress(filepath, target_path, delete_on_error, exception_msg)` | 解压文件 | `filepath: Path`, `target_path: Path/str`, `delete_on_error: bool`, `exception_msg: str` |
| `compress_folder(folder_path, save_path)` | 压缩文件夹为 7z | `folder_path: Path`, `save_path: Path/str` |
| `is_7zfile(filepath)` | 检查是否为有效的 7z 文件 | `filepath: Path` |

#### 支持的格式

| 格式 | 解压 | 压缩 |
|------|------|------|
| `.zip` | 支持 | - |
| `.7z` | 支持 | 支持 |
| `.tar.xz` | 支持 | - |

#### 使用示例

```python
from utils.package import uncompress, compress_folder
from pathlib import Path

# 解压文件
uncompress(Path('archive.7z'), Path('output_dir'))

# 压缩文件夹
compress_folder(Path('folder_to_compress'), Path('output.7z'))
```

---

### admin.py - 管理员权限

**功能概述：** 提供 Windows 管理员权限相关的功能。

#### 主要函数

| 函数名 | 功能说明 | 参数 | 返回值 |
|--------|----------|------|--------|
| `run_with_admin_privilege(executable, argument_line)` | 以管理员权限运行程序 | `executable: str`, `argument_line: str` | `int` (返回码) |
| `check_is_admin()` | 检查当前是否以管理员身份运行 | 无 | `bool` |

#### 使用示例

```python
from utils.admin import check_is_admin, run_with_admin_privilege

if not check_is_admin():
    # 以管理员权限运行命令
    run_with_admin_privilege('cmd', '/c copy file1.txt file2.txt')
```

---

### hardware.py - 硬件信息

**功能概述：** 获取系统硬件信息，包括 CPU 和 GPU。

#### 主要函数

| 函数名 | 功能说明 | 返回值 |
|--------|----------|--------|
| `get_gpu_info()` | 获取 NVIDIA GPU 信息 | `List[Dict]` |
| `get_cpu_info()` | 获取 CPU 信息 (简单) | `str` |
| `get_win32_cpu_info()` | 获取详细 CPU 信息 (Windows) | `List[Dict]` |

#### GPU 信息字段

```python
{
    'id': '0',
    'name': 'NVIDIA GeForce RTX 3080',
    'mem': '10240 MiB',
    'cores': '1',
    'mem_free': '8000 MiB',
    'mem_used': '2240 MiB',
    'util_gpu': '15 %',
    'util_mem': '22 %'
}
```

#### CPU 信息字段

```python
{
    'Processor': '0',
    'ProcessorNameString': 'AMD Ryzen 9 5900X',
    'Identifier': 'AMD64 Family 25 Model 33 Stepping 0',
    'Family': 25,
    'Model': 33,
    'Stepping': 0,
    ...
}
```

---

### string_util.py - 字符串工具

**功能概述：** 提供字符串处理相关的工具函数。

#### 主要函数

| 函数名 | 功能说明 | 参数 | 返回值 |
|--------|----------|------|--------|
| `auto_decode(input_bytes)` | 自动检测编码并解码 | `input_bytes: bytes` | `str` |

#### 使用示例

```python
from utils.string_util import auto_decode

# 自动检测编码
with open('file.txt', 'rb') as f:
    content = auto_decode(f.read())
```

**依赖：** 使用 `chardet` 库进行编码检测。

---

### webview2.py - WebView2 运行时

**功能概述：** 检测和安装 Microsoft Edge WebView2 运行时及 .NET Framework。

#### 主要函数

| 函数名 | 功能说明 | 返回值 |
|--------|----------|--------|
| `get_dot_net_version()` | 获取 .NET Framework 版本 | `int` (Release 号) |
| `is_chromium(verbose)` | 检查 WebView2 是否可用 | `bool` |
| `can_use_webview()` | 检查是否可以使用 WebView 模式 | `bool` |
| `ensure_runtime_components()` | 确保运行时组件已安装 | `bool` (是否需要重启) |
| `install_dot_net()` | 安装 .NET Framework | 无 |
| `install_webview2()` | 安装 WebView2 运行时 | 无 |
| `show_msgbox(title, content, style)` | 显示消息框 | `int` (用户选择) |

#### WebView2 版本检测

检测以下 WebView2 版本：
- Runtime (正式版)
- Beta (测试版)
- Dev (开发版)
- Canary (金丝雀版)

要求版本 >= 105.0.0.0

#### .NET Framework 版本要求

要求 .NET Framework 4.6.2 或更高版本 (Release >= 394802)

#### 使用示例

```python
from utils.webview2 import can_use_webview, ensure_runtime_components

# 检查是否可以使用 WebView
if can_use_webview():
    print("可以使用 WebView 模式")
else:
    # 安装必要组件
    need_restart = ensure_runtime_components()
    if need_restart:
        print("请重启程序")
```

---

### doh.py - DNS over HTTPS

**功能概述：** 实现 DNS over HTTPS (DoH) 功能，用于绕过 DNS 污染。

#### 主要函数

| 函数名 | 功能说明 | 参数 |
|--------|----------|------|
| `install_doh()` | 安装 DoH 补丁到 urllib3 | 无 |
| `query_address(name, record_type, server, path, fallback, verbose)` | 查询 DNS 记录 | `name: str`, `record_type: str`, ... |

#### 配置

- 默认 DoH 服务器: 阿里云 DNS (`223.5.5.5`)
- 备用 DNS: `223.5.5.5`, `119.29.29.29`
- 支持 IPv4 和 IPv6 (可配置)

#### 缓存机制

- 使用 `DnsCacheItem` 类缓存 DNS 查询结果
- 根据 TTL 自动过期
- 支持 A 和 AAAA 记录

#### 工作原理

1. 拦截 urllib3 的 `create_connection` 函数
2. 使用 DoH 解析域名
3. 尝试连接解析到的 IP 地址
4. 如果 DoH 失败，回退到系统 DNS

#### 使用示例

```python
from utils.doh import install_doh, query_address

# 安装 DoH
install_doh()

# 手动查询
ips = query_address('github.com', 'A')
print(f"GitHub IPs: {ips}")

# 查询 IPv6
ipv6s = query_address('github.com', 'AAAA')
print(f"GitHub IPv6: {ipv6s}")
```

#### 注意事项

- DoH 功能仅在配置中启用 `useDoh` 时生效
- 如果禁用 IPv6，将不会查询 AAAA 记录
- DoH 服务器本身不经过代理

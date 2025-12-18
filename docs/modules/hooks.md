# hooks 目录模块文档

`hooks` 目录包含 PyInstaller 打包时使用的钩子文件。

## 目录结构

```
hooks/
└── hook-api.py     # API 模块的 PyInstaller 钩子
```

---

## hook-api.py - API 模块钩子

### 功能概述

PyInstaller 钩子文件，用于确保 `api` 目录下的所有模块在打包时被正确包含。

### 工作原理

PyInstaller 在打包时可能无法自动检测到动态导入的模块。此钩子文件通过读取 `api/__init__.py` 中的 `__all__` 列表，显式声明需要包含的隐藏导入。

### 代码实现

```python
from api import __all__

hiddenimports = []
for m in __all__:
    hiddenimports.append(f'api.{m}')
```

### 生成的隐藏导入

根据 `api/__init__.py` 中的 `__all__` 定义，钩子会生成以下隐藏导入：

```python
hiddenimports = [
    'api.common_api',
    'api.yuzu_api',
    'api.ryujinx_api',
    'api.cheats_api',
    'api.save_manager_api',
    'api.updater_api',
    # ... 其他 API 模块
]
```

### PyInstaller 配置

在 PyInstaller 的 spec 文件或命令行中，需要指定钩子目录：

```bash
pyinstaller --additional-hooks-dir=hooks main.py
```

或在 spec 文件中：

```python
a = Analysis(
    ['main.py'],
    hookspath=['hooks'],
    ...
)
```

---

## 其他可能的钩子

如果项目中有其他需要特殊处理的模块，可以在 `hooks` 目录下添加相应的钩子文件：

| 钩子文件名 | 用途 |
|------------|------|
| `hook-<module>.py` | 为指定模块添加隐藏导入 |
| `hook-<package>.py` | 为指定包添加隐藏导入 |

### 钩子文件命名规则

- 文件名格式: `hook-<模块名>.py`
- 模块名使用点号分隔: `hook-module.submodule.py`

### 常用钩子变量

| 变量名 | 类型 | 说明 |
|--------|------|------|
| `hiddenimports` | `List[str]` | 隐藏导入的模块列表 |
| `datas` | `List[Tuple]` | 需要包含的数据文件 |
| `binaries` | `List[Tuple]` | 需要包含的二进制文件 |
| `excludedimports` | `List[str]` | 需要排除的模块 |

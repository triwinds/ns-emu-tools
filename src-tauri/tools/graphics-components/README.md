# 图形组件三分包

公共启动器通过 `--backend nvidia|xess` 选择运行路径。NVIDIA 与独立 XeSS 图形层仍分别编译。

## 本地构建与交付

在仓库根目录执行：

```powershell
python src-tauri/tools/graphics-components/package.py --build
cd src-tauri
& ../frontend/node_modules/.bin/tauri.cmd build --no-bundle
```

交付 `src-tauri/target/release/NsEmuTools.exe` 及旁边的目录：

```text
graphics-components/
  distribution.json
  common/
    graphics-launcher.exe
    streamline_probe_layer.dll
    xess-layer.dll
    nvngx.dll
    LICENSE.txt
  nvidia/
    sl.*.dll、nvngx_dlss*.dll、NvLowLatencyVk.dll
    许可证与来源说明
  xess/
    libxess.dll
    bin/libxess_fg.dll、libxell.dll
    D3D12/*.dll
    licenses/*
    package.json
```

脚本校验固定的外部文件和 NR 桥接 DLL，构建一个公共启动器及两个隔离的图形层。首次迁移从原 `streamline-fg-package`、`xess-package` 取经过校验的运行库，后续从现有分包目录取运行库。外部文件改变时拒绝打包。旧交付目录保留为 `graphics-components.previous-*`。默认仅保留目录，不生成安装器或交付 ZIP。

旧 `package-local.ps1`、`package-runtime.ps1`、XeSS 的 `package.py` / `release.py` 入口均转交此脚本。

## 安装与校验

- NVIDIA：公共程序包 + NVIDIA 运行库包。
- 独立 XeSS：同一公共程序包 + XeSS 运行库包，不下载 NVIDIA 依赖。
- 混合模式：同一启动器引用两套已安装的运行库。

三包各自使用内容版本和归档 SHA-256。ZIP 缓存位于工具配置目录的 `graphics/components/.archives/<SHA-256>.zip`；完整分包位于 `graphics/components/<分包版本>`。公共程序只安装一份，目录中不含外部运行库。优先复用已验证的安装、EXE 旁目录及 ZIP 缓存，离线交付不初始化网络下载器。ZIP 损坏时重新下载，已展开目录损坏或含额外文件时保留到 `.damaged-*`。全部成员校验通过后才原子发布；取消或失败不发布半成品。

NVIDIA 活动运行库仍位于 `graphics/streamline-fg/runtime`，XeSS 活动运行库位于 `graphics/xess/runtime/<版本>`。两者只放各自依赖，启动器、图形层及 NR 桥从共享公共目录读取。保留原有设置、会话及目标登记。卸载任一引擎不删除公共程序、另一引擎或分包缓存。启动期间主程序和启动器保持使用中文件的只读句柄。

主程序内嵌 `graphics-distribution.json` 及两份引擎清单，固定 URL、ZIP SHA-256、成员路径和逐文件 SHA-256。新协议为 2；旧安装记录仍可校验及迁移。本地配套构建固定使用自己的组件版本，避免被旧的线上 `streamline-layer-*` 发布替换。

## 准备线上发布

```powershell
python src-tauri/tools/graphics-components/package.py --archives
```

输出位于 `src-tauri/target/graphics-release`。三份运行组件 ZIP 各自使用 `graphics-common-*`、`graphics-nvidia-*`、`graphics-xess-*` tag，并提供源码及清单。仅公共程序改变时，两个运行库 ZIP 的版本和哈希不变。脚本不上传或发布 Release。必须先将 ZIP 上传到清单中的官方 runtimes Release 地址，再分发内嵌该清单的主程序，才能在无本地组件的机器上实际下载新版本。

## 验证

`python src-tauri/tools/graphics-components/smoke.py` 使用临时假模拟器检查 NVIDIA、XeSS、混合路由及非零退出码，核对 SDK / XeSS 子进程环境，不启动真实游戏或 GPU。Rust 的 `distribution::tests` 覆盖分包归属、离线共享安装、缓存复用、损坏保留、取消、错误哈希和恶意路径。显式启用的 `local_delivery_roundtrip` 验证完整本地分包安装。

这些检查验证打包与启动协议，不代表新增的游戏画质或硬件兼容性验收。

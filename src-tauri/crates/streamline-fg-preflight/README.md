# Streamline FG P0 校验工具

这是实施计划的第一批交付：只读核对参考分支、固定 SDK 源码和本机 Loader。
**不是 Vulkan layer、GPU 能力测试或模拟器启动器。P0 尚未通过，FG 未实现。**
接入发现和继续条件见 [P0 审查记录](P0-review.md)。

该 crate 独立构建，不加入 Tauri 默认构建，不链接或加载 Streamline DLL。
`baseline.json` 固定本次审查的 41 个文件；`Cargo.lock` 固定诊断工具依赖。
它们只是研究基线，不是获准分发的运行包或原版 Ryubing 白名单。

在仓库根目录执行：

```powershell
cargo fmt --manifest-path src-tauri/crates/streamline-fg-preflight/Cargo.toml
cargo check --locked --manifest-path src-tauri/crates/streamline-fg-preflight/Cargo.toml
cargo check --locked --manifest-path src-tauri/crates/streamline-fg-preflight/Cargo.toml --target x86_64-pc-windows-msvc
cargo test --locked --manifest-path src-tauri/crates/streamline-fg-preflight/Cargo.toml
```

如需重新取得审查用的 SDK（只下载源码，不需要 LFS 二进制），可执行：

```powershell
$env:GIT_LFS_SKIP_SMUDGE = '1'
git clone --depth 1 --branch v2.12.0 https://github.com/NVIDIA-RTX/Streamline.git src-tauri/target/streamline-sdk-v2.12.0
Remove-Item Env:GIT_LFS_SKIP_SMUDGE
git -C src-tauri/target/streamline-sdk-v2.12.0 rev-parse HEAD
```

期望 commit：`e8aaa6eaac968711fb62473d4ae8256dde20919b`。输入目录存在后运行：

```powershell
cargo run --quiet --locked --manifest-path src-tauri/crates/streamline-fg-preflight/Cargo.toml -- `
  --reference-dir D:\ryubing-dlss5-win-x64 `
  --sdk-dir D:\py\ns-emu-tools\src-tauri\target\streamline-sdk-v2.12.0 `
  --loader C:\Windows\System32\vulkan-1.dll `
  > src-tauri/target/streamline-p0-audit.json
$LASTEXITCODE
```

所有输入必须使用绝对路径。路径按本机位置修改；`--reference-dir` 必须指向调研中的分支，不能用它批准原版模拟器启动。标准输出只有 JSON，工具本身不写输入文件、环境变量或注册表。

| 退出码 | 含义 |
| --- | --- |
| 0 | 仅帮助命令成功 |
| 1 | 参数错误、文件缺失/不可读、哈希不匹配或非 Windows x64 宿主 |
| 2 | 所有证据匹配，但 P0 接入门槛仍未通过 |

二进制按原始字节计算 SHA-256。SDK 文本只将 CRLF 转为 LF 后计算 UTF-8 SHA-256，允许 Git 的换行转换；不忽略其他差异。检查失败仍输出其余文件结果。SDK 的 commit 字段为基线声明，工具核对列出的文件内容，不声称检查整个 Git 工作区。

Loader 升级、参考文件替换或 SDK 修改后必须重新审查，不自动更新基线。哈希相符不证明 DLL 与公开源码构建完全相同、可分发、ABI 已通过编译验证、设备支持 FG 或显示效果已验收。工具没有 DLL 加载功能，也没有将诊断结果直接用于随后加载的 TOCTOU 安全保证。

2026-09-25 实测：41 项匹配，退出码 2，`p0_passed=false`、`fg_enabled=false`、`runtime_queries_performed=false`。独立 crate 的格式化、宿主检查、显式 Windows 检查及 4 项测试通过，编译无警告。宿主即 `x86_64-pc-windows-msvc`；此结果不代表 GPU、其他宿主或 Tauri 应用验证。

新增只读源码路由审计二进制 sdk-route-audit：使用 --sdk-dir 绝对路径，校验 route-baseline.json 的 13 个文件并输出带行号证据。匹配退出 2，缺失或变更退出 1；不会通过 P0。详见 [源码适配审查](../streamline-layer-probe/SOURCE-ROUTING-REVIEW.md)。原有 cargo run 默认入口保持不变。

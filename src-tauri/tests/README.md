# Eden 集成测试

这是 Eden 模拟器的集成测试套件，用于测试获取版本信息和安装功能。

## 测试概述

### 可用的测试用例

1. **test_get_eden_all_versions** - 获取所有可用的 Eden 版本
2. **test_get_eden_release_info** - 获取指定版本的详细信息
3. **test_get_latest_and_install_eden** - 完整的集成测试：获取最新版本并安装
4. **test_install_specific_eden_version** - 安装指定版本的 Eden
5. **test_performance_get_multiple_releases** - 性能测试：批量获取版本信息

## 运行测试

### 前提条件

1. **网络连接** - 所有测试都需要访问 GitHub API
2. **Aria2** - 下载测试需要 Aria2 支持（安装测试需要）
3. **足够的磁盘空间** - 安装测试会下载实际的安装包（约 50-100MB）

### 运行所有集成测试

由于这些测试需要网络连接和较长时间，默认情况下它们被标记为 `#[ignore]`。

```bash
# 在 src-tauri 目录下
cd src-tauri

# 运行所有被忽略的测试（包括集成测试）
cargo test -- --ignored

# 或者只运行 Eden 集成测试
cargo test --test eden_integration_test -- --ignored

# 显示详细输出（推荐）
cargo test --test eden_integration_test -- --ignored --nocapture
```

### 运行特定测试

```bash
# 只测试获取版本列表（较快）
cargo test test_get_eden_all_versions -- --ignored --nocapture

# 只测试获取版本详情（较快）
cargo test test_get_eden_release_info -- --ignored --nocapture

# 完整安装测试（慢，会下载实际文件）
cargo test test_get_latest_and_install_eden -- --ignored --nocapture

# 性能测试
cargo test test_performance_get_multiple_releases -- --ignored --nocapture
```

### 调整日志级别

测试使用 `tracing` 进行日志输出，可以通过环境变量控制日志级别：

```bash
# 显示详细日志（debug 级别）
RUST_LOG=debug cargo test --test eden_integration_test -- --ignored --nocapture

# 只显示信息日志（默认）
RUST_LOG=info cargo test --test eden_integration_test -- --ignored --nocapture

# 只显示警告和错误
RUST_LOG=warn cargo test --test eden_integration_test -- --ignored --nocapture
```

## 测试说明

### test_get_eden_all_versions

- **用途**: 验证能否成功获取 Eden 的所有可用版本
- **耗时**: 约 1-3 秒
- **网络**: 需要
- **输出**: 版本列表，包含前 5 个版本

### test_get_eden_release_info

- **用途**: 验证能否获取指定版本的详细信息（包括下载链接等）
- **耗时**: 约 1-3 秒
- **网络**: 需要
- **输出**: 版本详情和资源文件列表

### test_get_latest_and_install_eden

- **用途**: 完整的集成测试流程
  1. 获取最新版本
  2. 下载到临时目录
  3. 安装到临时目录
  4. 验证安装结果
- **耗时**: 约 1-5 分钟（取决于网络速度）
- **网络**: 需要
- **磁盘**: 需要临时空间存储下载文件
- **输出**: 详细的安装过程和进度

### test_install_specific_eden_version

- **用途**: 测试安装非最新版本的能力
- **耗时**: 约 1-5 分钟
- **网络**: 需要
- **输出**: 安装过程

### test_performance_get_multiple_releases

- **用途**: 测试批量获取版本信息的性能
- **耗时**: 约 5-10 秒
- **网络**: 需要
- **输出**: 性能统计信息

## 注意事项

1. **临时文件**: 所有测试都使用临时目录，测试结束后会自动清理
2. **配置隔离**: 测试使用独立的配置，不会影响主程序的配置文件
3. **网络容错**: 如果网络失败，测试会记录警告而不是直接失败
4. **Aria2 依赖**: 下载测试需要系统中安装了 Aria2

## 故障排查

### 测试失败：无法连接到 GitHub

- 检查网络连接
- 检查是否需要配置代理
- GitHub API 可能有速率限制

### 测试失败：Aria2 错误

- 确认 Aria2 已安装并在 PATH 中
- 检查 Aria2 配置

### 测试超时

- 网络速度较慢时可能需要更长时间
- 考虑只运行轻量级测试（如版本获取测试）

## 示例输出

```
running 1 test
test test_get_latest_and_install_eden ... ok
开始测试：获取最新版本并安装 Eden
步骤 1: 获取最新版本
最新版本: v1.0.0
步骤 2: 创建临时安装目录
临时目录: C:\Users\...\Temp\...
步骤 3: 开始下载并安装 Eden v1.0.0
Eden 下载 15.2% (8/52 MB) @ 1024 KB/s
Eden 下载 35.8% (18/52 MB) @ 2048 KB/s
Eden 下载 58.4% (30/52 MB) @ 2048 KB/s
Eden 下载 81.7% (42/52 MB) @ 2048 KB/s
Eden 下载 99.9% (52/52 MB) @ 2048 KB/s
步骤 4: 验证安装结果
Eden 安装路径: C:\Users\...\Temp\...\emulator\eden.exe
测试完成: Eden v1.0.0 安装成功

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out
```

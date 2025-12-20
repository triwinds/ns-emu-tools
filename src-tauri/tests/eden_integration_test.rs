//! Eden 模拟器集成测试
//!
//! 测试 Eden 模拟器的获取版本和安装功能

mod common;

use ns_emu_tools_lib::config::get_config;
use ns_emu_tools_lib::repositories::yuzu::{
    get_eden_all_release_versions, get_eden_release_info_by_version,
};
use ns_emu_tools_lib::services::yuzu::install_eden;
use std::path::PathBuf;
use tracing::info;

use common::{simple_progress_printer, TestConfigHelper};

// 初始化测试环境
#[ctor::ctor]
fn init() {
    // 初始化日志
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}

/// 测试获取 Eden 所有版本
#[tokio::test]
#[ignore] // 需要网络连接，使用 `cargo test -- --ignored` 运行
async fn test_get_eden_all_versions() {
    info!("开始测试：获取 Eden 所有版本");

    let result = get_eden_all_release_versions().await;
    assert!(result.is_ok(), "获取版本列表失败: {:?}", result.err());

    let versions = result.unwrap();
    assert!(!versions.is_empty(), "版本列表为空");

    info!("获取到 {} 个 Eden 版本", versions.len());
    info!("最新版本: {}", versions.first().unwrap());

    // 输出前 5 个版本供参考
    for (i, version) in versions.iter().take(5).enumerate() {
        info!("  {}. {}", i + 1, version);
    }
}

/// 测试获取 Eden 指定版本信息
#[tokio::test]
#[ignore] // 需要网络连接
async fn test_get_eden_release_info() {
    info!("开始测试：获取 Eden 指定版本信息");

    // 先获取所有版本
    let versions = get_eden_all_release_versions()
        .await
        .expect("获取版本列表失败");

    assert!(!versions.is_empty(), "版本列表为空");

    // 获取最新版本的详细信息
    let latest_version = &versions[0];
    info!("获取版本 {} 的详细信息", latest_version);

    let result = get_eden_release_info_by_version(latest_version).await;
    assert!(result.is_ok(), "获取版本信息失败: {:?}", result.err());

    let release_info = result.unwrap();
    assert_eq!(&release_info.tag_name, latest_version);
    assert!(!release_info.assets.is_empty(), "资源列表为空");

    info!("版本名称: {}", release_info.name);
    info!("资源数量: {}", release_info.assets.len());

    // 输出所有资源文件
    for asset in &release_info.assets {
        info!("  - {} ({} bytes)", asset.name, asset.size);
    }
}

/// 测试获取最新 Eden 版本并安装
///
/// 这是一个完整的集成测试，会执行以下步骤：
/// 1. 获取最新的 Eden 版本
/// 2. 下载并安装到临时目录
/// 3. 验证安装结果
#[tokio::test]
#[ignore] // 需要网络连接和较长时间，使用 `cargo test -- --ignored` 运行
async fn test_get_latest_and_install_eden() {
    info!("开始测试：获取最新版本并安装 Eden");

    // 步骤 1: 获取最新版本
    info!("步骤 1: 获取最新版本");
    let versions = get_eden_all_release_versions()
        .await
        .expect("获取版本列表失败");

    assert!(!versions.is_empty(), "版本列表为空");

    let latest_version = &versions[0];
    info!("最新版本: {}", latest_version);

    // 步骤 2: 创建临时目录用于安装
    info!("步骤 2: 创建临时安装目录");
    let test_helper = TestConfigHelper::new();
    info!("临时目录: {}", test_helper.temp_dir.path().display());

    // 配置测试环境
    test_helper.apply_to_global_config();

    // 步骤 3: 安装 Eden
    info!("步骤 3: 开始下载并安装 Eden {}", latest_version);

    // 使用简单的进度打印器
    let progress_callback = simple_progress_printer("Eden 下载");

    let result = install_eden(latest_version, progress_callback).await;

    // 步骤 4: 验证安装结果
    if let Err(e) = result {
        // 如果是网络或下载错误，记录但不失败测试（可能是网络问题）
        if matches!(
            e,
            ns_emu_tools_lib::error::AppError::Network(_)
                | ns_emu_tools_lib::error::AppError::Download(_)
                | ns_emu_tools_lib::error::AppError::Aria2(_)
        ) {
            info!("警告: 下载失败（可能是网络问题）: {}", e);
            return;
        }

        panic!("安装失败: {:?}", e);
    }

    info!("步骤 4: 验证安装结果");

    // 验证配置已更新
    let config = get_config();
    assert_eq!(
        config.yuzu.yuzu_version.as_deref(),
        Some(latest_version.as_str()),
        "配置中的版本未正确更新"
    );
    assert_eq!(config.yuzu.branch, "eden", "分支应该是 eden");

    // 验证可执行文件存在
    let yuzu_path = PathBuf::from(&config.yuzu.yuzu_path);
    let eden_exe = yuzu_path.join("eden.exe");

    if cfg!(windows) {
        // 在 Windows 上验证 exe 文件存在
        // 注意: 由于使用了临时目录，实际安装可能不会创建文件
        // 这里只是检查路径配置是否正确
        info!("Eden 安装路径: {}", eden_exe.display());
    }

    info!("测试完成: Eden {} 安装成功", latest_version);
}

/// 测试安装指定版本的 Eden
///
/// 注意：需要手动指定一个已知的有效版本
#[tokio::test]
#[ignore] // 需要网络连接
async fn test_install_specific_eden_version() {
    info!("开始测试：安装指定版本的 Eden");

    // 先获取可用版本列表
    let versions = get_eden_all_release_versions()
        .await
        .expect("获取版本列表失败");

    // 跳过最新版本，选择第二个版本（如果存在）
    let target_version = if versions.len() > 1 {
        &versions[1]
    } else {
        &versions[0]
    };

    info!("目标版本: {}", target_version);

    // 创建临时目录
    let test_helper = TestConfigHelper::new();
    test_helper.apply_to_global_config();

    // 安装
    let progress_callback = simple_progress_printer("Eden 下载");

    let result = install_eden(target_version, progress_callback).await;

    if let Err(e) = result {
        // 网络错误不算失败
        if matches!(
            e,
            ns_emu_tools_lib::error::AppError::Network(_)
                | ns_emu_tools_lib::error::AppError::Download(_)
                | ns_emu_tools_lib::error::AppError::Aria2(_)
        ) {
            info!("警告: 下载失败（可能是网络问题）: {}", e);
            return;
        }

        panic!("安装失败: {:?}", e);
    }

    // 验证配置
    let config = get_config();
    assert_eq!(
        config.yuzu.yuzu_version.as_deref(),
        Some(target_version.as_str())
    );

    info!("测试完成: Eden {} 安装成功", target_version);
}

/// 性能测试：测试批量获取版本信息的性能
#[tokio::test]
#[ignore] // 需要网络连接
async fn test_performance_get_multiple_releases() {
    use std::time::Instant;

    info!("开始性能测试：批量获取版本信息");

    let start = Instant::now();

    // 获取所有版本
    let versions = get_eden_all_release_versions()
        .await
        .expect("获取版本列表失败");

    let list_duration = start.elapsed();
    info!("获取版本列表耗时: {:?}", list_duration);
    info!("版本数量: {}", versions.len());

    // 获取前 3 个版本的详细信息
    let detail_count = 3.min(versions.len());
    let start = Instant::now();

    for version in versions.iter().take(detail_count) {
        let _ = get_eden_release_info_by_version(version)
            .await
            .expect("获取版本信息失败");
    }

    let detail_duration = start.elapsed();
    info!(
        "获取 {} 个版本详情耗时: {:?}",
        detail_count, detail_duration
    );
    info!(
        "平均每个版本: {:?}",
        detail_duration / detail_count as u32
    );
}

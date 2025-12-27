//! 日志管理模块
//!
//! 提供统一的日志配置和初始化功能

use std::path::PathBuf;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// 获取日志文件路径
fn log_file_path() -> PathBuf {
    // 使用配置模块提供的应用程序数据目录
    let dir = crate::config::app_data_dir();

    // 确保目录存在
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("创建日志目录失败: {}", e);
    }

    dir.join("ns-emu-tools.log")
}

/// 初始化日志系统
///
/// 配置项：
/// - 默认级别：info
/// - ns_emu_tools 模块：debug
/// - aria2_ws::client：info（避免过多调试日志）
///
/// 日志输出：
/// - 同时输出到控制台和文件 `ns-emu-tools.log`
///
/// 可以通过环境变量 `RUST_LOG` 覆盖默认配置
///
/// 返回 WorkerGuard，用于保持日志写入器活跃
pub fn init() -> WorkerGuard {
    // 设置日志过滤器
    // 如果有 RUST_LOG 环境变量，使用它；否则使用默认配置
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // 默认配置：
        // - 全局默认 info 级别
        // - ns_emu_tools 模块设置为 debug
        // - aria2_ws::client 设置为 info（避免 debug/trace 日志过多）
        EnvFilter::new("info,ns_emu_tools=debug,aria2_ws::client=info")
    });

    // 创建文件 appender
    let log_path = log_file_path();
    let log_dir = log_path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let log_file = log_path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("ns-emu-tools.log");

    let file_appender = tracing_appender::rolling::never(log_dir, log_file);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // 配置文件日志格式
    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_target(true) // 显示日志来源模块
        .with_thread_ids(false) // 不显示线程 ID
        .with_file(true) // 显示文件名
        .with_line_number(true) // 显示行号
        .with_ansi(false); // 文件中不使用 ANSI 颜色

    // 配置控制台日志格式
    let console_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true)
        .with_ansi(true); // 控制台使用 ANSI 颜色

    // 初始化 tracing subscriber，同时输出到控制台和文件
    tracing_subscriber::registry()
        .with(filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    guard
}

/// 测试环境下的日志初始化
///
/// 仅用于测试，使用更详细的日志级别
#[cfg(test)]
pub fn init_test() {
    use once_cell::sync::OnceCell;

    static TRACING_INIT: OnceCell<()> = OnceCell::new();

    TRACING_INIT.get_or_init(|| {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("debug,aria2_ws::client=info"));

        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().with_test_writer())
            .init();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::{debug, info};

    #[test]
    fn test_logging_initialization() {
        init_test();
        info!("测试日志初始化");
        debug!("调试级别日志");
    }
}

//! 测试辅助工具模块
//!
//! 提供集成测试所需的通用辅助函数

use std::path::PathBuf;
use tempfile::TempDir;

/// 测试配置助手
pub struct TestConfigHelper {
    pub temp_dir: TempDir,
}

impl TestConfigHelper {
    /// 创建新的测试配置环境
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("创建临时目录失败");
        Self { temp_dir }
    }

    /// 获取模拟器安装路径
    pub fn emulator_path(&self) -> PathBuf {
        self.temp_dir.path().join("emulator")
    }

    /// 获取下载目录路径
    #[allow(dead_code)]
    pub fn download_path(&self) -> PathBuf {
        self.temp_dir.path().join("downloads")
    }

    /// 设置为全局配置
    pub fn apply_to_global_config(&self) {
        use ns_emu_tools_lib::config::CONFIG;

        let mut config = CONFIG.write();
        config.yuzu.yuzu_path = self.emulator_path();
        config.setting.download.auto_delete_after_install = true;
    }
}

impl Default for TestConfigHelper {
    fn default() -> Self {
        Self::new()
    }
}

/// 简单的进度打印回调
pub fn simple_progress_printer(prefix: &str) -> impl Fn(ns_emu_tools_lib::services::aria2::Aria2DownloadProgress) {
    let prefix = prefix.to_string();
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));

    move |progress| {
        let count = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // 每 20 次更新输出一次
        if count % 20 == 0 || progress.percentage >= 99.9 {
            tracing::info!(
                "{} {:.1}% ({}/{} MB) @ {} KB/s",
                prefix,
                progress.percentage,
                progress.downloaded / 1_048_576,
                progress.total / 1_048_576,
                progress.speed / 1024
            );
        }
    }
}

/// 断言路径存在
#[allow(dead_code)]
pub fn assert_path_exists(path: &std::path::Path, message: &str) {
    assert!(
        path.exists(),
        "{}: 路径不存在: {}",
        message,
        path.display()
    );
}

/// 断言文件存在
#[allow(dead_code)]
pub fn assert_file_exists(path: &std::path::Path, message: &str) {
    assert_path_exists(path, message);
    assert!(
        path.is_file(),
        "{}: 不是文件: {}",
        message,
        path.display()
    );
}

/// 断言目录存在
#[allow(dead_code)]
pub fn assert_dir_exists(path: &std::path::Path, message: &str) {
    assert_path_exists(path, message);
    assert!(
        path.is_dir(),
        "{}: 不是目录: {}",
        message,
        path.display()
    );
}

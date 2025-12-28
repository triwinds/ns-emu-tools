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
pub fn simple_progress_printer(prefix: &str) -> impl Fn(ns_emu_tools_lib::models::ProgressEvent) + Clone {
    use ns_emu_tools_lib::models::ProgressEvent;

    let prefix = prefix.to_string();

    move |event| {
        match event {
            ProgressEvent::Started { steps } => {
                tracing::info!("{} 开始，共 {} 个步骤", prefix, steps.len());
            }
            ProgressEvent::StepUpdate { step } => {
                tracing::info!(
                    "{} 步骤 [{}] {}: {:?}",
                    prefix,
                    step.id,
                    step.title,
                    step.status
                );
            }
            ProgressEvent::Finished { success, message } => {
                if success {
                    tracing::info!("{} 完成", prefix);
                } else {
                    tracing::warn!("{} 失败: {:?}", prefix, message);
                }
            }
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

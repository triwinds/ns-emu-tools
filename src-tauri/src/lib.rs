//! NS Emu Tools - Rust 后端库
//!
//! Nintendo Switch 模拟器管理工具

pub mod commands;
pub mod config;
pub mod error;
pub mod logging;
pub mod models;
pub mod repositories;
pub mod services;
pub mod utils;

pub use config::{Config, CONFIG, CURRENT_VERSION};
pub use error::{AppError, AppResult};

/// 测试辅助模块
#[cfg(test)]
pub mod test_utils {
    /// 全局测试初始化
    ///
    /// 使用 #[ctor] 属性，在测试模块加载时自动运行
    /// 这样所有测试都会自动初始化 tracing，无需手动调用
    #[ctor::ctor]
    fn init_test() {
        crate::logging::init_test();
    }
}

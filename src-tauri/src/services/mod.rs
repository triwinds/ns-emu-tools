//! 服务模块
//!
//! 业务逻辑层，处理核心功能

pub mod aria2;
pub mod crypto;
pub mod doh;
pub mod firmware;
pub mod keys;
pub mod msvc;
pub mod nca;
pub mod network;
pub mod ryujinx;
pub mod updater;
pub mod yuzu;

#[cfg(not(test))]
pub mod notifier;

// 在测试模式下提供 notifier 的 stub 实现
#[cfg(test)]
pub mod notifier {
    //! 测试模式下的 notifier stub

    /// 发送通知（测试模式下为空实现）
    pub fn send_notify(_window: &tauri::Window, _message: &str) -> Result<(), tauri::Error> {
        // 在测试模式下不做任何事情
        Ok(())
    }
}

pub use aria2::*;
pub use doh::*;
pub use firmware::*;
pub use nca::*;
pub use network::*;
pub use ryujinx::*;
pub use updater::*;
pub use yuzu::*;

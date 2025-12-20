//! 服务模块
//!
//! 业务逻辑层，处理核心功能

pub mod aria2;
pub mod doh;
pub mod firmware;
pub mod network;
pub mod ryujinx;
pub mod yuzu;

#[cfg(not(test))]
pub mod notifier;

pub use aria2::*;
pub use doh::*;
pub use firmware::*;
pub use network::*;
pub use ryujinx::*;
pub use yuzu::*;

#[cfg(not(test))]
pub use notifier::*;

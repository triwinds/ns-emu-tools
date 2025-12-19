//! 服务模块
//!
//! 业务逻辑层，处理核心功能

pub mod aria2;
pub mod doh;
pub mod downloader;
pub mod firmware;
pub mod network;

#[cfg(not(test))]
pub mod notifier;

pub use aria2::*;
pub use doh::*;
pub use downloader::*;
pub use firmware::*;
pub use network::*;

#[cfg(not(test))]
pub use notifier::*;

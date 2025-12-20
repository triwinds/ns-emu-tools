//! 数据访问层
//!
//! 处理外部数据源访问，如 GitHub/GitLab API

pub mod app_info;
pub mod config_data;
pub mod ryujinx;
pub mod yuzu;

pub use app_info::*;
pub use config_data::*;
pub use ryujinx::*;
pub use yuzu::*;
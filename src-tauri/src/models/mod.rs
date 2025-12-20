//! 数据模型模块
//!
//! 定义应用程序中使用的所有数据模型

pub mod release;
pub mod response;
pub mod storage;
pub mod installation;

pub use release::*;
pub use response::*;
pub use storage::*;
pub use installation::*;

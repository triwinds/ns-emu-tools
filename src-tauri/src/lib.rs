//! NS Emu Tools - Rust 后端库
//!
//! Nintendo Switch 模拟器管理工具

pub mod commands;
pub mod config;
pub mod error;
pub mod models;
pub mod repositories;
pub mod services;
pub mod utils;

pub use config::{Config, CONFIG, CURRENT_VERSION};
pub use error::{AppError, AppResult};

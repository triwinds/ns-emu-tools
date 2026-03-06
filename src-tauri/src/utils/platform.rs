//! 平台识别工具模块

use std::env::consts::{ARCH, OS};

/// 当前平台信息
#[derive(Debug, Clone, PartialEq)]
pub struct Platform {
    pub os: PlatformOS,
    pub arch: PlatformArch,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlatformOS {
    Windows,
    MacOS,
    Linux,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlatformArch {
    X86_64,
    Aarch64,
    Other(String),
}

impl Platform {
    /// 获取当前运行平台
    pub fn current() -> Self {
        let os = match OS {
            "windows" => PlatformOS::Windows,
            "macos" => PlatformOS::MacOS,
            "linux" => PlatformOS::Linux,
            _ => PlatformOS::Linux, // fallback
        };

        let arch = match ARCH {
            "x86_64" => PlatformArch::X86_64,
            "aarch64" => PlatformArch::Aarch64,
            other => PlatformArch::Other(other.to_string()),
        };

        Self { os, arch }
    }

    pub fn is_macos(&self) -> bool {
        matches!(self.os, PlatformOS::MacOS)
    }

    pub fn is_windows(&self) -> bool {
        matches!(self.os, PlatformOS::Windows)
    }

    pub fn is_linux(&self) -> bool {
        matches!(self.os, PlatformOS::Linux)
    }
}

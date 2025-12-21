//! 配置管理模块
//!
//! 管理应用程序的配置文件读写和全局配置状态

use crate::error::AppResult;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// 应用程序版本
pub const CURRENT_VERSION: &str = "0.6.0";

/// User-Agent 字符串
pub fn user_agent() -> String {
    format!("ns-emu-tools/{}", CURRENT_VERSION)
}

/// 全局配置实例
pub static CONFIG: Lazy<RwLock<Config>> = Lazy::new(|| {
    RwLock::new(Config::load().unwrap_or_else(|e| {
        warn!("加载配置失败，使用默认配置: {}", e);
        Config::default()
    }))
});

/// 获取配置文件路径
pub fn config_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("config.json")
}

/// Yuzu/Eden/Citron 模拟器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YuzuConfig {
    /// 模拟器安装路径
    #[serde(default = "default_yuzu_path")]
    pub yuzu_path: PathBuf,
    /// 模拟器版本
    #[serde(default)]
    pub yuzu_version: Option<String>,
    /// 固件版本
    #[serde(default)]
    pub yuzu_firmware: Option<String>,
    /// 分支 (eden, citron)
    #[serde(default = "default_yuzu_branch")]
    pub branch: String,
}

fn default_yuzu_path() -> PathBuf {
    PathBuf::from("D:\\Yuzu")
}

fn default_yuzu_branch() -> String {
    "eden".to_string()
}

impl Default for YuzuConfig {
    fn default() -> Self {
        Self {
            yuzu_path: default_yuzu_path(),
            yuzu_version: None,
            yuzu_firmware: None,
            branch: default_yuzu_branch(),
        }
    }
}

/// Ryujinx 模拟器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RyujinxConfig {
    /// 模拟器安装路径
    #[serde(default = "default_ryujinx_path")]
    pub path: PathBuf,
    /// 模拟器版本
    #[serde(default)]
    pub version: Option<String>,
    /// 固件版本
    #[serde(default)]
    pub firmware: Option<String>,
    /// 分支 (mainline, canary)
    #[serde(default = "default_ryujinx_branch")]
    pub branch: String,
}

fn default_ryujinx_path() -> PathBuf {
    PathBuf::from("D:\\Ryujinx")
}

fn default_ryujinx_branch() -> String {
    "mainline".to_string()
}

impl Default for RyujinxConfig {
    fn default() -> Self {
        Self {
            path: default_ryujinx_path(),
            version: None,
            firmware: None,
            branch: default_ryujinx_branch(),
        }
    }
}

/// 网络设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSetting {
    /// 固件下载源
    #[serde(default = "default_firmware_source")]
    pub firmware_download_source: String,
    /// GitHub API 模式
    #[serde(default = "default_github_api_mode")]
    pub github_api_mode: String,
    /// GitHub 下载镜像
    #[serde(default = "default_github_mirror")]
    pub github_download_mirror: String,
    /// Ryujinx GitLab 下载镜像
    #[serde(default = "default_direct")]
    pub ryujinx_git_lab_download_mirror: String,
    /// 是否使用 DoH
    #[serde(default = "default_true")]
    pub use_doh: bool,
    /// 代理设置
    #[serde(default = "default_proxy")]
    pub proxy: String,
}

fn default_firmware_source() -> String {
    "github".to_string()
}

fn default_github_api_mode() -> String {
    "direct".to_string()
}

fn default_github_mirror() -> String {
    "cloudflare_load_balance".to_string()
}

fn default_direct() -> String {
    "direct".to_string()
}

fn default_true() -> bool {
    true
}

fn default_proxy() -> String {
    "system".to_string()
}

impl Default for NetworkSetting {
    fn default() -> Self {
        Self {
            firmware_download_source: default_firmware_source(),
            github_api_mode: default_github_api_mode(),
            github_download_mirror: default_github_mirror(),
            ryujinx_git_lab_download_mirror: default_direct(),
            use_doh: default_true(),
            proxy: default_proxy(),
        }
    }
}

/// 下载设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadSetting {
    /// 安装后自动删除下载文件
    #[serde(default = "default_true")]
    pub auto_delete_after_install: bool,
    /// 禁用 aria2 IPv6
    #[serde(default = "default_true")]
    pub disable_aria2_ipv6: bool,
    /// 删除旧的 aria2 日志文件
    #[serde(default = "default_true")]
    pub remove_old_aria2_log_file: bool,
}

impl Default for DownloadSetting {
    fn default() -> Self {
        Self {
            auto_delete_after_install: true,
            disable_aria2_ipv6: true,
            remove_old_aria2_log_file: true,
        }
    }
}

/// UI 设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSetting {
    /// 上次打开的模拟器页面
    #[serde(default = "default_last_page")]
    pub last_open_emu_page: String,
    /// 是否深色模式
    #[serde(default = "default_true")]
    pub dark: bool,
    /// 主题模式 (auto, light, dark)
    #[serde(default = "default_theme_mode")]
    pub mode: String,
    /// 窗口宽度
    #[serde(default = "default_width")]
    pub width: u32,
    /// 窗口高度
    #[serde(default = "default_height")]
    pub height: u32,
}

fn default_last_page() -> String {
    "ryujinx".to_string()
}

fn default_theme_mode() -> String {
    "auto".to_string()
}

fn default_width() -> u32 {
    1300
}

fn default_height() -> u32 {
    850
}

impl Default for UiSetting {
    fn default() -> Self {
        Self {
            last_open_emu_page: default_last_page(),
            dark: true,
            mode: default_theme_mode(),
            width: default_width(),
            height: default_height(),
        }
    }
}

/// 其他设置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OtherSetting {
    /// 将 Yuzu 重命名为 Cemu
    #[serde(default)]
    pub rename_yuzu_to_cemu: bool,
}

/// 通用设置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommonSetting {
    /// UI 设置
    #[serde(default)]
    pub ui: UiSetting,
    /// 网络设置
    #[serde(default)]
    pub network: NetworkSetting,
    /// 下载设置
    #[serde(default)]
    pub download: DownloadSetting,
    /// 其他设置
    #[serde(default)]
    pub other: OtherSetting,
}

/// 应用程序配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Yuzu 配置
    #[serde(default)]
    pub yuzu: YuzuConfig,
    /// Ryujinx 配置
    #[serde(default)]
    pub ryujinx: RyujinxConfig,
    /// 通用设置
    #[serde(default)]
    pub setting: CommonSetting,
}

impl Config {
    /// 从文件加载配置
    pub fn load() -> AppResult<Self> {
        let path = config_path();
        if path.exists() {
            info!("从 {} 加载配置", path.display());
            let content = std::fs::read_to_string(&path)?;
            let config: Config = serde_json::from_str(&content)?;
            debug!("配置加载成功");
            Ok(config)
        } else {
            info!("配置文件不存在，创建默认配置");
            let config = Self::default();
            config.save()?;
            Ok(config)
        }
    }

    /// 保存配置到文件
    pub fn save(&self) -> AppResult<()> {
        let path = config_path();
        info!("保存配置到 {}", path.display());
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        debug!("配置保存成功");
        Ok(())
    }
}

/// 更新上次打开的模拟器页面
pub fn update_last_open_emu_page(page: &str) -> AppResult<()> {
    let mut config = CONFIG.write();
    config.setting.ui.last_open_emu_page = if page == "ryujinx" {
        "ryujinx".to_string()
    } else {
        "yuzu".to_string()
    };
    info!(
        "更新 lastOpenEmuPage 为 {}",
        config.setting.ui.last_open_emu_page
    );
    config.save()
}

/// 更新深色模式状态
pub fn update_dark_state(dark: bool) -> AppResult<()> {
    let mut config = CONFIG.write();
    config.setting.ui.dark = dark;
    info!("更新 dark 为 {}", config.setting.ui.dark);
    config.save()
}

/// 更新设置
pub fn update_setting(setting: CommonSetting) -> AppResult<()> {
    let mut config = CONFIG.write();
    info!("更新设置");
    config.setting = setting;
    config.save()
}

/// 更新窗口大小
pub fn update_window_size(width: u32, height: u32) -> AppResult<()> {
    let mut config = CONFIG.write();
    config.setting.ui.width = width;
    config.setting.ui.height = height;
    info!("更新窗口大小为 {}x{}", width, height);
    config.save()
}

/// 获取当前配置的克隆
pub fn get_config() -> Config {
    CONFIG.read().clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.yuzu.branch, "eden");
        assert_eq!(config.ryujinx.branch, "mainline");
        assert!(config.setting.ui.dark);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(config.yuzu.branch, parsed.yuzu.branch);
    }
}

//! 配置管理模块
//!
//! 管理应用程序的配置文件读写和全局配置状态

use crate::error::{AppError, AppResult};
use crate::utils::write_string_atomic;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

/// 应用程序版本（自动从 Cargo.toml 读取）
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

const CONFIG_SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

/// User-Agent 字符串
pub fn user_agent() -> String {
    format!("ns-emu-tools/{}", CURRENT_VERSION)
}

/// 全局配置实例
pub static CONFIG: Lazy<RwLock<Config>> = Lazy::new(|| {
    RwLock::new(Config::load().unwrap_or_else(|e| {
        warn!("加载配置失败，将使用默认配置：{}", e);
        Config::default()
    }))
});

static CONFIG_PERSISTENCE: Lazy<ConfigPersistence> = Lazy::new(ConfigPersistence::new);

struct ConfigPersistence {
    sender: mpsc::Sender<ConfigPersistenceCommand>,
}

enum ConfigPersistenceCommand {
    Schedule(Config),
    PersistNow(Config, SyncSender<AppResult<()>>),
    Flush(SyncSender<AppResult<()>>),
}

impl ConfigPersistence {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("config-persistence".to_string())
            .spawn(move || config_persistence_worker(receiver))
            .expect("failed to spawn config persistence worker");

        Self { sender }
    }

    fn schedule(&self, config: Config) -> AppResult<()> {
        self.sender
            .send(ConfigPersistenceCommand::Schedule(config))
            .map_err(|e| AppError::Config(format!("发送配置持久化任务失败: {}", e)))
    }

    fn persist_now(&self, config: Config) -> AppResult<()> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(0);
        self.sender
            .send(ConfigPersistenceCommand::PersistNow(config, reply_tx))
            .map_err(|e| AppError::Config(format!("发送立即保存配置任务失败: {}", e)))?;
        wait_config_persistence_reply(reply_rx, "等待立即保存配置结果失败")
    }

    fn flush(&self) -> AppResult<()> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(0);
        self.sender
            .send(ConfigPersistenceCommand::Flush(reply_tx))
            .map_err(|e| AppError::Config(format!("发送刷新配置任务失败: {}", e)))?;
        wait_config_persistence_reply(reply_rx, "等待刷新配置结果失败")
    }
}

fn wait_config_persistence_reply(
    receiver: mpsc::Receiver<AppResult<()>>,
    context: &str,
) -> AppResult<()> {
    receiver
        .recv()
        .map_err(|e| AppError::Config(format!("{}: {}", context, e)))?
}

fn config_persistence_worker(receiver: Receiver<ConfigPersistenceCommand>) {
    let mut last_saved: Option<Config> = None;
    let mut pending: Option<Config> = None;
    let mut deadline: Option<Instant> = None;

    loop {
        let command = match deadline {
            Some(target) => {
                let now = Instant::now();
                let timeout = target.saturating_duration_since(now);
                match receiver.recv_timeout(timeout) {
                    Ok(command) => Some(command),
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        if let Err(err) = flush_pending_config(&mut pending, &mut last_saved) {
                            error!("配置持久化线程退出前刷新失败: {}", err);
                        }
                        break;
                    }
                }
            }
            None => match receiver.recv() {
                Ok(command) => Some(command),
                Err(_) => {
                    if let Err(err) = flush_pending_config(&mut pending, &mut last_saved) {
                        error!("配置持久化线程退出前刷新失败: {}", err);
                    }
                    break;
                }
            },
        };

        match command {
            Some(ConfigPersistenceCommand::Schedule(config)) => {
                if last_saved.as_ref() == Some(&config) {
                    pending = None;
                    deadline = None;
                    debug!("配置内容未变化，跳过延迟保存");
                } else {
                    pending = Some(config);
                    deadline = Some(Instant::now() + CONFIG_SAVE_DEBOUNCE);
                }
            }
            Some(ConfigPersistenceCommand::PersistNow(config, reply_tx)) => {
                pending = None;
                deadline = None;

                let result = persist_config_snapshot(&config);
                if result.is_ok() {
                    last_saved = Some(config);
                }

                let _ = reply_tx.send(result);
            }
            Some(ConfigPersistenceCommand::Flush(reply_tx)) => {
                deadline = None;
                let result = flush_pending_config(&mut pending, &mut last_saved);
                let _ = reply_tx.send(result);
            }
            None => {
                deadline = None;
                if let Err(err) = flush_pending_config(&mut pending, &mut last_saved) {
                    error!("延迟保存配置失败: {}", err);
                }
            }
        }
    }
}

fn flush_pending_config(
    pending: &mut Option<Config>,
    last_saved: &mut Option<Config>,
) -> AppResult<()> {
    let Some(snapshot) = pending.clone() else {
        return Ok(());
    };

    if last_saved.as_ref() == Some(&snapshot) {
        *pending = None;
        return Ok(());
    }

    persist_config_snapshot(&snapshot)?;
    *last_saved = Some(snapshot);
    *pending = None;
    Ok(())
}

/// 获取应用程序数据目录
pub fn app_data_dir() -> PathBuf {
    // 优先使用 ProjectDirs 获取平台特定的数据目录
    // macOS: ~/Library/Application Support/com.nsemu.tools
    // Windows: C:\Users\<User>\AppData\Roaming\com.nsemu.tools
    // Linux: ~/.config/com.nsemu.tools
    if let Some(proj_dirs) = directories::ProjectDirs::from("com", "nsemu", "tools") {
        proj_dirs.data_dir().to_path_buf()
    } else {
        // 降级方案：使用当前目录
        warn!("无法获取应用数据目录，将使用当前目录");
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

/// 获取有效配置目录
///
/// 如果可执行文件所在目录中已存在 config.json，优先使用该目录；
/// 否则使用平台特定的 app_data 目录。
pub fn effective_config_dir() -> PathBuf {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            if exe_dir.join("config.json").exists() {
                return exe_dir.to_path_buf();
            }
        }
    }
    app_data_dir()
}

/// 获取配置文件路径
pub fn config_path() -> PathBuf {
    let dir = effective_config_dir();

    // 确保目录存在
    if let Err(e) = std::fs::create_dir_all(&dir) {
        warn!("创建应用数据目录失败：{}", e);
    }

    dir.join("config.json")
}

fn persist_config_snapshot(config: &Config) -> AppResult<()> {
    let path = config_path();
    info!("正在将配置保存到 {}", path.display());
    let content = serde_json::to_string_pretty(config)?;
    write_string_atomic(&path, &content)?;
    debug!("配置保存成功");
    Ok(())
}

fn schedule_config_save(config: Config) -> AppResult<()> {
    CONFIG_PERSISTENCE.schedule(config)
}

/// Yuzu/Eden 模拟器配置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    /// 分支（默认 eden，历史配置中可能保留 citron）
    #[serde(default = "default_yuzu_branch")]
    pub branch: String,
}

fn default_yuzu_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join("yuzu");
        }
        return PathBuf::from("~/yuzu");
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join("Yuzu");
        }
        return PathBuf::from("~/Yuzu");
    }

    #[cfg(target_os = "windows")]
    return PathBuf::from("D:\\Yuzu");
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join("ryujinx");
        }
        return PathBuf::from("~/ryujinx");
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join("Ryujinx");
        }
        return PathBuf::from("~/Ryujinx");
    }

    #[cfg(target_os = "windows")]
    return PathBuf::from("D:\\Ryujinx");
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSetting {
    /// GitHub API 模式
    #[serde(default = "default_github_api_mode")]
    pub github_api_mode: String,
    /// GitHub 下载镜像
    #[serde(default = "default_github_mirror")]
    pub github_download_mirror: String,
    /// Ryujinx 官方源下载镜像
    #[serde(
        rename = "ryujinxOfficialDownloadMirror",
        alias = "ryujinxGitLabDownloadMirror",
        default = "default_direct"
    )]
    pub ryujinx_official_download_mirror: String,
    /// Eden 官方源下载镜像
    #[serde(default = "default_auto_detect")]
    pub eden_git_download_mirror: String,
    /// 是否使用 DoH
    #[serde(default = "default_true")]
    pub use_doh: bool,
    /// 代理设置
    #[serde(default = "default_proxy")]
    pub proxy: String,
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

fn default_auto_detect() -> String {
    "auto-detect".to_string()
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
            github_api_mode: default_github_api_mode(),
            github_download_mirror: default_github_mirror(),
            ryujinx_official_download_mirror: default_direct(),
            eden_git_download_mirror: default_auto_detect(),
            use_doh: default_true(),
            proxy: default_proxy(),
        }
    }
}

/// 下载设置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    /// 下载后端 (auto, aria2, rust, bytehaul)。其中 auto 默认优先 rust，bytehaul 为兼容旧配置的别名。
    #[serde(default = "default_download_backend")]
    pub backend: String,
}

fn default_download_backend() -> String {
    "auto".to_string()
}

impl Default for DownloadSetting {
    fn default() -> Self {
        Self {
            auto_delete_after_install: true,
            disable_aria2_ipv6: true,
            remove_old_aria2_log_file: true,
            backend: default_download_backend(),
        }
    }
}

/// UI 设置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UiSetting {
    /// 上次打开的模拟器页面
    #[serde(default = "default_last_page")]
    pub last_open_emu_page: String,
    /// 是否深色模式
    #[serde(default = "default_true")]
    pub dark: bool,
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
            width: default_width(),
            height: default_height(),
        }
    }
}

/// 其他设置
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct OtherSetting {
    /// 将 Yuzu 重命名为 Cemu
    #[serde(default)]
    pub rename_yuzu_to_cemu: bool,
}

/// 通用设置
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
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
    fn normalize_download_backend_alias(&mut self) -> bool {
        normalize_download_backend_alias_value(&mut self.setting.download.backend)
    }

    /// 从文件加载配置
    pub fn load() -> AppResult<Self> {
        let path = config_path();
        info!("配置文件路径为：{}", path.display());
        if path.exists() {
            info!("正在从 {} 加载配置", path.display());
            let content = std::fs::read_to_string(&path)?;
            let mut config: Config = serde_json::from_str(&content)?;
            if config.normalize_download_backend_alias() {
                info!("download.backend = bytehaul 已废弃，自动映射为 rust");
                persist_config_snapshot(&config)?;
            }
            debug!("配置加载成功");
            Ok(config)
        } else {
            info!("配置文件不存在，将创建默认配置文件：{}", path.display());
            let config = Self::default();
            config.save()?;
            Ok(config)
        }
    }

    /// 保存配置到文件
    pub fn save(&self) -> AppResult<()> {
        let mut snapshot = self.clone();
        if snapshot.normalize_download_backend_alias() {
            info!("download.backend = bytehaul 已废弃，保存时自动映射为 rust");
        }
        CONFIG_PERSISTENCE.persist_now(snapshot)
    }
}

fn normalize_download_backend_alias_value(backend: &mut String) -> bool {
    if backend.trim().eq_ignore_ascii_case("bytehaul") {
        *backend = "rust".to_string();
        return true;
    }

    false
}

/// 更新上次打开的模拟器页面
pub fn update_last_open_emu_page(page: &str) -> AppResult<()> {
    let page = if page == "ryujinx" { "ryujinx" } else { "yuzu" };
    let snapshot = {
        let mut config = CONFIG.write();
        if config.setting.ui.last_open_emu_page == page {
            return Ok(());
        }
        config.setting.ui.last_open_emu_page = page.to_string();
        info!(
            "已将 lastOpenEmuPage 更新为 {}",
            config.setting.ui.last_open_emu_page
        );
        config.clone()
    };

    schedule_config_save(snapshot)
}

/// 更新深色模式状态
pub fn update_dark_state(dark: bool) -> AppResult<()> {
    let snapshot = {
        let mut config = CONFIG.write();
        if config.setting.ui.dark == dark {
            return Ok(());
        }
        config.setting.ui.dark = dark;
        info!("已将深色模式更新为 {}", config.setting.ui.dark);
        config.clone()
    };

    schedule_config_save(snapshot)
}

/// 更新设置
pub fn update_setting(mut setting: CommonSetting) -> AppResult<()> {
    if normalize_download_backend_alias_value(&mut setting.download.backend) {
        info!("download.backend = bytehaul 已废弃，更新设置时自动映射为 rust");
    }

    let snapshot = {
        let mut config = CONFIG.write();
        if config.setting == setting {
            info!("设置未变化，跳过保存");
            return Ok(());
        }
        info!("正在更新设置");
        config.setting = setting;
        config.clone()
    };

    schedule_config_save(snapshot)
}

/// 更新窗口大小
pub fn update_window_size(width: u32, height: u32) -> AppResult<()> {
    let snapshot = {
        let mut config = CONFIG.write();
        if config.setting.ui.width == width && config.setting.ui.height == height {
            return Ok(());
        }
        config.setting.ui.width = width;
        config.setting.ui.height = height;
        info!("已将窗口大小更新为 {}x{}", width, height);
        config.clone()
    };

    schedule_config_save(snapshot)
}

/// 获取当前配置的克隆
pub fn get_config() -> Config {
    CONFIG.read().clone()
}

/// 使用新配置替换当前配置并延迟保存。
pub fn replace_config(mut config: Config) -> AppResult<()> {
    if config.normalize_download_backend_alias() {
        info!("download.backend = bytehaul 已废弃，替换配置时自动映射为 rust");
    }

    let snapshot = {
        let mut current = CONFIG.write();
        if *current == config {
            info!("配置未变化，跳过保存");
            return Ok(());
        }
        *current = config;
        current.clone()
    };

    schedule_config_save(snapshot)
}

/// 立即刷新待保存的配置。
pub fn flush_pending_config_save() -> AppResult<()> {
    CONFIG_PERSISTENCE.flush()
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
        assert_eq!(
            config.setting.network.eden_git_download_mirror,
            "auto-detect"
        );
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(config.yuzu.branch, parsed.yuzu.branch);
        assert!(json.contains("ryujinxOfficialDownloadMirror"));
        assert!(!json.contains("ryujinxGitLabDownloadMirror"));
    }

    #[test]
    fn test_config_deserialization_accepts_legacy_ryujinx_gitlab_key() {
        let json = serde_json::json!({
            "setting": {
                "network": {
                    "ryujinxGitLabDownloadMirror": "cdn"
                }
            }
        });

        let parsed: Config = serde_json::from_value(json).unwrap();

        assert_eq!(
            parsed.setting.network.ryujinx_official_download_mirror,
            "cdn"
        );
    }

    #[test]
    fn test_normalize_download_backend_rewrites_bytehaul_alias_to_rust() {
        let mut config = Config::default();
        config.setting.download.backend = "bytehaul".to_string();

        let changed = config.normalize_download_backend_alias();

        assert!(changed);
        assert_eq!(config.setting.download.backend, "rust");
    }

    #[test]
    fn test_normalize_download_backend_keeps_rust_value() {
        let mut config = Config::default();
        config.setting.download.backend = "rust".to_string();

        let changed = config.normalize_download_backend_alias();

        assert!(!changed);
        assert_eq!(config.setting.download.backend, "rust");
    }
}

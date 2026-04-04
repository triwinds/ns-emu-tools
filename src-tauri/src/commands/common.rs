//! 通用命令模块
//!
//! 提供通用的 Tauri 命令，如配置管理、系统操作等

use crate::config::{self, CommonSetting, Config, CURRENT_VERSION};
use crate::error::AppError;
use crate::models::storage::{self, Storage};
use crate::repositories::app_info::{self, UpdateCheckResult};
use crate::repositories::config_data;
use crate::services::installer::{
    error_step, pending_step, running_step, success_step, InstallReporter, StepKind,
};
use tauri::{command, AppHandle, Emitter, Window};
use tauri_plugin_opener::OpenerExt;
use tracing::{error, info};

/// 获取当前配置
#[command]
pub fn get_config() -> Result<Config, String> {
    Ok(config::get_config())
}

/// 保存配置
#[command]
pub fn save_config(config: Config) -> Result<(), String> {
    config::replace_config(config).map_err(|e| e.to_string())
}

/// 获取存储数据
#[command]
pub fn get_storage() -> Result<Storage, String> {
    Ok(storage::get_storage())
}

/// 获取应用版本
#[command]
pub fn get_app_version() -> String {
    CURRENT_VERSION.to_string()
}

/// 获取当前运行平台
#[command]
pub fn get_platform() -> String {
    #[cfg(target_os = "windows")]
    return "windows".to_string();

    #[cfg(target_os = "macos")]
    return "macos".to_string();

    #[cfg(target_os = "linux")]
    return "linux".to_string();
}

/// 打开文件夹
#[command]
pub async fn open_folder(path: String) -> Result<(), String> {
    info!("打开文件夹: {}", path);
    let path = std::path::Path::new(&path);

    if !path.exists() {
        return Err(AppError::DirectoryNotFound(path.to_string_lossy().to_string()).to_string());
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// 打开 config.json 所在文件夹
#[command]
pub async fn open_config_folder() -> Result<(), String> {
    let config_path = config::config_path();
    let config_dir = config_path
        .parent()
        .ok_or_else(|| "无法获取 config.json 所在目录".to_string())?;

    open_folder(config_dir.to_string_lossy().to_string()).await
}

/// 打开 URL
#[command]
pub fn open_url(url: String, app: AppHandle) -> Result<(), String> {
    info!("正在打开链接：{}", url);
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| e.to_string())
}

/// 更新设置
#[command]
pub fn update_setting(setting: CommonSetting) -> Result<(), String> {
    config::update_setting(setting).map_err(|e| e.to_string())
}

/// 更新上次打开的模拟器页面
#[command]
pub fn update_last_open_emu_page(page: String) -> Result<(), String> {
    config::update_last_open_emu_page(&page).map_err(|e| e.to_string())
}

/// 更新深色模式状态
#[command]
pub fn update_dark_state(dark: bool) -> Result<(), String> {
    config::update_dark_state(dark).map_err(|e| e.to_string())
}

/// 更新窗口大小
#[command]
pub fn update_window_size(width: u32, height: u32) -> Result<(), String> {
    config::update_window_size(width, height).map_err(|e| e.to_string())
}

/// 删除历史路径
#[command]
pub fn delete_history_path(emu_type: String, path: String) -> Result<(), String> {
    storage::delete_history_path(&emu_type, &path).map_err(|e| e.to_string())
}

/// 删除路径（文件或文件夹）
#[command]
pub async fn delete_path(path: String, window: Window) -> Result<(), String> {
    use std::fs;
    use std::path::Path;

    let path = Path::new(&path);

    if !path.exists() {
        let msg = format!("{} 不存在", path.display());
        let _ = window.emit("message", &msg);
        return Err(msg);
    }

    if path.is_dir() {
        info!("删除文件夹: {}", path.display());
        let msg = format!("正在删除 {} 目录...", path.display());
        let _ = window.emit("message", &msg);

        fs::remove_dir_all(path).map_err(|e| {
            error!("删除文件夹失败: {}", e);
            e.to_string()
        })?;

        let msg = format!("{} 删除完成", path.display());
        let _ = window.emit("message", &msg);
    } else {
        info!("删除文件: {}", path.display());
        let msg = format!("正在删除 {}...", path.display());
        let _ = window.emit("message", &msg);

        fs::remove_file(path).map_err(|e| {
            error!("删除文件失败: {}", e);
            e.to_string()
        })?;

        let msg = format!("{} 删除完成", path.display());
        let _ = window.emit("message", &msg);
    }

    Ok(())
}

/// 检查应用更新
#[command]
pub async fn check_update(include_prerelease: bool) -> Result<UpdateCheckResult, String> {
    app_info::check_update(include_prerelease)
        .await
        .map_err(|e| e.to_string())
}

/// 加载变更日志
#[command]
pub async fn load_change_log() -> Result<String, String> {
    app_info::load_change_log().await.map_err(|e| e.to_string())
}

/// 获取 GitHub 镜像列表
#[command]
pub async fn get_github_mirrors(
) -> Result<crate::repositories::config_data::GithubMirrorListResponse, String> {
    config_data::get_github_mirrors()
        .await
        .map_err(|e| e.to_string())
}

/// 强制刷新 GitHub 镜像列表
#[command]
pub async fn refresh_github_mirrors(
) -> Result<crate::repositories::config_data::GithubMirrorListResponse, String> {
    config_data::refresh_github_mirrors()
        .await
        .map_err(|e| e.to_string())
}

/// 获取游戏数据映射
#[command]
pub async fn get_game_data() -> Result<std::collections::HashMap<String, serde_json::Value>, String>
{
    config_data::get_game_data()
        .await
        .map_err(|e| e.to_string())
}

/// 获取可用固件信息列表
#[command]
pub async fn get_available_firmware_infos(
) -> Result<Vec<crate::services::firmware::FirmwareInfo>, String> {
    crate::services::firmware::get_firmware_infos()
        .await
        .map_err(|e| e.to_string())
}

/// 加载历史路径列表
#[command]
pub fn load_history_path(emu_type: String) -> Result<Vec<String>, String> {
    storage::load_history_path(&emu_type).map_err(|e| e.to_string())
}

/// 检测固件版本
#[command]
pub async fn detect_firmware_version(emu_type: String, window: Window) -> Result<String, String> {
    let reporter = InstallReporter::from_window(window.clone());
    info!("开始检测 {} 的固件版本", emu_type);

    reporter.start(vec![
        pending_step("load_keys", "加载密钥文件"),
        pending_step("find_nca", "查找固件文件"),
        pending_step("extract_version", "提取版本信息"),
    ]);
    reporter.step(running_step("find_nca", "查找固件文件"));

    let result = match emu_type.as_str() {
        "yuzu" => crate::services::firmware::detect_yuzu_firmware_version(Some(&window))
            .await
            .map_err(|e| e.to_string()),
        "ryujinx" => crate::services::firmware::detect_ryujinx_firmware_version(Some(&window))
            .await
            .map_err(|e| e.to_string()),
        _ => Err(format!("不支持的模拟器类型: {}", emu_type)),
    };

    match result {
        Ok(version) => {
            reporter.step(success_step("find_nca", "查找固件文件"));
            reporter.step(success_step("extract_version", "提取版本信息"));
            reporter.finish(true, Some(format!("检测到固件版本: {}", version)));
            Ok(version)
        }
        Err(e) => {
            reporter.step(error_step(
                "find_nca",
                "查找固件文件",
                StepKind::Normal,
                e.clone(),
            ));
            reporter.finish_error(e.clone());
            Err(e)
        }
    }
}

/// 加载密钥文件
#[command]
pub fn load_keys(path: String) -> Result<(), String> {
    info!("加载密钥文件: {}", path);
    crate::services::keys::load_keys(&path).map_err(|e| e.to_string())
}

/// 检查密钥是否已加载
#[command]
pub fn is_keys_loaded() -> bool {
    crate::services::keys::is_keys_loaded()
}

/// 下载应用更新
#[command]
pub async fn download_app_update(
    window: Window,
    include_prerelease: bool,
    download_url: Option<String>,
) -> Result<String, String> {
    info!(
        "开始下载应用更新（包含预发布版本：{}，指定下载链接：{:?}）",
        include_prerelease, download_url
    );

    match crate::services::updater::download_update(&window, include_prerelease, download_url).await
    {
        Ok(path) => Ok(path.to_string_lossy().to_string()),
        Err(e) => Err(e.to_string()),
    }
}

/// 安装应用更新并重启
#[command]
pub async fn install_app_update(update_file: String) -> Result<(), String> {
    info!("开始安装应用更新：{}", update_file);

    let update_path = std::path::Path::new(&update_file);
    match crate::services::updater::install_update(update_path).await {
        Ok(_) => {
            info!("更新脚本已启动，程序即将退出");
            // 等待一小段时间确保脚本启动
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            // 退出程序
            std::process::exit(0);
        }
        Err(e) => Err(e.to_string()),
    }
}

/// 根据 tag 更新自身（一体化更新流程）
#[command]
pub async fn update_self_by_tag(tag: String, window: Window) -> Result<(), String> {
    info!("开始根据标签更新程序：{}", tag);

    // 1. 下载并解压更新文件
    let update_path = match crate::services::updater::update_self_by_tag(&window, &tag).await {
        Ok(path) => path,
        Err(e) => {
            let err_msg = format!("下载更新失败: {}", e);
            error!("{}", err_msg);
            return Err(err_msg);
        }
    };

    info!("更新文件已准备完成：{}", update_path.display());

    // 2. 安装更新（生成并执行更新脚本）
    match crate::services::updater::install_update(&update_path).await {
        Ok(_) => {
            info!("更新脚本已启动，程序即将退出");
            // 等待一小段时间确保脚本启动
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            // 退出程序
            std::process::exit(0);
        }
        Err(e) => {
            let err_msg = format!("安装更新失败: {}", e);
            error!("{}", err_msg);
            Err(err_msg)
        }
    }
}

/// 取消下载任务（统一接口）
///
/// # 参数
/// - `remove_files`: 是否删除已下载的文件和 aria2 控制文件
#[command]
pub async fn cancel_download_command(
    remove_files: Option<bool>,
) -> Result<crate::models::response::ApiResponse<Option<String>>, String> {
    use crate::models::response::ApiResponse;
    use crate::services::downloader::cancel_active_downloads;

    let should_remove = remove_files.unwrap_or(false);
    info!("开始取消下载任务，是否删除文件：{}", should_remove);

    match cancel_active_downloads(should_remove).await {
        Ok(file_path) => {
            info!("下载任务已取消，文件路径：{:?}", file_path);
            Ok(ApiResponse::success(file_path))
        }
        Err(e) => {
            error!("取消下载失败: {}", e);
            Err(e.to_string())
        }
    }
}

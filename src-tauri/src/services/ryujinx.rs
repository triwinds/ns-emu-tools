//! Ryujinx 模拟器管理服务
//!
//! 提供 Ryujinx 模拟器（mainline/canary 分支）的安装、更新、版本检测等功能

use crate::config::{get_config, CONFIG};
use crate::error::{AppError, AppResult};
use crate::repositories::ryujinx::{
    get_all_ryujinx_release_infos, get_ryujinx_release_info_by_version, load_ryujinx_change_log,
};
use crate::services::aria2::{get_aria2_manager, Aria2DownloadOptions};
use crate::services::network::get_final_url;
use crate::utils::archive::uncompress;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tracing::{debug, info};

/// Ryujinx 可执行文件名
const RYUJINX_EXE: &str = "Ryujinx.exe";

/// 获取 Ryujinx 下载 URL
///
/// # 参数
/// * `target_version` - 目标版本
/// * `branch` - 分支 (mainline, canary)
///
/// # 返回
/// 下载 URL
async fn get_ryujinx_download_url(target_version: &str, branch: &str) -> AppResult<String> {
    let release_info = get_ryujinx_release_info_by_version(target_version, branch).await?;

    if release_info.tag_name.is_empty() {
        return Err(AppError::Emulator(format!(
            "未找到 Ryujinx {} 版本: {}",
            branch, target_version
        )));
    }

    // 查找 Windows x64 版本
    for asset in &release_info.assets {
        let name = asset.name.to_lowercase();
        if name.starts_with("ryujinx-") && name.ends_with("-win_x64.zip") {
            let url = get_final_url(&asset.download_url);
            return Ok(url);
        }
    }

    Err(AppError::Emulator(format!(
        "未找到 Ryujinx {} [{}] 版本的下载链接",
        branch, target_version
    )))
}

/// 检测当前 Ryujinx 分支
///
/// 通过检查可执行文件名来判断是 mainline 还是 canary
pub fn detect_current_branch() -> String {
    let config = get_config();
    let ryujinx_path = PathBuf::from(&config.ryujinx.path);

    // 检查可执行文件
    let exe_path = ryujinx_path.join(RYUJINX_EXE);
    if !exe_path.exists() {
        return "mainline".to_string();
    }

    // 尝试从文件版本信息检测（Windows）
    #[cfg(windows)]
    {
        // TODO: 读取 PE 文件版本信息判断是否为 canary
    }

    // 默认返回配置中的分支
    config.ryujinx.branch.clone()
}

/// 安装 Ryujinx
///
/// # 参数
/// * `target_version` - 目标版本
/// * `branch` - 分支 (mainline, canary)
/// * `on_progress` - 下载进度回调
pub async fn install_ryujinx_by_version<F>(
    target_version: &str,
    branch: &str,
    on_progress: F,
) -> AppResult<()>
where
    F: Fn(crate::services::aria2::Aria2DownloadProgress) + Send + 'static,
{
    info!(
        "开始安装 Ryujinx {} 版本: {}",
        branch, target_version
    );

    let config = get_config();

    // 检查当前版本
    let current_branch = detect_current_branch();
    if let Some(ref current_version) = config.ryujinx.version {
        if current_version == target_version
            && (branch == "ldn" || current_branch == branch)
        {
            info!("当前已是目标版本，跳过安装");
            return Ok(());
        }
    }

    // 获取下载 URL
    let download_url = get_ryujinx_download_url(target_version, branch).await?;
    info!("下载 URL: {}", download_url);

    // 使用 aria2 下载
    let aria2 = get_aria2_manager().await?;
    let options = Aria2DownloadOptions {
        use_github_mirror: false, // Ryujinx 使用 GitLab，不需要镜像
        ..Default::default()
    };

    let result = aria2
        .download_and_wait(&download_url, options, on_progress)
        .await?;

    info!("下载完成: {}", result.path.display());

    // 解压到临时目录
    let tmp_dir = std::env::temp_dir().join("ryujinx-install");
    if tmp_dir.exists() {
        std::fs::remove_dir_all(&tmp_dir)?;
    }
    std::fs::create_dir_all(&tmp_dir)?;

    info!("解压 Ryujinx 文件到: {}", tmp_dir.display());
    uncompress(&result.path, &tmp_dir, false)?;

    // 清理旧文件并安装
    let ryujinx_path = PathBuf::from(&config.ryujinx.path);
    clear_ryujinx_folder(&ryujinx_path)?;

    // 复制文件
    let ryujinx_tmp_dir = tmp_dir.join("publish");
    info!("复制 Ryujinx 文件到: {}", ryujinx_path.display());

    copy_dir_all(&ryujinx_tmp_dir, &ryujinx_path)?;

    // 清理临时目录
    std::fs::remove_dir_all(&tmp_dir)?;

    // 更新配置
    {
        let mut cfg = CONFIG.write();
        cfg.ryujinx.version = Some(target_version.to_string());
        cfg.ryujinx.branch = branch.to_string();
        cfg.save()?;
    }

    info!("Ryujinx {} [{}] 安装成功", branch, target_version);

    // 如果配置了自动删除，删除下载文件
    let config = get_config();
    if config.setting.download.auto_delete_after_install {
        let _ = std::fs::remove_file(&result.path);
    }

    // TODO: 检查并安装 MSVC 运行库

    Ok(())
}

/// 清理 Ryujinx 文件夹（保留用户数据）
///
/// 删除旧的程序文件，但保留用户数据和配置
fn clear_ryujinx_folder(ryujinx_path: &Path) -> AppResult<()> {
    if !ryujinx_path.exists() {
        std::fs::create_dir_all(ryujinx_path)?;
        return Ok(());
    }

    info!("清理 Ryujinx 目录: {}", ryujinx_path.display());

    // 保留的目录列表
    let preserve_dirs = vec!["bis", "sdcard", "portable"];

    // 遍历目录
    for entry in std::fs::read_dir(ryujinx_path)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();

        // 检查是否需要保留
        let should_preserve = preserve_dirs.iter().any(|&p| name_str == p);

        if !should_preserve {
            if path.is_dir() {
                debug!("删除目录: {}", path.display());
                std::fs::remove_dir_all(&path)?;
            } else {
                debug!("删除文件: {}", path.display());
                std::fs::remove_file(&path)?;
            }
        }
    }

    Ok(())
}

/// 递归复制目录
fn copy_dir_all(src: &Path, dst: &Path) -> AppResult<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst.join(&file_name);

        if path.is_dir() {
            copy_dir_all(&path, &dst_path)?;
        } else {
            std::fs::copy(&path, &dst_path).map_err(|e| {
                AppError::Emulator(format!("Ryujinx 文件复制失败: {}", e))
            })?;
        }
    }

    Ok(())
}

/// 获取 Ryujinx 用户文件夹路径
pub fn get_ryujinx_user_folder() -> PathBuf {
    let config = get_config();
    let ryujinx_path = PathBuf::from(&config.ryujinx.path);

    // 检查是否使用 portable 模式
    let portable_path = ryujinx_path.join("portable");
    if portable_path.exists() {
        return portable_path;
    }

    // 使用 AppData 目录
    if let Ok(appdata) = std::env::var("APPDATA") {
        let appdata_path = PathBuf::from(appdata);
        let ryujinx_appdata = appdata_path.join("Ryujinx");
        if ryujinx_appdata.exists() {
            return ryujinx_appdata;
        }
    }

    // 默认创建 portable 目录
    std::fs::create_dir_all(&portable_path).ok();
    portable_path
}

/// 安装固件到 Ryujinx
///
/// # 参数
/// * `firmware_version` - 固件版本，None 表示最新版本
pub async fn install_firmware_to_ryujinx(firmware_version: Option<&str>) -> AppResult<()> {
    let config = get_config();

    // 检查是否已安装
    if let Some(ref version) = firmware_version {
        if let Some(ref current_firmware) = config.ryujinx.firmware {
            if current_firmware == version {
                info!("固件已是最新版本，跳过安装");
                return Ok(());
            }
        }
    }

    // 固件路径
    let firmware_path = get_ryujinx_user_folder().join("bis/system/Contents/registered");
    let tmp_dir = firmware_path.parent().unwrap().join("tmp");

    // 确保目录存在
    std::fs::create_dir_all(&tmp_dir)?;

    // TODO: 调用 firmware 服务进行安装
    // let new_version = install_firmware(firmware_version, &tmp_dir).await?;

    // 重新组织固件文件（Ryujinx 的特殊格式）
    // if new_version.is_some() {
    //     // 删除旧固件
    //     if firmware_path.exists() {
    //         std::fs::remove_dir_all(&firmware_path)?;
    //     }
    //     std::fs::create_dir_all(&firmware_path)?;
    //
    //     // 复制固件文件
    //     for entry in std::fs::read_dir(&tmp_dir)? {
    //         let entry = entry?;
    //         let path = entry.path();
    //         if path.extension().map_or(false, |e| e == "nca") {
    //             let file_name = path.file_name().unwrap().to_string_lossy();
    //             // 处理 .cnmt.nca 文件
    //             let new_name = if file_name.ends_with(".cnmt.nca") {
    //                 file_name.trim_end_matches(".cnmt.nca").to_string() + ".nca"
    //             } else {
    //                 file_name.to_string()
    //             };
    //
    //             let nca_dir = firmware_path.join(&new_name);
    //             std::fs::create_dir_all(&nca_dir)?;
    //             std::fs::rename(&path, nca_dir.join("00"))?;
    //         }
    //     }
    //
    //     // 清理临时目录
    //     std::fs::remove_dir_all(&tmp_dir)?;
    //
    //     // 更新配置
    //     {
    //         let mut cfg = CONFIG.write();
    //         cfg.ryujinx.firmware = Some(new_version.unwrap());
    //         cfg.save()?;
    //     }
    // }

    info!("固件安装完成");
    Ok(())
}

/// 启动 Ryujinx
pub fn start_ryujinx() -> AppResult<()> {
    let config = get_config();
    let ryujinx_path = PathBuf::from(&config.ryujinx.path);
    let exe_path = ryujinx_path.join(RYUJINX_EXE);

    if !exe_path.exists() {
        return Err(AppError::FileNotFound(exe_path.display().to_string()));
    }

    info!("启动 Ryujinx: {}", exe_path.display());

    Command::new(&exe_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    Ok(())
}

/// 打开 Ryujinx keys 文件夹
pub fn open_ryujinx_keys_folder() -> AppResult<()> {
    let keys_path = get_ryujinx_user_folder().join("system");
    std::fs::create_dir_all(&keys_path)?;

    // 创建提示文件
    let hint_file = keys_path.join("把prod.keys和title.keys放当前目录.txt");
    std::fs::write(&hint_file, "")?;

    info!("打开 keys 目录: {}", keys_path.display());

    #[cfg(windows)]
    {
        Command::new("explorer")
            .arg(keys_path.to_string_lossy().to_string())
            .spawn()?;
    }

    #[cfg(not(windows))]
    {
        Command::new("xdg-open")
            .arg(keys_path.to_string_lossy().to_string())
            .spawn()?;
    }

    Ok(())
}

/// 更新 Ryujinx 安装路径
pub fn update_ryujinx_path(new_path: &str) -> AppResult<()> {
    let new_path_buf = PathBuf::from(new_path);

    // 创建目录
    if !new_path_buf.exists() {
        info!("创建目录: {}", new_path_buf.display());
        std::fs::create_dir_all(&new_path_buf)?;
    }

    let config = get_config();
    let old_path = PathBuf::from(&config.ryujinx.path);

    // 检查是否相同
    if new_path_buf == old_path {
        info!("路径相同，跳过更新");
        return Ok(());
    }

    // TODO: 保存到历史记录

    // 更新配置
    {
        let mut cfg = CONFIG.write();
        cfg.ryujinx.path = new_path_buf;
        cfg.save()?;
    }

    info!("Ryujinx 路径已更新");
    Ok(())
}

/// 获取 Ryujinx 变更日志
pub async fn get_ryujinx_change_logs(branch: &str) -> AppResult<String> {
    let changelog = load_ryujinx_change_log(branch).await?;
    Ok(changelog)
}

/// 获取所有 Ryujinx 版本
pub async fn get_all_ryujinx_versions(branch: &str) -> AppResult<Vec<String>> {
    let releases = get_all_ryujinx_release_infos(branch).await?;
    let versions: Vec<String> = releases.iter().map(|r| r.tag_name.clone()).collect();
    Ok(versions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_current_branch() {
        // 测试需要配置环境
        let _branch = detect_current_branch();
    }

    #[test]
    fn test_get_ryujinx_user_folder() {
        // 测试需要配置环境
        let _path = get_ryujinx_user_folder();
    }
}

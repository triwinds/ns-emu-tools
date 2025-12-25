//! 固件管理服务
//!
//! 提供 Nintendo Switch 固件的下载、安装和版本检测功能

use crate::config::CONFIG;
use crate::error::{AppError, AppResult};
use crate::models::{ProgressEvent, ProgressStatus, ProgressStep};
use crate::services::aria2::{get_aria2_manager, Aria2DownloadOptions};
use crate::services::network::request_github_api;
use crate::utils::common::format_size;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// GitHub 固件仓库 API
const GITHUB_FIRMWARE_API: &str = "https://api.github.com/repos/THZoria/NX_Firmware/releases";

/// 固件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareInfo {
    /// 固件名称
    pub name: String,
    /// 版本号
    pub version: String,
    /// 下载 URL
    pub url: String,
    /// 文件名
    pub filename: String,
    /// 文件大小（格式化后的字符串）
    pub size: String,
}

/// 获取固件信息列表
pub async fn get_firmware_infos() -> AppResult<Vec<FirmwareInfo>> {
    get_firmware_infos_from_github().await
}

/// 从 GitHub 获取固件信息
pub async fn get_firmware_infos_from_github() -> AppResult<Vec<FirmwareInfo>> {
    info!("从 GitHub 获取固件信息");
    debug!("请求固件 API: {}", GITHUB_FIRMWARE_API);

    let data = request_github_api(GITHUB_FIRMWARE_API).await?;

    let releases = data
        .as_array()
        .ok_or_else(|| {
            debug!("API 响应格式无效，不是数组类型");
            AppError::InvalidArgument("无效的 API 响应格式".to_string())
        })?;

    debug!("收到 {} 个发布版本", releases.len());

    let mut infos: Vec<FirmwareInfo> = Vec::new();

    for release in releases.iter() {
        let assets = match release["assets"].as_array() {
            Some(a) => a,
            None => continue,
        };

        // 查找 zip 文件
        let target_asset = assets.iter().find(|asset| {
            asset["content_type"]
                .as_str()
                .map(|ct| ct.contains("zip"))
                .unwrap_or(false)
        });

        if let Some(asset) = target_asset {
            let name = release["name"].as_str().unwrap_or_default();
            let version = release["tag_name"].as_str().unwrap_or_default();
            let filename = asset["name"].as_str().unwrap_or_default();
            let size = asset["size"].as_u64().unwrap_or(0);
            let download_url = asset["browser_download_url"].as_str().unwrap_or_default();

            infos.push(FirmwareInfo {
                name: name.to_string(),
                version: version.to_string(),
                url: download_url.to_string(),
                filename: filename.to_string(),
                size: format_size(size),
            });
        }
    }

    info!("获取到 {} 个固件版本", infos.len());
    Ok(infos)
}

/// 安装固件（通用版本，支持安装事件）
pub async fn install_firmware<F>(
    firmware_version: &str,
    target_firmware_path: &Path,
    on_event: F,
) -> AppResult<String>
where
    F: Fn(ProgressEvent) + Send + Sync + 'static + Clone,
{
    info!("开始安装固件版本: {}", firmware_version);

    // 步骤1: 获取固件信息
    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "fetch_firmware_info".to_string(),
            title: "获取固件信息".to_string(),
            status: ProgressStatus::Running,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });

    let firmware_infos = match get_firmware_infos().await {
        Ok(infos) => infos,
        Err(e) => {
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "fetch_firmware_info".to_string(),
                    title: "获取固件信息".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: Some(e.to_string()),
                }
            });
            return Err(e);
        }
    };

    let firmware_map: std::collections::HashMap<_, _> = firmware_infos
        .iter()
        .map(|fi| (fi.version.as_str(), fi))
        .collect();

    let target_info = match firmware_map.get(firmware_version) {
        Some(info) => info,
        None => {
            let err_msg = format!("找不到固件版本: {}", firmware_version);
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "fetch_firmware_info".to_string(),
                    title: "获取固件信息".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: Some(err_msg.clone()),
                }
            });
            return Err(AppError::InvalidArgument(err_msg));
        }
    };

    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "fetch_firmware_info".to_string(),
            title: "获取固件信息".to_string(),
            status: ProgressStatus::Success,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });

    // 步骤2: 下载固件
    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "download_firmware".to_string(),
            title: "下载固件".to_string(),
            status: ProgressStatus::Running,
            step_type: "download".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });

    let url = target_info.url.clone();

    info!("下载固件: {}", url);

    let aria2_manager = get_aria2_manager().await?;
    let options = Aria2DownloadOptions {
        overwrite: false,
        use_github_mirror: url.contains("github.com"),
        ..Default::default()
    };

    let on_event_clone = on_event.clone();
    let result = match aria2_manager.download_and_wait(&url, options, move |progress| {
        on_event_clone(ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "download_firmware".to_string(),
                title: "下载固件".to_string(),
                status: ProgressStatus::Running,
                step_type: "download".to_string(),
                progress: progress.percentage,
                download_speed: progress.speed_string(),
                eta: progress.eta_string(),
                error: None,
            }
        });
    }).await {
        Ok(res) => res,
        Err(e) => {
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "download_firmware".to_string(),
                    title: "下载固件".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "download".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: Some(e.to_string()),
                }
            });
            return Err(e);
        }
    };

    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "download_firmware".to_string(),
            title: "下载固件".to_string(),
            status: ProgressStatus::Success,
            step_type: "download".to_string(),
            progress: 100.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });

    // 步骤3: 解压固件
    let auto_delete = {
        let config = CONFIG.read();
        config.setting.download.auto_delete_after_install
    };

    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "extract_firmware".to_string(),
            title: "解压固件".to_string(),
            status: ProgressStatus::Running,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });

    info!("解压固件到: {}", target_firmware_path.display());

    // 清理目标目录
    if target_firmware_path.exists() {
        if let Err(e) = tokio::fs::remove_dir_all(target_firmware_path).await {
            on_event(ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "extract_firmware".to_string(),
                    title: "解压固件".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: Some(format!("清理旧固件失败: {}", e)),
                }
            });
            return Err(e.into());
        }
    }
    if let Err(e) = tokio::fs::create_dir_all(target_firmware_path).await {
        on_event(ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "extract_firmware".to_string(),
                title: "解压固件".to_string(),
                status: ProgressStatus::Error,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                error: Some(format!("创建固件目录失败: {}", e)),
            }
        });
        return Err(e.into());
    }

    // 解压
    if let Err(e) = crate::utils::archive::extract_zip(&result.path, target_firmware_path) {
        on_event(ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "extract_firmware".to_string(),
                title: "解压固件".to_string(),
                status: ProgressStatus::Error,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                error: Some(e.to_string()),
            }
        });
        return Err(e);
    }

    on_event(ProgressEvent::StepUpdate {
        step: ProgressStep {
            id: "extract_firmware".to_string(),
            title: "解压固件".to_string(),
            status: ProgressStatus::Success,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });

    info!("固件 {} 解压成功", firmware_version);

    // 清理下载文件
    if auto_delete {
        let _ = tokio::fs::remove_file(&result.path).await;
    }

    Ok(firmware_version.to_string())
}

/// Ryujinx 固件文件重组织
///
/// Ryujinx 需要特殊的文件结构：每个 NCA 文件放在单独的目录中，文件名为 "00"
/// 例如: firmware_path/abc123.nca/00
pub async fn reorganize_firmware_for_ryujinx(
    tmp_firmware_path: &Path,
    target_firmware_path: &Path,
) -> AppResult<()> {
    info!("重组织固件文件为 Ryujinx 格式");

    // 清理旧固件
    if target_firmware_path.exists() {
        tokio::fs::remove_dir_all(target_firmware_path).await?;
    }
    tokio::fs::create_dir_all(target_firmware_path).await?;

    // 遍历所有 NCA 文件
    let mut entries = tokio::fs::read_dir(tmp_firmware_path).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "nca" {
                    let file_name = entry.file_name();
                    let name_str = file_name.to_string_lossy();

                    // 处理 .cnmt.nca 文件：移除 .cnmt 后缀
                    let new_name = if name_str.ends_with(".cnmt.nca") {
                        name_str.trim_end_matches(".cnmt.nca").to_string() + ".nca"
                    } else {
                        name_str.to_string()
                    };

                    // 创建目录
                    let nca_dir = target_firmware_path.join(&new_name);
                    tokio::fs::create_dir_all(&nca_dir).await?;

                    // 移动文件到目录中，命名为 "00"
                    let target_file = nca_dir.join("00");
                    tokio::fs::rename(&path, &target_file).await?;
                }
            }
        }
    }

    info!("固件文件重组织完成");
    Ok(())
}

/// 获取可用的固件下载源
pub fn get_available_firmware_sources() -> Vec<(&'static str, &'static str)> {
    vec![
        ("由 github.com/THZoria/NX_Firmware 提供的固件", "github"),
    ]
}

/// 获取 Yuzu 固件路径
pub fn get_yuzu_firmware_path() -> PathBuf {
    // 使用 yuzu.rs 中的函数获取 NAND 路径，这样可以从配置文件读取自定义路径
    let nand_path = crate::services::yuzu::get_yuzu_nand_path();
    nand_path.join("system/Contents/registered")
}

/// 获取 Yuzu keys 文件路径
///
/// 遵循 Python 实现的逻辑：使用 Yuzu 自己的 key 路径
pub fn get_yuzu_keys_path() -> PathBuf {
    let user_path = crate::services::yuzu::get_yuzu_user_path();
    user_path.join("keys/prod.keys")
}

/// 获取 Ryujinx 固件路径
pub fn get_ryujinx_firmware_path() -> PathBuf {
    let ryujinx_path = {
        let config = CONFIG.read();
        config.ryujinx.path.clone()
    };

    // Ryujinx 用户数据路径
    let user_path = if cfg!(windows) {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Ryujinx")
    } else {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config/Ryujinx")
    };

    // portable 模式检测
    let portable_path = ryujinx_path.join("portable");
    if portable_path.exists() {
        portable_path.join("bis/system/Contents/registered")
    } else {
        user_path.join("bis/system/Contents/registered")
    }
}

/// 获取 Ryujinx keys 文件路径
///
/// 遵循 Python 实现的逻辑：使用 Ryujinx 自己的 key 路径
pub fn get_ryujinx_keys_path() -> PathBuf {
    let user_folder = crate::services::ryujinx::get_ryujinx_user_folder();
    user_folder.join("system/prod.keys")
}

/// 检测 Yuzu 固件版本
pub async fn detect_yuzu_firmware_version(window: Option<&tauri::Window>) -> AppResult<String> {
    use tauri::Emitter;

    info!("开始检测 Yuzu 固件版本");

    let firmware_path = get_yuzu_firmware_path();

    if !firmware_path.exists() {
        return Err(AppError::DirectoryNotFound(format!(
            "Yuzu 固件目录不存在: {}",
            firmware_path.display()
        )));
    }

    // Step 1: Load keys
    if let Some(win) = window {
        let _ = win.emit("installation-event", ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "load_keys".to_string(),
                title: "加载密钥文件".to_string(),
                status: ProgressStatus::Running,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: String::new(),
                eta: String::new(),
                error: None,
            },
        });
    }

    // 加载 Yuzu 的 key 文件
    // 遵循 Python 实现：使用各个模拟器自己的 key，而不是自动查找别的模拟器的 key
    let key_path = get_yuzu_keys_path();
    if !key_path.exists() {
        let err_msg = format!("未找到 Yuzu keys 文件: {}", key_path.display());
        if let Some(win) = window {
            let _ = win.emit("installation-event", ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "load_keys".to_string(),
                    title: "加载密钥文件".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: String::new(),
                    eta: String::new(),
                    error: Some(err_msg.clone()),
                },
            });
        }
        return Err(AppError::FileNotFound(err_msg));
    }

    info!("加载 Yuzu keys: {}", key_path.display());
    if let Err(e) = crate::services::keys::load_keys(&key_path) {
        if let Some(win) = window {
            let _ = win.emit("installation-event", ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "load_keys".to_string(),
                    title: "加载密钥文件".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: String::new(),
                    eta: String::new(),
                    error: Some(e.to_string()),
                },
            });
        }
        return Err(e);
    }

    if let Some(win) = window {
        let _ = win.emit("installation-event", ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "load_keys".to_string(),
                title: "加载密钥文件".to_string(),
                status: ProgressStatus::Success,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: String::new(),
                eta: String::new(),
                error: None,
            },
        });
    }

    // 查找系统版本归档 NCA 文件
    let nca_path = crate::services::nca::find_system_version_nca(&firmware_path)?;

    match nca_path {
        Some(path) => {
            info!("找到系统版本归档: {}", path.display());

            // 提取固件版本
            let version = crate::services::nca::extract_firmware_version(&path)?;

            info!("Yuzu 固件版本: {}", version);

            // 更新配置并保存
            {
                let mut config = CONFIG.write();
                config.yuzu.yuzu_firmware = Some(version.clone());
                config.save()?;
            }

            Ok(version)
        }
        None => Err(AppError::FileNotFound(
            "未找到系统版本归档文件".to_string(),
        )),
    }
}

/// 检测 Ryujinx 固件版本
pub async fn detect_ryujinx_firmware_version(window: Option<&tauri::Window>) -> AppResult<String> {
    use tauri::Emitter;

    info!("开始检测 Ryujinx 固件版本");

    let firmware_path = get_ryujinx_firmware_path();

    if !firmware_path.exists() {
        return Err(AppError::DirectoryNotFound(format!(
            "Ryujinx 固件目录不存在: {}",
            firmware_path.display()
        )));
    }

    // Step 1: Load keys
    if let Some(win) = window {
        let _ = win.emit("installation-event", ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "load_keys".to_string(),
                title: "加载密钥文件".to_string(),
                status: ProgressStatus::Running,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: String::new(),
                eta: String::new(),
                error: None,
            },
        });
    }

    // 加载 Ryujinx 的 key 文件
    // 遵循 Python 实现：使用各个模拟器自己的 key，而不是自动查找别的模拟器的 key
    let key_path = get_ryujinx_keys_path();
    if !key_path.exists() {
        let err_msg = format!("未找到 Ryujinx keys 文件: {}", key_path.display());
        if let Some(win) = window {
            let _ = win.emit("installation-event", ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "load_keys".to_string(),
                    title: "加载密钥文件".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: String::new(),
                    eta: String::new(),
                    error: Some(err_msg.clone()),
                },
            });
        }
        return Err(AppError::FileNotFound(err_msg));
    }

    info!("加载 Ryujinx keys: {}", key_path.display());
    if let Err(e) = crate::services::keys::load_keys(&key_path) {
        if let Some(win) = window {
            let _ = win.emit("installation-event", ProgressEvent::StepUpdate {
                step: ProgressStep {
                    id: "load_keys".to_string(),
                    title: "加载密钥文件".to_string(),
                    status: ProgressStatus::Error,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: String::new(),
                    eta: String::new(),
                    error: Some(e.to_string()),
                },
            });
        }
        return Err(e);
    }

    if let Some(win) = window {
        let _ = win.emit("installation-event", ProgressEvent::StepUpdate {
            step: ProgressStep {
                id: "load_keys".to_string(),
                title: "加载密钥文件".to_string(),
                status: ProgressStatus::Success,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: String::new(),
                eta: String::new(),
                error: None,
            },
        });
    }

    // 查找系统版本归档 NCA 文件（Ryujinx 格式）
    let nca_path = crate::services::nca::find_system_version_nca_ryujinx(&firmware_path)?;

    match nca_path {
        Some(path) => {
            info!("找到系统版本归档: {}", path.display());

            // 提取固件版本
            let version = crate::services::nca::extract_firmware_version(&path)?;

            info!("Ryujinx 固件版本: {}", version);

            // 更新配置并保存
            {
                let mut config = CONFIG.write();
                config.ryujinx.firmware = Some(version.clone());
                config.save()?;
            }

            Ok(version)
        }
        None => Err(AppError::FileNotFound(
            "未找到系统版本归档文件".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // 需要网络连接
    async fn test_get_firmware_from_github() {
        let infos = get_firmware_infos_from_github().await.unwrap();
        assert!(!infos.is_empty());
        println!("GitHub 固件版本数: {}", infos.len());
        if let Some(first) = infos.first() {
            println!("最新版本: {} ({})", first.version, first.size);
        }
    }

    #[test]
    fn test_firmware_sources() {
        let sources = get_available_firmware_sources();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].1, "github");
    }

    #[tokio::test]
    #[ignore] // 需要实际的固件文件
    async fn test_detect_ryujinx_firmware() {
        // 初始化日志以便查看详细信息
        let _ = tracing_subscriber::fmt()
            .with_env_filter("debug")
            .try_init();

        println!("\n========== 测试 Ryujinx 固件版本检测 ==========\n");

        // 获取固件路径
        let firmware_path = get_ryujinx_firmware_path();
        println!("固件路径: {}", firmware_path.display());

        // 检查路径是否存在
        if !firmware_path.exists() {
            println!("⚠️  固件目录不存在，跳过测试");
            println!("提示: 请先在 Ryujinx 中安装固件");
            return;
        }

        println!("✓ 固件目录存在\n");

        // 尝试加载密钥文件
        println!("尝试加载密钥文件...");
        let key_paths = get_ryujinx_key_paths();
        let mut keys_loaded = false;
        for key_path in &key_paths {
            println!("  检查: {}", key_path.display());
            if key_path.exists() {
                match crate::services::keys::load_keys(key_path) {
                    Ok(_) => {
                        println!("✓ 成功加载密钥: {}", key_path.display());
                        keys_loaded = true;
                        break;
                    }
                    Err(e) => {
                        println!("  加载失败: {}", e);
                    }
                }
            }
        }
        if !keys_loaded {
            println!("⚠️  未找到有效的密钥文件，将尝试不解密读取");
        }
        println!();

        // 扫描固件文件
        println!("开始扫描固件文件...");
        match crate::services::nca::find_system_version_nca_ryujinx(&firmware_path) {
            Ok(Some(nca_path)) => {
                println!("✓ 找到系统版本归档:");
                println!("  路径: {}", nca_path.display());

                // 尝试提取版本
                println!("\n开始提取固件版本...");
                match crate::services::nca::extract_firmware_version(&nca_path) {
                    Ok(version) => {
                        println!("✓ 成功检测到固件版本: {}", version);
                        assert!(!version.is_empty(), "版本字符串不应为空");
                        assert!(
                            version.contains('.'),
                            "版本应该包含点号 (例如: 15.0.0)"
                        );
                    }
                    Err(e) => {
                        println!("✗ 提取版本失败: {}", e);
                        if !keys_loaded {
                            println!("提示: 可能需要密钥文件来解密 NCA");
                        }
                        panic!("固件版本提取失败");
                    }
                }
            }
            Ok(None) => {
                println!("⚠️  未找到系统版本归档文件 (Title ID: 0100000000000809)");
                println!("提示: 固件可能不完整或格式不正确");
            }
            Err(e) => {
                println!("✗ 扫描失败: {}", e);
                panic!("固件扫描失败");
            }
        }

        // 清理密钥
        crate::services::keys::clear_keys();

        println!("\n========== 测试完成 ==========\n");
    }

    #[tokio::test]
    #[ignore] // 需要实际的固件文件
    async fn test_detect_yuzu_firmware() {
        // 初始化日志以便查看详细信息
        let _ = tracing_subscriber::fmt()
            .with_env_filter("debug")
            .try_init();

        println!("\n========== 测试 Yuzu 固件版本检测 ==========\n");

        // 获取固件路径
        let firmware_path = get_yuzu_firmware_path();
        println!("固件路径: {}", firmware_path.display());

        // 检查路径是否存在
        if !firmware_path.exists() {
            println!("⚠️  固件目录不存在，跳过测试");
            println!("提示: 请先在 Yuzu/Eden/Citron 中安装固件");
            return;
        }

        println!("✓ 固件目录存在\n");

        // 尝试加载密钥文件
        println!("尝试加载密钥文件...");
        let key_paths = get_yuzu_key_paths();
        let mut keys_loaded = false;
        for key_path in &key_paths {
            println!("  检查: {}", key_path.display());
            if key_path.exists() {
                match crate::services::keys::load_keys(key_path) {
                    Ok(_) => {
                        println!("✓ 成功加载密钥: {}", key_path.display());
                        keys_loaded = true;
                        break;
                    }
                    Err(e) => {
                        println!("  加载失败: {}", e);
                    }
                }
            }
        }
        if !keys_loaded {
            println!("⚠️  未找到有效的密钥文件，将尝试不解密读取");
        }
        println!();

        // 扫描固件文件
        println!("开始扫描固件文件...");
        match crate::services::nca::find_system_version_nca(&firmware_path) {
            Ok(Some(nca_path)) => {
                println!("✓ 找到系统版本归档:");
                println!("  路径: {}", nca_path.display());

                // 尝试提取版本
                println!("\n开始提取固件版本...");
                match crate::services::nca::extract_firmware_version(&nca_path) {
                    Ok(version) => {
                        println!("✓ 成功检测到固件版本: {}", version);
                        assert!(!version.is_empty(), "版本字符串不应为空");
                        assert!(
                            version.contains('.'),
                            "版本应该包含点号 (例如: 15.0.0)"
                        );
                    }
                    Err(e) => {
                        println!("✗ 提取版本失败: {}", e);
                        if !keys_loaded {
                            println!("提示: 可能需要密钥文件来解密 NCA");
                        }
                        panic!("固件版本提取失败");
                    }
                }
            }
            Ok(None) => {
                println!("⚠️  未找到系统版本归档文件 (Title ID: 0100000000000809)");
                println!("提示: 固件可能不完整或格式不正确");
            }
            Err(e) => {
                println!("✗ 扫描失败: {}", e);
                panic!("固件扫描失败");
            }
        }

        // 清理密钥
        crate::services::keys::clear_keys();

        println!("\n========== 测试完成 ==========\n");
    }

    /// 获取 Ryujinx 密钥文件可能的路径
    fn get_ryujinx_key_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Windows: %APPDATA%/Ryujinx/system/prod.keys
        if let Some(data_dir) = dirs::data_dir() {
            paths.push(data_dir.join("Ryujinx").join("system").join("prod.keys"));
        }

        // Linux: ~/.config/Ryujinx/system/prod.keys
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".config/Ryujinx/system/prod.keys"));
            // 通用位置
            paths.push(home.join(".switch").join("prod.keys"));
        }

        paths
    }

    /// 获取 Yuzu 密钥文件可能的路径
    fn get_yuzu_key_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Windows: %LOCALAPPDATA%/yuzu/keys/prod.keys
        if let Some(data_local) = dirs::data_local_dir() {
            paths.push(data_local.join("yuzu").join("keys").join("prod.keys"));
        }

        // Linux: ~/.local/share/yuzu/keys/prod.keys
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".local/share/yuzu/keys/prod.keys"));
            // 通用位置
            paths.push(home.join(".switch").join("prod.keys"));
        }

        paths
    }

    #[tokio::test]
    #[ignore] // 需要实际的固件文件
    async fn test_full_ryujinx_firmware_detection() {
        // 初始化日志
        let _ = tracing_subscriber::fmt()
            .with_env_filter("debug")
            .try_init();

        println!("\n========== 完整测试: Ryujinx 固件版本检测流程 ==========\n");

        match detect_ryujinx_firmware_version(None).await {
            Ok(version) => {
                println!("✓ 检测成功!");
                println!("  固件版本: {}", version);

                // 验证配置已更新
                let config = CONFIG.read();
                assert_eq!(
                    config.ryujinx.firmware,
                    Some(version),
                    "配置中的固件版本应该已更新"
                );
                println!("✓ 配置已更新");
            }
            Err(e) => {
                println!("⚠️  检测失败: {}", e);
                println!("这可能是因为:");
                println!("  1. Ryujinx 未安装");
                println!("  2. 固件未安装");
                println!("  3. 固件目录路径不正确");
            }
        }

        println!("\n========== 测试完成 ==========\n");
    }
}

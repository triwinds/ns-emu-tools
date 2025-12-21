//! 固件管理服务
//!
//! 提供 Nintendo Switch 固件的下载、安装和版本检测功能

use crate::config::CONFIG;
use crate::error::{AppError, AppResult};
use crate::models::{InstallationEvent, InstallationStatus, InstallationStep};
use crate::services::aria2::{get_aria2_manager, Aria2DownloadOptions};
use crate::services::network::{create_client, get_final_url, get_github_download_url, request_github_api};
use crate::utils::common::{check_file_md5, format_size};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// NS Archive 固件信息 API
const NSARCHIVE_FIRMWARE_API: &str = "https://nsarchive.e6ex.com/nsf/firmwares.json";

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
    /// MD5 校验值（可选）
    #[serde(default)]
    pub md5: Option<String>,
}

/// 获取固件信息列表
pub async fn get_firmware_infos() -> AppResult<Vec<FirmwareInfo>> {
    let source = {
        let config = CONFIG.read();
        config.setting.network.firmware_download_source.clone()
    };

    match source.as_str() {
        "nsarchive" => get_firmware_infos_from_nsarchive().await,
        _ => get_firmware_infos_from_github().await,
    }
}

/// 从 NS Archive 获取固件信息
pub async fn get_firmware_infos_from_nsarchive() -> AppResult<Vec<FirmwareInfo>> {
    info!("从 NS Archive 获取固件信息");

    let url = get_final_url(NSARCHIVE_FIRMWARE_API);
    let client = create_client()?;
    let resp = client.get(&url).send().await?;

    if !resp.status().is_success() {
        return Err(AppError::Unknown(format!(
            "获取固件信息失败: {}",
            resp.status()
        )));
    }

    let data: Vec<serde_json::Value> = resp.json().await?;

    let infos: Vec<FirmwareInfo> = data
        .iter()
        .filter_map(|item| {
            let name = item["name"].as_str()?;
            let filename = item["filename"].as_str()?;
            let version = if name.len() > 9 {
                &name[9..]
            } else {
                name
            };

            Some(FirmwareInfo {
                name: name.to_string(),
                version: version.to_string(),
                url: format!(
                    "https://nsarchive.e6ex.com/nsf/{}",
                    urlencoding::encode(filename)
                ),
                filename: filename.to_string(),
                size: item["size"]
                    .as_str()
                    .unwrap_or("未知")
                    .to_string(),
                md5: item["md5"].as_str().map(|s| s.to_string()),
            })
        })
        .collect();

    debug!("获取到 {} 个固件版本", infos.len());
    Ok(infos)
}

/// 从 GitHub 获取固件信息
pub async fn get_firmware_infos_from_github() -> AppResult<Vec<FirmwareInfo>> {
    info!("从 GitHub 获取固件信息");

    let data = request_github_api(GITHUB_FIRMWARE_API).await?;

    let releases = data
        .as_array()
        .ok_or_else(|| AppError::InvalidArgument("无效的 API 响应格式".to_string()))?;

    let mut infos: Vec<FirmwareInfo> = Vec::new();

    for release in releases {
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
                md5: None,
            });
        }
    }

    debug!("获取到 {} 个固件版本", infos.len());
    Ok(infos)
}

/// 安装固件（通用版本，支持安装事件）
pub async fn install_firmware<F>(
    firmware_version: &str,
    target_firmware_path: &Path,
    on_event: F,
) -> AppResult<String>
where
    F: Fn(InstallationEvent) + Send + Sync + 'static + Clone,
{
    info!("开始安装固件版本: {}", firmware_version);

    // 步骤1: 获取固件信息
    on_event(InstallationEvent::StepUpdate {
        step: InstallationStep {
            id: "fetch_firmware_info".to_string(),
            title: "获取固件信息".to_string(),
            status: InstallationStatus::Running,
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
            on_event(InstallationEvent::StepUpdate {
                step: InstallationStep {
                    id: "fetch_firmware_info".to_string(),
                    title: "获取固件信息".to_string(),
                    status: InstallationStatus::Error,
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
            on_event(InstallationEvent::StepUpdate {
                step: InstallationStep {
                    id: "fetch_firmware_info".to_string(),
                    title: "获取固件信息".to_string(),
                    status: InstallationStatus::Error,
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

    on_event(InstallationEvent::StepUpdate {
        step: InstallationStep {
            id: "fetch_firmware_info".to_string(),
            title: "获取固件信息".to_string(),
            status: InstallationStatus::Success,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });

    // 步骤2: 下载固件
    on_event(InstallationEvent::StepUpdate {
        step: InstallationStep {
            id: "download_firmware".to_string(),
            title: "下载固件".to_string(),
            status: InstallationStatus::Running,
            step_type: "download".to_string(),
            progress: 0.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });

    let url = if target_info.url.contains("github.com") {
        get_github_download_url(&target_info.url)
    } else {
        target_info.url.clone()
    };

    info!("下载固件: {}", url);

    let aria2_manager = get_aria2_manager().await?;
    let options = Aria2DownloadOptions {
        overwrite: false,
        use_github_mirror: url.contains("github.com"),
        ..Default::default()
    };

    let on_event_clone = on_event.clone();
    let result = match aria2_manager.download_and_wait(&url, options, move |progress| {
        on_event_clone(InstallationEvent::StepUpdate {
            step: InstallationStep {
                id: "download_firmware".to_string(),
                title: "下载固件".to_string(),
                status: InstallationStatus::Running,
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
            on_event(InstallationEvent::StepUpdate {
                step: InstallationStep {
                    id: "download_firmware".to_string(),
                    title: "下载固件".to_string(),
                    status: InstallationStatus::Error,
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

    on_event(InstallationEvent::StepUpdate {
        step: InstallationStep {
            id: "download_firmware".to_string(),
            title: "下载固件".to_string(),
            status: InstallationStatus::Success,
            step_type: "download".to_string(),
            progress: 100.0,
            download_speed: "".to_string(),
            eta: "".to_string(),
            error: None,
        }
    });

    // 步骤3: 验证 MD5（可选）
    let (verify_md5, auto_delete) = {
        let config = CONFIG.read();
        (config.setting.download.verify_firmware_md5,
         config.setting.download.auto_delete_after_install)
    };

    if verify_md5 {
        if let Some(ref expected_md5) = target_info.md5 {
            on_event(InstallationEvent::StepUpdate {
                step: InstallationStep {
                    id: "verify_md5".to_string(),
                    title: "验证固件完整性".to_string(),
                    status: InstallationStatus::Running,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: None,
                }
            });

            info!("验证固件 MD5...");
            if !check_file_md5(&result.path, expected_md5).await? {
                let _ = tokio::fs::remove_file(&result.path).await;
                let err_msg = "固件 MD5 校验失败";
                on_event(InstallationEvent::StepUpdate {
                    step: InstallationStep {
                        id: "verify_md5".to_string(),
                        title: "验证固件完整性".to_string(),
                        status: InstallationStatus::Error,
                        step_type: "normal".to_string(),
                        progress: 0.0,
                        download_speed: "".to_string(),
                        eta: "".to_string(),
                        error: Some(err_msg.to_string()),
                    }
                });
                return Err(AppError::Download(err_msg.to_string()));
            }

            on_event(InstallationEvent::StepUpdate {
                step: InstallationStep {
                    id: "verify_md5".to_string(),
                    title: "验证固件完整性".to_string(),
                    status: InstallationStatus::Success,
                    step_type: "normal".to_string(),
                    progress: 0.0,
                    download_speed: "".to_string(),
                    eta: "".to_string(),
                    error: None,
                }
            });
            info!("MD5 校验通过");
        }
    }

    // 步骤4: 解压固件
    on_event(InstallationEvent::StepUpdate {
        step: InstallationStep {
            id: "extract_firmware".to_string(),
            title: "解压固件".to_string(),
            status: InstallationStatus::Running,
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
            on_event(InstallationEvent::StepUpdate {
                step: InstallationStep {
                    id: "extract_firmware".to_string(),
                    title: "解压固件".to_string(),
                    status: InstallationStatus::Error,
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
        on_event(InstallationEvent::StepUpdate {
            step: InstallationStep {
                id: "extract_firmware".to_string(),
                title: "解压固件".to_string(),
                status: InstallationStatus::Error,
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
        on_event(InstallationEvent::StepUpdate {
            step: InstallationStep {
                id: "extract_firmware".to_string(),
                title: "解压固件".to_string(),
                status: InstallationStatus::Error,
                step_type: "normal".to_string(),
                progress: 0.0,
                download_speed: "".to_string(),
                eta: "".to_string(),
                error: Some(e.to_string()),
            }
        });
        return Err(e);
    }

    on_event(InstallationEvent::StepUpdate {
        step: InstallationStep {
            id: "extract_firmware".to_string(),
            title: "解压固件".to_string(),
            status: InstallationStatus::Success,
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

                    debug!("重组织文件: {} -> {}", path.display(), target_file.display());
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
        ("由 darthsternie.net 提供的固件", "nsarchive"),
    ]
}

/// 获取 Yuzu 固件路径
pub fn get_yuzu_firmware_path() -> PathBuf {
    let yuzu_path = {
        let config = CONFIG.read();
        config.yuzu.yuzu_path.clone()
    };

    // Yuzu 用户数据路径
    let user_path = if cfg!(windows) {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("yuzu")
    } else {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".local/share/yuzu")
    };

    // portable 模式检测
    let portable_path = yuzu_path.join("user");
    if portable_path.exists() {
        portable_path.join("nand/system/Contents/registered")
    } else {
        user_path.join("nand/system/Contents/registered")
    }
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

    #[tokio::test]
    #[ignore] // 需要网络连接
    async fn test_get_firmware_from_nsarchive() {
        let infos = get_firmware_infos_from_nsarchive().await.unwrap();
        assert!(!infos.is_empty());
        println!("NS Archive 固件版本数: {}", infos.len());
        if let Some(first) = infos.first() {
            println!("最新版本: {} ({})", first.version, first.size);
        }
    }

    #[test]
    fn test_firmware_sources() {
        let sources = get_available_firmware_sources();
        assert_eq!(sources.len(), 2);
    }
}

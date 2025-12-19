//! Yuzu 系列模拟器（Eden/Citron）数据访问层
//!
//! 提供 Eden 和 Citron 模拟器的版本信息获取功能

use crate::error::{AppError, AppResult};
use crate::models::release::ReleaseInfo;
use crate::services::network::{create_client, request_github_api};
use tracing::{debug, info};

/// Eden 仓库 API 地址
const EDEN_RELEASES_API: &str = "https://api.github.com/repos/eden-emulator/Releases/releases";

/// Citron 仓库 API 地址
const CITRON_RELEASES_API: &str = "https://git.citron-emu.org/api/v1/repos/Citron/Emulator/releases";

/// 获取所有 Yuzu 系列版本（根据分支）
pub async fn get_all_yuzu_release_versions(branch: &str) -> AppResult<Vec<String>> {
    match branch {
        "eden" => get_eden_all_release_versions().await,
        "citron" => get_citron_all_release_versions().await,
        _ => Ok(vec![]),
    }
}

/// 获取所有 Yuzu 系列 Release 信息（根据分支）
pub async fn get_yuzu_all_release_info(branch: &str) -> AppResult<Vec<ReleaseInfo>> {
    match branch {
        "eden" => get_eden_all_release_info().await,
        "citron" => get_citron_all_release_info().await,
        _ => Err(AppError::InvalidArgument(format!(
            "不支持的分支: {}，仅支持 eden/citron",
            branch
        ))),
    }
}

/// 获取指定版本的 Yuzu Release 信息（根据分支）
pub async fn get_yuzu_release_info_by_version(
    version: &str,
    branch: &str,
) -> AppResult<ReleaseInfo> {
    match branch {
        "eden" => get_eden_release_info_by_version(version).await,
        "citron" => get_citron_release_info_by_version(version).await,
        _ => Err(AppError::InvalidArgument(format!(
            "不支持的分支: {}，仅支持 eden/citron",
            branch
        ))),
    }
}

// ============ Eden 相关函数 ============

/// 获取所有 Eden Release 信息
pub async fn get_eden_all_release_info() -> AppResult<Vec<ReleaseInfo>> {
    info!("获取 Eden 所有 Release 信息");

    let data = request_github_api(EDEN_RELEASES_API).await?;

    let releases: Vec<ReleaseInfo> = data
        .as_array()
        .ok_or_else(|| AppError::InvalidArgument("无效的 API 响应格式".to_string()))?
        .iter()
        .filter_map(|item| ReleaseInfo::from_github_api(item))
        .collect();

    debug!("获取到 {} 个 Eden Release", releases.len());
    Ok(releases)
}

/// 获取所有 Eden 版本号
pub async fn get_eden_all_release_versions() -> AppResult<Vec<String>> {
    let releases = get_eden_all_release_info().await?;
    let versions: Vec<String> = releases.iter().map(|r| r.tag_name.clone()).collect();
    Ok(versions)
}

/// 获取指定版本的 Eden Release 信息
pub async fn get_eden_release_info_by_version(version: &str) -> AppResult<ReleaseInfo> {
    info!("获取 Eden 版本 {} 的 Release 信息", version);

    let url = format!("{}/tags/{}", EDEN_RELEASES_API.replace("/releases", ""), version);
    let data = request_github_api(&url).await?;

    ReleaseInfo::from_github_api(&data).ok_or_else(|| {
        AppError::InvalidArgument(format!("无法解析 Eden 版本 {} 的信息", version))
    })
}

// ============ Citron 相关函数 ============

/// 获取所有 Citron Release 信息
pub async fn get_citron_all_release_info() -> AppResult<Vec<ReleaseInfo>> {
    info!("获取 Citron 所有 Release 信息");

    let client = create_client()?;
    let resp = client.get(CITRON_RELEASES_API).send().await?;

    if !resp.status().is_success() {
        return Err(AppError::Unknown(format!(
            "Citron API 请求失败: {}",
            resp.status()
        )));
    }

    let data: serde_json::Value = resp.json().await?;

    let releases: Vec<ReleaseInfo> = data
        .as_array()
        .ok_or_else(|| AppError::InvalidArgument("无效的 API 响应格式".to_string()))?
        .iter()
        .filter_map(|item| ReleaseInfo::from_forgejo_api(item))
        .collect();

    debug!("获取到 {} 个 Citron Release", releases.len());
    Ok(releases)
}

/// 获取所有 Citron 版本号
pub async fn get_citron_all_release_versions() -> AppResult<Vec<String>> {
    let releases = get_citron_all_release_info().await?;
    let versions: Vec<String> = releases.iter().map(|r| r.tag_name.clone()).collect();
    Ok(versions)
}

/// 获取指定版本的 Citron Release 信息
pub async fn get_citron_release_info_by_version(version: &str) -> AppResult<ReleaseInfo> {
    info!("获取 Citron 版本 {} 的 Release 信息", version);

    let url = format!("{}/tags/{}", CITRON_RELEASES_API, version);
    let client = create_client()?;
    let resp = client.get(&url).send().await?;

    if !resp.status().is_success() {
        return Err(AppError::Emulator(format!(
            "找不到 Citron 版本: {}",
            version
        )));
    }

    let data: serde_json::Value = resp.json().await?;

    // 检查是否返回 404 消息
    if let Some(message) = data.get("message").and_then(|m| m.as_str()) {
        if message.contains("404") {
            return Err(AppError::Emulator(format!(
                "找不到 Citron 版本: {}",
                version
            )));
        }
    }

    ReleaseInfo::from_forgejo_api(&data).ok_or_else(|| {
        AppError::InvalidArgument(format!("无法解析 Citron 版本 {} 的信息", version))
    })
}

/// 获取最新变更日志
pub async fn get_latest_change_log(branch: &str) -> AppResult<String> {
    let releases = get_yuzu_all_release_info(branch).await?;

    if releases.is_empty() {
        return Ok(format!("无法获取 {} 最新版本变更信息", branch));
    }

    Ok(releases[0].description.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // 需要网络连接
    async fn test_get_eden_releases() {
        let versions = get_eden_all_release_versions().await.unwrap();
        assert!(!versions.is_empty());
        println!("Eden versions: {:?}", &versions[..5.min(versions.len())]);
    }

    #[tokio::test]
    #[ignore] // 需要网络连接
    async fn test_get_citron_releases() {
        let versions = get_citron_all_release_versions().await.unwrap();
        assert!(!versions.is_empty());
        println!("Citron versions: {:?}", &versions[..5.min(versions.len())]);
    }
}

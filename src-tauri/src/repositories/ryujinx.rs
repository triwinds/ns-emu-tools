//! Ryujinx 模拟器数据访问层
//!
//! 提供 Ryujinx 模拟器（mainline/canary 分支）的版本信息获取功能

use crate::error::{AppError, AppResult};
use crate::models::release::ReleaseInfo;
use crate::services::network::create_client;
use tracing::{debug, info};

/// Ryujinx GitLab Mainline Releases API
const RYUJINX_MAINLINE_API: &str = "https://git.ryujinx.app/api/v4/projects/1/releases";

/// Ryujinx GitLab Canary Releases API
const RYUJINX_CANARY_API: &str = "https://git.ryujinx.app/api/v4/projects/68/releases";

/// 获取所有 Ryujinx Release 信息（根据分支）
pub async fn get_all_ryujinx_release_infos(branch: &str) -> AppResult<Vec<ReleaseInfo>> {
    match branch {
        "canary" => get_all_canary_ryujinx_release_infos().await,
        _ => get_all_mainline_ryujinx_release_infos().await,
    }
}

/// 获取所有 Mainline Ryujinx Release 信息
pub async fn get_all_mainline_ryujinx_release_infos() -> AppResult<Vec<ReleaseInfo>> {
    info!("获取 Ryujinx Mainline 所有 Release 信息");

    let client = create_client()?;
    let resp = client.get(RYUJINX_MAINLINE_API).send().await?;

    if !resp.status().is_success() {
        return Err(AppError::Unknown(format!(
            "Ryujinx API 请求失败: {}",
            resp.status()
        )));
    }

    let data: serde_json::Value = resp.json().await?;

    let releases: Vec<ReleaseInfo> = data
        .as_array()
        .ok_or_else(|| AppError::InvalidArgument("无效的 API 响应格式".to_string()))?
        .iter()
        .filter_map(|item| ReleaseInfo::from_gitlab_api(item))
        .collect();

    debug!("获取到 {} 个 Ryujinx Mainline Release", releases.len());
    Ok(releases)
}

/// 获取所有 Canary Ryujinx Release 信息
pub async fn get_all_canary_ryujinx_release_infos() -> AppResult<Vec<ReleaseInfo>> {
    info!("获取 Ryujinx Canary 所有 Release 信息");

    let client = create_client()?;
    let resp = client.get(RYUJINX_CANARY_API).send().await?;

    if !resp.status().is_success() {
        return Err(AppError::Unknown(format!(
            "Ryujinx Canary API 请求失败: {}",
            resp.status()
        )));
    }

    let data: serde_json::Value = resp.json().await?;

    let releases: Vec<ReleaseInfo> = data
        .as_array()
        .ok_or_else(|| AppError::InvalidArgument("无效的 API 响应格式".to_string()))?
        .iter()
        .filter_map(|item| ReleaseInfo::from_gitlab_api(item))
        .collect();

    debug!("获取到 {} 个 Ryujinx Canary Release", releases.len());
    Ok(releases)
}

/// 获取最新 Ryujinx Release 信息
pub async fn get_latest_ryujinx_release_info(branch: &str) -> AppResult<ReleaseInfo> {
    let releases = get_all_ryujinx_release_infos(branch).await?;

    releases
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Emulator("没有找到任何 Ryujinx Release".to_string()))
}

/// 获取指定版本的 Ryujinx Release 信息
pub async fn get_ryujinx_release_info_by_version(
    version: &str,
    branch: &str,
) -> AppResult<ReleaseInfo> {
    match branch {
        "canary" => get_canary_ryujinx_release_info_by_version(version).await,
        _ => get_mainline_ryujinx_release_info_by_version(version).await,
    }
}

/// 获取指定版本的 Mainline Ryujinx Release 信息
pub async fn get_mainline_ryujinx_release_info_by_version(version: &str) -> AppResult<ReleaseInfo> {
    info!("获取 Ryujinx Mainline 版本 {} 的 Release 信息", version);

    let url = format!("{}/{}", RYUJINX_MAINLINE_API, version);
    let client = create_client()?;
    let resp = client.get(&url).send().await?;

    if !resp.status().is_success() {
        return Err(AppError::Emulator(format!(
            "找不到 Ryujinx Mainline 版本: {}",
            version
        )));
    }

    let data: serde_json::Value = resp.json().await?;

    ReleaseInfo::from_gitlab_api(&data).ok_or_else(|| {
        AppError::InvalidArgument(format!("无法解析 Ryujinx 版本 {} 的信息", version))
    })
}

/// 获取指定版本的 Canary Ryujinx Release 信息
pub async fn get_canary_ryujinx_release_info_by_version(version: &str) -> AppResult<ReleaseInfo> {
    info!("获取 Ryujinx Canary 版本 {} 的 Release 信息", version);

    let url = format!("{}/{}", RYUJINX_CANARY_API, version);
    let client = create_client()?;
    let resp = client.get(&url).send().await?;

    if !resp.status().is_success() {
        return Err(AppError::Emulator(format!(
            "找不到 Ryujinx Canary 版本: {}",
            version
        )));
    }

    let data: serde_json::Value = resp.json().await?;

    ReleaseInfo::from_gitlab_api(&data).ok_or_else(|| {
        AppError::InvalidArgument(format!("无法解析 Ryujinx Canary 版本 {} 的信息", version))
    })
}

/// 获取所有 Ryujinx 版本号
pub async fn get_all_ryujinx_release_versions(branch: &str) -> AppResult<Vec<String>> {
    let releases = get_all_ryujinx_release_infos(branch).await?;
    let versions: Vec<String> = releases.iter().map(|r| r.tag_name.clone()).collect();
    Ok(versions)
}

/// 加载 Ryujinx 变更日志
pub async fn load_ryujinx_change_log(branch: &str) -> AppResult<String> {
    let releases = get_all_ryujinx_release_infos(branch).await?;

    if releases.is_empty() {
        return Ok(format!("无法获取 Ryujinx {} 最新版本变更信息", branch));
    }

    Ok(releases[0].description.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // 需要网络连接
    async fn test_get_mainline_releases() {
        let releases = get_all_mainline_ryujinx_release_infos().await.unwrap();
        assert!(!releases.is_empty());
        println!(
            "Ryujinx Mainline releases: {:?}",
            releases
                .iter()
                .take(5)
                .map(|r| &r.tag_name)
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    #[ignore] // 需要网络连接
    async fn test_get_canary_releases() {
        let releases = get_all_canary_ryujinx_release_infos().await.unwrap();
        assert!(!releases.is_empty());
        println!(
            "Ryujinx Canary releases: {:?}",
            releases
                .iter()
                .take(5)
                .map(|r| &r.tag_name)
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    #[ignore] // 需要网络连接
    async fn test_get_latest_release() {
        let release = get_latest_ryujinx_release_info("mainline").await.unwrap();
        println!("Latest Ryujinx Mainline: {}", release.tag_name);
    }
}

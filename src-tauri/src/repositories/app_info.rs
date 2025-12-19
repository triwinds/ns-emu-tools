//! 应用信息数据访问层
//!
//! 提供 NS Emu Tools 自身的版本信息、更新检查等功能

use crate::config::CURRENT_VERSION;
use crate::error::{AppError, AppResult};
use crate::models::release::ReleaseInfo;
use crate::services::network::{create_client, get_final_url, request_github_api};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// NS Emu Tools GitHub 仓库 API
const APP_RELEASES_API: &str = "https://api.github.com/repos/triwinds/ns-emu-tools/releases";

/// 变更日志 URL
const CHANGELOG_URL: &str =
    "https://raw.githubusercontent.com/triwinds/ns-emu-tools/main/changelog.md";

/// 获取所有应用版本
pub async fn get_all_release() -> AppResult<Vec<serde_json::Value>> {
    info!("获取所有应用版本信息");

    let data = request_github_api(APP_RELEASES_API).await?;

    let releases = data
        .as_array()
        .ok_or_else(|| AppError::InvalidArgument("无效的 API 响应格式".to_string()))?
        .clone();

    debug!("获取到 {} 个应用版本", releases.len());
    Ok(releases)
}

/// 获取最新版本
pub async fn get_latest_release(prerelease: bool) -> AppResult<ReleaseInfo> {
    info!("获取最新应用版本 (prerelease: {})", prerelease);

    let releases = get_all_release().await?;

    let release_list: Vec<&serde_json::Value> = if prerelease {
        releases.iter().collect()
    } else {
        releases
            .iter()
            .filter(|r| !r["prerelease"].as_bool().unwrap_or(true))
            .collect()
    };

    if release_list.is_empty() {
        return Err(AppError::Emulator("没有找到任何版本".to_string()));
    }

    ReleaseInfo::from_github_api(release_list[0])
        .ok_or_else(|| AppError::InvalidArgument("无法解析版本信息".to_string()))
}

/// 获取指定标签的版本信息
pub async fn get_release_info_by_tag(tag: &str) -> AppResult<ReleaseInfo> {
    info!("获取版本 {} 的信息", tag);

    let url = format!("{}/tags/{}", APP_RELEASES_API, tag);
    let data = request_github_api(&url).await?;

    ReleaseInfo::from_github_api(&data)
        .ok_or_else(|| AppError::InvalidArgument(format!("无法解析版本 {} 的信息", tag)))
}

/// 加载变更日志
pub async fn load_change_log() -> AppResult<String> {
    info!("加载变更日志");

    let url = get_final_url(CHANGELOG_URL);
    let client = create_client()?;
    let resp = client.get(&url).send().await?;

    if !resp.status().is_success() {
        return Err(AppError::Unknown(format!(
            "获取变更日志失败: {}",
            resp.status()
        )));
    }

    let text = resp.text().await?;
    Ok(text)
}

/// 更新检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    /// 是否有更新
    pub has_update: bool,
    /// 当前版本
    pub current_version: String,
    /// 最新版本
    pub latest_version: String,
    /// 更新描述
    pub description: String,
    /// 下载 URL
    pub download_url: Option<String>,
    /// 发布页面 URL
    pub html_url: Option<String>,
}

/// 检查更新
pub async fn check_update(include_prerelease: bool) -> AppResult<UpdateCheckResult> {
    info!(
        "检查更新 (当前版本: {}, 包含预发布: {})",
        CURRENT_VERSION, include_prerelease
    );

    let latest = get_latest_release(include_prerelease).await?;

    let has_update = is_newer_version(&latest.tag_name, CURRENT_VERSION);

    let download_url = latest.find_windows_asset().map(|a| a.download_url.clone());

    Ok(UpdateCheckResult {
        has_update,
        current_version: CURRENT_VERSION.to_string(),
        latest_version: latest.tag_name.clone(),
        description: latest.description.clone(),
        download_url,
        html_url: latest.html_url.clone(),
    })
}

/// 比较版本号，判断 new_version 是否比 current_version 新
fn is_newer_version(new_version: &str, current_version: &str) -> bool {
    let parse_version = |v: &str| -> Vec<u32> {
        v.trim_start_matches('v')
            .split('.')
            .filter_map(|s| {
                // 处理类似 "1.0.0-beta" 的情况
                s.split('-').next().and_then(|n| n.parse().ok())
            })
            .collect()
    };

    let new_parts = parse_version(new_version);
    let current_parts = parse_version(current_version);

    for i in 0..new_parts.len().max(current_parts.len()) {
        let new_part = new_parts.get(i).copied().unwrap_or(0);
        let current_part = current_parts.get(i).copied().unwrap_or(0);

        if new_part > current_part {
            return true;
        } else if new_part < current_part {
            return false;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_comparison() {
        assert!(is_newer_version("1.0.1", "1.0.0"));
        assert!(is_newer_version("1.1.0", "1.0.9"));
        assert!(is_newer_version("2.0.0", "1.9.9"));
        assert!(is_newer_version("v1.0.1", "v1.0.0"));
        assert!(!is_newer_version("1.0.0", "1.0.0"));
        assert!(!is_newer_version("1.0.0", "1.0.1"));
        assert!(!is_newer_version("0.9.0", "1.0.0"));
    }

    #[tokio::test]
    #[ignore] // 需要网络连接
    async fn test_get_latest_release() {
        let release = get_latest_release(false).await.unwrap();
        println!("Latest release: {}", release.tag_name);
    }

    #[tokio::test]
    #[ignore] // 需要网络连接
    async fn test_load_changelog() {
        let changelog = load_change_log().await.unwrap();
        assert!(!changelog.is_empty());
        println!("Changelog length: {} chars", changelog.len());
    }

    #[tokio::test]
    #[ignore] // 需要网络连接
    async fn test_check_update() {
        let result = check_update(false).await.unwrap();
        println!("Current: {}, Latest: {}, Has update: {}",
            result.current_version,
            result.latest_version,
            result.has_update
        );
    }
}

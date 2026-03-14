//! Yuzu 系列模拟器数据访问层
//!
//! 提供 Eden 模拟器的在线版本信息获取功能

use crate::error::{AppError, AppResult};
use crate::models::release::ReleaseInfo;
use crate::services::network::request_git_api;
use tracing::{debug, info};

/// Eden 仓库 API 地址（Forgejo/Gitea）
const EDEN_RELEASES_API: &str = "https://git.eden-emu.dev/api/v1/repos/eden-emu/eden/releases";

fn unsupported_yuzu_branch_error(branch: &str) -> AppError {
    if branch == "citron" {
        AppError::Unsupported("Citron 分支已不再支持在线版本、下载和安装".to_string())
    } else {
        AppError::InvalidArgument(format!("不支持的分支: {}，当前仅支持 eden", branch))
    }
}

/// 获取所有 Yuzu 系列版本（根据分支）
pub async fn get_all_yuzu_release_versions(branch: &str) -> AppResult<Vec<String>> {
    match branch {
        "eden" => get_eden_all_release_versions().await,
        _ => Err(unsupported_yuzu_branch_error(branch)),
    }
}

/// 获取所有 Yuzu 系列 Release 信息（根据分支）
pub async fn get_yuzu_all_release_info(branch: &str) -> AppResult<Vec<ReleaseInfo>> {
    match branch {
        "eden" => get_eden_all_release_info().await,
        _ => Err(unsupported_yuzu_branch_error(branch)),
    }
}

/// 获取指定版本的 Yuzu Release 信息（根据分支）
pub async fn get_yuzu_release_info_by_version(
    version: &str,
    branch: &str,
) -> AppResult<ReleaseInfo> {
    match branch {
        "eden" => get_eden_release_info_by_version(version).await,
        _ => Err(unsupported_yuzu_branch_error(branch)),
    }
}

// ============ Eden 相关函数 ============

/// 获取所有 Eden Release 信息
pub async fn get_eden_all_release_info() -> AppResult<Vec<ReleaseInfo>> {
    info!("正在获取 Eden 的所有发布信息");

    let data = request_git_api(EDEN_RELEASES_API).await?;

    let releases: Vec<ReleaseInfo> = data
        .as_array()
        .ok_or_else(|| AppError::InvalidArgument("无效的 API 响应格式".to_string()))?
        .iter()
        .filter_map(|item| ReleaseInfo::from_forgejo_api(item))
        .collect();

    debug!("共获取到 {} 个 Eden 发布", releases.len());
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
    info!("正在获取 Eden 版本 {} 的发布信息", version);

    let url = format!("{}/tags/{}", EDEN_RELEASES_API, version);
    let data = request_git_api(&url).await?;

    ReleaseInfo::from_forgejo_api(&data)
        .ok_or_else(|| AppError::InvalidArgument(format!("无法解析 Eden 版本 {} 的信息", version)))
}

/// 获取最新变更日志
pub async fn get_latest_change_log(branch: &str) -> AppResult<String> {
    let releases = get_yuzu_all_release_info(branch).await?;

    if releases.is_empty() {
        return Ok(format!("无法获取 {} 的最新版本变更信息", branch));
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
    async fn test_citron_releases_are_unsupported() {
        let error = get_all_yuzu_release_versions("citron").await.unwrap_err();
        assert!(matches!(error, AppError::Unsupported(_)));
        assert!(error.to_string().contains("Citron"));
    }
}

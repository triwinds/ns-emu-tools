//! Ryujinx 模拟器数据访问层
//!
//! 提供 Ryujinx 模拟器（mainline/canary 分支）的版本信息获取功能

use crate::error::{AppError, AppResult};
use crate::models::release::ReleaseInfo;
use crate::services::network::request_git_api;
use tracing::{debug, info, warn};

/// Ryujinx Forgejo Mainline Releases API
const RYUJINX_MAINLINE_API: &str = "https://git.ryujinx.app/api/v1/repos/ryubing/ryujinx/releases";

/// Ryujinx Forgejo Canary Releases API
const RYUJINX_CANARY_API: &str = "https://git.ryujinx.app/api/v1/repos/ryubing/canary/releases";

/// 获取所有 Ryujinx Release 信息（根据分支）
pub async fn get_all_ryujinx_release_infos(branch: &str) -> AppResult<Vec<ReleaseInfo>> {
    match branch {
        "canary" => get_all_canary_ryujinx_release_infos().await,
        _ => get_all_mainline_ryujinx_release_infos().await,
    }
}

/// 获取所有 Mainline Ryujinx Release 信息
pub async fn get_all_mainline_ryujinx_release_infos() -> AppResult<Vec<ReleaseInfo>> {
    info!("正在获取 Ryujinx Mainline 的所有发布信息");

    let data = request_git_api(RYUJINX_MAINLINE_API).await?;

    let releases: Vec<ReleaseInfo> = data
        .as_array()
        .ok_or_else(|| AppError::InvalidArgument("无效的 API 响应格式".to_string()))?
        .iter()
        .filter_map(|item| ReleaseInfo::from_forgejo_api(item))
        .collect();

    debug!("共获取到 {} 个 Ryujinx Mainline 发布", releases.len());
    Ok(releases)
}

/// 获取所有 Canary Ryujinx Release 信息
pub async fn get_all_canary_ryujinx_release_infos() -> AppResult<Vec<ReleaseInfo>> {
    info!("正在获取 Ryujinx Canary 的所有发布信息");

    let data = request_git_api(RYUJINX_CANARY_API).await?;

    let releases: Vec<ReleaseInfo> = data
        .as_array()
        .ok_or_else(|| AppError::InvalidArgument("无效的 API 响应格式".to_string()))?
        .iter()
        .filter_map(|item| ReleaseInfo::from_forgejo_api(item))
        .collect();

    debug!("共获取到 {} 个 Ryujinx Canary 发布", releases.len());
    Ok(releases)
}

/// 获取最新 Ryujinx Release 信息
pub async fn get_latest_ryujinx_release_info(branch: &str) -> AppResult<ReleaseInfo> {
    let releases = get_all_ryujinx_release_infos(branch).await?;

    releases
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Emulator("未找到任何 Ryujinx 发布版本".to_string()))
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
    info!("正在获取 Ryujinx Mainline 版本 {} 的发布信息", version);

    let url = format!("{}/tags/{}", RYUJINX_MAINLINE_API, version);
    let data = request_git_api(&url).await?;

    ReleaseInfo::from_forgejo_api(&data).ok_or_else(|| {
        AppError::InvalidArgument(format!("无法解析 Ryujinx 版本 {} 的信息", version))
    })
}

/// 获取指定版本的 Canary Ryujinx Release 信息
pub async fn get_canary_ryujinx_release_info_by_version(version: &str) -> AppResult<ReleaseInfo> {
    info!("正在获取 Ryujinx Canary 版本 {} 的发布信息", version);

    let url = format!("{}/tags/{}", RYUJINX_CANARY_API, version);
    let data = request_git_api(&url).await?;

    ReleaseInfo::from_forgejo_api(&data).ok_or_else(|| {
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
    let releases = match get_all_ryujinx_release_infos(branch).await {
        Ok(releases) => releases,
        Err(error) => {
            warn!(
                "获取 Ryujinx {} 变更日志失败，返回兜底说明: {}",
                branch, error
            );
            return Ok(ryujinx_changelog_error_message(branch, &error.to_string()));
        }
    };

    if releases.is_empty() {
        return Ok(ryujinx_empty_changelog_message(branch, None));
    }

    let latest_release = &releases[0];
    if latest_release.description.trim().is_empty() {
        return Ok(ryujinx_empty_changelog_message(
            branch,
            Some(latest_release),
        ));
    }

    Ok(latest_release.description.clone())
}

fn ryujinx_branch_label(branch: &str) -> &str {
    match branch {
        "canary" => "Canary",
        _ => "Mainline",
    }
}

fn ryujinx_empty_changelog_message(branch: &str, release: Option<&ReleaseInfo>) -> String {
    let branch_label = ryujinx_branch_label(branch);
    let version_line = release
        .map(|item| format!("- 最新版本: `{}`\n", item.tag_name))
        .unwrap_or_default();
    let release_link_line = release
        .and_then(|item| item.html_url.as_ref())
        .map(|url| format!("- 发布页面: {}\n", url))
        .unwrap_or_default();

    format!(
        concat!(
            "# Ryujinx {branch_label}\n\n",
            "当前官方发布没有提供可显示的变更日志内容。\n\n",
            "- 分支: `{branch}`\n",
            "{version_line}{release_link_line}\n",
            "如果后续官方补充了 release notes，这里会自动显示。"
        ),
        branch_label = branch_label,
        branch = branch,
        version_line = version_line,
        release_link_line = release_link_line,
    )
}

fn ryujinx_changelog_error_message(branch: &str, error: &str) -> String {
    let branch_label = ryujinx_branch_label(branch);

    format!(
        concat!(
            "# Ryujinx {branch_label}\n\n",
            "当前无法从官方源获取变更日志。\n\n",
            "可能原因:\n",
            "- 官方 API 启用了网页验证\n",
            "- CDN 尚未同步对应的 release 接口\n",
            "- 当前发布未提供 release notes\n\n",
            "错误信息: `{error}`"
        ),
        branch_label = branch_label,
        error = error,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_ryujinx_change_log_fallback_for_empty_description() {
        let release = ReleaseInfo {
            name: "Canary 1.3.268".to_string(),
            tag_name: "1.3.268".to_string(),
            description: String::new(),
            assets: vec![],
            published_at: None,
            prerelease: false,
            html_url: Some(
                "https://git.ryujinx.app/ryubing/canary/releases/tag/1.3.268".to_string(),
            ),
        };

        let message = ryujinx_empty_changelog_message("canary", Some(&release));

        assert!(message.contains("当前官方发布没有提供可显示的变更日志内容"));
        assert!(message.contains("1.3.268"));
        assert!(message.contains("https://git.ryujinx.app/ryubing/canary/releases/tag/1.3.268"));
    }

    #[test]
    fn test_load_ryujinx_change_log_fallback_for_request_error() {
        let message =
            ryujinx_changelog_error_message("mainline", "Git API 返回了网页验证而非 JSON");

        assert!(message.contains("当前无法从官方源获取变更日志"));
        assert!(message.contains("网页验证"));
        assert!(message.contains("Mainline"));
    }

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

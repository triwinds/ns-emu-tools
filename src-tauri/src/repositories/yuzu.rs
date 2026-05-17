//! Release metadata access for Eden/Citron emulator branches.

use crate::error::{AppError, AppResult};
use crate::models::release::ReleaseInfo;
use crate::services::network::{request_git_api, request_github_api};
use tracing::{debug, info};

const EDEN_RELEASES_API: &str = "https://git.eden-emu.dev/api/v1/repos/eden-emu/eden/releases";
const CITRON_RELEASES_API: &str = "https://api.github.com/repos/citron-neo/emulator/releases";

fn unsupported_yuzu_branch_error(branch: &str) -> AppError {
    AppError::InvalidArgument(format!(
        "Unsupported Yuzu branch: {}; supported branches: eden, citron",
        branch
    ))
}

pub async fn get_all_yuzu_release_versions(branch: &str) -> AppResult<Vec<String>> {
    match branch {
        "eden" => get_eden_all_release_versions().await,
        "citron" => get_citron_all_release_versions().await,
        _ => Err(unsupported_yuzu_branch_error(branch)),
    }
}

pub async fn get_yuzu_all_release_info(branch: &str) -> AppResult<Vec<ReleaseInfo>> {
    match branch {
        "eden" => get_eden_all_release_info().await,
        "citron" => get_citron_all_release_info().await,
        _ => Err(unsupported_yuzu_branch_error(branch)),
    }
}

pub async fn get_yuzu_release_info_by_version(
    version: &str,
    branch: &str,
) -> AppResult<ReleaseInfo> {
    match branch {
        "eden" => get_eden_release_info_by_version(version).await,
        "citron" => get_citron_release_info_by_version(version).await,
        _ => Err(unsupported_yuzu_branch_error(branch)),
    }
}

pub async fn get_eden_all_release_info() -> AppResult<Vec<ReleaseInfo>> {
    info!("Fetching Eden release information");

    let data = request_git_api(EDEN_RELEASES_API).await?;

    let releases: Vec<ReleaseInfo> = data
        .as_array()
        .ok_or_else(|| AppError::InvalidArgument("Invalid Forgejo API response".to_string()))?
        .iter()
        .filter_map(ReleaseInfo::from_forgejo_api)
        .collect();

    debug!("Fetched {} Eden releases", releases.len());
    Ok(releases)
}

pub async fn get_eden_all_release_versions() -> AppResult<Vec<String>> {
    let releases = get_eden_all_release_info().await?;
    let versions: Vec<String> = releases.iter().map(|r| r.tag_name.clone()).collect();
    Ok(versions)
}

pub async fn get_eden_release_info_by_version(version: &str) -> AppResult<ReleaseInfo> {
    info!("Fetching Eden release information for {}", version);

    let url = format!("{}/tags/{}", EDEN_RELEASES_API, version);
    let data = request_git_api(&url).await?;

    ReleaseInfo::from_forgejo_api(&data).ok_or_else(|| {
        AppError::InvalidArgument(format!("Unable to parse Eden release {}", version))
    })
}

async fn get_citron_all_release_info() -> AppResult<Vec<ReleaseInfo>> {
    info!("Fetching Citron release information");

    let data = request_github_api(CITRON_RELEASES_API).await?;

    let releases: Vec<ReleaseInfo> = data
        .as_array()
        .ok_or_else(|| AppError::InvalidArgument("Invalid GitHub API response".to_string()))?
        .iter()
        .filter_map(ReleaseInfo::from_github_api)
        .collect();

    debug!("Fetched {} Citron releases", releases.len());
    Ok(releases)
}

async fn get_citron_all_release_versions() -> AppResult<Vec<String>> {
    let releases = get_citron_all_release_info().await?;
    let versions: Vec<String> = releases.iter().map(|r| r.tag_name.clone()).collect();
    Ok(versions)
}

async fn get_citron_release_info_by_version(version: &str) -> AppResult<ReleaseInfo> {
    info!("Fetching Citron release information for {}", version);

    let url = format!("{}/tags/{}", CITRON_RELEASES_API, version);
    let data = request_github_api(&url).await?;

    ReleaseInfo::from_github_api(&data).ok_or_else(|| {
        AppError::InvalidArgument(format!("Unable to parse Citron release {}", version))
    })
}

pub async fn get_latest_change_log(branch: &str) -> AppResult<String> {
    let releases = get_yuzu_all_release_info(branch).await?;

    if releases.is_empty() {
        return Ok(format!(
            "Unable to fetch latest changelog information for {}",
            branch
        ));
    }

    Ok(releases[0].description.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_get_eden_releases() {
        let versions = get_eden_all_release_versions().await.unwrap();
        assert!(!versions.is_empty());
        println!("Eden versions: {:?}", &versions[..5.min(versions.len())]);
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_citron_releases() {
        let versions = get_citron_all_release_versions().await.unwrap();
        assert!(!versions.is_empty());
        println!("Citron versions: {:?}", &versions[..5.min(versions.len())]);
    }

    #[test]
    fn test_parse_citron_github_release() {
        let json = serde_json::json!({
            "name": "2026-04-27",
            "tag_name": "2026-04-27",
            "body": "Citron release notes",
            "published_at": "2026-04-27T00:00:00Z",
            "prerelease": false,
            "html_url": "https://github.com/citron-neo/emulator/releases/tag/2026-04-27",
            "assets": [
                {
                    "name": "Citron-windows-nightly-0237a9b88-x64-msvc.zip",
                    "browser_download_url": "https://github.com/citron-neo/emulator/releases/download/2026-04-27/Citron-windows-nightly-0237a9b88-x64-msvc.zip",
                    "size": 40495923,
                    "content_type": "application/zip"
                }
            ]
        });

        let release = ReleaseInfo::from_github_api(&json).unwrap();

        assert_eq!(release.tag_name, "2026-04-27");
        assert_eq!(release.description, "Citron release notes");
        assert_eq!(release.assets.len(), 1);
        assert_eq!(
            release.assets[0].content_type.as_deref(),
            Some("application/zip")
        );
    }

    #[test]
    fn test_unknown_yuzu_branch_is_rejected() {
        let error = unsupported_yuzu_branch_error("unknown");
        assert!(matches!(error, AppError::InvalidArgument(_)));
        assert!(error.to_string().contains("eden"));
        assert!(error.to_string().contains("citron"));
    }
}

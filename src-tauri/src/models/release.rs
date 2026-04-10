//! Release 信息模型
//!
//! 用于表示 GitHub/GitLab/Forgejo 等平台的 Release 信息

use serde::{Deserialize, Serialize};
use tracing::info;

/// 自更新资产分类
#[derive(Debug, Clone, PartialEq)]
pub enum ReleaseAssetKind {
    /// Windows portable zip（自更新主资产）
    WindowsPortableZip,
    /// Windows 裸 exe（自更新兜底资产）
    WindowsExe,
    /// macOS app zip（自更新主资产）
    MacosZip,
    /// macOS app bundle 归档 tar.gz（自更新兜底资产）
    MacosAppArchive,
    /// 未分类
    Unknown,
}

/// 自更新资产选择结果
#[derive(Debug, Clone)]
pub struct SelfUpdateAssetSelection {
    /// 首选资产
    pub primary: Option<ReleaseAsset>,
    /// 兜底资产
    pub fallback: Option<ReleaseAsset>,
}

/// Release 资源信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseAsset {
    /// 资源名称
    pub name: String,
    /// 下载 URL
    pub download_url: String,
    /// 文件大小（字节）
    #[serde(default)]
    pub size: u64,
    /// 内容类型
    #[serde(default)]
    pub content_type: Option<String>,
}

/// Release 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseInfo {
    /// Release 名称
    pub name: String,
    /// 标签名称
    pub tag_name: String,
    /// 描述/变更日志
    #[serde(default)]
    pub description: String,
    /// 资源列表
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
    /// 发布时间
    #[serde(default)]
    pub published_at: Option<String>,
    /// 是否预发布
    #[serde(default)]
    pub prerelease: bool,
    /// Release URL
    #[serde(default)]
    pub html_url: Option<String>,
}

impl ReleaseInfo {
    /// 从 GitHub API 响应解析 Release 信息
    pub fn from_github_api(data: &serde_json::Value) -> Option<Self> {
        Some(Self {
            name: data["name"].as_str().unwrap_or_default().to_string(),
            tag_name: data["tag_name"].as_str().unwrap_or_default().to_string(),
            description: data["body"].as_str().unwrap_or_default().to_string(),
            published_at: data["published_at"].as_str().map(|s| s.to_string()),
            prerelease: data["prerelease"].as_bool().unwrap_or(false),
            html_url: data["html_url"].as_str().map(|s| s.to_string()),
            assets: data["assets"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|a| {
                    Some(ReleaseAsset {
                        name: a["name"].as_str()?.to_string(),
                        download_url: a["browser_download_url"].as_str()?.to_string(),
                        size: a["size"].as_u64().unwrap_or(0),
                        content_type: a["content_type"].as_str().map(|s| s.to_string()),
                    })
                })
                .collect(),
        })
    }

    /// 从 GitLab API 响应解析 Release 信息
    pub fn from_gitlab_api(data: &serde_json::Value) -> Option<Self> {
        Some(Self {
            name: data["name"].as_str().unwrap_or_default().to_string(),
            tag_name: data["tag_name"].as_str().unwrap_or_default().to_string(),
            description: data["description"].as_str().unwrap_or_default().to_string(),
            published_at: data["released_at"].as_str().map(|s| s.to_string()),
            prerelease: false,
            html_url: data["_links"]["self"].as_str().map(|s| s.to_string()),
            assets: data["assets"]["links"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|a| {
                    Some(ReleaseAsset {
                        name: a["name"].as_str()?.to_string(),
                        download_url: a["direct_asset_url"]
                            .as_str()
                            .or_else(|| a["url"].as_str())?
                            .to_string(),
                        size: 0,
                        content_type: None,
                    })
                })
                .collect(),
        })
    }

    /// 从 Forgejo/Gitea API 响应解析 Release 信息
    pub fn from_forgejo_api(data: &serde_json::Value) -> Option<Self> {
        Some(Self {
            name: data["name"].as_str().unwrap_or_default().to_string(),
            tag_name: data["tag_name"].as_str().unwrap_or_default().to_string(),
            description: data["body"].as_str().unwrap_or_default().to_string(),
            published_at: data["published_at"].as_str().map(|s| s.to_string()),
            prerelease: data["prerelease"].as_bool().unwrap_or(false),
            html_url: data["html_url"].as_str().map(|s| s.to_string()),
            assets: data["assets"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|a| {
                    Some(ReleaseAsset {
                        name: a["name"].as_str()?.to_string(),
                        download_url: a["browser_download_url"].as_str()?.to_string(),
                        size: a["size"].as_u64().unwrap_or(0),
                        content_type: None,
                    })
                })
                .collect(),
        })
    }

    /// 根据名称查找资源
    pub fn find_asset_by_name(&self, name: &str) -> Option<&ReleaseAsset> {
        self.assets.iter().find(|a| a.name == name)
    }

    /// 根据名称模式查找资源
    pub fn find_asset_by_pattern(&self, pattern: &str) -> Option<&ReleaseAsset> {
        let pattern_lower = pattern.to_lowercase();
        self.assets
            .iter()
            .find(|a| a.name.to_lowercase().contains(&pattern_lower))
    }

    /// 查找 Windows 资源
    ///
    /// 匹配策略：
    /// 1. 首先精确查找 `NsEmuTools.exe`
    /// 2. 其次兼容查找 `ns-emu-tools.exe`
    /// 3. 最后模糊匹配任意包含 `emu-tools` 的 `.exe` 文件
    pub fn find_windows_asset(&self) -> Option<&ReleaseAsset> {
        info!(
            "开始查找 Windows 资源，总共有 {} 个 assets",
            self.assets.len()
        );

        for asset in &self.assets {
            info!("  可用文件: {}", asset.name);
        }

        let target = self
            .assets
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("NsEmuTools.exe"));
        if target.is_some() {
            info!("找到精确匹配: NsEmuTools.exe");
            return target;
        }
        info!("未找到 NsEmuTools.exe");

        let target = self
            .assets
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("ns-emu-tools.exe"));
        if target.is_some() {
            info!("找到兼容匹配: ns-emu-tools.exe");
            return target;
        }
        info!("未找到 ns-emu-tools.exe");

        let target = self.assets.iter().find(|a| {
            let name_lower = a.name.to_lowercase();
            name_lower.contains("emu-tools") && name_lower.ends_with(".exe")
        });
        if let Some(asset) = target {
            info!("找到模糊匹配: {}", asset.name);
            return Some(asset);
        }

        info!("警告: 未找到任何合适的 Windows 可执行文件！");
        None
    }

    /// 对单个资产进行自更新分类
    pub fn classify_asset(name: &str) -> ReleaseAssetKind {
        let lower = name.to_lowercase();

        // Windows portable zip: 包含 "windows" 且以 .zip 结尾
        if lower.contains("windows") && lower.ends_with(".zip") {
            return ReleaseAssetKind::WindowsPortableZip;
        }

        // Windows exe: NsEmuTools.exe 或类似名称
        if lower.ends_with(".exe") && (lower.contains("nsemutools") || lower.contains("emu-tools"))
        {
            return ReleaseAssetKind::WindowsExe;
        }

        // macOS app archive (tar.gz): 包含 "macos" 且以 .tar.gz 结尾
        // 注意: 先匹配 .tar.gz 再匹配 .zip，避免误判
        if lower.contains("macos") && lower.ends_with(".tar.gz") {
            return ReleaseAssetKind::MacosAppArchive;
        }

        // macOS zip: 包含 "macos" 且以 .zip 结尾
        if lower.contains("macos") && lower.ends_with(".zip") {
            return ReleaseAssetKind::MacosZip;
        }

        ReleaseAssetKind::Unknown
    }

    /// 为当前平台查找自更新资产（primary + fallback）
    pub fn find_self_update_assets(&self) -> SelfUpdateAssetSelection {
        info!(
            "开始为当前平台查找自更新资产，总共有 {} 个 assets",
            self.assets.len()
        );
        for asset in &self.assets {
            let kind = Self::classify_asset(&asset.name);
            info!("  资产: {} -> {:?}", asset.name, kind);
        }

        #[cfg(target_os = "windows")]
        {
            let primary = self
                .assets
                .iter()
                .find(|a| {
                    matches!(
                        Self::classify_asset(&a.name),
                        ReleaseAssetKind::WindowsPortableZip
                    )
                })
                .cloned();

            let fallback = if primary.is_none() {
                // zip 不存在时，fallback 到 exe（复用已有的精确匹配逻辑）
                self.find_windows_asset().cloned()
            } else {
                // zip 存在时，exe 作为 fallback
                self.assets
                    .iter()
                    .find(|a| matches!(Self::classify_asset(&a.name), ReleaseAssetKind::WindowsExe))
                    .cloned()
            };

            if let Some(ref p) = primary {
                info!("Windows 主资产: {}", p.name);
            } else {
                info!("未找到 Windows portable zip 主资产");
            }
            if let Some(ref f) = fallback {
                info!("Windows 兜底资产: {}", f.name);
            }

            SelfUpdateAssetSelection { primary, fallback }
        }

        #[cfg(target_os = "macos")]
        {
            let primary = self
                .assets
                .iter()
                .find(|a| matches!(Self::classify_asset(&a.name), ReleaseAssetKind::MacosZip))
                .cloned();

            let fallback = self
                .assets
                .iter()
                .find(|a| {
                    matches!(
                        Self::classify_asset(&a.name),
                        ReleaseAssetKind::MacosAppArchive
                    )
                })
                .cloned();

            if let Some(ref p) = primary {
                info!("macOS 主资产: {}", p.name);
            } else {
                info!("未找到 macOS zip 主资产");
            }
            if let Some(ref f) = fallback {
                info!("macOS 兜底资产: {}", f.name);
            }

            SelfUpdateAssetSelection { primary, fallback }
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            info!("当前平台不支持自更新");
            SelfUpdateAssetSelection {
                primary: None,
                fallback: None,
            }
        }
    }

    /// 获取当前平台的最佳自更新资产（primary 优先，fallback 兜底）
    pub fn best_self_update_asset(&self) -> Option<ReleaseAsset> {
        let selection = self.find_self_update_assets();
        selection.primary.or(selection.fallback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_api_parsing() {
        let json = serde_json::json!({
            "name": "v1.0.0",
            "tag_name": "v1.0.0",
            "body": "Release notes",
            "prerelease": false,
            "html_url": "https://github.com/test/test/releases/tag/v1.0.0",
            "assets": [
                {
                    "name": "test-windows.zip",
                    "browser_download_url": "https://example.com/test.zip",
                    "size": 1024,
                    "content_type": "application/zip"
                }
            ]
        });

        let release = ReleaseInfo::from_github_api(&json).unwrap();
        assert_eq!(release.tag_name, "v1.0.0");
        assert_eq!(release.assets.len(), 1);
        assert_eq!(release.assets[0].name, "test-windows.zip");
    }

    #[test]
    fn test_forgejo_api_parsing() {
        let json = serde_json::json!({
            "name": "v0.0.4-rc3",
            "tag_name": "v0.0.4-rc3",
            "body": "Forgejo release notes",
            "prerelease": false,
            "html_url": "https://git.eden-emu.dev/eden-emu/eden/releases/tag/v0.0.4-rc3",
            "assets": [
                {
                    "name": "Eden-Windows-v0.0.4-rc3-amd64-msvc-standard.zip",
                    "browser_download_url": "https://git.eden-emu.dev/attachments/test.zip",
                    "size": 2048
                }
            ]
        });

        let release = ReleaseInfo::from_forgejo_api(&json).unwrap();
        assert_eq!(release.tag_name, "v0.0.4-rc3");
        assert_eq!(release.assets.len(), 1);
        assert_eq!(
            release.assets[0].download_url,
            "https://git.eden-emu.dev/attachments/test.zip"
        );
    }

    #[test]
    fn test_find_windows_asset() {
        let release = ReleaseInfo {
            name: "Test".to_string(),
            tag_name: "v1.0.0".to_string(),
            description: "".to_string(),
            published_at: None,
            prerelease: false,
            html_url: None,
            assets: vec![
                ReleaseAsset {
                    name: "test-linux.tar.gz".to_string(),
                    download_url: "https://example.com/linux.tar.gz".to_string(),
                    size: 0,
                    content_type: None,
                },
                ReleaseAsset {
                    name: "NsEmuTools.exe".to_string(),
                    download_url: "https://example.com/NsEmuTools.exe".to_string(),
                    size: 0,
                    content_type: None,
                },
            ],
        };

        let asset = release.find_windows_asset().unwrap();
        assert_eq!(asset.name, "NsEmuTools.exe");
    }

    #[test]
    fn test_classify_asset_windows_portable_zip() {
        assert_eq!(
            ReleaseInfo::classify_asset("NsEmuTools-windows-portable.zip"),
            ReleaseAssetKind::WindowsPortableZip
        );
    }

    #[test]
    fn test_classify_asset_windows_exe() {
        assert_eq!(
            ReleaseInfo::classify_asset("NsEmuTools.exe"),
            ReleaseAssetKind::WindowsExe
        );
        assert_eq!(
            ReleaseInfo::classify_asset("ns-emu-tools.exe"),
            ReleaseAssetKind::WindowsExe
        );
    }

    #[test]
    fn test_classify_asset_macos_zip() {
        assert_eq!(
            ReleaseInfo::classify_asset("NS-Emu-Tools-macos-app.zip"),
            ReleaseAssetKind::MacosZip
        );
    }

    #[test]
    fn test_classify_asset_macos_app_archive() {
        assert_eq!(
            ReleaseInfo::classify_asset("NS-Emu-Tools-macos-app.tar.gz"),
            ReleaseAssetKind::MacosAppArchive
        );
    }

    #[test]
    fn test_classify_asset_unknown() {
        assert_eq!(
            ReleaseInfo::classify_asset("readme.md"),
            ReleaseAssetKind::Unknown
        );
        assert_eq!(
            ReleaseInfo::classify_asset("test-linux.tar.gz"),
            ReleaseAssetKind::Unknown
        );
    }

    #[test]
    fn test_find_self_update_assets_with_full_matrix() {
        let release = ReleaseInfo {
            name: "Test".to_string(),
            tag_name: "v1.0.0".to_string(),
            description: "".to_string(),
            published_at: None,
            prerelease: false,
            html_url: None,
            assets: vec![
                ReleaseAsset {
                    name: "NsEmuTools-windows-portable.zip".to_string(),
                    download_url: "https://example.com/win.zip".to_string(),
                    size: 0,
                    content_type: None,
                },
                ReleaseAsset {
                    name: "NsEmuTools.exe".to_string(),
                    download_url: "https://example.com/NsEmuTools.exe".to_string(),
                    size: 0,
                    content_type: None,
                },
                ReleaseAsset {
                    name: "NS-Emu-Tools-macos-app.zip".to_string(),
                    download_url: "https://example.com/macos.zip".to_string(),
                    size: 0,
                    content_type: None,
                },
                ReleaseAsset {
                    name: "NS-Emu-Tools-macos-app.tar.gz".to_string(),
                    download_url: "https://example.com/macos.tar.gz".to_string(),
                    size: 0,
                    content_type: None,
                },
            ],
        };

        let selection = release.find_self_update_assets();

        #[cfg(target_os = "windows")]
        {
            assert!(selection.primary.is_some());
            assert_eq!(
                selection.primary.unwrap().name,
                "NsEmuTools-windows-portable.zip"
            );
            assert!(selection.fallback.is_some());
            assert_eq!(selection.fallback.unwrap().name, "NsEmuTools.exe");
        }

        #[cfg(target_os = "macos")]
        {
            assert!(selection.primary.is_some());
            assert_eq!(
                selection.primary.unwrap().name,
                "NS-Emu-Tools-macos-app.zip"
            );
            assert!(selection.fallback.is_some());
            assert_eq!(
                selection.fallback.unwrap().name,
                "NS-Emu-Tools-macos-app.tar.gz"
            );
        }
    }

    #[test]
    fn test_find_self_update_assets_fallback_when_no_zip() {
        let release = ReleaseInfo {
            name: "Test".to_string(),
            tag_name: "v1.0.0".to_string(),
            description: "".to_string(),
            published_at: None,
            prerelease: false,
            html_url: None,
            assets: vec![ReleaseAsset {
                name: "NsEmuTools.exe".to_string(),
                download_url: "https://example.com/NsEmuTools.exe".to_string(),
                size: 0,
                content_type: None,
            }],
        };

        let best = release.best_self_update_asset();

        #[cfg(target_os = "windows")]
        {
            assert!(best.is_some());
            assert_eq!(best.unwrap().name, "NsEmuTools.exe");
        }

        #[cfg(target_os = "macos")]
        {
            assert!(best.is_none());
        }
    }
}

//! Release 信息模型
//!
//! 用于表示 GitHub/GitLab/Forgejo 等平台的 Release 信息

use serde::{Deserialize, Serialize};

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
    /// 1. 首先精确查找 "NsEmuTools-dir.7z"
    /// 2. 如果找不到，查找 "NsEmuTools.exe"
    /// 3. 如果还找不到，查找 "ns-emu-tools.exe"（Tauri 版本）
    pub fn find_windows_asset(&self) -> Option<&ReleaseAsset> {
        use tracing::info;

        info!("开始查找 Windows 资源，总共有 {} 个 assets", self.assets.len());

        // 打印所有 assets 的名称
        for asset in &self.assets {
            info!("  可用文件: {}", asset.name);
        }

        // 1. 首先查找 NsEmuTools-dir.7z (Python 版本的压缩包)
        let target = self.assets.iter().find(|a| a.name == "NsEmuTools-dir.7z");
        if target.is_some() {
            info!("找到精确匹配: NsEmuTools-dir.7z");
            return target;
        }
        info!("未找到 NsEmuTools-dir.7z");

        // 2. 查找 NsEmuTools.exe (Python 版本)
        let target = self.assets.iter().find(|a| a.name == "NsEmuTools.exe");
        if target.is_some() {
            info!("找到精确匹配: NsEmuTools.exe");
            return target;
        }
        info!("未找到 NsEmuTools.exe");

        // 3. 查找 ns-emu-tools.exe (Tauri 版本)
        let target = self.assets.iter().find(|a| a.name == "ns-emu-tools.exe");
        if target.is_some() {
            info!("找到精确匹配: ns-emu-tools.exe");
            return target;
        }
        info!("未找到 ns-emu-tools.exe");

        // 4. 查找任何包含 "emu-tools" 且以 .exe 或 .7z 结尾的文件
        let target = self.assets.iter().find(|a| {
            let name_lower = a.name.to_lowercase();
            name_lower.contains("emu-tools") &&
            (name_lower.ends_with(".exe") || name_lower.ends_with(".7z") || name_lower.ends_with(".zip"))
        });
        if target.is_some() {
            info!("找到模糊匹配: {}", target.unwrap().name);
            return target;
        }

        info!("警告: 未找到任何合适的 Windows 资源文件！");
        None
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
                    name: "test-windows-x64.zip".to_string(),
                    download_url: "https://example.com/windows.zip".to_string(),
                    size: 0,
                    content_type: None,
                },
            ],
        };

        let asset = release.find_windows_asset().unwrap();
        assert_eq!(asset.name, "test-windows-x64.zip");
    }
}

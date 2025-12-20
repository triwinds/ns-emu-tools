//! 配置数据访问层
//!
//! 提供固件下载源、GitHub 镜像等静态配置数据

use crate::error::AppResult;
use crate::services::network::{create_client, get_final_url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{info, warn};

/// 名称-值对类型 (对应前端 [name, value])
pub type NameValuePair = (String, String);

/// GitHub 镜像信息 (对应前端 [value, region, name])
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubMirror {
    pub value: String,
    pub region: String,
    pub name: String,
}

impl GithubMirror {
    /// 转换为前端期望的数组格式 [value, region, name]
    pub fn to_tuple(&self) -> (String, String, String) {
        (self.value.clone(), self.region.clone(), self.name.clone())
    }
}

/// 获取可用的固件下载源
pub fn get_available_firmware_sources() -> Vec<NameValuePair> {
    vec![
        (
            "由 github.com/THZoria/NX_Firmware 提供的固件".to_string(),
            "github".to_string(),
        ),
        (
            "由 darthsternie.net 提供的固件".to_string(),
            "nsarchive".to_string(),
        ),
    ]
}

/// 获取可用的 GitHub 镜像列表
pub fn get_github_mirrors() -> Vec<(String, String, String)> {
    let mut mirrors = vec![
        (
            "cloudflare_load_balance".to_string(),
            "美国".to_string(),
            "[美国 Cloudflare CDN] 随机选择 Cloudflare 服务器".to_string(),
        ),
        (
            "direct".to_string(),
            "美国".to_string(),
            "直连 GitHub".to_string(),
        ),
    ];

    // 美国镜像
    let us_mirrors = vec![
        (
            "https://nsarchive.e6ex.com/gh".to_string(),
            "美国".to_string(),
            "[美国 Cloudflare CDN] - 自建代理服务器".to_string(),
        ),
        (
            "https://gh.h233.eu.org/https://github.com".to_string(),
            "美国".to_string(),
            "[美国 Cloudflare CDN] - 该公益加速源由 [@X.I.U/XIU2] 提供".to_string(),
        ),
        (
            "https://gh-proxy.com/https://github.com".to_string(),
            "美国".to_string(),
            "[美国 Cloudflare CDN] - 该公益加速源由 [gh-proxy.com] 提供".to_string(),
        ),
        (
            "https://hub.gitmirror.com/https://github.com".to_string(),
            "美国".to_string(),
            "[美国 Cloudflare CDN] - 该公益加速源由 [GitMirror] 提供".to_string(),
        ),
        (
            "https://github.boki.moe/https://github.com".to_string(),
            "美国".to_string(),
            "[美国 Cloudflare CDN] - 该公益加速源由 [blog.boki.moe] 提供".to_string(),
        ),
        (
            "https://gh-proxy.net/https://github.com".to_string(),
            "美国".to_string(),
            "[美国 Cloudflare CDN] - 该公益加速源由 [gh-proxy.net] 提供".to_string(),
        ),
        (
            "https://gh.jasonzeng.dev/https://github.com".to_string(),
            "美国".to_string(),
            "[美国 Cloudflare CDN] - 该公益加速源由 [gh.jasonzeng.dev] 提供".to_string(),
        ),
        (
            "https://gh.monlor.com/https://github.com".to_string(),
            "美国".to_string(),
            "[美国 Cloudflare CDN] - 该公益加速源由 [gh.monlor.com] 提供".to_string(),
        ),
        (
            "https://fastgit.cc/https://github.com".to_string(),
            "美国".to_string(),
            "[美国 Cloudflare CDN] - 该公益加速源由 [fastgit.cc] 提供".to_string(),
        ),
        (
            "https://github.tbedu.top/https://github.com".to_string(),
            "美国".to_string(),
            "[美国 Cloudflare CDN] - 该公益加速源由 [github.tbedu.top] 提供".to_string(),
        ),
        (
            "https://gh.chjina.com/https://github.com".to_string(),
            "美国".to_string(),
            "[美国 Cloudflare CDN] - 该公益加速源由 [gh.chjina.com] 提供".to_string(),
        ),
        (
            "https://ghpxy.hwinzniej.top/https://github.com".to_string(),
            "美国".to_string(),
            "[美国 Cloudflare CDN] - 该公益加速源由 [ghpxy.hwinzniej.top] 提供".to_string(),
        ),
        (
            "https://git.yylx.win/https://github.com".to_string(),
            "美国".to_string(),
            "[美国 Cloudflare CDN] - 该公益加速源由 [git.yylx.win] 提供".to_string(),
        ),
    ];

    // 其他地区镜像
    let other_mirrors = vec![
        (
            "https://ghproxy.com/https://github.com".to_string(),
            "韩国".to_string(),
            "[韩国 首尔] - 该公益加速源由 [ghproxy] 提供，有日本、韩国、德国、巴西等地区的服务器，不过国内一般分配为韩国".to_string(),
        ),
        (
            "https://kgithub.com".to_string(),
            "新加坡".to_string(),
            "[新加坡] - 该公益加速源由 [KGitHub] 提供".to_string(),
        ),
    ];

    mirrors.extend(us_mirrors);
    mirrors.extend(other_mirrors);

    mirrors
}

/// 获取游戏数据映射 (Title ID -> Game Name)
pub async fn get_game_data() -> AppResult<HashMap<String, Value>> {
    info!("获取游戏数据映射");

    let url = "https://raw.githubusercontent.com/triwinds/ns-emu-tools/main/game_data.json";
    let final_url = get_final_url(url);

    let client = create_client()?;
    let resp = client.get(&final_url).send().await;

    match resp {
        Ok(response) => {
            if response.status().is_success() {
                let data = response.json::<HashMap<String, Value>>().await?;
                info!("成功获取 {} 个游戏数据", data.len());
                Ok(data)
            } else {
                warn!("获取游戏数据失败: {}", response.status());
                Ok(HashMap::new())
            }
        }
        Err(e) => {
            warn!("获取游戏数据失败: {}", e);
            Ok(HashMap::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_firmware_sources() {
        let sources = get_available_firmware_sources();
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].1, "github");
        assert_eq!(sources[1].1, "nsarchive");
    }

    #[test]
    fn test_get_github_mirrors() {
        let mirrors = get_github_mirrors();
        assert!(mirrors.len() > 10);
        // 验证第一个镜像是 cloudflare_load_balance
        assert_eq!(mirrors[0].0, "cloudflare_load_balance");
    }
}

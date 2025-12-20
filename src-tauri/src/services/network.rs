//! 网络服务模块
//!
//! 提供 HTTP 请求、代理配置、镜像选择等网络相关功能

use crate::config::{user_agent, CONFIG};
use crate::error::{AppError, AppResult};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use rand::prelude::IndexedRandom;
use reqwest::{Client, Proxy};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, info, warn};
use url::Url;

/// URL 覆盖映射表
static URL_OVERRIDE_MAP: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert(
        "https://archive.org/download/nintendo-switch-global-firmwares/",
        "https://nsarchive.e6ex.com/nsfrp/",
    );
    map.insert("https://api.github.com", "https://cfrp.e6ex.com/ghapi");
    map.insert(
        "https://raw.githubusercontent.com",
        "https://ghproxy.net/https://raw.githubusercontent.com",
    );
    map.insert(
        "https://git.ryujinx.app",
        "https://nsarchive.e6ex.com/ryujinx_official",
    );
    map
});

/// GitHub 美国 CDN 镜像列表
pub static GITHUB_US_MIRRORS: Lazy<Vec<GithubMirror>> = Lazy::new(|| {
    vec![
        GithubMirror::new(
            "https://nsarchive.e6ex.com/gh",
            "美国",
            "[美国 Cloudflare CDN] - 自建代理服务器",
        ),
        GithubMirror::new(
            "https://gh.h233.eu.org/https://github.com",
            "美国",
            "[美国 Cloudflare CDN] - 该公益加速源由 [@X.I.U/XIU2] 提供",
        ),
        GithubMirror::new(
            "https://gh-proxy.com/https://github.com",
            "美国",
            "[美国 Cloudflare CDN] - 该公益加速源由 [gh-proxy.com] 提供",
        ),
        GithubMirror::new(
            "https://hub.gitmirror.com/https://github.com",
            "美国",
            "[美国 Cloudflare CDN] - 该公益加速源由 [GitMirror] 提供",
        ),
        GithubMirror::new(
            "https://github.boki.moe/https://github.com",
            "美国",
            "[美国 Cloudflare CDN] - 该公益加速源由 [blog.boki.moe] 提供",
        ),
        GithubMirror::new(
            "https://gh-proxy.net/https://github.com",
            "美国",
            "[美国 Cloudflare CDN] - 该公益加速源由 [gh-proxy.net] 提供",
        ),
        GithubMirror::new(
            "https://gh.jasonzeng.dev/https://github.com",
            "美国",
            "[美国 Cloudflare CDN] - 该公益加速源由 [gh.jasonzeng.dev] 提供",
        ),
        GithubMirror::new(
            "https://gh.monlor.com/https://github.com",
            "美国",
            "[美国 Cloudflare CDN] - 该公益加速源由 [gh.monlor.com] 提供",
        ),
        GithubMirror::new(
            "https://fastgit.cc/https://github.com",
            "美国",
            "[美国 Cloudflare CDN] - 该公益加速源由 [fastgit.cc] 提供",
        ),
        GithubMirror::new(
            "https://github.tbedu.top/https://github.com",
            "美国",
            "[美国 Cloudflare CDN] - 该公益加速源由 [github.tbedu.top] 提供",
        ),
        GithubMirror::new(
            "https://gh.nxnow.top/https://github.com",
            "美国",
            "[美国 洛杉矶] - 该公益加速源由 [gh.nxnow.top] 提供",
        ),
    ]
});

/// GitHub 其他地区镜像列表
pub static GITHUB_OTHER_MIRRORS: Lazy<Vec<GithubMirror>> = Lazy::new(|| {
    vec![
        GithubMirror::new(
            "https://ghproxy.com/https://github.com",
            "韩国",
            "[韩国 首尔] - 该公益加速源由 [ghproxy] 提供",
        ),
        GithubMirror::new(
            "https://kgithub.com",
            "新加坡",
            "[新加坡] - 该公益加速源由 [KGitHub] 提供",
        ),
    ]
});

/// Chrome User-Agent
pub const CHROME_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36";

/// GitHub API 回退标志
static GITHUB_API_FALLBACK_FLAG: Lazy<RwLock<bool>> = Lazy::new(|| RwLock::new(false));

/// GitHub 镜像信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubMirror {
    /// 镜像 URL
    pub url: String,
    /// 地区
    pub region: String,
    /// 描述
    pub description: String,
}

impl GithubMirror {
    pub fn new(url: &str, region: &str, description: &str) -> Self {
        Self {
            url: url.to_string(),
            region: region.to_string(),
            description: description.to_string(),
        }
    }
}

/// 下载选项（用于多线程下载配置）
#[derive(Debug, Clone)]
pub struct NetworkDownloadOptions {
    /// 分片数量
    pub split: u32,
    /// 每服务器最大连接数
    pub max_connection_per_server: u32,
    /// 最小分片大小
    pub min_split_size: String,
}

impl Default for NetworkDownloadOptions {
    fn default() -> Self {
        Self {
            split: 4,
            max_connection_per_server: 4,
            min_split_size: "12M".to_string(),
        }
    }
}

impl NetworkDownloadOptions {
    /// 代理模式下的选项
    pub fn for_proxy() -> Self {
        Self {
            split: 16,
            max_connection_per_server: 16,
            min_split_size: "4M".to_string(),
        }
    }

    /// CDN 模式下的选项
    pub fn for_cdn() -> Self {
        Self::default()
    }
}

/// 创建 HTTP 客户端
pub fn create_client() -> AppResult<Client> {
    create_client_with_timeout(Duration::from_secs(30))
}

/// 创建带超时的 HTTP 客户端
pub fn create_client_with_timeout(timeout: Duration) -> AppResult<Client> {
    let mut builder = Client::builder()
        .user_agent(user_agent())
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(10));

    // 配置代理
    if let Some(proxy_url) = get_proxy_url() {
        if !proxy_url.is_empty() {
            debug!("使用代理: {}", proxy_url);
            let proxy = Proxy::all(&proxy_url).map_err(|e| AppError::from(e))?;
            builder = builder.proxy(proxy);
        }
    }

    builder.build().map_err(|e| AppError::from(e))
}

/// 获取代理 URL
pub fn get_proxy_url() -> Option<String> {
    let proxy = {
        let config = CONFIG.read();
        config.setting.network.proxy.clone()
    };

    if proxy == "system" {
        get_system_proxy()
    } else if proxy.is_empty() {
        None
    } else if is_valid_url(&proxy) {
        Some(proxy.clone())
    } else {
        warn!("无效的代理 URL: {}", proxy);
        None
    }
}

/// 获取系统代理
#[cfg(windows)]
pub fn get_system_proxy() -> Option<String> {
    use std::env;

    // 首先尝试环境变量
    if let Ok(proxy) = env::var("HTTPS_PROXY").or_else(|_| env::var("https_proxy")) {
        if !proxy.is_empty() {
            return Some(proxy);
        }
    }
    if let Ok(proxy) = env::var("HTTP_PROXY").or_else(|_| env::var("http_proxy")) {
        if !proxy.is_empty() {
            return Some(proxy);
        }
    }

    // Windows 注册表读取
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::*;

    unsafe {
        let mut hkey = HKEY::default();
        let subkey: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings\0"
            .encode_utf16()
            .collect();

        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            0,
            KEY_READ,
            &mut hkey,
        )
        .is_ok()
        {
            // 检查代理是否启用
            let value_name: Vec<u16> = "ProxyEnable\0".encode_utf16().collect();
            let mut enabled: u32 = 0;
            let mut size = std::mem::size_of::<u32>() as u32;

            if RegQueryValueExW(
                hkey,
                PCWSTR(value_name.as_ptr()),
                None,
                None,
                Some(&mut enabled as *mut u32 as *mut u8),
                Some(&mut size),
            )
            .is_ok()
                && enabled == 1
            {
                // 读取代理服务器
                let value_name: Vec<u16> = "ProxyServer\0".encode_utf16().collect();
                let mut buffer = vec![0u16; 512];
                let mut size = (buffer.len() * 2) as u32;

                if RegQueryValueExW(
                    hkey,
                    PCWSTR(value_name.as_ptr()),
                    None,
                    None,
                    Some(buffer.as_mut_ptr() as *mut u8),
                    Some(&mut size),
                )
                .is_ok()
                {
                    let proxy_str = String::from_utf16_lossy(&buffer[..(size as usize / 2) - 1]);
                    let _ = RegCloseKey(hkey);

                    // 如果没有协议前缀，添加 http://
                    if !proxy_str.is_empty() {
                        if proxy_str.contains("://") {
                            return Some(proxy_str);
                        } else {
                            return Some(format!("http://{}", proxy_str));
                        }
                    }
                }
            }
            let _ = RegCloseKey(hkey);
        }
    }

    None
}

#[cfg(not(windows))]
pub fn get_system_proxy() -> Option<String> {
    use std::env;

    env::var("HTTPS_PROXY")
        .or_else(|_| env::var("https_proxy"))
        .or_else(|_| env::var("HTTP_PROXY"))
        .or_else(|_| env::var("http_proxy"))
        .ok()
        .filter(|s| !s.is_empty())
}

/// 验证 URL 是否有效
pub fn is_valid_url(url: &str) -> bool {
    Url::parse(url).is_ok()
}

/// 检查是否正在使用代理
pub fn is_using_proxy() -> bool {
    get_proxy_url().is_some()
}

/// 获取所有 GitHub 镜像列表
pub fn get_github_mirrors() -> Vec<GithubMirror> {
    let mut mirrors = vec![
        GithubMirror::new(
            "cloudflare_load_balance",
            "美国",
            "[美国 Cloudflare CDN] 随机选择 Cloudflare 服务器",
        ),
        GithubMirror::new("direct", "美国", "直连 GitHub"),
    ];
    mirrors.extend(GITHUB_US_MIRRORS.clone());
    mirrors.extend(GITHUB_OTHER_MIRRORS.clone());
    mirrors
}

/// 获取 GitHub 下载 URL（应用镜像）
pub fn get_github_download_url(origin_url: &str) -> String {
    let mirror = {
        let config = CONFIG.read();
        config.setting.network.github_download_mirror.clone()
    };

    if mirror.is_empty() || mirror == "direct" {
        debug!("使用原始 URL: {}", origin_url);
        return origin_url.to_string();
    }

    if mirror == "cloudflare_load_balance" {
        // 随机选择一个 Cloudflare 镜像
        let mut rng = rand::rng();
        if let Some(choice) = GITHUB_US_MIRRORS.choose(&mut rng) {
            info!("使用 GitHub 镜像: {}", choice.description);
            return origin_url.replace("https://github.com", &choice.url);
        }
    }

    let new_url = origin_url.replace("https://github.com", &mirror);
    debug!("使用镜像 URL: {}", new_url);
    new_url
}

/// 根据网络设置获取最终 URL
pub fn get_final_url(origin_url: &str) -> String {
    let (github_api_mode, ryujinx_mirror, firmware_source) = {
        let config = CONFIG.read();
        let network = &config.setting.network;
        (
            network.github_api_mode.clone(),
            network.ryujinx_git_lab_download_mirror.clone(),
            network.firmware_download_source.clone(),
        )
    };

    if origin_url.starts_with("https://api.github.com") {
        get_final_url_with_mode(origin_url, &github_api_mode)
    } else if origin_url.starts_with("https://git.ryujinx.app") {
        get_final_url_with_mode(origin_url, &ryujinx_mirror)
    } else {
        get_final_url_with_mode(origin_url, &firmware_source)
    }
}

/// 根据模式获取最终 URL
fn get_final_url_with_mode(origin_url: &str, mode: &str) -> String {
    match mode {
        "direct" => {
            debug!("直连模式，使用原始 URL: {}", origin_url);
            origin_url.to_string()
        }
        "cdn" => get_override_url(origin_url),
        _ => {
            // auto-detect 模式
            if is_using_proxy() {
                debug!("检测到代理，使用原始 URL: {}", origin_url);
                origin_url.to_string()
            } else {
                get_override_url(origin_url)
            }
        }
    }
}

/// 获取 URL 覆盖（使用 CDN）
fn get_override_url(origin_url: &str) -> String {
    for (key, value) in URL_OVERRIDE_MAP.iter() {
        if origin_url.starts_with(key) {
            let new_url = origin_url.replace(key, value);
            debug!("使用 CDN URL: {}", new_url);
            return new_url;
        }
    }
    debug!("无可用 CDN，使用原始 URL: {}", origin_url);
    origin_url.to_string()
}

/// 请求 GitHub API
pub async fn request_github_api(url: &str) -> AppResult<serde_json::Value> {
    info!("请求 GitHub API: {}", url);

    let github_api_mode = {
        let config = CONFIG.read();
        config.setting.network.github_api_mode.clone()
    };

    let fallback = *GITHUB_API_FALLBACK_FLAG.read();

    // 如果不是 CDN 模式且没有回退标志，尝试直连
    if github_api_mode != "cdn" && !fallback {
        let client = create_client_with_timeout(Duration::from_secs(5))?;
        match client.get(url).send().await {
            Ok(resp) => {
                if let Ok(data) = resp.json::<serde_json::Value>().await {
                    // 检查是否触发 API 限制
                    if let Some(message) = data.get("message").and_then(|m| m.as_str()) {
                        if message.contains("API rate limit exceeded") {
                            warn!("GitHub API 限制: {}", message);
                            *GITHUB_API_FALLBACK_FLAG.write() = true;
                        } else {
                            return Ok(data);
                        }
                    } else {
                        return Ok(data);
                    }
                }
            }
            Err(e) => {
                warn!("直连 GitHub API 失败: {}", e);
                *GITHUB_API_FALLBACK_FLAG.write() = true;
            }
        }
    }

    // 使用 CDN
    let cdn_url = get_override_url(url);
    let client = create_client()?;
    let resp = client.get(&cdn_url).send().await?;
    let data = resp.json::<serde_json::Value>().await?;
    Ok(data)
}

/// 检查端口是否被占用
pub fn is_port_in_use(port: u16) -> bool {
    use std::net::TcpListener;
    TcpListener::bind(("127.0.0.1", port)).is_err()
}

/// 获取可用端口
pub fn get_available_port() -> u16 {
    use rand::Rng;
    let mut rng = rand::rng();
    loop {
        let port: u16 = rng.random_range(20000..60000);
        if !is_port_in_use(port) {
            return port;
        }
    }
}

/// 获取固件下载源列表 (network 模块版本)
pub fn get_network_firmware_sources() -> Vec<(&'static str, &'static str)> {
    vec![
        ("由 github.com/THZoria/NX_Firmware 提供的固件", "github"),
        ("由 darthsternie.net 提供的固件", "nsarchive"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_mirror_list() {
        let mirrors = get_github_mirrors();
        assert!(mirrors.len() > 2);
        assert_eq!(mirrors[0].url, "cloudflare_load_balance");
        assert_eq!(mirrors[1].url, "direct");
    }

    #[test]
    fn test_get_override_url() {
        let url = "https://api.github.com/repos/test/test";
        let overridden = get_override_url(url);
        assert!(overridden.contains("cfrp.e6ex.com"));
    }

    #[test]
    fn test_is_valid_url() {
        assert!(is_valid_url("http://127.0.0.1:8080"));
        assert!(is_valid_url("https://proxy.example.com"));
        assert!(!is_valid_url("not a url"));
    }

    #[test]
    fn test_get_available_port() {
        let port = get_available_port();
        assert!(port >= 20000 && port < 60000);
    }

    #[test]
    #[ignore]
    fn test_get_system_proxy() {
        let proxy = get_system_proxy();
        println!("System proxy: {:?}", proxy);
        assert!(proxy.is_none() || !proxy.unwrap().is_empty());
    }
}

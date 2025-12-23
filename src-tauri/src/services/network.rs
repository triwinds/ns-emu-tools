//! 网络服务模块
//!
//! 提供 HTTP 请求、代理配置、镜像选择等网络相关功能

use crate::config::{user_agent, CONFIG};
use crate::error::{AppError, AppResult};
use http_cache_reqwest::{Cache, CacheMode, HttpCache, HttpCacheOptions, MokaManager};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use rand::prelude::IndexedRandom;
use reqwest::{Client, Proxy, Request, Response};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware, Middleware, Next};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tauri::http;
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

/// 缓存日志中间件
#[derive(Debug, Clone)]
pub struct CacheLoggingMiddleware;

#[async_trait::async_trait]
impl Middleware for CacheLoggingMiddleware {
    async fn handle(
        &self,
        req: Request,
        extensions: &mut http::Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<Response> {
        let url = req.url().to_string();
        let response = next.run(req, extensions).await?;

        // // 打印所有响应头部用于调试
        // debug!("请求 URL: {}", url);
        // debug!("响应头部:");
        // for (key, value) in response.headers().iter() {
        //     if let Ok(v) = value.to_str() {
        //         debug!("  {}: {}", key, v);
        //     }
        // }

        // 检查缓存状态
        if let Some(cache_status) = response.headers().get("x-cache") {
            if let Ok(status_str) = cache_status.to_str() {
                match status_str {
                    "HIT" => info!("✓ 缓存命中: {}", url),
                    "MISS" => info!("✗ 缓存未命中: {}", url),
                    _ => info!("? 缓存状态 ({}): {}", status_str, url),
                }
            }
        } else {
            info!("⚠ 无缓存头部: {}", url);
        }

        Ok(response)
    }
}

/// GitHub API 回退标志
static GITHUB_API_FALLBACK_FLAG: Lazy<RwLock<bool>> = Lazy::new(|| RwLock::new(false));

/// 全局缓存客户端（内存缓存）
static CACHED_CLIENT: Lazy<ClientWithMiddleware> = Lazy::new(|| {
    create_cached_client().expect("Failed to create cached client")
});

/// 全局持久化缓存客户端（磁盘缓存）
static DURABLE_CACHED_CLIENT: Lazy<ClientWithMiddleware> = Lazy::new(|| {
    create_durable_cached_client().expect("Failed to create durable cached client")
});

/// Git API JSON 响应缓存（5 分钟 TTL，忽略 cache-control）
static GIT_API_JSON_CACHE: Lazy<moka::future::Cache<String, serde_json::Value>> =
    Lazy::new(|| {
        moka::future::Cache::builder()
            .max_capacity(100) // 最多缓存 100 个响应
            .time_to_live(std::time::Duration::from_secs(300)) // 5 分钟过期
            .build()
    });

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
    debug!("创建 HTTP 客户端，超时: {:?}", timeout);
    let mut builder = Client::builder()
        .user_agent(user_agent())
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(10));

    // 配置代理
    if let Some(proxy_url) = get_proxy_url() {
        if !proxy_url.is_empty() {
            info!("使用代理创建客户端: {}", proxy_url);
            let proxy = Proxy::all(&proxy_url).map_err(|e| {
                warn!("创建代理失败: {}", e);
                AppError::from(e)
            })?;
            builder = builder.proxy(proxy);
        }
    } else {
        debug!("不使用代理创建客户端");
    }

    builder.build().map_err(|e| {
        warn!("创建 HTTP 客户端失败: {}", e);
        AppError::from(e)
    })
}

/// 创建内存缓存客户端（用于一般请求）
pub fn create_cached_client() -> AppResult<ClientWithMiddleware> {
    let client = create_client()?;
    let cached_client = ClientBuilder::new(client)
        .with(CacheLoggingMiddleware)
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: MokaManager::default(),
            options: HttpCacheOptions::default(),
        }))
        .build();
    Ok(cached_client)
}

/// 创建持久化缓存客户端（用于需要长期缓存的请求，如固件信息）
pub fn create_durable_cached_client() -> AppResult<ClientWithMiddleware> {
    use http_cache_reqwest::CACacheManager;

    let client = create_client()?;

    let cached_client = ClientBuilder::new(client)
        .with(CacheLoggingMiddleware)
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: CACacheManager::default(),
            options: HttpCacheOptions::default(),
        }))
        .build();
    Ok(cached_client)
}

/// 获取全局缓存客户端（内存缓存）
pub fn get_cached_client() -> &'static ClientWithMiddleware {
    &CACHED_CLIENT
}

/// 获取全局持久化缓存客户端（磁盘缓存，类似 Python 的 get_durable_cache_session）
pub fn get_durable_cached_client() -> &'static ClientWithMiddleware {
    &DURABLE_CACHED_CLIENT
}

/// 获取代理 URL
pub fn get_proxy_url() -> Option<String> {
    let proxy = {
        let config = CONFIG.read();
        config.setting.network.proxy.clone()
    };

    debug!("配置的代理设置: {}", proxy);

    if proxy == "system" {
        debug!("使用系统代理");
        let system_proxy = get_system_proxy();
        if let Some(ref p) = system_proxy {
            info!("检测到系统代理: {}", p);
        } else {
            debug!("未检测到系统代理");
        }
        system_proxy
    } else if proxy.is_empty() {
        debug!("未配置代理");
        None
    } else if is_valid_url(&proxy) {
        info!("使用自定义代理: {}", proxy);
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

    debug!("获取 GitHub 下载 URL，原始 URL: {}", origin_url);
    debug!("配置的镜像: {}", mirror);

    if mirror.is_empty() || mirror == "direct" {
        info!("使用直连 GitHub: {}", origin_url);
        return origin_url.to_string();
    }

    if mirror == "cloudflare_load_balance" {
        // 随机选择一个 Cloudflare 镜像
        let mut rng = rand::rng();
        if let Some(choice) = GITHUB_US_MIRRORS.choose(&mut rng) {
            info!("使用 GitHub 镜像: {}", choice.description);
            let new_url = origin_url.replace("https://github.com", &choice.url);
            debug!("镜像 URL: {}", new_url);
            return new_url;
        }
    }

    let new_url = origin_url.replace("https://github.com", &mirror);
    info!("使用自定义镜像 URL: {}", new_url);
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

    debug!("GitHub API 模式: {}", github_api_mode);

    let fallback = *GITHUB_API_FALLBACK_FLAG.read();
    debug!("API 回退标志: {}", fallback);

    // 如果不是 CDN 模式且没有回退标志，尝试直连
    if github_api_mode != "cdn" && !fallback {
        info!("尝试直连 GitHub API");
        // 使用缓存客户端进行直连
        let cached_client = get_cached_client();
        match cached_client.get(url).send().await {
            Ok(resp) => {
                debug!("直连请求成功，HTTP 状态: {}", resp.status());
                if let Ok(data) = resp.json::<serde_json::Value>().await {
                    // 检查是否触发 API 限制
                    if let Some(message) = data.get("message").and_then(|m| m.as_str()) {
                        if message.contains("API rate limit exceeded") {
                            warn!("GitHub API 限制: {}", message);
                            debug!("设置回退标志");
                            *GITHUB_API_FALLBACK_FLAG.write() = true;
                        } else {
                            debug!("GitHub API 响应成功");
                            return Ok(data);
                        }
                    } else {
                        debug!("GitHub API 响应成功");
                        return Ok(data);
                    }
                } else {
                    warn!("解析 JSON 响应失败，回退到 CDN");
                }
            }
            Err(e) => {
                warn!("直连 GitHub API 失败: {}", e);
                debug!("设置回退标志");
                *GITHUB_API_FALLBACK_FLAG.write() = true;
            }
        }
    }

    // 使用 CDN 和持久化缓存
    let cdn_url = get_override_url(url);
    info!("使用 CDN 请求 GitHub API: {}", cdn_url);
    let durable_cached_client = get_durable_cached_client();
    let resp = durable_cached_client.get(&cdn_url).send().await.map_err(|e| {
        warn!("CDN 请求失败: {}", e);
        AppError::Unknown(e.to_string())
    })?;
    debug!("CDN 请求成功，HTTP 状态: {}", resp.status());
    let data = resp.json::<serde_json::Value>().await.map_err(|e| {
        warn!("解析 CDN 响应失败: {}", e);
        AppError::Unknown(e.to_string())
    })?;
    debug!("CDN GitHub API 响应成功");
    Ok(data)
}

/// 请求 Git 托管平台 API（GitLab/Forgejo）
/// 用于 git.ryujinx.app 和 git.citron-emu.org
/// 使用手动缓存（5 分钟 TTL）忽略 cache-control: private
pub async fn request_git_api(url: &str) -> AppResult<serde_json::Value> {
    info!("请求 Git API: {}", url);

    // 检查缓存
    if let Some(cached_data) = GIT_API_JSON_CACHE.get(url).await {
        info!("✓ 缓存命中 (手动缓存): {}", url);
        debug!("使用缓存数据，跳过网络请求");
        return Ok(cached_data);
    }

    info!("✗ 缓存未命中 (手动缓存): {}", url);
    debug!("创建新的网络请求");

    // 使用普通客户端发送请求（不使用 HTTP 缓存中间件）
    let client = create_client()?;
    debug!("发送 GET 请求到 Git API");
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| {
            warn!("Git API 请求失败: {}", e);
            AppError::Unknown(e.to_string())
        })?;

    debug!("Git API 响应状态: {}", resp.status());

    if !resp.status().is_success() {
        warn!("Git API 请求失败，HTTP 状态: {}", resp.status());
        return Err(AppError::Unknown(format!(
            "Git API 请求失败: {} - {}",
            resp.status(),
            url
        )));
    }

    let data = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e| AppError::Unknown(e.to_string()))?;

    // 将响应存入缓存
    GIT_API_JSON_CACHE
        .insert(url.to_string(), data.clone())
        .await;

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

    #[tokio::test]
    #[ignore] // 需要网络连接，默认忽略
    async fn test_github_us_mirrors() {
        let mut invalid_mirrors = Vec::new();

        // 跳过第一个镜像（与 Python 版本一致）
        let test_mirrors = &GITHUB_US_MIRRORS[1..];

        for mirror in test_mirrors {
            print!("testing {}... ", mirror.description);

            // 构建测试 URL
            let test_url = format!(
                "{}/XIU2/CloudflareSpeedTest/releases/download/v2.2.2/CloudflareST_windows_amd64.zip",
                mirror.url
            );

            match try_test_mirror(&test_url).await {
                Ok(true) => println!("worked"),
                Ok(false) => {
                    println!("failed (invalid content length)");
                    invalid_mirrors.push(mirror.description.clone());
                }
                Err(e) => {
                    println!("failed: {}", e);
                    invalid_mirrors.push(mirror.description.clone());
                }
            }
        }

        println!("====================================");
        if !invalid_mirrors.is_empty() {
            println!("Invalid mirrors:");
            for mirror in &invalid_mirrors {
                println!("  - {}", mirror);
            }
        } else {
            println!("All mirrors are working!");
        }

        // 如果有失败的镜像，测试失败
        assert!(
            invalid_mirrors.is_empty(),
            "Found {} invalid mirror(s): {:?}",
            invalid_mirrors.len(),
            invalid_mirrors
        );
    }

    /// 测试单个镜像是否可用
    async fn try_test_mirror(url: &str) -> Result<bool, String> {
        let client = Client::builder()
            .user_agent(CHROME_UA)
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;

        let resp = client
            .head(url)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        // 检查 Content-Length
        if let Some(content_length) = resp.headers().get("content-length") {
            if let Ok(length_str) = content_length.to_str() {
                if let Ok(length) = length_str.parse::<u64>() {
                    // 检查文件大小是否合理（至少 20000 字节）
                    return Ok(length >= 20000);
                }
            }
        }

        Ok(false)
    }
}

//! 网络服务模块
//!
//! 提供 HTTP 请求、代理配置、镜像选择等网络相关功能

use crate::config::{effective_config_dir, user_agent, CONFIG};
use crate::error::{AppError, AppResult};
use http_cache_reqwest::{Cache, CacheMode, HttpCache, HttpCacheOptions, MokaManager};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use rand::prelude::IndexedRandom;
use reqwest::header::HeaderMap;
use reqwest::{Client, Proxy, Request, Response};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware, Middleware, Next};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::http;
use tracing::{debug, info, warn};
use url::Url;

/// URL 覆盖映射表
static URL_OVERRIDE_MAP: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert("https://api.github.com", "https://cfrp.e6ex.com/ghapi");
    map.insert(
        "https://raw.githubusercontent.com",
        "https://cfrp.e6ex.com/rawgit",
    );
    map.insert(
        "https://git.ryujinx.app",
        "https://nsarchive.e6ex.com/ryujinx_official",
    );
    map.insert(
        "https://git.eden-emu.dev",
        "https://nsarchive.e6ex.com/eden_official",
    );
    map
});

// https://github.com/XIU2/UserScript/blob/master/GithubEnhanced-High-Speed-Download.user.js
/// GitHub 美国 CDN 镜像列表
pub static GITHUB_US_MIRRORS: Lazy<Vec<GithubMirror>> = Lazy::new(|| {
    vec![
        GithubMirror::new(
            "https://nsarchive.e6ex.com/gh",
            "美国",
            "[美国 Cloudflare CDN] - 自建代理服务器",
        ),
        GithubMirror::new(
            "https://gh-proxy.org/https://github.com",
            "美国",
            "[美国 Cloudflare CDN] - 该公益加速源由 [gh-proxy.com] 提供",
        ),
        GithubMirror::new(
            "https://cdn.gh-proxy.org/https://github.com",
            "美国",
            "[Fastly CDN] - 该公益加速源由 [gh-proxy.com] 提供",
        ),
        GithubMirror::new(
            "https://edgeone.gh-proxy.org/https://github.com",
            "美国",
            "[edgeone] - 该公益加速源由 [gh-proxy.com] 提供",
        ),
        GithubMirror::new(
            "https://github.boki.moe/https://github.com",
            "美国",
            "[美国 Cloudflare CDN] - 该公益加速源由 [blog.boki.moe] 提供",
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
            "https://github.ednovas.xyz/https://github.com",
            "美国",
            "[美国 Cloudflare CDN] - 该公益加速源由 [github.ednovas.xyz] 提供",
        ),
    ]
});

/// GitHub 其他地区镜像列表
pub static GITHUB_OTHER_MIRRORS: Lazy<Vec<GithubMirror>> = Lazy::new(|| {
    vec![
        GithubMirror::new(
            "https://wget.la/https://github.com",
            "香港",
            "[中国香港、中国台湾、日本、美国等]（CDN 不固定） - 该公益加速源由 [ucdn.me] 提供",
        ),
        GithubMirror::new(
            "https://hk.gh-proxy.org/https://github.com",
            "香港",
            "[中国香港] - 该公益加速源由 [gh-proxy.com] 提供",
        ),
        GithubMirror::new(
            "https://ghfast.top/https://github.com",
            "韩国",
            "[日本、韩国、新加坡、美国、德国等]（CDN 不固定） - 该公益加速源由 [ghproxy] 提供",
        ),
        GithubMirror::new(
            "https://githubfast.com",
            "韩国",
            "[韩国] - 该公益加速源由 [Github Fast] 提供",
        ),
    ]
});

/// Chrome User-Agent
pub const CHROME_UA: &str = concat!(
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) ",
    "AppleWebKit/537.36 (KHTML, like Gecko) ",
    "Chrome/136.0.0.0 Safari/537.36"
);
const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const GITHUB_API_FALLBACK_TTL: Duration = Duration::from_secs(120);
const GITHUB_API_RATE_LIMIT_THRESHOLD: u32 = 2;
const GIT_API_JSON_CACHE_TTL: Duration = Duration::from_secs(300);
const GIT_API_JSON_CACHE_CAPACITY: u64 = 100;

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
                    "HIT" => debug!("缓存命中: {}", url),
                    "MISS" => debug!("缓存未命中: {}", url),
                    _ => debug!("缓存状态 ({}): {}", status_str, url),
                }
            }
        } else {
            debug!("响应未返回缓存头部: {}", url);
        }

        Ok(response)
    }
}

/// GitHub API 回退标志
#[derive(Debug, Default)]
struct GithubApiCircuitBreaker {
    open_until: Option<Instant>,
    last_reason: Option<String>,
}

impl GithubApiCircuitBreaker {
    fn allows_direct_request(&mut self, now: Instant) -> bool {
        match self.open_until {
            Some(deadline) if deadline > now => false,
            Some(_) => {
                if let Some(reason) = self.last_reason.take() {
                    info!(
                        "GitHub API direct cooldown expired, retrying direct requests: {}",
                        reason
                    );
                } else {
                    info!("GitHub API direct cooldown expired, retrying direct requests");
                }
                self.open_until = None;
                true
            }
            None => true,
        }
    }

    fn record_failure(&mut self, now: Instant, cooldown: Duration, reason: impl Into<String>) {
        self.open_until = Some(now + cooldown);
        self.last_reason = Some(reason.into());
    }

    fn record_success(&mut self) -> bool {
        let was_open = self.open_until.take().is_some();
        self.last_reason = None;
        was_open
    }

    #[cfg(test)]
    fn is_open_at(&self, now: Instant) -> bool {
        matches!(self.open_until, Some(deadline) if deadline > now)
    }
}

static GITHUB_API_CIRCUIT_BREAKER: Lazy<RwLock<GithubApiCircuitBreaker>> =
    Lazy::new(|| RwLock::new(GithubApiCircuitBreaker::default()));

/// 全局缓存客户端（内存缓存）
static CACHED_CLIENT: Lazy<ClientWithMiddleware> =
    Lazy::new(|| create_cached_client().expect("Failed to create cached client"));

/// 全局持久化缓存客户端（磁盘缓存）
static DURABLE_CACHED_CLIENT: Lazy<ClientWithMiddleware> =
    Lazy::new(|| create_durable_cached_client().expect("Failed to create durable cached client"));

/// Git API JSON 响应缓存（5 分钟 TTL，忽略 cache-control）
static GIT_API_JSON_CACHE: Lazy<moka::future::Cache<String, serde_json::Value>> = Lazy::new(|| {
    moka::future::Cache::builder()
        .max_capacity(GIT_API_JSON_CACHE_CAPACITY)
        .time_to_live(GIT_API_JSON_CACHE_TTL)
        .build()
});

/// GitHub 镜像信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubMirror {
    /// 镜像 URL
    pub url: Cow<'static, str>,
    /// 地区
    pub region: Cow<'static, str>,
    /// 描述
    pub description: Cow<'static, str>,
}

impl GithubMirror {
    pub fn new(url: &'static str, region: &'static str, description: &'static str) -> Self {
        Self {
            url: Cow::Borrowed(url),
            region: Cow::Borrowed(region),
            description: Cow::Borrowed(description),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubDownloadTarget {
    pub url: String,
    pub source_name: String,
}

pub static GITHUB_MIRRORS: Lazy<Vec<GithubMirror>> = Lazy::new(|| {
    let mut mirrors = vec![
        GithubMirror::new(
            "cloudflare_load_balance",
            "美国",
            "[美国 Cloudflare CDN] 随机选择 Cloudflare 服务器",
        ),
        GithubMirror::new("direct", "美国", "直连 GitHub"),
    ];
    mirrors.extend(GITHUB_US_MIRRORS.iter().cloned());
    mirrors.extend(GITHUB_OTHER_MIRRORS.iter().cloned());
    mirrors
});

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
    create_client_with_timeout(DEFAULT_HTTP_TIMEOUT)
}

/// 创建带超时的 HTTP 客户端
pub fn create_client_with_timeout(timeout: Duration) -> AppResult<Client> {
    debug!("创建 HTTP 客户端，超时: {:?}", timeout);
    let mut builder = Client::builder()
        .user_agent(user_agent())
        .timeout(timeout)
        .connect_timeout(DEFAULT_CONNECT_TIMEOUT);

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

fn durable_cache_dir_path() -> PathBuf {
    effective_config_dir().join("http-cacache")
}

fn durable_cache_dir() -> AppResult<PathBuf> {
    let dir = durable_cache_dir_path();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// 创建持久化缓存客户端（用于需要长期缓存的请求，如固件信息）
pub fn create_durable_cached_client() -> AppResult<ClientWithMiddleware> {
    use http_cache_reqwest::CACacheManager;

    let client = create_client()?;
    let cache_dir = durable_cache_dir()?;

    let cached_client = ClientBuilder::new(client)
        .with(CacheLoggingMiddleware)
        .with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: CACacheManager { path: cache_dir },
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

fn github_api_direct_request_allowed() -> bool {
    let now = Instant::now();
    let mut breaker = GITHUB_API_CIRCUIT_BREAKER.write();
    let allowed = breaker.allows_direct_request(now);

    if !allowed {
        let remaining = breaker
            .open_until
            .map(|deadline| deadline.saturating_duration_since(now))
            .unwrap_or_default();
        if let Some(reason) = breaker.last_reason.as_deref() {
            debug!(
                "GitHub API 直连暂时停用，还需等待 {:?}: {}",
                remaining, reason
            );
        } else {
            debug!("GitHub API 直连暂时停用，还需等待 {:?}", remaining);
        }
    }

    allowed
}

fn open_github_api_circuit(reason: impl Into<String>) {
    open_github_api_circuit_for(GITHUB_API_FALLBACK_TTL, reason);
}

fn open_github_api_circuit_for(cooldown: Duration, reason: impl Into<String>) {
    let reason = reason.into();
    GITHUB_API_CIRCUIT_BREAKER
        .write()
        .record_failure(Instant::now(), cooldown, reason.clone());
    warn!(
        "GitHub API 直连已切换到 CDN，将持续 {:?}: {}",
        cooldown, reason
    );
}

fn close_github_api_circuit() {
    if GITHUB_API_CIRCUIT_BREAKER.write().record_success() {
        info!("GitHub API 直连已恢复，关闭 CDN 回退熔断");
    }
}

fn parse_header_u32(headers: &HeaderMap, name: &str) -> Option<u32> {
    headers.get(name)?.to_str().ok()?.parse().ok()
}

fn parse_rate_limit_reset_cooldown(headers: &HeaderMap) -> Option<Duration> {
    let reset_at = headers
        .get("x-ratelimit-reset")?
        .to_str()
        .ok()?
        .parse::<u64>()
        .ok()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    let seconds = reset_at.saturating_sub(now).max(1);
    Some(Duration::from_secs(seconds))
}

fn github_rate_limit_cooldown(headers: &HeaderMap) -> Option<(u32, Duration)> {
    let remaining = parse_header_u32(headers, "x-ratelimit-remaining")?;
    if remaining > GITHUB_API_RATE_LIMIT_THRESHOLD {
        return None;
    }

    let cooldown = parse_rate_limit_reset_cooldown(headers).unwrap_or(GITHUB_API_FALLBACK_TTL);
    Some((remaining, cooldown))
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
        warn!("无效的代理链接：{}", proxy);
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
    GITHUB_MIRRORS.clone()
}

fn find_github_mirror_description(mirror: &str) -> Option<String> {
    GITHUB_MIRRORS
        .iter()
        .find(|candidate| {
            mirror.starts_with(candidate.url.as_ref()) || candidate.url.as_ref().starts_with(mirror)
        })
        .map(|candidate| candidate.description.to_string())
}

fn github_source_name_from_mirror(mirror: &str) -> String {
    if mirror.is_empty() || mirror == "direct" {
        return "直连".to_string();
    }

    if mirror == "cloudflare_load_balance" {
        return "Cloudflare CDN 负载均衡".to_string();
    }

    find_github_mirror_description(mirror).unwrap_or_else(|| format!("自定义镜像: {}", mirror))
}

fn resolve_github_download_target_with_mirror(
    origin_url: &str,
    mirror: &str,
) -> GithubDownloadTarget {
    debug!("获取 GitHub 下载链接，原始链接：{}", origin_url);
    debug!("配置的镜像: {}", mirror);

    if mirror.is_empty() || mirror == "direct" {
        info!("使用直连 GitHub: {}", origin_url);
        return GithubDownloadTarget {
            url: origin_url.to_string(),
            source_name: "直连".to_string(),
        };
    }

    if mirror == "cloudflare_load_balance" {
        let mut rng = rand::rng();
        if let Some(choice) = GITHUB_US_MIRRORS.choose(&mut rng) {
            info!("使用 GitHub 镜像: {}", choice.description);
            let url = origin_url.replace("https://github.com", choice.url.as_ref());
            debug!("镜像链接：{}", url);
            return GithubDownloadTarget {
                url,
                source_name: choice.description.to_string(),
            };
        }

        warn!("GitHub mirror pool is empty, falling back to direct download");
        return GithubDownloadTarget {
            url: origin_url.to_string(),
            source_name: "直连".to_string(),
        };
    }

    let url = origin_url.replace("https://github.com", mirror);
    info!("使用自定义镜像链接：{}", url);
    GithubDownloadTarget {
        url,
        source_name: github_source_name_from_mirror(mirror),
    }
}

pub fn resolve_github_download_target(origin_url: &str) -> GithubDownloadTarget {
    let mirror = {
        let config = CONFIG.read();
        config.setting.network.github_download_mirror.clone()
    };

    resolve_github_download_target_with_mirror(origin_url, &mirror)
}

/// 获取 GitHub 下载 URL（应用镜像）
pub fn get_github_download_url(origin_url: &str) -> String {
    resolve_github_download_target(origin_url).url
}

/// 获取 GitHub 下载源名称（用于显示）
pub fn get_github_download_source_name() -> String {
    let mirror = {
        let config = CONFIG.read();
        config.setting.network.github_download_mirror.clone()
    };

    github_source_name_from_mirror(&mirror)
}

/// 获取最终 URL 的下载源名称（通用）
pub fn get_download_source_name(origin_url: &str) -> String {
    let (github_api_mode, ryujinx_mirror, eden_mirror) = {
        let config = CONFIG.read();
        let network = &config.setting.network;
        (
            network.github_api_mode.clone(),
            network.ryujinx_git_lab_download_mirror.clone(),
            network.eden_git_download_mirror.clone(),
        )
    };

    if origin_url.starts_with("https://api.github.com")
        || origin_url.starts_with("https://raw.githubusercontent.com")
    {
        get_source_name_by_mode(&github_api_mode, "GitHub")
    } else if origin_url.starts_with("https://git.ryujinx.app") {
        get_source_name_by_mode(&ryujinx_mirror, "Ryujinx GitLab")
    } else if origin_url.starts_with("https://git.eden-emu.dev") {
        get_source_name_by_mode(&eden_mirror, "Eden 官方源")
    } else if origin_url.starts_with("https://github.com") {
        get_github_download_source_name()
    } else {
        "直连".to_string()
    }
}

/// 根据模式获取源名称
fn get_source_name_by_mode(mode: &str, default_name: &str) -> String {
    match mode {
        "direct" => "直连".to_string(),
        "cdn" => format!("{} CDN", default_name),
        _ => format!("{} 自动检测", default_name),
    }
}

/// 根据网络设置获取最终 URL
pub fn get_final_url(origin_url: &str) -> String {
    let (github_api_mode, ryujinx_mirror, eden_mirror) = {
        let config = CONFIG.read();
        let network = &config.setting.network;
        (
            network.github_api_mode.clone(),
            network.ryujinx_git_lab_download_mirror.clone(),
            network.eden_git_download_mirror.clone(),
        )
    };

    if origin_url.starts_with("https://api.github.com")
        || origin_url.starts_with("https://raw.githubusercontent.com")
    {
        get_final_url_with_mode(origin_url, &github_api_mode)
    } else if origin_url.starts_with("https://git.ryujinx.app") {
        get_final_url_with_mode(origin_url, &ryujinx_mirror)
    } else if origin_url.starts_with("https://git.eden-emu.dev") {
        get_final_url_with_mode(origin_url, &eden_mirror)
    } else {
        origin_url.to_string()
    }
}

/// 根据模式获取最终 URL
fn get_final_url_with_mode(origin_url: &str, mode: &str) -> String {
    match mode {
        "direct" => {
            debug!("直连模式下使用原始链接：{}", origin_url);
            origin_url.to_string()
        }
        "cdn" => get_override_url(origin_url),
        _ => {
            // auto-detect 模式
            if is_using_proxy() {
                debug!("检测到代理，使用原始链接：{}", origin_url);
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
            debug!("使用 CDN 链接：{}", new_url);
            return new_url;
        }
    }
    debug!("没有可用的 CDN，使用原始链接：{}", origin_url);
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

    let direct_allowed = github_api_direct_request_allowed();
    debug!("GitHub API direct allowed: {}", direct_allowed);

    if github_api_mode != "cdn" && direct_allowed {
        info!("尝试直连 GitHub API");
        let cached_client = get_cached_client();
        match cached_client.get(url).send().await {
            Ok(resp) => {
                let status = resp.status();
                let headers = resp.headers().clone();
                debug!("直连请求成功，HTTP 状态: {}", status);

                match resp.json::<serde_json::Value>().await {
                    Ok(data) => {
                        let message = data
                            .get("message")
                            .and_then(|value| value.as_str())
                            .map(str::to_string);
                        let rate_limited = message
                            .as_deref()
                            .map(|value| value.contains("API rate limit exceeded"))
                            .unwrap_or(false);

                        if status.is_success() && !rate_limited {
                            if let Some((remaining, cooldown)) =
                                github_rate_limit_cooldown(&headers)
                            {
                                open_github_api_circuit_for(
                                    cooldown,
                                    format!(
                                        "GitHub API 剩余额度过低（remaining={}），在 reset 前主动切换到 CDN",
                                        remaining
                                    ),
                                );
                            } else {
                                close_github_api_circuit();
                            }
                            debug!("GitHub API 响应成功");
                            return Ok(data);
                        }

                        let reason = match message {
                            Some(message) => {
                                format!("GitHub API 直连返回 HTTP {}: {}", status, message)
                            }
                            None => format!("GitHub API 直连返回 HTTP {}", status),
                        };

                        if let Some((remaining, cooldown)) = github_rate_limit_cooldown(&headers) {
                            open_github_api_circuit_for(
                                cooldown,
                                format!("{}；剩余额度 {}", reason, remaining),
                            );
                        } else {
                            open_github_api_circuit(reason);
                        }
                    }
                    Err(e) => {
                        open_github_api_circuit(format!("解析 GitHub API 直连响应失败: {}", e));
                    }
                }
            }
            Err(e) => {
                open_github_api_circuit(format!("GitHub API 直连请求失败: {}", e));
            }
        }
    }

    // 使用 CDN 和持久化缓存
    let cdn_url = get_override_url(url);
    info!("使用 CDN 请求 GitHub API: {}", cdn_url);
    let durable_cached_client = get_durable_cached_client();
    let resp = durable_cached_client
        .get(&cdn_url)
        .send()
        .await
        .map_err(|e| {
            warn!("CDN 请求失败: {}", e);
            AppError::Network(format!("GitHub API CDN 请求失败: {}", e))
        })?;
    let status = resp.status();
    debug!("CDN 请求成功，HTTP 状态: {}", status);
    if !status.is_success() {
        warn!("GitHub API CDN 请求失败，HTTP 状态: {}", status);
        return Err(AppError::Network(format!(
            "GitHub API CDN 请求失败: {} - {}",
            status, cdn_url
        )));
    }
    let data = resp.json::<serde_json::Value>().await.map_err(|e| {
        warn!("解析 CDN 响应失败: {}", e);
        AppError::Network(format!("解析 GitHub API CDN 响应失败: {}", e))
    })?;
    debug!("CDN GitHub API 响应成功");
    Ok(data)
}

/// 请求 Git 托管平台 API（GitLab/Forgejo）
/// 用于 git.ryujinx.app 和 git.eden-emu.dev
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
    let final_url = get_final_url(url);
    debug!("发送 GET 请求到 Git API: {}", final_url);
    let resp = client.get(&final_url).send().await.map_err(|e| {
        warn!("Git API 请求失败: {}", e);
        AppError::Network(format!("Git API 请求失败: {}", e))
    })?;

    debug!("Git API 响应状态: {}", resp.status());

    if !resp.status().is_success() {
        warn!("Git API 请求失败，HTTP 状态: {}", resp.status());
        return Err(AppError::Network(format!(
            "Git API 请求失败: {} - {}",
            resp.status(),
            final_url
        )));
    }

    let data = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e| AppError::Network(format!("解析 Git API 响应失败: {}", e)))?;

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
    fn test_get_override_url_for_git_hosts() {
        let ryujinx_url = "https://git.ryujinx.app/api/v4/projects/1/releases";
        assert_eq!(
            get_override_url(ryujinx_url),
            "https://nsarchive.e6ex.com/ryujinx_official/api/v4/projects/1/releases"
        );

        let eden_url = "https://git.eden-emu.dev/api/v1/repos/eden-emu/eden/releases";
        assert_eq!(
            get_override_url(eden_url),
            "https://nsarchive.e6ex.com/eden_official/api/v1/repos/eden-emu/eden/releases"
        );
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
    fn test_durable_cache_dir_uses_config_directory() {
        let cache_dir = durable_cache_dir_path();

        assert_eq!(
            cache_dir,
            crate::config::effective_config_dir().join("http-cacache")
        );
    }

    #[test]
    fn test_circuit_breaker_recovers_after_ttl() {
        let now = Instant::now();
        let mut breaker = GithubApiCircuitBreaker::default();

        assert!(breaker.allows_direct_request(now));

        breaker.record_failure(now, GITHUB_API_FALLBACK_TTL, "timeout");
        assert!(breaker.is_open_at(now + Duration::from_secs(1)));
        assert!(!breaker.allows_direct_request(now + Duration::from_secs(1)));
        assert!(breaker.allows_direct_request(now + GITHUB_API_FALLBACK_TTL));
    }

    #[test]
    fn test_github_rate_limit_cooldown_uses_reset_header() {
        let reset_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 30;
        let mut headers = HeaderMap::new();
        headers.insert("x-ratelimit-remaining", "1".parse().unwrap());
        headers.insert("x-ratelimit-reset", reset_at.to_string().parse().unwrap());

        let (remaining, cooldown) = github_rate_limit_cooldown(&headers).unwrap();

        assert_eq!(remaining, 1);
        assert!(cooldown.as_secs() >= 1);
        assert!(cooldown.as_secs() <= 30);
    }

    #[test]
    fn test_resolve_github_download_target_direct_mode() {
        let origin = "https://github.com/example/project/releases/download/v1.0.0/app.zip";
        let target = resolve_github_download_target_with_mirror(origin, "direct");

        assert_eq!(target.url, origin);
        assert_eq!(target.source_name, "直连");
    }

    #[test]
    fn test_resolve_github_download_target_custom_mirror() {
        let origin = "https://github.com/example/project/releases/download/v1.0.0/app.zip";
        let target = resolve_github_download_target_with_mirror(
            origin,
            "https://gh-proxy.org/https://github.com",
        );

        assert_eq!(
            target.url,
            "https://gh-proxy.org/https://github.com/example/project/releases/download/v1.0.0/app.zip"
        );
        assert!(target.source_name.contains("gh-proxy.com"));
    }

    #[test]
    fn test_resolve_github_download_target_load_balance_uses_specific_source() {
        let origin = "https://github.com/example/project/releases/download/v1.0.0/app.zip";
        let target = resolve_github_download_target_with_mirror(origin, "cloudflare_load_balance");

        assert_ne!(target.url, origin);
        assert_ne!(target.source_name, "Cloudflare CDN 负载均衡");
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

        let resp = client.head(url).send().await.map_err(|e| e.to_string())?;

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

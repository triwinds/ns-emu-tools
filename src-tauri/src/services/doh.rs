//! DNS over HTTPS (DoH) 模块
//!
//! 使用 hickory-resolver 提供 DNS over HTTPS 解析功能，并可直接接入 reqwest。

use crate::config::CONFIG;
use crate::error::{AppError, AppResult};
use hickory_proto::xfer::Protocol;
use hickory_resolver::config::{LookupIpStrategy, NameServerConfig, ResolverConfig, ResolverOpts};
use hickory_resolver::name_server::TokioConnectionProvider;
use hickory_resolver::{Resolver, TokioResolver};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use reqwest::dns::{
    Addrs as ReqwestAddrs, Name as ReqwestName, Resolve as ReqwestResolve, Resolving,
};
use reqwest::ClientBuilder as ReqwestClientBuilder;
use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

const DEFAULT_DOH_SERVERS: &[(&str, &str)] = &[
    ("223.5.5.5:443", "dns.alidns.com"),
    ("223.6.6.6:443", "dns.alidns.com"),
    ("1.1.1.1:443", "cloudflare-dns.com"),
    ("1.0.0.1:443", "cloudflare-dns.com"),
];

const DEFAULT_DOH_URLS: &[&str] = &[
    "https://dns.alidns.com/dns-query",
    "https://cloudflare-dns.com/dns-query",
];

/// DNS 缓存项
#[derive(Debug, Clone)]
struct DnsCacheItem {
    /// 过期时间
    expire_at: Instant,
    /// IP 地址列表
    addresses: Vec<IpAddr>,
}

/// DNS 缓存
static DNS_CACHE: Lazy<RwLock<HashMap<String, DnsCacheItem>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// DoH 解析器
#[derive(Clone)]
pub struct DohResolver {
    resolver: TokioResolver,
    try_ipv6: bool,
}

fn doh_enabled() -> bool {
    let config = CONFIG.read();
    config.setting.network.use_doh
}

fn try_ipv6_enabled() -> bool {
    let config = CONFIG.read();
    !config.setting.download.disable_aria2_ipv6
}

fn resolver_ip_strategy(try_ipv6: bool) -> LookupIpStrategy {
    if try_ipv6 {
        LookupIpStrategy::Ipv4AndIpv6
    } else {
        LookupIpStrategy::Ipv4Only
    }
}

fn build_name_server(socket_addr: SocketAddr, tls_dns_name: impl Into<String>) -> NameServerConfig {
    NameServerConfig {
        socket_addr,
        protocol: Protocol::Https,
        tls_dns_name: Some(tls_dns_name.into()),
        trust_negative_responses: true,
        bind_addr: None,
        http_endpoint: None,
    }
}

fn build_resolver_from_config(resolver_config: ResolverConfig, try_ipv6: bool) -> TokioResolver {
    let mut opts = ResolverOpts::default();
    opts.timeout = Duration::from_secs(5);
    opts.attempts = 2;
    opts.cache_size = 256;
    opts.ip_strategy = resolver_ip_strategy(try_ipv6);

    Resolver::builder_with_config(resolver_config, TokioConnectionProvider::default())
        .with_options(opts)
        .build()
}

fn build_default_resolver_config() -> AppResult<ResolverConfig> {
    let mut resolver_config = ResolverConfig::new();
    for (socket_addr, tls_dns_name) in default_doh_servers()? {
        resolver_config.add_name_server(build_name_server(socket_addr, tls_dns_name));
    }
    Ok(resolver_config)
}

pub fn default_doh_servers() -> AppResult<Vec<(SocketAddr, String)>> {
    DEFAULT_DOH_SERVERS
        .iter()
        .map(|(socket_addr, tls_dns_name)| {
            let socket_addr = socket_addr.parse().map_err(|error| {
                AppError::Config(format!(
                    "解析 DoH 服务器地址失败: {} - {}",
                    socket_addr, error
                ))
            })?;
            Ok((socket_addr, (*tls_dns_name).to_string()))
        })
        .collect::<AppResult<Vec<_>>>()
}

pub fn default_doh_urls() -> Vec<String> {
    DEFAULT_DOH_URLS
        .iter()
        .map(|url| (*url).to_string())
        .collect()
}

impl DohResolver {
    /// 创建新的 DoH 解析器（默认先走阿里云，再回退 Cloudflare）
    pub fn new() -> AppResult<Self> {
        let try_ipv6 = try_ipv6_enabled();
        Ok(Self {
            resolver: build_resolver_from_config(build_default_resolver_config()?, try_ipv6),
            try_ipv6,
        })
    }

    /// 使用阿里云 DoH 创建解析器
    pub fn with_alidns() -> AppResult<Self> {
        let mut resolver_config = ResolverConfig::new();
        for (socket_addr, tls_dns_name) in default_doh_servers()?
            .into_iter()
            .filter(|(_, tls_dns_name)| tls_dns_name == "dns.alidns.com")
        {
            resolver_config.add_name_server(build_name_server(socket_addr, tls_dns_name));
        }

        let try_ipv6 = try_ipv6_enabled();
        Ok(Self {
            resolver: build_resolver_from_config(resolver_config, try_ipv6),
            try_ipv6,
        })
    }

    /// 使用 Cloudflare DoH 创建解析器
    pub fn with_cloudflare() -> AppResult<Self> {
        let try_ipv6 = try_ipv6_enabled();
        Ok(Self {
            resolver: build_resolver_from_config(ResolverConfig::cloudflare_https(), try_ipv6),
            try_ipv6,
        })
    }

    /// 使用 Google DoH 创建解析器
    pub fn with_google() -> AppResult<Self> {
        let try_ipv6 = try_ipv6_enabled();
        Ok(Self {
            resolver: build_resolver_from_config(ResolverConfig::google_https(), try_ipv6),
            try_ipv6,
        })
    }

    /// 使用 Quad9 DoH 创建解析器
    pub fn with_quad9() -> AppResult<Self> {
        let try_ipv6 = try_ipv6_enabled();
        Ok(Self {
            resolver: build_resolver_from_config(ResolverConfig::quad9_https(), try_ipv6),
            try_ipv6,
        })
    }

    /// 解析域名
    pub async fn resolve(&self, hostname: &str) -> AppResult<Vec<IpAddr>> {
        if let Ok(ip) = hostname.parse::<IpAddr>() {
            return Ok(vec![ip]);
        }

        if let Some(cached) = self.get_from_cache(hostname) {
            debug!("DNS 缓存命中: {} -> {:?}", hostname, cached);
            return Ok(cached);
        }

        let mut addresses = Vec::new();

        match self.resolver.lookup_ip(hostname).await {
            Ok(response) => {
                for addr in response.iter() {
                    if self.try_ipv6 || addr.is_ipv4() {
                        addresses.push(addr);
                    }
                }
            }
            Err(e) => {
                warn!("DNS 解析失败: {} - {}", hostname, e);
                return Err(AppError::Unknown(format!("DNS 解析失败: {}", e)));
            }
        }

        if addresses.is_empty() {
            return Err(AppError::Unknown(format!(
                "DNS 解析失败: {} 没有找到有效地址",
                hostname
            )));
        }

        debug!("DoH 解析: {} -> {:?}", hostname, addresses);
        self.update_cache(hostname, addresses.clone(), Duration::from_secs(300));

        Ok(addresses)
    }

    fn get_from_cache(&self, hostname: &str) -> Option<Vec<IpAddr>> {
        let cache = DNS_CACHE.read();
        if let Some(item) = cache.get(hostname) {
            if item.expire_at > Instant::now() {
                return Some(item.addresses.clone());
            }
        }
        None
    }

    fn update_cache(&self, hostname: &str, addresses: Vec<IpAddr>, ttl: Duration) {
        let mut cache = DNS_CACHE.write();
        cache.insert(
            hostname.to_string(),
            DnsCacheItem {
                expire_at: Instant::now() + ttl,
                addresses,
            },
        );
    }
}

impl ReqwestResolve for DohResolver {
    fn resolve(&self, name: ReqwestName) -> Resolving {
        let resolver = self.clone();
        let hostname = name.as_str().to_string();

        Box::pin(async move {
            let addresses = DohResolver::resolve(&resolver, &hostname)
                .await
                .map_err(|error| {
                    Box::new(io::Error::new(io::ErrorKind::Other, error.to_string()))
                        as Box<dyn std::error::Error + Send + Sync>
                })?;

            let socket_addrs: Vec<SocketAddr> = addresses
                .into_iter()
                .map(|address| SocketAddr::new(address, 0))
                .collect();

            Ok(Box::new(socket_addrs.into_iter()) as ReqwestAddrs)
        })
    }
}

pub fn configure_reqwest_client_builder(
    builder: ReqwestClientBuilder,
) -> AppResult<ReqwestClientBuilder> {
    if !doh_enabled() {
        return Ok(builder);
    }

    debug!("为 HTTP 客户端启用 DoH 解析器");
    Ok(builder.dns_resolver2(DohResolver::new()?))
}

/// 清除 DNS 缓存
pub fn clear_dns_cache() {
    let mut cache = DNS_CACHE.write();
    cache.clear();
    info!("DNS 缓存已清除");
}

/// 获取缓存统计
pub fn get_cache_stats() -> (usize, usize) {
    let cache = DNS_CACHE.read();
    let total = cache.len();
    let valid = cache
        .values()
        .filter(|item| item.expire_at > Instant::now())
        .count();
    (total, valid)
}

/// 预热 DNS 缓存
pub async fn warmup_dns_cache(domains: &[&str]) -> AppResult<()> {
    let resolver = DohResolver::new()?;

    for domain in domains {
        match resolver.resolve(domain).await {
            Ok(addrs) => {
                info!("DNS 预热成功: {} -> {:?}", domain, addrs);
            }
            Err(e) => {
                warn!("DNS 预热失败: {} - {}", domain, e);
            }
        }
    }

    Ok(())
}

/// 常用域名列表（用于预热）
pub const COMMON_DOMAINS: &[&str] = &[
    "api.github.com",
    "github.com",
    "nsarchive.e6ex.com",
    "cfrp.e6ex.com",
    "git.ryujinx.app",
    "git.eden-emu.dev",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_doh_servers_parse() {
        let servers = default_doh_servers().unwrap();
        assert!(servers.len() >= 2);
        assert_eq!(servers[0].0, "223.5.5.5:443".parse().unwrap());
        assert_eq!(servers[0].1, "dns.alidns.com");
    }

    #[test]
    fn test_default_doh_urls() {
        let urls = default_doh_urls();
        assert!(urls.contains(&"https://dns.alidns.com/dns-query".to_string()));
        assert!(urls.contains(&"https://cloudflare-dns.com/dns-query".to_string()));
    }

    #[tokio::test]
    async fn test_doh_resolve() {
        let resolver = DohResolver::new().unwrap();
        let result = resolver.resolve("github.com").await;
        println!("DoH resolve result: {:?}", result);
        assert!(result.is_ok(), "DoH resolve failed: {:?}", result.err());
        let addresses = result.unwrap();
        assert!(!addresses.is_empty());
    }

    #[tokio::test]
    async fn test_doh_resolve_alidns() {
        let resolver = DohResolver::with_alidns().unwrap();
        let result = resolver.resolve("github.com").await;
        println!("AliDNS DoH resolve result: {:?}", result);
        assert!(
            result.is_ok(),
            "AliDNS DoH resolve failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    #[ignore] // 需要网络连接
    async fn test_doh_resolve_google() {
        let resolver = DohResolver::with_google().unwrap();
        let result = resolver.resolve("github.com").await;
        println!("Google DoH resolve result: {:?}", result);
        assert!(
            result.is_ok(),
            "Google DoH resolve failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_ip_address_bypass() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let resolver = DohResolver::new().unwrap();
            let result = resolver.resolve("127.0.0.1").await.unwrap();
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].to_string(), "127.0.0.1");
        });
    }

    #[test]
    fn test_cache_stats() {
        let (total, valid) = get_cache_stats();
        assert!(total >= valid);
    }
}

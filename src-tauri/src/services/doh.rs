//! DNS over HTTPS (DoH) 模块
//!
//! 使用 hickory-resolver 提供 DNS over HTTPS 解析功能

use crate::config::CONFIG;
use crate::error::{AppError, AppResult};
use hickory_resolver::config::{NameServerConfig, ResolverConfig, ResolverOpts};
use hickory_resolver::name_server::TokioConnectionProvider;
use hickory_resolver::{Resolver, TokioResolver};
use hickory_proto::xfer::Protocol;
use std::net::SocketAddr;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

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
pub struct DohResolver {
    resolver: TokioResolver,
    try_ipv6: bool,
}

impl DohResolver {
    /// 创建新的 DoH 解析器（默认使用阿里云 DoH）
    pub fn new() -> AppResult<Self> {
        Self::with_alidns()
    }

    /// 使用阿里云 DoH 创建解析器
    pub fn with_alidns() -> AppResult<Self> {
        let config = CONFIG.read();
        let try_ipv6 = !config.setting.download.disable_aria2_ipv6;
        drop(config);

        // 阿里云 DoH: https://dns.alidns.com/dns-query
        // IPv4: 223.5.5.5, 223.6.6.6
        let mut resolver_config = ResolverConfig::new();

        // 添加阿里云 DNS 服务器
        let alidns_addr: SocketAddr = "223.5.5.5:443".parse().unwrap();
        let ns_config = NameServerConfig {
            socket_addr: alidns_addr,
            protocol: Protocol::Https,
            tls_dns_name: Some("dns.alidns.com".to_string()),
            trust_negative_responses: true,
            bind_addr: None,
            http_endpoint: None,
        };
        resolver_config.add_name_server(ns_config);

        // 添加备用阿里云 DNS 服务器
        let alidns_addr2: SocketAddr = "223.6.6.6:443".parse().unwrap();
        let ns_config2 = NameServerConfig {
            socket_addr: alidns_addr2,
            protocol: Protocol::Https,
            tls_dns_name: Some("dns.alidns.com".to_string()),
            trust_negative_responses: true,
            bind_addr: None,
            http_endpoint: None,
        };
        resolver_config.add_name_server(ns_config2);

        let mut opts = ResolverOpts::default();
        opts.timeout = Duration::from_secs(5);
        opts.attempts = 2;
        opts.cache_size = 256;

        let resolver = Resolver::builder_with_config(resolver_config, TokioConnectionProvider::default())
            .with_options(opts)
            .build();

        Ok(Self { resolver, try_ipv6 })
    }

    /// 使用 Cloudflare DoH 创建解析器
    pub fn with_cloudflare() -> AppResult<Self> {
        let config = CONFIG.read();
        let try_ipv6 = !config.setting.download.disable_aria2_ipv6;
        drop(config);

        let resolver_config = ResolverConfig::cloudflare_https();

        let mut opts = ResolverOpts::default();
        opts.timeout = Duration::from_secs(5);
        opts.attempts = 2;
        opts.cache_size = 256;

        let resolver = Resolver::builder_with_config(resolver_config, TokioConnectionProvider::default())
            .with_options(opts)
            .build();

        Ok(Self { resolver, try_ipv6 })
    }

    /// 使用 Google DoH 创建解析器
    pub fn with_google() -> AppResult<Self> {
        let config = CONFIG.read();
        let try_ipv6 = !config.setting.download.disable_aria2_ipv6;
        drop(config);

        let resolver_config = ResolverConfig::google_https();

        let mut opts = ResolverOpts::default();
        opts.timeout = Duration::from_secs(5);
        opts.attempts = 2;
        opts.cache_size = 256;

        let resolver = Resolver::builder_with_config(resolver_config, TokioConnectionProvider::default())
            .with_options(opts)
            .build();

        Ok(Self { resolver, try_ipv6 })
    }

    /// 使用 Quad9 DoH 创建解析器
    pub fn with_quad9() -> AppResult<Self> {
        let config = CONFIG.read();
        let try_ipv6 = !config.setting.download.disable_aria2_ipv6;
        drop(config);

        let resolver_config = ResolverConfig::quad9_https();

        let mut opts = ResolverOpts::default();
        opts.timeout = Duration::from_secs(5);
        opts.attempts = 2;
        opts.cache_size = 256;

        let resolver = Resolver::builder_with_config(resolver_config, TokioConnectionProvider::default())
            .with_options(opts)
            .build();

        Ok(Self { resolver, try_ipv6 })
    }

    /// 解析域名
    pub async fn resolve(&self, hostname: &str) -> AppResult<Vec<IpAddr>> {
        // 检查是否是 IP 地址
        if let Ok(ip) = hostname.parse::<IpAddr>() {
            return Ok(vec![ip]);
        }

        // 检查缓存
        if let Some(cached) = self.get_from_cache(hostname) {
            debug!("DNS 缓存命中: {} -> {:?}", hostname, cached);
            return Ok(cached);
        }

        let mut addresses = Vec::new();

        // 查询 DNS
        match self.resolver.lookup_ip(hostname).await {
            Ok(response) => {
                for addr in response.iter() {
                    // 根据配置决定是否包含 IPv6
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

        // 更新缓存（TTL 默认 5 分钟）
        self.update_cache(hostname, addresses.clone(), Duration::from_secs(300));

        Ok(addresses)
    }

    /// 从缓存获取
    fn get_from_cache(&self, hostname: &str) -> Option<Vec<IpAddr>> {
        let cache = DNS_CACHE.read();
        if let Some(item) = cache.get(hostname) {
            if item.expire_at > Instant::now() {
                return Some(item.addresses.clone());
            }
        }
        None
    }

    /// 更新缓存
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
    "git.citron-emu.org",
    "git.ryujinx.app",
];

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(result.is_ok(), "AliDNS DoH resolve failed: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_doh_resolve_google() {
        let resolver = DohResolver::with_google().unwrap();
        let result = resolver.resolve("github.com").await;
        println!("Google DoH resolve result: {:?}", result);
        assert!(result.is_ok(), "Google DoH resolve failed: {:?}", result.err());
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

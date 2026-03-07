//! HTTP Client 配置和构建
//!
//! 为 RustDownloader 提供 HTTP 客户端配置，复用 services::network 的代理/UA 规则

use crate::config::user_agent;
use crate::error::{AppError, AppResult};
use crate::services::network::{get_proxy_url, CHROME_UA};
use reqwest::{Client, Proxy};
use std::time::Duration;
use tracing::debug;

/// HTTP Client 配置
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// 连接超时，默认 30s
    pub connect_timeout: Duration,
    /// 读取超时，默认 60s
    pub read_timeout: Duration,
    /// 最大重定向次数，默认 10
    pub max_redirects: usize,
    /// 连接池空闲超时，默认 90s
    pub pool_idle_timeout: Duration,
    /// 每个 host 最大空闲连接数，默认 10
    pub pool_max_idle_per_host: usize,
    /// 是否接受无效证书（仅调试用）
    pub danger_accept_invalid_certs: bool,
    /// 自定义 User-Agent
    pub user_agent: Option<String>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(30),
            read_timeout: Duration::from_secs(60),
            max_redirects: 10,
            pool_idle_timeout: Duration::from_secs(90),
            pool_max_idle_per_host: 10,
            danger_accept_invalid_certs: false,
            user_agent: None,
        }
    }
}

impl ClientConfig {
    /// 创建用于下载的配置
    pub fn for_download() -> Self {
        Self {
            connect_timeout: Duration::from_secs(30),
            read_timeout: Duration::from_secs(120), // 下载时读取超时更长
            max_redirects: 10,
            pool_idle_timeout: Duration::from_secs(90),
            pool_max_idle_per_host: 16, // 支持更多并发连接
            danger_accept_invalid_certs: false,
            user_agent: None,
        }
    }

    /// 创建用于探测的配置（短超时）
    pub fn for_probe() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(30),
            max_redirects: 10,
            pool_idle_timeout: Duration::from_secs(30),
            pool_max_idle_per_host: 4,
            danger_accept_invalid_certs: false,
            user_agent: None,
        }
    }

    /// 设置自定义 User-Agent
    pub fn with_user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }

    /// 使用 Chrome User-Agent
    pub fn with_chrome_ua(mut self) -> Self {
        self.user_agent = Some(CHROME_UA.to_string());
        self
    }
}

/// 构建 HTTP 客户端
///
/// 自动应用代理配置（如果有）
pub fn build_client(config: &ClientConfig) -> AppResult<Client> {
    debug!("构建 HTTP 客户端，配置: {:?}", config);

    let ua = config
        .user_agent
        .clone()
        .unwrap_or_else(|| user_agent().to_string());

    let mut builder = Client::builder()
        .user_agent(&ua)
        .connect_timeout(config.connect_timeout)
        .read_timeout(config.read_timeout)
        .redirect(reqwest::redirect::Policy::limited(config.max_redirects))
        .pool_idle_timeout(config.pool_idle_timeout)
        .pool_max_idle_per_host(config.pool_max_idle_per_host);

    // 代理配置：复用 services::network::get_proxy_url()
    if let Some(proxy_url) = get_proxy_url() {
        if !proxy_url.is_empty() {
            debug!("使用代理: {}", proxy_url);
            let proxy = Proxy::all(&proxy_url)
                .map_err(|e| AppError::Network(format!("无效的代理 URL: {}", e)))?;
            builder = builder.proxy(proxy);
        }
    }

    if config.danger_accept_invalid_certs {
        debug!("警告: 接受无效证书");
        builder = builder.danger_accept_invalid_certs(true);
    }

    builder
        .build()
        .map_err(|e| AppError::Network(format!("构建 HTTP 客户端失败: {}", e)))
}

/// 构建默认下载客户端
pub fn build_download_client() -> AppResult<Client> {
    build_client(&ClientConfig::for_download())
}

/// 构建探测客户端（用于检测 Range 支持等）
pub fn build_probe_client() -> AppResult<Client> {
    build_client(&ClientConfig::for_probe())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_config_default() {
        let config = ClientConfig::default();

        assert_eq!(config.connect_timeout, Duration::from_secs(30));
        assert_eq!(config.read_timeout, Duration::from_secs(60));
        assert_eq!(config.max_redirects, 10);
        assert!(!config.danger_accept_invalid_certs);
        assert!(config.user_agent.is_none());
    }

    #[test]
    fn test_client_config_for_download() {
        let config = ClientConfig::for_download();

        assert_eq!(config.read_timeout, Duration::from_secs(120));
        assert_eq!(config.pool_max_idle_per_host, 16);
    }

    #[test]
    fn test_client_config_for_probe() {
        let config = ClientConfig::for_probe();

        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.read_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_client_config_with_user_agent() {
        let config = ClientConfig::default().with_user_agent("TestAgent/1.0");

        assert_eq!(config.user_agent, Some("TestAgent/1.0".to_string()));
    }

    #[test]
    fn test_client_config_with_chrome_ua() {
        let config = ClientConfig::default().with_chrome_ua();

        assert_eq!(config.user_agent, Some(CHROME_UA.to_string()));
    }

    #[test]
    fn test_build_client() {
        let config = ClientConfig::default();
        let result = build_client(&config);

        assert!(result.is_ok());
    }

    #[test]
    fn test_build_download_client() {
        let result = build_download_client();

        assert!(result.is_ok());
    }

    #[test]
    fn test_build_probe_client() {
        let result = build_probe_client();

        assert!(result.is_ok());
    }
}

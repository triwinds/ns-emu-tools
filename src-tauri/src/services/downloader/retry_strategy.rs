//! 智能重试策略
//!
//! 提供错误分类、指数退避、网络感知等重试机制

use crate::error::AppError;
use rand::Rng;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// 错误类别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// 临时错误（超时、连接重置等）- 可重试
    Temporary,
    /// 永久错误（404、403等）- 不重试
    Permanent,
    /// 网络不可用 - 等待网络恢复
    NetworkUnavailable,
    /// 限流（429/Retry-After）
    RateLimited,
    /// SSL/TLS 错误 - 可能需要用户干预
    SslError,
    /// DNS 解析失败 - 可重试
    DnsError,
    /// 磁盘错误（空间不足/权限问题）- 不重试
    DiskError,
}

/// 重试动作
#[derive(Debug, Clone)]
pub enum RetryAction {
    /// 等待网络恢复
    WaitForNetwork {
        timeout: Duration,
        retry_num: u32,
        max_retries: u32,
    },
    /// 休眠指定时间
    Sleep {
        duration: Duration,
        retry_num: u32,
        max_retries: u32,
        reason: String,
    },
}

/// 重试策略
pub struct RetryStrategy {
    /// 最大重试次数
    max_retries: u32,
    /// 当前重试次数
    current_retry: u32,
    /// 基础退避时间（秒）
    base_backoff: u64,
    /// 最大退避时间（秒）
    max_backoff: u64,
    /// 是否启用 jitter
    enable_jitter: bool,
}

impl RetryStrategy {
    /// 创建新的重试策略
    ///
    /// # 参数
    /// - `max_retries`: 最大重试次数（默认 5）
    pub fn new(max_retries: u32) -> Self {
        Self {
            max_retries,
            current_retry: 0,
            base_backoff: 1,
            max_backoff: 60,
            enable_jitter: true,
        }
    }

    /// 创建默认重试策略
    pub fn default() -> Self {
        Self::new(5)
    }

    /// 错误分类
    pub fn categorize_error(error: &AppError) -> ErrorCategory {
        match error {
            // 网络错误分类
            AppError::Network(msg) => {
                let msg_lower = msg.to_lowercase();

                // 检查 HTTP 状态码
                if msg_lower.contains("404") || msg_lower.contains("not found") {
                    return ErrorCategory::Permanent;
                }
                if msg_lower.contains("403") || msg_lower.contains("forbidden") {
                    return ErrorCategory::Permanent;
                }
                if msg_lower.contains("401") || msg_lower.contains("unauthorized") {
                    return ErrorCategory::Permanent;
                }
                if msg_lower.contains("429") || msg_lower.contains("too many requests") {
                    return ErrorCategory::RateLimited;
                }
                if msg_lower.contains("500") || msg_lower.contains("502") || msg_lower.contains("503") {
                    return ErrorCategory::Temporary;
                }

                // 检查网络错误类型
                if msg_lower.contains("timeout") || msg_lower.contains("connection reset") {
                    ErrorCategory::Temporary
                } else if msg_lower.contains("certificate")
                    || msg_lower.contains("ssl")
                    || msg_lower.contains("tls")
                {
                    ErrorCategory::SslError
                } else if msg_lower.contains("dns") || msg_lower.contains("resolve") {
                    ErrorCategory::DnsError
                } else if msg_lower.contains("network") || msg_lower.contains("unreachable") {
                    ErrorCategory::NetworkUnavailable
                } else {
                    ErrorCategory::Temporary
                }
            }

            // IO 错误分类
            AppError::Io(io_err) => match io_err.kind() {
                std::io::ErrorKind::PermissionDenied => ErrorCategory::DiskError,
                std::io::ErrorKind::StorageFull => ErrorCategory::DiskError,
                _ => ErrorCategory::Temporary,
            },

            // 下载错误
            AppError::Download(msg) => {
                let msg_lower = msg.to_lowercase();
                if msg_lower.contains("disk space") || msg_lower.contains("permission") {
                    ErrorCategory::DiskError
                } else {
                    ErrorCategory::Temporary
                }
            }

            // 其他错误默认为临时错误
            _ => ErrorCategory::Temporary,
        }
    }

    /// 判断是否应该重试
    pub fn should_retry(&self, error: &AppError) -> bool {
        if self.current_retry >= self.max_retries {
            return false;
        }

        match Self::categorize_error(error) {
            ErrorCategory::Permanent | ErrorCategory::DiskError | ErrorCategory::SslError => false,
            _ => true,
        }
    }

    /// 计算退避延迟
    ///
    /// 使用指数退避 + jitter 策略
    pub fn backoff_delay(&self) -> Duration {
        // 计算指数退避时间：base * 2^retry
        let exponential = self
            .base_backoff
            .saturating_mul(2u64.pow(self.current_retry));

        // 限制最大退避时间
        let delay_secs = exponential.min(self.max_backoff);

        // 添加 jitter（随机 ±25%）
        let delay_secs = if self.enable_jitter {
            let mut rng = rand::rng();
            let jitter_factor = rng.random_range(0.75..=1.25);
            ((delay_secs as f64) * jitter_factor) as u64
        } else {
            delay_secs
        };

        Duration::from_secs(delay_secs)
    }

    /// 执行重试等待
    ///
    /// 根据错误类别执行不同的等待策略
    pub async fn wait_before_retry(&mut self, error: &AppError) -> bool {
        let category = Self::categorize_error(error);

        match category {
            ErrorCategory::Permanent | ErrorCategory::DiskError | ErrorCategory::SslError => {
                debug!("错误不可重试: {:?}", category);
                return false;
            }

            ErrorCategory::NetworkUnavailable => {
                warn!(
                    "网络不可用，等待网络恢复（重试 {}/{}）",
                    self.current_retry + 1,
                    self.max_retries
                );
                // 等待网络恢复（最多等待 60 秒）
                if !Self::wait_for_network(Duration::from_secs(60)).await {
                    return false;
                }
            }

            ErrorCategory::DnsError => {
                warn!(
                    "DNS 解析失败，等待 10 秒后重试（重试 {}/{}）",
                    self.current_retry + 1,
                    self.max_retries
                );
                sleep(Duration::from_secs(10)).await;
            }

            ErrorCategory::RateLimited => {
                let delay = self.backoff_delay();
                warn!(
                    "触发限流，等待 {:?} 后重试（重试 {}/{}）",
                    delay,
                    self.current_retry + 1,
                    self.max_retries
                );
                sleep(delay).await;
            }

            ErrorCategory::Temporary => {
                let delay = self.backoff_delay();
                info!(
                    "临时错误，等待 {:?} 后重试（重试 {}/{}）",
                    delay,
                    self.current_retry + 1,
                    self.max_retries
                );
                sleep(delay).await;
            }
        }

        self.current_retry += 1;
        true
    }

    /// 准备重试（不持有锁的版本）
    ///
    /// 返回是否应该重试以及需要等待的操作
    pub fn prepare_retry(&self, error: &AppError) -> Option<RetryAction> {
        let category = Self::categorize_error(error);

        match category {
            ErrorCategory::Permanent | ErrorCategory::DiskError | ErrorCategory::SslError => {
                debug!("错误不可重试: {:?}", category);
                None
            }

            ErrorCategory::NetworkUnavailable => {
                Some(RetryAction::WaitForNetwork {
                    timeout: Duration::from_secs(60),
                    retry_num: self.current_retry + 1,
                    max_retries: self.max_retries,
                })
            }

            ErrorCategory::DnsError => {
                Some(RetryAction::Sleep {
                    duration: Duration::from_secs(10),
                    retry_num: self.current_retry + 1,
                    max_retries: self.max_retries,
                    reason: "DNS 解析失败".to_string(),
                })
            }

            ErrorCategory::RateLimited => {
                let delay = self.backoff_delay();
                Some(RetryAction::Sleep {
                    duration: delay,
                    retry_num: self.current_retry + 1,
                    max_retries: self.max_retries,
                    reason: "触发限流".to_string(),
                })
            }

            ErrorCategory::Temporary => {
                let delay = self.backoff_delay();
                Some(RetryAction::Sleep {
                    duration: delay,
                    retry_num: self.current_retry + 1,
                    max_retries: self.max_retries,
                    reason: "临时错误".to_string(),
                })
            }
        }
    }

    /// 增加重试计数
    pub fn increment_retry(&mut self) {
        self.current_retry += 1;
    }


    /// 等待网络恢复
    ///
    /// 每 5 秒检测一次网络连接，直到网络恢复或超时
    pub async fn wait_for_network(timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        let check_interval = Duration::from_secs(5);

        while start.elapsed() < timeout {
            if Self::check_network_available().await {
                info!("网络已恢复");
                return true;
            }
            sleep(check_interval).await;
        }

        warn!("等待网络恢复超时");
        false
    }

    /// 检查网络是否可用
    ///
    /// 尝试连接多个公共 DNS 服务器
    pub async fn check_network_available() -> bool {
        use tokio::net::TcpStream;

        let dns_servers = [
            "8.8.8.8:53",       // Google DNS
            "1.1.1.1:53",       // Cloudflare DNS
            "223.5.5.5:53",     // 阿里 DNS
            "114.114.114.114:53", // 114 DNS
        ];

        for server in &dns_servers {
            match tokio::time::timeout(Duration::from_secs(3), TcpStream::connect(server)).await {
                Ok(Ok(_)) => {
                    debug!("网络连接正常（通过 {}）", server);
                    return true;
                }
                Ok(Err(e)) => {
                    debug!("连接 {} 失败: {}", server, e);
                }
                Err(_) => {
                    debug!("连接 {} 超时", server);
                }
            }
        }

        false
    }

    /// 重置重试计数器
    pub fn reset(&mut self) {
        self.current_retry = 0;
    }

    /// 获取当前重试次数
    pub fn current_retry(&self) -> u32 {
        self.current_retry
    }

    /// 获取最大重试次数
    pub fn max_retries(&self) -> u32 {
        self.max_retries
    }

    /// 尝试切换镜像（仅 GitHub）
    ///
    /// 如果 URL 是 GitHub 下载链接，尝试切换到镜像
    pub fn try_switch_mirror(&self, url: &str) -> Option<String> {
        // 检查是否是 GitHub URL
        if !url.contains("github.com") && !url.contains("githubusercontent.com") {
            return None;
        }

        // 复用现有的镜像策略
        // 注意：这里需要调用 services::network::get_github_download_url
        // 但由于循环依赖问题，这个功能应该在 RustDownloader 中实现
        debug!("检测到 GitHub URL，建议在上层重新计算镜像 URL");
        None
    }
}

impl Default for RetryStrategy {
    fn default() -> Self {
        Self::new(5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_categorize_http_404() {
        let error = AppError::Network("404 Not Found".to_string());
        assert_eq!(
            RetryStrategy::categorize_error(&error),
            ErrorCategory::Permanent
        );
    }

    #[test]
    fn test_categorize_http_429() {
        let error = AppError::Network("429 Too Many Requests".to_string());
        assert_eq!(
            RetryStrategy::categorize_error(&error),
            ErrorCategory::RateLimited
        );
    }

    #[test]
    fn test_categorize_http_500() {
        let error = AppError::Network("500 Internal Server Error".to_string());
        assert_eq!(
            RetryStrategy::categorize_error(&error),
            ErrorCategory::Temporary
        );
    }

    #[test]
    fn test_categorize_network_timeout() {
        let error = AppError::Network("connection timeout".to_string());
        assert_eq!(
            RetryStrategy::categorize_error(&error),
            ErrorCategory::Temporary
        );
    }

    #[test]
    fn test_categorize_ssl_error() {
        let error = AppError::Network("SSL certificate error".to_string());
        assert_eq!(
            RetryStrategy::categorize_error(&error),
            ErrorCategory::SslError
        );
    }

    #[test]
    fn test_categorize_dns_error() {
        let error = AppError::Network("DNS resolution failed".to_string());
        assert_eq!(
            RetryStrategy::categorize_error(&error),
            ErrorCategory::DnsError
        );
    }

    #[test]
    fn test_categorize_http_403() {
        let error = AppError::Network("403 Forbidden".to_string());
        assert_eq!(
            RetryStrategy::categorize_error(&error),
            ErrorCategory::Permanent
        );
    }

    #[test]
    fn test_categorize_http_401() {
        let error = AppError::Network("401 Unauthorized".to_string());
        assert_eq!(
            RetryStrategy::categorize_error(&error),
            ErrorCategory::Permanent
        );
    }

    #[test]
    fn test_categorize_http_502() {
        let error = AppError::Network("502 Bad Gateway".to_string());
        assert_eq!(
            RetryStrategy::categorize_error(&error),
            ErrorCategory::Temporary
        );
    }

    #[test]
    fn test_categorize_http_503() {
        let error = AppError::Network("503 Service Unavailable".to_string());
        assert_eq!(
            RetryStrategy::categorize_error(&error),
            ErrorCategory::Temporary
        );
    }

    #[test]
    fn test_categorize_connection_reset() {
        let error = AppError::Network("connection reset by peer".to_string());
        assert_eq!(
            RetryStrategy::categorize_error(&error),
            ErrorCategory::Temporary
        );
    }

    #[test]
    fn test_categorize_network_unreachable() {
        let error = AppError::Network("network unreachable".to_string());
        assert_eq!(
            RetryStrategy::categorize_error(&error),
            ErrorCategory::NetworkUnavailable
        );
    }

    #[test]
    fn test_categorize_tls_error() {
        let error = AppError::Network("TLS handshake failed".to_string());
        assert_eq!(
            RetryStrategy::categorize_error(&error),
            ErrorCategory::SslError
        );
    }

    #[test]
    fn test_categorize_io_permission_denied() {
        let error = AppError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "permission denied",
        ));
        assert_eq!(
            RetryStrategy::categorize_error(&error),
            ErrorCategory::DiskError
        );
    }

    #[test]
    fn test_categorize_disk_space_error() {
        let error = AppError::Download("disk space not enough".to_string());
        assert_eq!(
            RetryStrategy::categorize_error(&error),
            ErrorCategory::DiskError
        );
    }

    #[test]
    fn test_should_retry_permanent() {
        let strategy = RetryStrategy::new(5);
        let error = AppError::Network("404 Not Found".to_string());
        assert!(!strategy.should_retry(&error));
    }

    #[test]
    fn test_should_retry_temporary() {
        let strategy = RetryStrategy::new(5);
        let error = AppError::Network("500 Internal Server Error".to_string());
        assert!(strategy.should_retry(&error));
    }

    #[test]
    fn test_should_retry_max_retries() {
        let mut strategy = RetryStrategy::new(3);
        strategy.current_retry = 3;
        let error = AppError::Network("500 Internal Server Error".to_string());
        assert!(!strategy.should_retry(&error));
    }

    #[test]
    fn test_should_retry_ssl_error() {
        let strategy = RetryStrategy::new(5);
        let error = AppError::Network("SSL certificate error".to_string());
        assert!(!strategy.should_retry(&error));
    }

    #[test]
    fn test_should_retry_disk_error() {
        let strategy = RetryStrategy::new(5);
        let error = AppError::Download("disk space not enough".to_string());
        assert!(!strategy.should_retry(&error));
    }

    #[test]
    fn test_should_retry_dns_error() {
        let strategy = RetryStrategy::new(5);
        let error = AppError::Network("DNS resolution failed".to_string());
        assert!(strategy.should_retry(&error));
    }

    #[test]
    fn test_should_retry_rate_limited() {
        let strategy = RetryStrategy::new(5);
        let error = AppError::Network("429 Too Many Requests".to_string());
        assert!(strategy.should_retry(&error));
    }

    #[test]
    fn test_backoff_delay() {
        let mut strategy = RetryStrategy::new(5);
        strategy.enable_jitter = false; // 禁用 jitter 以便测试

        // 第 0 次重试：1 * 2^0 = 1 秒
        assert_eq!(strategy.backoff_delay(), Duration::from_secs(1));

        // 第 1 次重试：1 * 2^1 = 2 秒
        strategy.current_retry = 1;
        assert_eq!(strategy.backoff_delay(), Duration::from_secs(2));

        // 第 2 次重试：1 * 2^2 = 4 秒
        strategy.current_retry = 2;
        assert_eq!(strategy.backoff_delay(), Duration::from_secs(4));

        // 第 3 次重试：1 * 2^3 = 8 秒
        strategy.current_retry = 3;
        assert_eq!(strategy.backoff_delay(), Duration::from_secs(8));
    }

    #[test]
    fn test_backoff_delay_max() {
        let mut strategy = RetryStrategy::new(10);
        strategy.enable_jitter = false;
        strategy.current_retry = 10; // 1 * 2^10 = 1024 秒

        // 应该被限制在 max_backoff (60 秒)
        assert_eq!(strategy.backoff_delay(), Duration::from_secs(60));
    }

    #[test]
    fn test_backoff_delay_with_jitter() {
        let mut strategy = RetryStrategy::new(5);
        strategy.enable_jitter = true;
        strategy.current_retry = 2; // 基础延迟 4 秒

        // 由于 jitter，延迟应该在 3-5 秒之间
        let delay = strategy.backoff_delay();
        assert!(delay >= Duration::from_secs(3));
        assert!(delay <= Duration::from_secs(5));
    }

    #[test]
    fn test_reset() {
        let mut strategy = RetryStrategy::new(5);
        strategy.current_retry = 3;
        strategy.reset();
        assert_eq!(strategy.current_retry(), 0);
    }

    #[test]
    fn test_increment_retry() {
        let mut strategy = RetryStrategy::new(5);
        assert_eq!(strategy.current_retry(), 0);

        strategy.increment_retry();
        assert_eq!(strategy.current_retry(), 1);

        strategy.increment_retry();
        assert_eq!(strategy.current_retry(), 2);
    }

    #[test]
    fn test_max_retries() {
        let strategy = RetryStrategy::new(10);
        assert_eq!(strategy.max_retries(), 10);
    }

    #[test]
    fn test_default() {
        let strategy = RetryStrategy::default();
        assert_eq!(strategy.max_retries(), 5);
        assert_eq!(strategy.current_retry(), 0);
    }

    #[test]
    fn test_prepare_retry_permanent() {
        let strategy = RetryStrategy::new(5);
        let error = AppError::Network("404 Not Found".to_string());

        let action = strategy.prepare_retry(&error);
        assert!(action.is_none());
    }

    #[test]
    fn test_prepare_retry_temporary() {
        let strategy = RetryStrategy::new(5);
        let error = AppError::Network("500 Internal Server Error".to_string());

        let action = strategy.prepare_retry(&error);
        assert!(action.is_some());

        if let Some(RetryAction::Sleep { reason, .. }) = action {
            assert_eq!(reason, "临时错误");
        } else {
            panic!("Expected Sleep action");
        }
    }

    #[test]
    fn test_prepare_retry_rate_limited() {
        let strategy = RetryStrategy::new(5);
        let error = AppError::Network("429 Too Many Requests".to_string());

        let action = strategy.prepare_retry(&error);
        assert!(action.is_some());

        if let Some(RetryAction::Sleep { reason, .. }) = action {
            assert_eq!(reason, "触发限流");
        } else {
            panic!("Expected Sleep action for rate limited");
        }
    }

    #[test]
    fn test_prepare_retry_network_unavailable() {
        let strategy = RetryStrategy::new(5);
        let error = AppError::Network("network unreachable".to_string());

        let action = strategy.prepare_retry(&error);
        assert!(action.is_some());

        if let Some(RetryAction::WaitForNetwork { timeout, .. }) = action {
            assert_eq!(timeout, Duration::from_secs(60));
        } else {
            panic!("Expected WaitForNetwork action");
        }
    }

    #[test]
    fn test_prepare_retry_dns_error() {
        let strategy = RetryStrategy::new(5);
        let error = AppError::Network("DNS resolution failed".to_string());

        let action = strategy.prepare_retry(&error);
        assert!(action.is_some());

        if let Some(RetryAction::Sleep { duration, reason, .. }) = action {
            assert_eq!(duration, Duration::from_secs(10));
            assert_eq!(reason, "DNS 解析失败");
        } else {
            panic!("Expected Sleep action for DNS error");
        }
    }

    #[test]
    fn test_try_switch_mirror_github() {
        let strategy = RetryStrategy::new(5);

        // GitHub URL 返回 None（建议在上层处理）
        let result = strategy.try_switch_mirror("https://github.com/user/repo/releases/download/v1.0/file.zip");
        assert!(result.is_none());

        // githubusercontent URL 同样
        let result = strategy.try_switch_mirror("https://raw.githubusercontent.com/user/repo/main/file.txt");
        assert!(result.is_none());
    }

    #[test]
    fn test_try_switch_mirror_non_github() {
        let strategy = RetryStrategy::new(5);

        // 非 GitHub URL
        let result = strategy.try_switch_mirror("https://example.com/file.zip");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_check_network_available() {
        // 这个测试依赖于实际的网络连接
        // 在 CI 环境中可能会失败
        let available = RetryStrategy::check_network_available().await;
        // 只记录结果，不做断言
        println!("Network available: {}", available);
    }
}

//! API 响应模型
//!
//! 定义统一的 API 响应格式

use serde::{Deserialize, Serialize};

/// 统一 API 响应格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    /// 状态码 (0 表示成功)
    pub code: i32,
    /// 响应数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    /// 消息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg: Option<String>,
}

impl<T> ApiResponse<T> {
    /// 创建成功响应
    pub fn success(data: T) -> Self {
        Self {
            code: 0,
            data: Some(data),
            msg: None,
        }
    }

    /// 创建成功响应（带消息）
    pub fn success_with_msg(data: T, msg: impl Into<String>) -> Self {
        Self {
            code: 0,
            data: Some(data),
            msg: Some(msg.into()),
        }
    }

    /// 创建错误响应
    pub fn error(code: i32, msg: impl Into<String>) -> Self {
        Self {
            code,
            data: None,
            msg: Some(msg.into()),
        }
    }

    /// 创建简单错误响应
    pub fn fail(msg: impl Into<String>) -> Self {
        Self::error(-1, msg)
    }
}

impl ApiResponse<()> {
    /// 创建无数据的成功响应
    pub fn ok() -> Self {
        Self {
            code: 0,
            data: None,
            msg: None,
        }
    }

    /// 创建无数据的成功响应（带消息）
    pub fn ok_with_msg(msg: impl Into<String>) -> Self {
        Self {
            code: 0,
            data: None,
            msg: Some(msg.into()),
        }
    }
}

/// 下载进度信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    /// 已下载字节数
    pub downloaded: u64,
    /// 总字节数
    pub total: u64,
    /// 下载速度（字节/秒）
    pub speed: u64,
    /// 进度百分比 (0-100)
    pub percentage: f64,
    /// 剩余时间（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta: Option<u64>,
}

impl DownloadProgress {
    /// 创建新的下载进度
    pub fn new(downloaded: u64, total: u64, speed: u64) -> Self {
        let percentage = if total > 0 {
            (downloaded as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let eta = if speed > 0 && total > downloaded {
            Some((total - downloaded) / speed)
        } else {
            None
        };

        Self {
            downloaded,
            total,
            speed,
            percentage,
            eta,
        }
    }
}

/// 安装进度信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallProgress {
    /// 当前阶段
    pub stage: String,
    /// 当前步骤
    pub step: u32,
    /// 总步骤数
    pub total_steps: u32,
    /// 详细消息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// 下载进度（如果正在下载）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download: Option<DownloadProgress>,
}

impl InstallProgress {
    /// 创建新的安装进度
    pub fn new(stage: impl Into<String>, step: u32, total_steps: u32) -> Self {
        Self {
            stage: stage.into(),
            step,
            total_steps,
            message: None,
            download: None,
        }
    }

    /// 添加消息
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// 添加下载进度
    pub fn with_download(mut self, download: DownloadProgress) -> Self {
        self.download = Some(download);
        self
    }
}

/// 消息通知类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageType {
    Info,
    Success,
    Warning,
    Error,
}

/// 消息通知
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyMessage {
    /// 消息类型
    #[serde(rename = "type")]
    pub msg_type: MessageType,
    /// 消息内容
    pub content: String,
    /// 是否持久显示
    #[serde(default)]
    pub persistent: bool,
}

impl NotifyMessage {
    /// 创建信息消息
    pub fn info(content: impl Into<String>) -> Self {
        Self {
            msg_type: MessageType::Info,
            content: content.into(),
            persistent: false,
        }
    }

    /// 创建成功消息
    pub fn success(content: impl Into<String>) -> Self {
        Self {
            msg_type: MessageType::Success,
            content: content.into(),
            persistent: false,
        }
    }

    /// 创建警告消息
    pub fn warning(content: impl Into<String>) -> Self {
        Self {
            msg_type: MessageType::Warning,
            content: content.into(),
            persistent: false,
        }
    }

    /// 创建错误消息
    pub fn error(content: impl Into<String>) -> Self {
        Self {
            msg_type: MessageType::Error,
            content: content.into(),
            persistent: false,
        }
    }

    /// 设置为持久显示
    pub fn persistent(mut self) -> Self {
        self.persistent = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_response_success() {
        let response = ApiResponse::success("test data".to_string());
        assert_eq!(response.code, 0);
        assert_eq!(response.data, Some("test data".to_string()));
        assert!(response.msg.is_none());
    }

    #[test]
    fn test_api_response_error() {
        let response: ApiResponse<()> = ApiResponse::error(1001, "Error message");
        assert_eq!(response.code, 1001);
        assert!(response.data.is_none());
        assert_eq!(response.msg, Some("Error message".to_string()));
    }

    #[test]
    fn test_download_progress() {
        let progress = DownloadProgress::new(500, 1000, 100);
        assert_eq!(progress.percentage, 50.0);
        assert_eq!(progress.eta, Some(5));
    }
}

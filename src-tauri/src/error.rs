//! 统一错误处理模块
//!
//! 定义应用程序中使用的所有错误类型

use serde::Serialize;
use std::error::Error;
use thiserror::Error as ThisError;

/// 应用程序错误类型
#[derive(ThisError, Debug)]
pub enum AppError {
    /// 配置相关错误
    #[error("配置错误: {0}")]
    Config(String),

    /// IO 错误
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// JSON 序列化/反序列化错误
    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),

    /// 网络请求错误
    #[error("{0}")]
    Network(String),

    /// 文件未找到
    #[error("文件未找到: {0}")]
    FileNotFound(String),

    /// 目录未找到
    #[error("目录未找到: {0}")]
    DirectoryNotFound(String),

    /// 模拟器相关错误
    #[error("模拟器错误: {0}")]
    Emulator(String),

    /// 下载错误
    #[error("下载错误: {0}")]
    Download(String),

    /// Aria2 错误
    #[error("Aria2 错误: {0}")]
    Aria2(String),

    /// 解压错误
    #[error("解压错误: {0}")]
    Extract(String),

    /// 进程错误
    #[error("进程错误: {0}")]
    Process(String),

    /// 权限错误
    #[error("权限错误: {0}")]
    Permission(String),

    /// 无效参数
    #[error("无效参数: {0}")]
    InvalidArgument(String),

    /// 不支持的操作
    #[error("不支持的操作: {0}")]
    Unsupported(String),

    /// 未知错误
    #[error("未知错误: {0}")]
    Unknown(String),
}

/// 可序列化的错误响应，用于前端通信
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl From<AppError> for ErrorResponse {
    fn from(error: AppError) -> Self {
        let (code, message, details) = match &error {
            AppError::Config(msg) => (1001, "配置错误".to_string(), Some(msg.clone())),
            AppError::Io(e) => (1002, "IO 错误".to_string(), Some(e.to_string())),
            AppError::Json(e) => (1003, "JSON 错误".to_string(), Some(e.to_string())),
            AppError::Network(msg) => (1004, "网络错误".to_string(), Some(msg.clone())),
            AppError::FileNotFound(path) => {
                (1005, "文件未找到".to_string(), Some(path.clone()))
            }
            AppError::DirectoryNotFound(path) => {
                (1006, "目录未找到".to_string(), Some(path.clone()))
            }
            AppError::Emulator(msg) => (2001, "模拟器错误".to_string(), Some(msg.clone())),
            AppError::Download(msg) => (2002, "下载错误".to_string(), Some(msg.clone())),
            AppError::Aria2(msg) => (2006, "Aria2 错误".to_string(), Some(msg.clone())),
            AppError::Extract(msg) => (2003, "解压错误".to_string(), Some(msg.clone())),
            AppError::Process(msg) => (2004, "进程错误".to_string(), Some(msg.clone())),
            AppError::Permission(msg) => (2005, "权限错误".to_string(), Some(msg.clone())),
            AppError::InvalidArgument(msg) => {
                (3001, "无效参数".to_string(), Some(msg.clone()))
            }
            AppError::Unsupported(msg) => {
                (3002, "不支持的操作".to_string(), Some(msg.clone()))
            }
            AppError::Unknown(msg) => (9999, "未知错误".to_string(), Some(msg.clone())),
        };

        ErrorResponse {
            code,
            message,
            details,
        }
    }
}

// 实现 Serialize for AppError，以便可以直接序列化错误
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        ErrorResponse::from(self.to_owned_error()).serialize(serializer)
    }
}

impl From<reqwest::Error> for AppError {
    fn from(error: reqwest::Error) -> Self {
        let mut details = Vec::new();

        // 基本错误信息
        details.push(format!("网络错误"));

        // URL 信息
        if let Some(url) = error.url() {
            details.push(format!("请求地址: {}", url));
        }

        // 详细错误类型
        if error.is_timeout() {
            details.push("原因: 请求超时".to_string());
        } else if error.is_connect() {
            details.push("原因: 无法连接到服务器".to_string());
            if let Some(source) = error.source() {
                details.push(format!("详细信息: {}", source));
            }
        } else if error.is_request() {
            details.push("原因: 请求构造失败".to_string());
        } else if error.is_redirect() {
            details.push("原因: 重定向过多".to_string());
        } else if error.is_status() {
            if let Some(status) = error.status() {
                details.push(format!("原因: HTTP 状态码 {}", status));
            }
        } else if error.is_body() {
            details.push("原因: 响应体解析失败".to_string());
        } else if error.is_decode() {
            details.push("原因: 响应数据解码失败".to_string());
        } else {
            // 其他错误，提供更多上下文
            details.push(format!("原因: {}", error));
        }

        AppError::Network(details.join("\n"))
    }
}

impl AppError {
    /// 将错误转换为可拥有的版本（用于序列化）
    fn to_owned_error(&self) -> AppError {
        match self {
            AppError::Config(s) => AppError::Config(s.clone()),
            AppError::Io(e) => AppError::Unknown(e.to_string()),
            AppError::Json(e) => AppError::Unknown(e.to_string()),
            AppError::Network(s) => AppError::Network(s.clone()),
            AppError::FileNotFound(s) => AppError::FileNotFound(s.clone()),
            AppError::DirectoryNotFound(s) => AppError::DirectoryNotFound(s.clone()),
            AppError::Emulator(s) => AppError::Emulator(s.clone()),
            AppError::Download(s) => AppError::Download(s.clone()),
            AppError::Aria2(s) => AppError::Aria2(s.clone()),
            AppError::Extract(s) => AppError::Extract(s.clone()),
            AppError::Process(s) => AppError::Process(s.clone()),
            AppError::Permission(s) => AppError::Permission(s.clone()),
            AppError::InvalidArgument(s) => AppError::InvalidArgument(s.clone()),
            AppError::Unsupported(s) => AppError::Unsupported(s.clone()),
            AppError::Unknown(s) => AppError::Unknown(s.clone()),
        }
    }
}

/// 应用程序 Result 类型别名
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_serialization() {
        let error = AppError::Config("测试配置错误".to_string());
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("1001"));
        assert!(json.contains("配置错误"));
    }

    #[test]
    fn test_error_response_conversion() {
        let error = AppError::FileNotFound("/path/to/file".to_string());
        let response: ErrorResponse = error.into();
        assert_eq!(response.code, 1005);
        assert_eq!(response.message, "文件未找到");
        assert_eq!(response.details, Some("/path/to/file".to_string()));
    }
}

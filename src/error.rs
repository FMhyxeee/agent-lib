//! 错误类型定义
//!
//! 提供细化的错误类型，便于错误处理和调试。

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Agent 错误类型
#[derive(Debug, Error, Clone, Serialize, Deserialize)]
pub enum AgentError {
    /// 模型相关错误
    #[error("model error: {0}")]
    Model(#[from] ModelError),

    /// 工具执行错误
    #[error("tool error: {0}")]
    Tool(String),

    /// MCP 相关错误
    #[error("mcp error: {0}")]
    Mcp(String),

    /// 会话相关错误
    #[error("session error: {0}")]
    Session(String),

    /// 未实现的功能
    #[error("not implemented: {0}")]
    NotImplemented(String),

    /// 配置错误
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    /// 超时错误
    #[error("operation timed out after {0:?}")]
    Timeout(std::time::Duration),
}

/// 模型相关错误
#[derive(Debug, Error, Clone, Serialize, Deserialize)]
pub enum ModelError {
    /// API 请求失败
    #[error("API request failed: {0}")]
    RequestFailed(String),

    /// 请求超时
    #[error("request timed out after {0:?}")]
    Timeout(std::time::Duration),

    /// 无效响应
    #[error("invalid response: {0}")]
    InvalidResponse(String),

    /// API 密钥缺失
    #[error("API key missing")]
    ApiKeyMissing,

    /// 速率限制
    #[error("rate limit exceeded")]
    RateLimitExceeded,

    /// 其他模型错误
    #[error("model error: {0}")]
    Other(String),
}

impl From<String> for ModelError {
    fn from(s: String) -> Self {
        ModelError::Other(s)
    }
}

impl From<&str> for ModelError {
    fn from(s: &str) -> Self {
        ModelError::Other(s.to_string())
    }
}

pub type AgentResult<T> = Result<T, AgentError>;


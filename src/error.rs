use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, Clone, Serialize, Deserialize)]
pub enum AgentError {
    #[error("model error: {0}")]
    Model(String),
    #[error("tool error: {0}")]
    Tool(String),
    #[error("mcp error: {0}")]
    Mcp(String),
    #[error("session error: {0}")]
    Session(String),
    #[error("not implemented: {0}")]
    NotImplemented(String),
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

pub type AgentResult<T> = Result<T, AgentError>;

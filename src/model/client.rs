use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AgentError, AgentResult};
use crate::model::Message;
use crate::tools::ToolDef;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// 工具调用信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// 调用 ID (用于关联工具结果)
    pub id: String,
    /// 工具名称
    pub name: String,
    /// 工具参数 (JSON 对象)
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponse {
    /// 响应文本内容
    pub content: String,
    /// Token 使用情况
    pub usage: TokenUsage,
    /// 工具调用列表 (如果有)
    pub tool_calls: Vec<ToolCall>,
}

impl ModelResponse {
    /// 检查是否有工具调用
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub delta: String,
}

#[async_trait]
pub trait ModelClient: Send + Sync {
    async fn chat(&self, messages: Vec<Message>, tools: Vec<ToolDef>)
    -> AgentResult<ModelResponse>;

    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>>;
}

pub fn not_implemented_client(name: &str) -> AgentError {
    AgentError::NotImplemented(format!("provider {name} not implemented"))
}

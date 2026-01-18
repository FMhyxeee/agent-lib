use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};

use crate::error::{AgentError, AgentResult};
use crate::model::Message;
use crate::tools::ToolDef;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponse {
    pub content: String,
    pub usage: TokenUsage,
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

use async_trait::async_trait;

use crate::error::{AgentError, AgentResult};
use crate::model::{Message, ModelClient, ModelResponse, StreamChunk, not_implemented_client};
use crate::tools::ToolDef;

#[derive(Debug, Clone)]
pub struct LocalProvider {
    pub model: String,
}

impl LocalProvider {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
        }
    }
}

#[async_trait]
impl ModelClient for LocalProvider {
    async fn chat(
        &self,
        _messages: Vec<Message>,
        _tools: Vec<ToolDef>,
    ) -> AgentResult<ModelResponse> {
        Err(AgentError::Model(
            not_implemented_client("local").to_string(),
        ))
    }

    async fn chat_stream(
        &self,
        _messages: Vec<Message>,
        _tools: Vec<ToolDef>,
    ) -> AgentResult<std::pin::Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>>> {
        Err(AgentError::Model(
            not_implemented_client("local").to_string(),
        ))
    }
}

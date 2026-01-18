use async_trait::async_trait;

use crate::error::{AgentError, AgentResult};
use crate::model::{not_implemented_client, Message, ModelClient, ModelResponse, StreamChunk};
use crate::tools::ToolDef;

#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    pub model: String,
    pub api_key: Option<String>,
}

impl AnthropicProvider {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            api_key: None,
        }
    }

    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }
}

#[async_trait]
impl ModelClient for AnthropicProvider {
    async fn chat(
        &self,
        _messages: Vec<Message>,
        _tools: Vec<ToolDef>,
    ) -> AgentResult<ModelResponse> {
        Err(AgentError::Model(
            not_implemented_client("anthropic").to_string(),
        ))
    }

    async fn chat_stream(
        &self,
        _messages: Vec<Message>,
        _tools: Vec<ToolDef>,
    ) -> AgentResult<std::pin::Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>>> {
        Err(AgentError::Model(
            not_implemented_client("anthropic").to_string(),
        ))
    }
}

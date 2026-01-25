use async_trait::async_trait;
use std::pin::Pin;

use futures::StreamExt;

use crate::error::{AgentError, AgentResult};
use crate::model::{Message, ModelClient, ModelResponse, StreamChunk, TokenUsage};
use crate::tools::ToolDef;

#[derive(Debug, Clone)]
pub struct OpenAiProvider {
    pub model: String,
    pub api_key: Option<String>,
}

impl OpenAiProvider {
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
impl ModelClient for OpenAiProvider {
    async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
    ) -> AgentResult<ModelResponse> {
        #[cfg(feature = "openai")]
        {
            use async_openai::config::OpenAIConfig;
            use async_openai::types::{
                ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
                ChatCompletionRequestSystemMessageContent, ChatCompletionRequestUserMessage,
                ChatCompletionRequestUserMessageContent, ChatCompletionTool,
                ChatCompletionToolType, CreateChatCompletionRequest, FunctionObject,
            };
            let config = if let Some(key) = &self.api_key {
                OpenAIConfig::new().with_api_key(key)
            } else {
                OpenAIConfig::new()
            };
            let client = async_openai::Client::with_config(config);

            let converted: Vec<ChatCompletionRequestMessage> = messages
                .into_iter()
                .map(|msg| match msg.role {
                    crate::model::MessageRole::System => {
                        ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
                            content: ChatCompletionRequestSystemMessageContent::Text(msg.content),
                            name: None,
                        })
                    }
                    _ => ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                        content: ChatCompletionRequestUserMessageContent::Text(msg.content),
                        name: None,
                    }),
                })
                .collect();

            let tool_defs: Vec<ChatCompletionTool> = tools
                .into_iter()
                .map(|tool| ChatCompletionTool {
                    r#type: ChatCompletionToolType::Function,
                    function: FunctionObject {
                        name: tool.name,
                        description: Some(tool.description),
                        parameters: Some(tool.schema),
                        strict: None,
                    },
                })
                .collect();

            let request = CreateChatCompletionRequest {
                model: self.model.clone(),
                messages: converted,
                tools: if tool_defs.is_empty() {
                    None
                } else {
                    Some(tool_defs)
                },
                ..Default::default()
            };

            let response = client
                .chat()
                .create(request)
                .await
                .map_err(|err| AgentError::Model(err.to_string()))?;

            let content = response
                .choices
                .first()
                .and_then(|choice| choice.message.content.clone())
                .unwrap_or_default();

            let usage = response.usage.map(|usage| TokenUsage {
                prompt_tokens: usage.prompt_tokens as u32,
                completion_tokens: usage.completion_tokens as u32,
                total_tokens: usage.total_tokens as u32,
            });

            Ok(ModelResponse {
                content,
                usage: usage.unwrap_or_default(),
            })
        }
        #[cfg(not(feature = "openai"))]
        {
            Err(AgentError::Model("openai feature not enabled".to_string()))
        }
    }

    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
    ) -> AgentResult<Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>>> {
        #[cfg(feature = "openai")]
        {
            use async_openai::config::OpenAIConfig;
            use async_openai::types::{
                ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
                ChatCompletionRequestSystemMessageContent, ChatCompletionRequestUserMessage,
                ChatCompletionRequestUserMessageContent, ChatCompletionTool,
                ChatCompletionToolType, CreateChatCompletionRequest, FunctionObject,
            };
            let config = if let Some(key) = &self.api_key {
                OpenAIConfig::new().with_api_key(key)
            } else {
                OpenAIConfig::new()
            };
            let client = async_openai::Client::with_config(config);

            let converted: Vec<ChatCompletionRequestMessage> = messages
                .into_iter()
                .map(|msg| match msg.role {
                    crate::model::MessageRole::System => {
                        ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
                            content: ChatCompletionRequestSystemMessageContent::Text(msg.content),
                            name: None,
                        })
                    }
                    _ => ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                        content: ChatCompletionRequestUserMessageContent::Text(msg.content),
                        name: None,
                    }),
                })
                .collect();

            let tool_defs: Vec<ChatCompletionTool> = tools
                .into_iter()
                .map(|tool| ChatCompletionTool {
                    r#type: ChatCompletionToolType::Function,
                    function: FunctionObject {
                        name: tool.name,
                        description: Some(tool.description),
                        parameters: Some(tool.schema),
                        strict: None,
                    },
                })
                .collect();

            let request = CreateChatCompletionRequest {
                model: self.model.clone(),
                messages: converted,
                stream: Some(true),
                tools: if tool_defs.is_empty() {
                    None
                } else {
                    Some(tool_defs)
                },
                ..Default::default()
            };

            let stream = client
                .chat()
                .create_stream(request)
                .await
                .map_err(|err| AgentError::Model(err.to_string()))?;

            let mapped = stream.filter_map(|event| async move {
                match event {
                    Ok(response) => {
                        let delta = response
                            .choices
                            .first()
                            .and_then(|choice| choice.delta.content.clone())
                            .unwrap_or_default();
                        if delta.is_empty() {
                            None
                        } else {
                            Some(StreamChunk { delta })
                        }
                    }
                    Err(_) => None,
                }
            });

            Ok(Box::pin(mapped))
        }
        #[cfg(not(feature = "openai"))]
        {
            Err(AgentError::Model("openai feature not enabled".to_string()))
        }
    }
}

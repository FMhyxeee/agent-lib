use async_trait::async_trait;
use std::pin::Pin;

use futures::StreamExt;
use serde_json::Value;

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
    #[allow(deprecated)]
    async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
    ) -> AgentResult<ModelResponse> {
        #[cfg(feature = "openai")]
        {
            use async_openai::config::OpenAIConfig;
            use async_openai::types::{
                ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessage,
                ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestMessage,
                ChatCompletionRequestSystemMessage, ChatCompletionRequestSystemMessageContent,
                ChatCompletionRequestToolMessage, ChatCompletionRequestToolMessageContent,
                ChatCompletionRequestUserMessage, ChatCompletionRequestUserMessageContent,
                ChatCompletionTool, ChatCompletionToolType, CreateChatCompletionRequest,
                FunctionObject,
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
                    crate::model::MessageRole::User => {
                        ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                            content: ChatCompletionRequestUserMessageContent::Text(msg.content),
                            name: None,
                        })
                    }
                    crate::model::MessageRole::Assistant => {
                        // 助手消息可能包含工具调用
                        if let Some(tool_calls) = msg.tool_calls {
                            let openai_tool_calls: Vec<ChatCompletionMessageToolCall> = tool_calls
                                .into_iter()
                                .map(|tc| ChatCompletionMessageToolCall {
                                    id: tc.id,
                                    r#type: ChatCompletionToolType::Function,
                                    function: async_openai::types::FunctionCall {
                                        name: tc.name,
                                        arguments: serde_json::to_string(&tc.arguments)
                                            .unwrap_or_else(|_| "{}".to_string()),
                                    },
                                })
                                .collect();
                            ChatCompletionRequestMessage::Assistant(
                                ChatCompletionRequestAssistantMessage {
                                    content: Some(
                                        ChatCompletionRequestAssistantMessageContent::Text(
                                            msg.content,
                                        ),
                                    ),
                                    refusal: None,
                                    tool_calls: Some(openai_tool_calls),
                                    name: None,
                                    function_call: None,
                                },
                            )
                        } else {
                            // 普通助手消息
                            ChatCompletionRequestMessage::Assistant(
                                ChatCompletionRequestAssistantMessage {
                                    content: Some(
                                        ChatCompletionRequestAssistantMessageContent::Text(
                                            msg.content,
                                        ),
                                    ),
                                    refusal: None,
                                    tool_calls: None,
                                    name: None,
                                    function_call: None,
                                },
                            )
                        }
                    }
                    crate::model::MessageRole::Tool => {
                        // 工具结果消息
                        ChatCompletionRequestMessage::Tool(ChatCompletionRequestToolMessage {
                            content: ChatCompletionRequestToolMessageContent::Text(msg.content),
                            tool_call_id: msg.tool_call_id.unwrap_or_default(),
                        })
                    }
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

            let choice = response.choices.first();
            let content = choice
                .and_then(|c| c.message.content.clone())
                .unwrap_or_default();

            // 解析工具调用
            let tool_calls = choice
                .and_then(|c| c.message.tool_calls.as_ref())
                .map(|calls| {
                    calls
                        .iter()
                        .map(|tc| crate::model::ToolCall {
                            id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            arguments: serde_json::from_str(&tc.function.arguments)
                                .unwrap_or_else(|_| Value::Object(serde_json::Map::default())),
                        })
                        .collect()
                })
                .unwrap_or_default();

            let usage = response.usage.map(|usage| TokenUsage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
            });

            Ok(ModelResponse {
                content,
                usage: usage.unwrap_or_default(),
                tool_calls,
            })
        }
        #[cfg(not(feature = "openai"))]
        {
            Err(AgentError::Model("openai feature not enabled".to_string()))
        }
    }

    #[allow(deprecated)]
    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
    ) -> AgentResult<Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>>> {
        #[cfg(feature = "openai")]
        {
            use async_openai::config::OpenAIConfig;
            use async_openai::types::{
                ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessage,
                ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestMessage,
                ChatCompletionRequestSystemMessage, ChatCompletionRequestSystemMessageContent,
                ChatCompletionRequestToolMessage, ChatCompletionRequestToolMessageContent,
                ChatCompletionRequestUserMessage, ChatCompletionRequestUserMessageContent,
                ChatCompletionTool, ChatCompletionToolType, CreateChatCompletionRequest,
                FunctionObject,
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
                    crate::model::MessageRole::User => {
                        ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                            content: ChatCompletionRequestUserMessageContent::Text(msg.content),
                            name: None,
                        })
                    }
                    crate::model::MessageRole::Assistant => {
                        if let Some(tool_calls) = msg.tool_calls {
                            let openai_tool_calls: Vec<ChatCompletionMessageToolCall> = tool_calls
                                .into_iter()
                                .map(|tc| ChatCompletionMessageToolCall {
                                    id: tc.id,
                                    r#type: ChatCompletionToolType::Function,
                                    function: async_openai::types::FunctionCall {
                                        name: tc.name,
                                        arguments: serde_json::to_string(&tc.arguments)
                                            .unwrap_or_else(|_| "{}".to_string()),
                                    },
                                })
                                .collect();
                            ChatCompletionRequestMessage::Assistant(
                                ChatCompletionRequestAssistantMessage {
                                    content: Some(
                                        ChatCompletionRequestAssistantMessageContent::Text(
                                            msg.content,
                                        ),
                                    ),
                                    refusal: None,
                                    tool_calls: Some(openai_tool_calls),
                                    name: None,
                                    function_call: None,
                                },
                            )
                        } else {
                            ChatCompletionRequestMessage::Assistant(
                                ChatCompletionRequestAssistantMessage {
                                    content: Some(
                                        ChatCompletionRequestAssistantMessageContent::Text(
                                            msg.content,
                                        ),
                                    ),
                                    refusal: None,
                                    tool_calls: None,
                                    name: None,
                                    function_call: None,
                                },
                            )
                        }
                    }
                    crate::model::MessageRole::Tool => {
                        ChatCompletionRequestMessage::Tool(ChatCompletionRequestToolMessage {
                            content: ChatCompletionRequestToolMessageContent::Text(msg.content),
                            tool_call_id: msg.tool_call_id.unwrap_or_default(),
                        })
                    }
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

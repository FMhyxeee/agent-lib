use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::{AgentError, AgentResult};
use crate::model::{Message, MessageRole, ModelClient, ModelResponse, StreamChunk, ToolCall, TokenUsage};
use crate::tools::ToolDef;

#[derive(Debug, Clone)]
pub struct GlmProvider {
    pub model: String,
    pub api_key: String,
    pub base_url: String,
}

impl GlmProvider {
    pub fn new(model: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            api_key: api_key.into(),
            base_url: "https://open.bigmodel.cn/api/paas/v4/chat/completions".to_string(),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }
}

#[derive(Debug, Serialize)]
struct GlmChatRequest {
    model: String,
    messages: Vec<GlmMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GlmTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize)]
struct GlmMessage {
    role: String,
    content: String,
    /// 工具调用 ID (仅 tool 角色使用)
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    /// 工具调用列表 (仅 assistant 角色使用)
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<GlmToolCallRequest>>,
}

/// 请求时使用的工具调用格式
#[derive(Debug, Serialize)]
struct GlmToolCallRequest {
    id: String,
    r#type: String,
    function: GlmToolCallFunctionRequest,
}

#[derive(Debug, Serialize)]
struct GlmToolCallFunctionRequest {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct GlmTool {
    r#type: String,
    function: GlmFunction,
}

#[derive(Debug, Serialize)]
struct GlmFunction {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct GlmChatResponse {
    choices: Vec<GlmChoice>,
    #[serde(default)]
    usage: Option<GlmUsage>,
}

#[derive(Debug, Deserialize)]
struct GlmChoice {
    message: Option<GlmMessageResponse>,
    delta: Option<GlmMessageResponse>,
}

#[derive(Debug, Deserialize)]
struct GlmMessageResponse {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<GlmToolCall>>,
}

#[derive(Debug, Deserialize)]
struct GlmToolCall {
    id: String,
    function: GlmToolCallFunction,
}

#[derive(Debug, Deserialize)]
struct GlmToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct GlmUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[async_trait]
impl ModelClient for GlmProvider {
    async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
    ) -> AgentResult<ModelResponse> {
        let client = reqwest::Client::new();
        let request = GlmChatRequest {
            model: self.model.clone(),
            messages: messages.into_iter().map(map_message).collect(),
            tools: map_tools(tools),
            stream: None,
        };

        let response = client
            .post(&self.base_url)
            .headers(auth_headers(&self.api_key)?)
            .json(&request)
            .send()
            .await
            .map_err(|err| AgentError::Model(format!("glm request failed: {err}")))?;

        let response: GlmChatResponse = response
            .json()
            .await
            .map_err(|err| AgentError::Model(format!("glm parse failed: {err}")))?;

        let choice = response.choices.first();
        let message = choice.and_then(|c| c.message.as_ref());

        let content = message
            .and_then(|msg| msg.content.clone())
            .unwrap_or_default();

        // 解析工具调用
        let tool_calls = message
            .and_then(|msg| msg.tool_calls.as_ref())
            .map(|calls| {
                calls
                    .iter()
                    .filter_map(|tc| {
                        // GLM 的 arguments 是字符串形式的 JSON，需要解析
                        let args: Value = serde_json::from_str(&tc.function.arguments)
                            .unwrap_or_else(|_| Value::Object(Default::default()));
                        Some(ToolCall {
                            id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            arguments: args,
                        })
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

    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>> {
        let client = reqwest::Client::new();
        let request = GlmChatRequest {
            model: self.model.clone(),
            messages: messages.into_iter().map(map_message).collect(),
            tools: map_tools(tools),
            stream: Some(true),
        };

        let response = client
            .post(&self.base_url)
            .headers(auth_headers(&self.api_key)?)
            .json(&request)
            .send()
            .await
            .map_err(|err| AgentError::Model(format!("glm request failed: {err}")))?;

        let mut stream = response.bytes_stream();
        let (sender, receiver) = mpsc::channel(64);

        tokio::spawn(async move {
            let mut buffer = String::new();
            while let Some(item) = tokio_stream::StreamExt::next(&mut stream).await {
                match item {
                    Ok(bytes) => {
                        let chunk = String::from_utf8_lossy(&bytes);
                        buffer.push_str(&chunk);
                        while let Some(pos) = buffer.find('\n') {
                            let line = buffer[..pos].trim().to_string();
                            buffer = buffer[pos + 1..].to_string();
                            if line.is_empty() {
                                continue;
                            }
                            if let Some(payload) = line.strip_prefix("data:") {
                                let payload = payload.trim();
                                if payload == "[DONE]" {
                                    return;
                                }
                                if let Ok(Some(delta)) = parse_delta(payload) {
                                    let _ = sender.send(StreamChunk { delta }).await;
                                }
                            }
                        }
                    }
                    Err(_) => return,
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(receiver)))
    }
}

fn map_message(message: Message) -> GlmMessage {
    let role = match message.role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    };

    match message.role {
        MessageRole::Tool => {
            // 工具结果消息需要 tool_call_id
            GlmMessage {
                role: role.to_string(),
                content: message.content,
                tool_call_id: message.tool_call_id,
                tool_calls: None,
            }
        }
        MessageRole::Assistant => {
            // 助手消息可能包含工具调用
            if let Some(tool_calls) = message.tool_calls {
                let request_tool_calls = tool_calls
                    .into_iter()
                    .map(|tc| GlmToolCallRequest {
                        id: tc.id,
                        r#type: "function".to_string(),
                        function: GlmToolCallFunctionRequest {
                            name: tc.name,
                            // arguments 在我们的结构中是 Value，需要转为字符串
                            arguments: serde_json::to_string(&tc.arguments)
                                .unwrap_or_else(|_| "{}".to_string()),
                        },
                    })
                    .collect();

                GlmMessage {
                    role: role.to_string(),
                    content: message.content,
                    tool_call_id: None,
                    tool_calls: Some(request_tool_calls),
                }
            } else {
                // 普通助手消息
                GlmMessage {
                    role: role.to_string(),
                    content: message.content,
                    tool_call_id: None,
                    tool_calls: None,
                }
            }
        }
        _ => {
            // System 和 User 消息
            GlmMessage {
                role: role.to_string(),
                content: message.content,
                tool_call_id: None,
                tool_calls: None,
            }
        }
    }
}

fn map_tools(tools: Vec<ToolDef>) -> Option<Vec<GlmTool>> {
    if tools.is_empty() {
        return None;
    }
    Some(
        tools
            .into_iter()
            .map(|tool| GlmTool {
                r#type: "function".to_string(),
                function: GlmFunction {
                    name: tool.name,
                    description: Some(tool.description),
                    parameters: Some(tool.schema),
                },
            })
            .collect(),
    )
}

fn auth_headers(api_key: &str) -> AgentResult<HeaderMap> {
    let mut headers = HeaderMap::new();
    let value = format!("Bearer {}", api_key);
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&value)
            .map_err(|err| AgentError::Model(format!("auth header invalid: {err}")))?,
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    Ok(headers)
}

fn parse_delta(payload: &str) -> AgentResult<Option<String>> {
    let response: GlmChatResponse = serde_json::from_str(payload)
        .map_err(|err| AgentError::Model(format!("glm stream parse failed: {err}")))?;
    let delta = response.choices.first().and_then(|choice| {
        choice
            .delta
            .as_ref()
            .or(choice.message.as_ref())
            .and_then(|msg| msg.content.clone())
    });
    Ok(delta)
}

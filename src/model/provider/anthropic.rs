use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::{AgentError, AgentResult};
use crate::model::{
    Message, MessageRole, ModelClient, ModelResponse, StreamChunk, TokenUsage, ToolCall,
};
use crate::tools::ToolDef;

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com/v1/messages";
const DEFAULT_MAX_TOKENS: u32 = 1024;
#[allow(dead_code)]
const ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: String,
    pub max_tokens: u32,
}

impl AnthropicProvider {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            api_key: None,
            base_url: DEFAULT_BASE_URL.to_string(),
            max_tokens: DEFAULT_MAX_TOKENS,
        }
    }

    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContentBlock>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(dead_code)]
enum AnthropicContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct AnthropicTool {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_schema: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AnthropicResponse {
    #[serde(default)]
    content: Vec<AnthropicContentBlock>,
    #[serde(default)]
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AnthropicStreamEvent {
    #[serde(default)]
    event: Option<String>,
    #[serde(default)]
    data: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(dead_code)]
enum AnthropicStreamData {
    ContentBlockDelta {
        delta: AnthropicDelta,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(dead_code)]
enum AnthropicDelta {
    TextDelta {
        text: String,
    },
    #[serde(other)]
    Other,
}

#[async_trait]
impl ModelClient for AnthropicProvider {
    async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
    ) -> AgentResult<ModelResponse> {
        let client = reqwest::Client::new();
        let (system, converted) = map_messages(messages);
        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            messages: converted,
            system,
            tools: map_tools(tools),
            stream: None,
        };

        let response = client
            .post(&self.base_url)
            .headers(auth_headers(self.api_key.as_deref())?)
            .json(&request)
            .send()
            .await
            .map_err(|err| AgentError::Model(format!("anthropic request failed: {err}")))?;

        if !response.status().is_success() {
            return Err(AgentError::Model(format!(
                "anthropic request failed with status {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let response: AnthropicResponse = response
            .json()
            .await
            .map_err(|err| AgentError::Model(format!("anthropic parse failed: {err}")))?;

        let mut content = String::new();
        let mut tool_calls = Vec::new();
        for block in response.content {
            match block {
                AnthropicContentBlock::Text { text } => {
                    if !content.is_empty() {
                        content.push('\n');
                    }
                    content.push_str(&text);
                }
                AnthropicContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments: input,
                    });
                }
                AnthropicContentBlock::ToolResult { .. } => {}
            }
        }

        let usage = response.usage.map(|usage| TokenUsage {
            prompt_tokens: usage.input_tokens,
            completion_tokens: usage.output_tokens,
            total_tokens: usage.input_tokens + usage.output_tokens,
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
        let (system, converted) = map_messages(messages);
        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            messages: converted,
            system,
            tools: map_tools(tools),
            stream: Some(true),
        };

        let response = client
            .post(&self.base_url)
            .headers(auth_headers(self.api_key.as_deref())?)
            .json(&request)
            .send()
            .await
            .map_err(|err| AgentError::Model(format!("anthropic request failed: {err}")))?;

        if !response.status().is_success() {
            return Err(AgentError::Model(format!(
                "anthropic stream failed with status {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

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
                            if let Some(delta) = parse_stream_delta(&line) {
                                let _ = sender.send(StreamChunk { delta }).await;
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

#[allow(dead_code)]
fn auth_headers(api_key: Option<&str>) -> AgentResult<HeaderMap> {
    let key = api_key
        .map(|s| s.to_string())
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
        .ok_or_else(|| AgentError::Model("anthropic api key missing".to_string()))?;

    let mut headers = HeaderMap::new();
    headers.insert(
        "x-api-key",
        HeaderValue::from_str(&key)
            .map_err(|err| AgentError::Model(format!("auth header invalid: {err}")))?,
    );
    headers.insert(
        "anthropic-version",
        HeaderValue::from_static(ANTHROPIC_VERSION),
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    Ok(headers)
}

#[allow(dead_code)]
fn map_messages(messages: Vec<Message>) -> (Option<String>, Vec<AnthropicMessage>) {
    let mut system_parts = Vec::new();
    let mut result = Vec::new();

    for msg in messages {
        match msg.role {
            MessageRole::System => system_parts.push(msg.content),
            MessageRole::User => {
                result.push(AnthropicMessage {
                    role: "user".to_string(),
                    content: vec![AnthropicContentBlock::Text { text: msg.content }],
                });
            }
            MessageRole::Assistant => {
                let mut blocks = Vec::new();
                if !msg.content.is_empty() {
                    blocks.push(AnthropicContentBlock::Text { text: msg.content });
                }
                if let Some(tool_calls) = msg.tool_calls {
                    for call in tool_calls {
                        blocks.push(AnthropicContentBlock::ToolUse {
                            id: call.id,
                            name: call.name,
                            input: call.arguments,
                        });
                    }
                }
                if !blocks.is_empty() {
                    result.push(AnthropicMessage {
                        role: "assistant".to_string(),
                        content: blocks,
                    });
                }
            }
            MessageRole::Tool => {
                if let Some(tool_call_id) = msg.tool_call_id {
                    result.push(AnthropicMessage {
                        role: "user".to_string(),
                        content: vec![AnthropicContentBlock::ToolResult {
                            tool_use_id: tool_call_id,
                            content: msg.content,
                        }],
                    });
                }
            }
        }
    }

    let system = if system_parts.is_empty() {
        None
    } else {
        Some(system_parts.join("\n"))
    };

    (system, result)
}

#[allow(dead_code)]
fn map_tools(tools: Vec<ToolDef>) -> Option<Vec<AnthropicTool>> {
    if tools.is_empty() {
        return None;
    }

    Some(
        tools
            .into_iter()
            .map(|tool| AnthropicTool {
                name: tool.name,
                description: Some(tool.description),
                input_schema: Some(tool.schema),
            })
            .collect(),
    )
}

#[allow(dead_code)]
fn parse_stream_delta(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("event:") {
        return None;
    }
    if let Some(payload) = trimmed.strip_prefix("data:") {
        let payload = payload.trim();
        if payload == "[DONE]" {
            return None;
        }
        if let Ok(data) = serde_json::from_str::<AnthropicStreamData>(payload)
            && let AnthropicStreamData::ContentBlockDelta { delta } = data
            && let AnthropicDelta::TextDelta { text } = delta
        {
            return Some(text);
        }
    }
    None
}

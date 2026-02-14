use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::{AgentError, AgentResult};
use crate::model::{
    Message, MessageRole, ModelClient, ModelResponse, StreamChunk, TokenUsage, ToolCall,
};
use crate::tools::ToolDef;

const DEFAULT_BASE_URL: &str = "http://localhost:11434/api/chat";

#[derive(Debug, Clone)]
pub struct LocalProvider {
    pub model: String,
    pub base_url: String,
}

impl LocalProvider {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct LocalChatRequest {
    model: String,
    messages: Vec<LocalMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<LocalTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct LocalMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<LocalToolCallRequest>>,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct LocalToolCallRequest {
    id: String,
    r#type: String,
    function: LocalToolCallFunctionRequest,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct LocalToolCallFunctionRequest {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct LocalTool {
    r#type: String,
    function: LocalToolFunction,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct LocalToolFunction {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LocalChatResponse {
    #[serde(default)]
    message: Option<LocalMessageResponse>,
    #[serde(default)]
    done: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LocalMessageResponse {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<LocalToolCall>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LocalToolCall {
    id: Option<String>,
    function: LocalToolCallFunction,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LocalToolCallFunction {
    name: String,
    arguments: Value,
}

#[async_trait]
impl ModelClient for LocalProvider {
    async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
    ) -> AgentResult<ModelResponse> {
        let client = reqwest::Client::new();
        let request = LocalChatRequest {
            model: self.model.clone(),
            messages: messages.into_iter().map(map_message).collect(),
            tools: map_tools(tools),
            stream: None,
        };

        let response = client
            .post(&self.base_url)
            .json(&request)
            .send()
            .await
            .map_err(|err| AgentError::Model(format!("local request failed: {err}")))?;

        if !response.status().is_success() {
            return Err(AgentError::Model(format!(
                "local request failed with status {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let response: LocalChatResponse = response
            .json()
            .await
            .map_err(|err| AgentError::Model(format!("local parse failed: {err}")))?;

        let message = response.message;
        let content = message
            .as_ref()
            .and_then(|msg| msg.content.clone())
            .unwrap_or_default();

        let tool_calls = message
            .and_then(|msg| msg.tool_calls)
            .map(|calls| {
                calls
                    .into_iter()
                    .map(|tc| ToolCall {
                        id: tc.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                        name: tc.function.name,
                        arguments: tc.function.arguments,
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(ModelResponse {
            content,
            usage: TokenUsage::default(),
            tool_calls,
        })
    }

    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDef>,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>> {
        let client = reqwest::Client::new();
        let request = LocalChatRequest {
            model: self.model.clone(),
            messages: messages.into_iter().map(map_message).collect(),
            tools: map_tools(tools),
            stream: Some(true),
        };

        let response = client
            .post(&self.base_url)
            .json(&request)
            .send()
            .await
            .map_err(|err| AgentError::Model(format!("local request failed: {err}")))?;

        if !response.status().is_success() {
            return Err(AgentError::Model(format!(
                "local stream failed with status {}: {}",
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
                            if let Ok(delta) = parse_stream_delta(&line)
                                && let Some(text) = delta
                            {
                                let _ = sender.send(StreamChunk { delta: text }).await;
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
fn map_message(message: Message) -> LocalMessage {
    let role = match message.role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    };

    match message.role {
        MessageRole::Tool => LocalMessage {
            role: role.to_string(),
            content: message.content,
            tool_call_id: message.tool_call_id,
            tool_calls: None,
        },
        MessageRole::Assistant => {
            if let Some(tool_calls) = message.tool_calls {
                let request_tool_calls = tool_calls
                    .into_iter()
                    .map(|tc| LocalToolCallRequest {
                        id: tc.id,
                        r#type: "function".to_string(),
                        function: LocalToolCallFunctionRequest {
                            name: tc.name,
                            arguments: serde_json::to_string(&tc.arguments)
                                .unwrap_or_else(|_| "{}".to_string()),
                        },
                    })
                    .collect();
                LocalMessage {
                    role: role.to_string(),
                    content: message.content,
                    tool_call_id: None,
                    tool_calls: Some(request_tool_calls),
                }
            } else {
                LocalMessage {
                    role: role.to_string(),
                    content: message.content,
                    tool_call_id: None,
                    tool_calls: None,
                }
            }
        }
        _ => LocalMessage {
            role: role.to_string(),
            content: message.content,
            tool_call_id: None,
            tool_calls: None,
        },
    }
}

#[allow(dead_code)]
fn map_tools(tools: Vec<ToolDef>) -> Option<Vec<LocalTool>> {
    if tools.is_empty() {
        return None;
    }

    Some(
        tools
            .into_iter()
            .map(|tool| LocalTool {
                r#type: "function".to_string(),
                function: LocalToolFunction {
                    name: tool.name,
                    description: Some(tool.description),
                    parameters: Some(tool.schema),
                },
            })
            .collect(),
    )
}

#[allow(dead_code)]
fn parse_stream_delta(line: &str) -> AgentResult<Option<String>> {
    let response: LocalChatResponse = serde_json::from_str(line)
        .map_err(|err| AgentError::Model(format!("local stream parse failed: {err}")))?;
    let delta = response
        .message
        .and_then(|msg| msg.content)
        .unwrap_or_default();
    if delta.is_empty() {
        Ok(None)
    } else {
        Ok(Some(delta))
    }
}

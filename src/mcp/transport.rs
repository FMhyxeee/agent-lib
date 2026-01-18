use std::sync::Arc;

use serde::{Deserialize, Serialize};
use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

use crate::error::{AgentError, AgentResult};
use crate::mcp::{McpRequest, McpResponse};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    pub endpoint: String,
}

#[derive(Debug)]
pub enum TransportKind {
    Stdio(Arc<Mutex<Child>>),
    Tcp(String),
    Http(String),
    Ws(String),
}

#[derive(Debug)]
pub struct McpTransport {
    endpoint: String,
    kind: TransportKind,
}

impl McpTransport {
    pub async fn new(config: TransportConfig) -> AgentResult<Self> {
        let endpoint = config.endpoint;
        let kind = if endpoint.starts_with("stdio://") {
            let command = endpoint.trim_start_matches("stdio://");
            let mut parts = command.split_whitespace();
            let program = parts
                .next()
                .ok_or_else(|| AgentError::Mcp("missing stdio command".to_string()))?;
            let args: Vec<String> = parts.map(|arg| arg.to_string()).collect();

            let child = Command::new(program)
                .args(args)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .map_err(|err| AgentError::Mcp(format!("spawn stdio failed: {err}")))?;
            TransportKind::Stdio(Arc::new(Mutex::new(child)))
        } else if endpoint.starts_with("tcp://") {
            TransportKind::Tcp(endpoint.trim_start_matches("tcp://").to_string())
        } else if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
            TransportKind::Http(endpoint.clone())
        } else if endpoint.starts_with("ws://") || endpoint.starts_with("wss://") {
            TransportKind::Ws(endpoint.clone())
        } else {
            return Err(AgentError::Mcp(format!(
                "unsupported transport endpoint: {endpoint}"
            )));
        };

        Ok(Self { endpoint, kind })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub async fn send(&self, request: McpRequest) -> AgentResult<McpResponse> {
        match &self.kind {
            TransportKind::Stdio(child) => {
                let mut child = child.lock().await;
                let payload = serde_json::to_string(&request)
                    .map_err(|err| AgentError::Mcp(format!("serialize failed: {err}")))?;
                {
                    let stdin = child
                        .stdin
                        .as_mut()
                        .ok_or_else(|| AgentError::Mcp("stdio stdin closed".to_string()))?;
                    stdin
                        .write_all(payload.as_bytes())
                        .await
                        .map_err(|err| AgentError::Mcp(format!("write failed: {err}")))?;
                    stdin
                        .write_all(b"\n")
                        .await
                        .map_err(|err| AgentError::Mcp(format!("write newline failed: {err}")))?;
                    stdin
                        .flush()
                        .await
                        .map_err(|err| AgentError::Mcp(format!("flush failed: {err}")))?;
                }

                let stdout = child
                    .stdout
                    .as_mut()
                    .ok_or_else(|| AgentError::Mcp("stdio stdout closed".to_string()))?;
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                reader
                    .read_line(&mut line)
                    .await
                    .map_err(|err| AgentError::Mcp(format!("read failed: {err}")))?;
                serde_json::from_str::<McpResponse>(&line)
                    .map_err(|err| AgentError::Mcp(format!("parse failed: {err}")))
            }
            TransportKind::Tcp(address) => {
                let mut stream = TcpStream::connect(address)
                    .await
                    .map_err(|err| AgentError::Mcp(format!("tcp connect failed: {err}")))?;
                let payload = serde_json::to_string(&request)
                    .map_err(|err| AgentError::Mcp(format!("serialize failed: {err}")))?;
                stream
                    .write_all(payload.as_bytes())
                    .await
                    .map_err(|err| AgentError::Mcp(format!("tcp write failed: {err}")))?;
                stream
                    .write_all(b"\n")
                    .await
                    .map_err(|err| AgentError::Mcp(format!("tcp write newline failed: {err}")))?;

                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                reader
                    .read_line(&mut line)
                    .await
                    .map_err(|err| AgentError::Mcp(format!("tcp read failed: {err}")))?;
                serde_json::from_str::<McpResponse>(&line)
                    .map_err(|err| AgentError::Mcp(format!("parse failed: {err}")))
            }
            TransportKind::Http(endpoint) => {
                let client = reqwest::Client::new();
                let response = client
                    .post(endpoint)
                    .json(&request)
                    .send()
                    .await
                    .map_err(|err| AgentError::Mcp(format!("http failed: {err}")))?;

                response
                    .json::<McpResponse>()
                    .await
                    .map_err(|err| AgentError::Mcp(format!("http parse failed: {err}")))
            }
            TransportKind::Ws(endpoint) => {
                let (mut stream, _) = tokio_tungstenite::connect_async(endpoint.as_str())
                    .await
                    .map_err(|err| AgentError::Mcp(format!("ws connect failed: {err}")))?;
                let payload = serde_json::to_string(&request)
                    .map_err(|err| AgentError::Mcp(format!("serialize failed: {err}")))?;
                stream
                    .send(Message::Text(payload))
                    .await
                    .map_err(|err| AgentError::Mcp(format!("ws send failed: {err}")))?;

                let response = stream
                    .next()
                    .await
                    .ok_or_else(|| AgentError::Mcp("ws closed".to_string()))?
                    .map_err(|err| AgentError::Mcp(format!("ws read failed: {err}")))?;

                let text = match response {
                    Message::Text(text) => text,
                    Message::Binary(bytes) => String::from_utf8(bytes)
                        .map_err(|err| AgentError::Mcp(format!("ws utf8 failed: {err}")))?,
                    _ => {
                        return Err(AgentError::Mcp(
                            "ws response not text/binary".to_string(),
                        ))
                    }
                };
                serde_json::from_str::<McpResponse>(&text)
                    .map_err(|err| AgentError::Mcp(format!("parse failed: {err}")))
            }
        }
    }
}

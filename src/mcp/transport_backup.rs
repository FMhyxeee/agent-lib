use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

use crate::error::{AgentError, AgentResult};
use crate::mcp::{McpRequest, McpResponse, config::{AuthConfig, TransportType}};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    pub endpoint: String,
}

#[derive(Debug, Clone)]
pub struct EnhancedTransportConfig {
    pub endpoint: String,
    pub transport_type: TransportType,
    pub auth: Option<AuthConfig>,
    pub headers: HashMap<String, String>,
    pub env: HashMap<String, String>,
    pub timeout: Duration,
}

impl Default for EnhancedTransportConfig {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            transport_type: TransportType::Stdio,
            auth: None,
            headers: HashMap::new(),
            env: HashMap::new(),
            timeout: Duration::from_secs(30),
        }
    }
}

impl From<TransportConfig> for EnhancedTransportConfig {
    fn from(config: TransportConfig) -> Self {
        Self {
            endpoint: config.endpoint,
            ..Default::default()
        }
    }
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
    auth: Option<AuthConfig>,
    headers: HashMap<String, String>,
}

impl McpTransport {
    pub async fn new(config: TransportConfig) -> AgentResult<Self> {
        Self::from_enhanced_config(config.into()).await
    }

    pub async fn from_enhanced_config(config: EnhancedTransportConfig) -> AgentResult<Self> {
        let endpoint = config.endpoint.clone();
        let auth = config.auth.clone();
        let headers = config.headers.clone();

        let kind = match config.transport_type {
            TransportType::Stdio => {
                let command = endpoint.trim_start_matches("stdio://");
                let mut parts = command.split_whitespace();
                let program = parts
                    .next()
                    .ok_or_else(|| AgentError::Mcp("missing stdio command".to_string()))?;
                let args: Vec<String> = parts.map(|arg| arg.to_string()).collect();

                // Set environment variables
                let mut command = Command::new(program);
                for (key, value) in &config.env {
                    command.env(key, value);
                }
                command.args(args);

                let child = command
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|err| AgentError::Mcp(format!("spawn stdio failed: {err}")))?;
                TransportKind::Stdio(Arc::new(Mutex::new(child)))
            }
            TransportType::Tcp => {
                // For TCP, the endpoint should be the address
                TransportKind::Tcp(endpoint)
            }
            TransportType::Http | TransportType::Https => {
                TransportKind::Http(endpoint)
            }
            TransportType::WebSocket | TransportType::Wss => {
                TransportKind::Ws(endpoint)
            }
            TransportType::Sse => {
                // SSE over HTTP
                TransportKind::Http(endpoint)
            }
        };

        Ok(Self {
            endpoint,
            kind,
            auth,
            headers,
        })
    }

    pub async fn send(&self, request: McpRequest) -> AgentResult<McpResponse> {
        match &self.kind {
            TransportKind::Stdio(child) => {
                let mut child = child.lock().await;
                let stdin = child.stdin.as_mut()
                    .ok_or_else(|| AgentError::Mcp("stdio stdin closed".to_string()))?;
                let payload = serde_json::to_string(&request)
                    .map_err(|err| AgentError::Mcp(format!("serialize failed: {err}")))?;
                stdin
                    .write_all(payload.as_bytes())
                    .await
                    .map_err(|err| AgentError::Mcp(format!("stdio write failed: {err}")))?;
                stdin
                    .flush()
                    .await
                    .map_err(|err| AgentError::Mcp(format!("stdio flush failed: {err}")))?;

                let stdout = child.stdout.as_mut()
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
                stream.flush().await
                    .map_err(|err| AgentError::Mcp(format!("tcp flush failed: {err}")))?;

                let mut buffer = vec![0u8; 1024];
                let n = stream
                    .read(&mut buffer)
                    .await
                    .map_err(|err| AgentError::Mcp(format!("tcp read failed: {err}")))?;
                let response = String::from_utf8_lossy(&buffer[..n]);
                serde_json::from_str::<McpResponse>(&response)
                    .map_err(|err| AgentError::Mcp(format!("parse failed: {err}")))
            }
            TransportKind::Http(url) => {
                self.send_http_request(url, request).await
            }
            TransportKind::Ws(url) => {
                self.send_websocket_request(url, request).await
            }
        }
    }

    async fn send_http_request(&self, url: &str, request: McpRequest) -> AgentResult<McpResponse> {
        let client = reqwest::Client::new();

        let mut request_builder = client
            .post(url)
            .header("Content-Type", "application/json")
            .json(&request);

        // Add custom headers
        for (key, value) in &self.headers {
            request_builder = request_builder.header(key, value);
        }

        // Add authentication
        if let Some(auth) = &self.auth {
            request_builder = self.apply_auth(request_builder, auth)?;
        }

        let response = request_builder
            .send()
            .await
            .map_err(|err| AgentError::Mcp(format!("http request failed: {err}")))?;

        if !response.status().is_success() {
            return Err(AgentError::Mcp(format!(
                "http request failed with status {}: {}",
                response.status(),
                response.text().await.unwrap_or_else(|_| "Unknown error".to_string())
            )));
        }

        response.json().await
            .map_err(|err| AgentError::Mcp(format!("http response parse failed: {err}")))
    }

    async fn send_websocket_request(&self, url: &str, request: McpRequest) -> AgentResult<McpResponse> {
        use tokio_tungstenite::connect_async;

        let (ws_stream, _) = connect_async(url)
            .await
            .map_err(|err| AgentError::Mcp(format!("websocket connect failed: {err}")))?;

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        let payload = serde_json::to_string(&request)
            .map_err(|err| AgentError::Mcp(format!("serialize failed: {err}")))?;

        ws_sender.send(Message::Text(payload))
            .await
            .map_err(|err| AgentError::Mcp(format!("websocket send failed: {err}")))?;

        let response = ws_receiver
            .next()
            .await
            .ok_or_else(|| AgentError::Mcp("websocket response missing".to_string()))?;

        match response {
            Ok(Message::Text(text)) => {
                serde_json::from_str::<McpResponse>(&text)
                    .map_err(|err| AgentError::Mcp(format!("parse failed: {err}")))
            }
            Ok(msg) => Err(AgentError::Mcp(
                "ws response not text/binary".to_string(),
            )),
            Err(err) => Err(AgentError::Mcp(format!("websocket error: {err}"))),
        }
    }

    /// Apply authentication headers to HTTP request
    fn apply_auth(
        &self,
        mut request_builder: reqwest::RequestBuilder,
        auth: &AuthConfig,
    ) -> AgentResult<reqwest::RequestBuilder> {
        match auth.auth_type {
            crate::mcp::config::AuthType::Bearer => {
                if let Some(token) = &auth.token {
                    request_builder = request_builder.header("Authorization", format!("Bearer {}", token));
                }
            }
            crate::mcp::config::AuthType::Basic => {
                if let (Some(username), Some(password)) = (&auth.username, &auth.password) {
                    let credentials = base64::encode(format!("{}:{}", username, password));
                    request_builder = request_builder.header("Authorization", format!("Basic {}", credentials));
                }
            }
            crate::mcp::config::AuthType::ApiKey => {
                if let Some(api_key) = &auth.api_key {
                    let header_name = auth.api_key_header.as_deref().unwrap_or("X-API-Key");
                    request_builder = request_builder.header(header_name, api_key);
                }
            }
            crate::mcp::config::AuthType::None => {
                // No authentication
            }
            crate::mcp::config::AuthType::OAuth2 => {
                // OAuth2 not yet implemented
                return Err(AgentError::Mcp("OAuth2 authentication not yet supported".to_string()));
            }
        }
        Ok(request_builder)
    }
}

    pub async fn from_enhanced_config(config: EnhancedTransportConfig) -> AgentResult<Self> {
        let endpoint = config.endpoint.clone();
        let auth = config.auth.clone();
        let headers = config.headers.clone();

        let kind = match config.transport_type {
            TransportType::Stdio => {
                let command = endpoint.trim_start_matches("stdio://");
                let mut parts = command.split_whitespace();
                let program = parts
                    .next()
                    .ok_or_else(|| AgentError::Mcp("missing stdio command".to_string()))?;
                let args: Vec<String> = parts.map(|arg| arg.to_string()).collect();

                // Set environment variables
                let mut command = Command::new(program);
                for (key, value) in &config.env {
                    command.env(key, value);
                }
                command.args(args);

                let child = command
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|err| AgentError::Mcp(format!("spawn stdio failed: {err}")))?;
                TransportKind::Stdio(Arc::new(Mutex::new(child)))
            }
            TransportType::Tcp => {
                // For TCP, the endpoint should be the address
                TransportKind::Tcp(endpoint)
            }
            TransportType::Http | TransportType::Https => {
                TransportKind::Http(endpoint)
            }
            TransportType::WebSocket | TransportType::Wss => {
                TransportKind::Ws(endpoint)
            }
            TransportType::Sse => {
                // SSE over HTTP
                TransportKind::Http(endpoint)
            }
        };

        Ok(Self {
            endpoint,
            kind,
            auth,
            headers,
        })
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

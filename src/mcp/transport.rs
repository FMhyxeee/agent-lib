use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::error::{AgentError, AgentResult};
use crate::mcp::config::{AuthConfig, TlsConfig, TransportType};
use crate::mcp::{McpRequest, McpResponse};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

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
    pub max_retries: usize,
    pub tls: Option<TlsConfig>,
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
            max_retries: 3,
            tls: None,
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
    #[allow(dead_code)]
    timeout: Duration,
    max_retries: usize,
    http_client: Option<reqwest::Client>,
    oauth_token_cache: Arc<Mutex<Option<OAuthToken>>>,
}

#[derive(Debug, Clone)]
struct OAuthToken {
    access_token: String,
    expires_at: Option<Instant>,
}

impl McpTransport {
    pub async fn new(config: TransportConfig) -> AgentResult<Self> {
        Self::from_enhanced_config(config.into()).await
    }

    pub async fn from_enhanced_config(config: EnhancedTransportConfig) -> AgentResult<Self> {
        let endpoint = config.endpoint;
        let auth = config.auth.clone();
        let headers = config.headers.clone();
        let timeout = config.timeout;
        let max_retries = config.max_retries;
        let http_client = if matches!(
            config.transport_type,
            TransportType::Http | TransportType::Https | TransportType::Sse
        ) {
            Some(build_http_client(timeout, config.tls.as_ref()).await?)
        } else {
            None
        };

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
                TransportKind::Tcp(endpoint.clone())
            }
            TransportType::Http | TransportType::Https => TransportKind::Http(endpoint.clone()),
            TransportType::WebSocket | TransportType::Wss => TransportKind::Ws(endpoint.clone()),
            TransportType::Sse => {
                // SSE over HTTP
                TransportKind::Http(endpoint.clone())
            }
        };

        Ok(Self {
            endpoint,
            kind,
            auth,
            headers,
            timeout,
            max_retries,
            http_client,
            oauth_token_cache: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn send(&self, request: McpRequest) -> AgentResult<McpResponse> {
        let mut attempt = 0;
        let base_request = request;
        loop {
            let request = base_request.clone();
            let result = match &self.kind {
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
                        stdin.write_all(b"\n").await.map_err(|err| {
                            AgentError::Mcp(format!("write newline failed: {err}"))
                        })?;
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
                    let mut stream = tokio::net::TcpStream::connect(address)
                        .await
                        .map_err(|err| AgentError::Mcp(format!("tcp connect failed: {err}")))?;
                    let payload = serde_json::to_string(&request)
                        .map_err(|err| AgentError::Mcp(format!("serialize failed: {err}")))?;
                    stream
                        .write_all(payload.as_bytes())
                        .await
                        .map_err(|err| AgentError::Mcp(format!("tcp write failed: {err}")))?;
                    stream.write_all(b"\n").await.map_err(|err| {
                        AgentError::Mcp(format!("tcp write newline failed: {err}"))
                    })?;
                    stream
                        .flush()
                        .await
                        .map_err(|err| AgentError::Mcp(format!("tcp flush failed: {err}")))?;

                    let mut reader = BufReader::new(stream);
                    let mut line = String::new();
                    reader
                        .read_line(&mut line)
                        .await
                        .map_err(|err| AgentError::Mcp(format!("tcp read failed: {err}")))?;
                    serde_json::from_str::<McpResponse>(&line)
                        .map_err(|err| AgentError::Mcp(format!("parse failed: {err}")))
                }
                TransportKind::Http(url) => self.send_http_request(url, request).await,
                TransportKind::Ws(url) => self.send_websocket_request(url, request).await,
            };

            match result {
                Ok(response) => return Ok(response),
                Err(err) => {
                    attempt += 1;
                    if attempt > self.max_retries {
                        return Err(err);
                    }
                    let backoff = Duration::from_millis(100 * attempt as u64);
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    }

    async fn send_http_request(&self, url: &str, request: McpRequest) -> AgentResult<McpResponse> {
        let client = self
            .http_client
            .clone()
            .unwrap_or_else(reqwest::Client::new);

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
            request_builder = self.apply_auth(request_builder, auth).await?;
        }

        let response = request_builder
            .send()
            .await
            .map_err(|err| AgentError::Mcp(format!("http request failed: {err}")))?;

        if !response.status().is_success() {
            return Err(AgentError::Mcp(format!(
                "http request failed with status {}: {}",
                response.status(),
                response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string())
            )));
        }

        response
            .json()
            .await
            .map_err(|err| AgentError::Mcp(format!("http response parse failed: {err}")))
    }

    async fn send_websocket_request(
        &self,
        url: &str,
        request: McpRequest,
    ) -> AgentResult<McpResponse> {
        use tokio_tungstenite::connect_async;

        let (ws_stream, _) = connect_async(url)
            .await
            .map_err(|err| AgentError::Mcp(format!("websocket connect failed: {err}")))?;

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        let payload = serde_json::to_string(&request)
            .map_err(|err| AgentError::Mcp(format!("serialize failed: {err}")))?;

        ws_sender
            .send(tokio_tungstenite::tungstenite::Message::Text(payload))
            .await
            .map_err(|err| AgentError::Mcp(format!("websocket send failed: {err}")))?;

        let response = ws_receiver
            .next()
            .await
            .ok_or_else(|| AgentError::Mcp("websocket response missing".to_string()))?;

        match response {
            Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                serde_json::from_str::<McpResponse>(&text)
                    .map_err(|err| AgentError::Mcp(format!("parse failed: {err}")))
            }
            Ok(_msg) => Err(AgentError::Mcp("ws response not text/binary".to_string())),
            Err(err) => Err(AgentError::Mcp(format!("websocket error: {err}"))),
        }
    }

    /// Apply authentication headers to HTTP request
    async fn apply_auth(
        &self,
        mut request_builder: reqwest::RequestBuilder,
        auth: &AuthConfig,
    ) -> AgentResult<reqwest::RequestBuilder> {
        match auth.auth_type {
            crate::mcp::config::AuthType::Bearer => {
                if let Some(token) = &auth.token {
                    request_builder =
                        request_builder.header("Authorization", format!("Bearer {}", token));
                    if let Some(param) = &auth.query_param {
                        request_builder = request_builder.query(&[(param, token)]);
                    }
                }
            }
            crate::mcp::config::AuthType::Basic => {
                if let (Some(username), Some(password)) = (&auth.username, &auth.password) {
                    let credentials = STANDARD.encode(format!("{}:{}", username, password));
                    request_builder =
                        request_builder.header("Authorization", format!("Basic {}", credentials));
                }
            }
            crate::mcp::config::AuthType::ApiKey => {
                if let Some(api_key) = &auth.api_key {
                    let header_name = auth.api_key_header.as_deref().unwrap_or("X-API-Key");
                    request_builder = request_builder.header(header_name, api_key);
                    if let Some(param) = &auth.query_param {
                        request_builder = request_builder.query(&[(param, api_key)]);
                    }
                }
            }
            crate::mcp::config::AuthType::None => {
                // No authentication
            }
            crate::mcp::config::AuthType::OAuth2 => {
                let token = self.resolve_oauth_token(auth).await?;
                request_builder =
                    request_builder.header("Authorization", format!("Bearer {}", token));
                if let Some(param) = &auth.query_param {
                    request_builder = request_builder.query(&[(param, token)]);
                }
            }
        }
        Ok(request_builder)
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

async fn build_http_client(
    timeout: Duration,
    tls: Option<&TlsConfig>,
) -> AgentResult<reqwest::Client> {
    let mut builder = reqwest::Client::builder().timeout(timeout);
    if let Some(tls) = tls {
        if let Some(path) = &tls.ca_cert_path {
            let pem = tokio::fs::read(path)
                .await
                .map_err(|err| AgentError::Mcp(format!("read ca cert failed: {err}")))?;
            let cert = reqwest::Certificate::from_pem(&pem)
                .map_err(|err| AgentError::Mcp(format!("invalid ca cert: {err}")))?;
            builder = builder.add_root_certificate(cert);
        }
        if tls.danger_accept_invalid_certs {
            builder = builder.danger_accept_invalid_certs(true);
        }
        if tls.danger_accept_invalid_hostnames {
            builder = builder.danger_accept_invalid_hostnames(true);
        }
        if let Some(cert_path) = &tls.client_cert_path {
            let mut pem = tokio::fs::read(cert_path)
                .await
                .map_err(|err| AgentError::Mcp(format!("read client cert failed: {err}")))?;
            if let Some(key_path) = &tls.client_key_path {
                let key = tokio::fs::read(key_path)
                    .await
                    .map_err(|err| AgentError::Mcp(format!("read client key failed: {err}")))?;
                pem.extend_from_slice(&key);
            }
            let identity = reqwest::Identity::from_pem(&pem)
                .map_err(|err| AgentError::Mcp(format!("invalid client identity: {err}")))?;
            builder = builder.identity(identity);
        }
    }
    builder
        .build()
        .map_err(|err| AgentError::Mcp(format!("build http client failed: {err}")))
}

impl McpTransport {
    async fn resolve_oauth_token(&self, auth: &AuthConfig) -> AgentResult<String> {
        if let Some(token) = &auth.token {
            return Ok(token.clone());
        }

        let token_url = auth
            .token_url
            .as_ref()
            .ok_or_else(|| AgentError::Mcp("oauth2 token_url missing".to_string()))?;
        let client_id = auth
            .client_id
            .as_ref()
            .ok_or_else(|| AgentError::Mcp("oauth2 client_id missing".to_string()))?;
        let client_secret = auth
            .client_secret
            .as_ref()
            .ok_or_else(|| AgentError::Mcp("oauth2 client_secret missing".to_string()))?;

        if let Some(cached) = self.oauth_token_cache.lock().await.clone() {
            if let Some(expires_at) = cached.expires_at {
                if Instant::now() < expires_at {
                    return Ok(cached.access_token);
                }
            } else {
                return Ok(cached.access_token);
            }
        }

        let mut params = vec![
            ("grant_type", "client_credentials".to_string()),
            ("client_id", client_id.clone()),
            ("client_secret", client_secret.clone()),
        ];
        if let Some(scope) = &auth.scope {
            params.push(("scope", scope.clone()));
        }
        if let Some(audience) = &auth.audience {
            params.push(("audience", audience.clone()));
        }

        let client = reqwest::Client::new();
        let response = client
            .post(token_url)
            .form(&params)
            .send()
            .await
            .map_err(|err| AgentError::Mcp(format!("oauth2 token request failed: {err}")))?;

        if !response.status().is_success() {
            return Err(AgentError::Mcp(format!(
                "oauth2 token request failed with status {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        #[derive(serde::Deserialize)]
        struct TokenResponse {
            access_token: String,
            #[serde(default)]
            expires_in: Option<u64>,
            #[serde(default)]
            token_type: Option<String>,
        }

        let token: TokenResponse = response
            .json()
            .await
            .map_err(|err| AgentError::Mcp(format!("oauth2 token parse failed: {err}")))?;

        let expires_at = token
            .expires_in
            .map(|secs| Instant::now() + Duration::from_secs(secs.saturating_sub(30)));

        let access_token = token.access_token;
        let mut cache = self.oauth_token_cache.lock().await;
        *cache = Some(OAuthToken {
            access_token: access_token.clone(),
            expires_at,
        });

        let token_type = token.token_type.unwrap_or_else(|| "Bearer".to_string());
        if token_type.to_lowercase() != "bearer" {
            return Err(AgentError::Mcp(format!(
                "unsupported oauth2 token type: {token_type}"
            )));
        }

        Ok(access_token)
    }
}

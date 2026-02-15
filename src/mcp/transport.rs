use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::{SinkExt, StreamExt};
use rmcp::model::{
    CallToolRequestParam, GetPromptRequestParam, PromptMessageContent, PromptMessageRole,
    ReadResourceRequestParam, ResourceContents,
};
use rmcp::service::RunningService;
use rmcp::transport::TokioChildProcess;
use rmcp::{RoleClient, ServiceExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::error::{AgentError, AgentResult};
use crate::mcp::config::{AuthConfig, TlsConfig, TransportType};
use crate::mcp::{
    McpPrompt, McpPromptArgument, McpPromptContent, McpPromptMessage, McpPromptResult, McpRequest,
    McpResource, McpResourceContent, McpResponse, McpTool,
};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

type RmcpClientService = RunningService<RoleClient, ()>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    #[serde(default)]
    pub endpoint: String,
}

#[derive(Debug, Clone)]
pub struct EnhancedTransportConfig {
    pub endpoint: String,
    pub command: Option<String>,
    pub args: Vec<String>,
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
            command: None,
            args: Vec::new(),
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
            command: None,
            args: Vec::new(),
            ..Default::default()
        }
    }
}

#[derive(Debug)]
pub enum TransportKind {
    Stdio(Arc<Mutex<RmcpClientService>>),
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
                let (program, args) = if let Some(command) = config.command.as_ref() {
                    let trimmed = command.trim();
                    if trimmed.is_empty() {
                        return Err(AgentError::Mcp("missing stdio command".to_string()));
                    }
                    (trimmed.to_string(), config.args.clone())
                } else {
                    let command = endpoint.trim_start_matches("stdio://");
                    let mut parts = command.split_whitespace();
                    let program = parts
                        .next()
                        .ok_or_else(|| AgentError::Mcp("missing stdio command".to_string()))?;
                    let args: Vec<String> = parts.map(|arg| arg.to_string()).collect();
                    (program.to_string(), args)
                };

                let service = connect_stdio_client(&program, &args, &config.env)
                    .await
                    .map_err(|err| AgentError::Mcp(format!("spawn stdio failed: {err}")))?;
                TransportKind::Stdio(Arc::new(Mutex::new(service)))
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
                TransportKind::Stdio(service) => {
                    let mut service = service.lock().await;
                    self.send_stdio_request(&mut service, request).await
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

    async fn send_stdio_request(
        &self,
        service: &mut RmcpClientService,
        request: McpRequest,
    ) -> AgentResult<McpResponse> {
        let method = request.method.as_str();
        let result = match method {
            "tools/list" => {
                let tools = service
                    .list_all_tools()
                    .await
                    .map_err(|err| AgentError::Mcp(format!("tools/list failed: {err}")))?;
                let mapped: Vec<McpTool> = tools.into_iter().map(map_rmcp_tool).collect();
                serde_json::to_value(mapped)
                    .map_err(|err| AgentError::Mcp(format!("serialize tools failed: {err}")))?
            }
            "tools/call" => {
                let params = request.params.as_object().ok_or_else(|| {
                    AgentError::Mcp("tools/call params must be an object".to_string())
                })?;
                let name = params
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| AgentError::Mcp("tools/call missing name".to_string()))?;
                let arguments = params
                    .get("args")
                    .or_else(|| params.get("arguments"))
                    .and_then(Value::as_object)
                    .cloned();

                let call_result = service
                    .call_tool(CallToolRequestParam {
                        name: name.to_string().into(),
                        arguments,
                    })
                    .await
                    .map_err(|err| AgentError::Mcp(format!("tools/call failed: {err}")))?;

                serde_json::to_value(call_result).map_err(|err| {
                    AgentError::Mcp(format!("serialize tool result failed: {err}"))
                })?
            }
            "resources/list" => {
                let resources = service
                    .list_all_resources()
                    .await
                    .map_err(|err| AgentError::Mcp(format!("resources/list failed: {err}")))?;
                let mapped: Vec<McpResource> =
                    resources.into_iter().map(map_rmcp_resource).collect();
                serde_json::to_value(mapped)
                    .map_err(|err| AgentError::Mcp(format!("serialize resources failed: {err}")))?
            }
            "resources/read" => {
                let params = request.params.as_object().ok_or_else(|| {
                    AgentError::Mcp("resources/read params must be an object".to_string())
                })?;
                let uri = params
                    .get("uri")
                    .and_then(Value::as_str)
                    .ok_or_else(|| AgentError::Mcp("resources/read missing uri".to_string()))?;

                let read_result = service
                    .read_resource(ReadResourceRequestParam {
                        uri: uri.to_string(),
                    })
                    .await
                    .map_err(|err| AgentError::Mcp(format!("resources/read failed: {err}")))?;
                let mapped = map_rmcp_resource_content(read_result)?;
                serde_json::to_value(mapped).map_err(|err| {
                    AgentError::Mcp(format!("serialize resource content failed: {err}"))
                })?
            }
            "prompts/list" => {
                let prompts = service
                    .list_all_prompts()
                    .await
                    .map_err(|err| AgentError::Mcp(format!("prompts/list failed: {err}")))?;
                let mapped: Vec<McpPrompt> = prompts.into_iter().map(map_rmcp_prompt).collect();
                serde_json::to_value(mapped)
                    .map_err(|err| AgentError::Mcp(format!("serialize prompts failed: {err}")))?
            }
            "prompts/get" => {
                let params = request.params.as_object().ok_or_else(|| {
                    AgentError::Mcp("prompts/get params must be an object".to_string())
                })?;
                let name = params
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| AgentError::Mcp("prompts/get missing name".to_string()))?;
                let arguments = params.get("arguments").and_then(Value::as_object).cloned();

                let prompt_result = service
                    .get_prompt(GetPromptRequestParam {
                        name: name.to_string(),
                        arguments,
                    })
                    .await
                    .map_err(|err| AgentError::Mcp(format!("prompts/get failed: {err}")))?;
                let mapped = map_rmcp_prompt_result(prompt_result);
                serde_json::to_value(mapped).map_err(|err| {
                    AgentError::Mcp(format!("serialize prompt result failed: {err}"))
                })?
            }
            _ => {
                return Err(AgentError::Mcp(format!(
                    "unsupported MCP method for stdio transport: {method}"
                )));
            }
        };

        Ok(McpResponse {
            id: request.id,
            result,
            error: None,
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_stdio_command_attempts_keep_first_command() {
        let args = vec!["-y".to_string(), "@z_ai/mcp-server".to_string()];
        let attempts = stdio_command_attempts("npx", &args);

        assert!(!attempts.is_empty());
        assert_eq!(attempts[0].program, "npx");
        assert_eq!(attempts[0].args, args);
    }

    #[test]
    fn test_map_rmcp_tool_uses_input_schema() {
        let mut schema = serde_json::Map::new();
        schema.insert("type".to_string(), Value::String("object".to_string()));

        let tool = rmcp::model::Tool {
            name: "demo".into(),
            title: None,
            description: Some("demo tool".into()),
            input_schema: Arc::new(schema.clone()),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        };

        let mapped = map_rmcp_tool(tool);
        assert_eq!(mapped.name, "demo");
        assert_eq!(mapped.description, "demo tool");
        assert_eq!(mapped.schema, Value::Object(schema));
    }

    #[test]
    fn test_map_rmcp_resource_content_from_text() {
        let result = rmcp::model::ReadResourceResult {
            contents: vec![ResourceContents::TextResourceContents {
                uri: "file:///demo.txt".to_string(),
                mime_type: Some("text/plain".to_string()),
                text: "hello".to_string(),
                meta: None,
            }],
        };

        let mapped = map_rmcp_resource_content(result).expect("mapping should succeed");
        assert_eq!(mapped.uri, "file:///demo.txt");
        assert_eq!(mapped.mime_type.as_deref(), Some("text/plain"));
        assert_eq!(mapped.content, "hello");
    }

    #[test]
    fn test_map_rmcp_resource_content_from_blob() {
        let result = rmcp::model::ReadResourceResult {
            contents: vec![ResourceContents::BlobResourceContents {
                uri: "file:///demo.bin".to_string(),
                mime_type: Some("application/octet-stream".to_string()),
                blob: "YmFzZTY0".to_string(),
                meta: None,
            }],
        };

        let mapped = map_rmcp_resource_content(result).expect("mapping should succeed");
        assert_eq!(mapped.uri, "file:///demo.bin");
        assert_eq!(
            mapped.mime_type.as_deref(),
            Some("application/octet-stream")
        );
        assert_eq!(mapped.content, "YmFzZTY0");
    }
}

#[derive(Debug, Clone)]
struct CommandAttempt {
    program: String,
    args: Vec<String>,
}

impl CommandAttempt {
    fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
        }
    }

    fn display(&self) -> String {
        if self.args.is_empty() {
            self.program.clone()
        } else {
            format!("{} {}", self.program, self.args.join(" "))
        }
    }
}

#[cfg(windows)]
fn stdio_command_attempts(program: &str, args: &[String]) -> Vec<CommandAttempt> {
    let mut attempts = vec![CommandAttempt::new(program.to_string(), args.to_vec())];

    let is_simple_program_name =
        !program.contains('.') && !program.contains('\\') && !program.contains('/');
    if is_simple_program_name {
        for suffix in [".cmd", ".exe", ".bat"] {
            attempts.push(CommandAttempt::new(
                format!("{program}{suffix}"),
                args.to_vec(),
            ));
        }

        let mut shell_args = Vec::with_capacity(args.len() + 2);
        shell_args.push("/C".to_string());
        shell_args.push(program.to_string());
        shell_args.extend(args.iter().cloned());

        attempts.push(CommandAttempt::new("cmd", shell_args.clone()));
        attempts.push(CommandAttempt::new("cmd.exe", shell_args));
    }

    attempts
}

#[cfg(not(windows))]
fn stdio_command_attempts(program: &str, args: &[String]) -> Vec<CommandAttempt> {
    vec![CommandAttempt::new(program.to_string(), args.to_vec())]
}

async fn connect_stdio_client(
    program: &str,
    args: &[String],
    env: &HashMap<String, String>,
) -> Result<RmcpClientService, String> {
    let mut last_error: Option<String> = None;

    for attempt in stdio_command_attempts(program, args) {
        let label = attempt.display();

        let mut command = Command::new(&attempt.program);
        command.args(&attempt.args);
        for (key, value) in env {
            command.env(key, value);
        }

        match TokioChildProcess::new(command) {
            Ok(transport) => match ().serve(transport).await {
                Ok(service) => return Ok(service),
                Err(err) => {
                    last_error = Some(format!("initialize failed for `{label}`: {err}"));
                }
            },
            Err(err) => {
                last_error = Some(format!("spawn failed for `{label}`: {err}"));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| "unknown error".to_string()))
}

fn map_rmcp_tool(tool: rmcp::model::Tool) -> McpTool {
    McpTool {
        name: tool.name.into_owned(),
        description: tool
            .description
            .map(|desc| desc.into_owned())
            .unwrap_or_default(),
        schema: Value::Object((*tool.input_schema).clone()),
    }
}

fn map_rmcp_resource(resource: rmcp::model::Resource) -> McpResource {
    let raw = resource.raw;
    McpResource {
        uri: raw.uri,
        name: raw.name,
        description: raw.description,
        mime_type: raw.mime_type,
    }
}

fn map_rmcp_resource_content(
    result: rmcp::model::ReadResourceResult,
) -> AgentResult<McpResourceContent> {
    let first = result
        .contents
        .into_iter()
        .next()
        .ok_or_else(|| AgentError::Mcp("resource response had no contents".to_string()))?;

    match first {
        ResourceContents::TextResourceContents {
            uri,
            mime_type,
            text,
            ..
        } => Ok(McpResourceContent {
            uri,
            mime_type,
            content: text,
        }),
        ResourceContents::BlobResourceContents {
            uri,
            mime_type,
            blob,
            ..
        } => Ok(McpResourceContent {
            uri,
            mime_type,
            content: blob,
        }),
    }
}

fn map_rmcp_prompt(prompt: rmcp::model::Prompt) -> McpPrompt {
    McpPrompt {
        name: prompt.name,
        description: prompt.description,
        arguments: prompt.arguments.map(|arguments| {
            arguments
                .into_iter()
                .map(|arg| McpPromptArgument {
                    name: arg.name,
                    description: arg.description,
                    required: arg.required.unwrap_or(false),
                })
                .collect()
        }),
    }
}

fn map_rmcp_prompt_result(result: rmcp::model::GetPromptResult) -> McpPromptResult {
    McpPromptResult {
        description: result.description,
        messages: result
            .messages
            .into_iter()
            .map(|message| {
                let role = match message.role {
                    PromptMessageRole::User => "user".to_string(),
                    PromptMessageRole::Assistant => "assistant".to_string(),
                };

                let content = match message.content {
                    PromptMessageContent::Text { text } => McpPromptContent::Text { text },
                    PromptMessageContent::Image { image } => {
                        let raw = image.raw;
                        McpPromptContent::Image {
                            data: raw.data,
                            mime_type: raw.mime_type,
                        }
                    }
                    PromptMessageContent::Resource { resource } => match resource.raw.resource {
                        ResourceContents::TextResourceContents { text, .. } => {
                            McpPromptContent::Text { text }
                        }
                        ResourceContents::BlobResourceContents { blob, .. } => {
                            McpPromptContent::Text { text: blob }
                        }
                    },
                    PromptMessageContent::ResourceLink { link } => {
                        McpPromptContent::Text { text: link.raw.uri }
                    }
                };

                McpPromptMessage { role, content }
            })
            .collect(),
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

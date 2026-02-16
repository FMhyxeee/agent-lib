use std::collections::HashMap;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use once_cell::sync::Lazy;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use rmcp::service::RunningService;
use rmcp::transport::{
    StreamableHttpClientTransport, TokioChildProcess,
    streamable_http_client::StreamableHttpClientTransportConfig,
};
use rmcp::{RoleClient, ServiceExt};
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::error::{AgentError, AgentResult};
use crate::mcp::config::{AuthConfig, AuthType, ServerConfig, TlsConfig, TransportType};

pub(crate) type RmcpClientService = RunningService<RoleClient, ()>;

#[derive(Debug, Clone)]
struct OAuthToken {
    access_token: String,
    expires_at: Option<Instant>,
}

static OAUTH_TOKEN_CACHE: Lazy<Mutex<HashMap<String, OAuthToken>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub(crate) async fn connect_client(config: &ServerConfig) -> AgentResult<RmcpClientService> {
    match config.transport {
        TransportType::Stdio => connect_stdio_client(config).await,
        TransportType::StreamableHttp => connect_streamable_http_client(config).await,
    }
}

async fn connect_stdio_client(config: &ServerConfig) -> AgentResult<RmcpClientService> {
    let (program, args) = resolve_stdio_command(config)?;

    let service = connect_stdio_process(&program, &args, &config.env)
        .await
        .map_err(|err| AgentError::Mcp(format!("spawn stdio failed: {err}")))?;

    Ok(service)
}

async fn connect_streamable_http_client(config: &ServerConfig) -> AgentResult<RmcpClientService> {
    let endpoint = config.endpoint.trim();
    if endpoint.is_empty() {
        return Err(AgentError::Mcp(
            "streamable_http endpoint cannot be empty".to_string(),
        ));
    }
    if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
        return Err(AgentError::Mcp(
            "streamable_http endpoint must start with http:// or https://".to_string(),
        ));
    }

    let mut endpoint_with_auth = endpoint.to_string();
    let mut headers = config.headers.clone();
    let mut auth_header: Option<String> = None;

    if let Some(auth) = &config.auth {
        apply_auth_to_streamable_http(
            auth,
            config.timeout,
            &mut endpoint_with_auth,
            &mut headers,
            &mut auth_header,
        )
        .await?;
    }

    let client = build_http_client(config.timeout, config.tls.as_ref(), &headers).await?;

    let mut transport_config = StreamableHttpClientTransportConfig::with_uri(endpoint_with_auth);
    if let Some(token) = auth_header {
        transport_config = transport_config.auth_header(token);
    }

    let transport = StreamableHttpClientTransport::with_client(client, transport_config);
    ().serve(transport)
        .await
        .map_err(|err| AgentError::Mcp(format!("streamable_http connect failed: {err}")))
}

fn resolve_stdio_command(config: &ServerConfig) -> AgentResult<(String, Vec<String>)> {
    if let Some(command) = config.command.as_ref() {
        let trimmed = command.trim();
        if trimmed.is_empty() {
            return Err(AgentError::Mcp("missing stdio command".to_string()));
        }
        return Ok((trimmed.to_string(), config.args.clone()));
    }

    let endpoint = config.endpoint.trim();
    let command = endpoint.strip_prefix("stdio://").unwrap_or(endpoint);

    let mut parts = command.split_whitespace();
    let program = parts
        .next()
        .ok_or_else(|| AgentError::Mcp("missing stdio command".to_string()))?;
    let args: Vec<String> = parts.map(|arg| arg.to_string()).collect();

    Ok((program.to_string(), args))
}

async fn apply_auth_to_streamable_http(
    auth: &AuthConfig,
    timeout: Duration,
    endpoint: &mut String,
    headers: &mut HashMap<String, String>,
    auth_header: &mut Option<String>,
) -> AgentResult<()> {
    match auth.auth_type {
        AuthType::Bearer => {
            let token = auth
                .token
                .clone()
                .ok_or_else(|| AgentError::Mcp("bearer auth requires token".to_string()))?;
            *auth_header = Some(token.clone());
            if let Some(param) = auth.query_param.as_ref() {
                *endpoint = add_query_param(endpoint, param, &token)?;
            }
        }
        AuthType::Basic => {
            let username = auth
                .username
                .as_ref()
                .ok_or_else(|| AgentError::Mcp("basic auth requires username".to_string()))?;
            let password = auth
                .password
                .as_ref()
                .ok_or_else(|| AgentError::Mcp("basic auth requires password".to_string()))?;
            let encoded = STANDARD.encode(format!("{username}:{password}"));
            headers.insert("Authorization".to_string(), format!("Basic {encoded}"));
        }
        AuthType::ApiKey => {
            let api_key = auth
                .api_key
                .clone()
                .ok_or_else(|| AgentError::Mcp("api key auth requires api_key".to_string()))?;
            let header_name = auth
                .api_key_header
                .clone()
                .unwrap_or_else(|| "X-API-Key".to_string());
            headers.insert(header_name, api_key.clone());
            if let Some(param) = auth.query_param.as_ref() {
                *endpoint = add_query_param(endpoint, param, &api_key)?;
            }
        }
        AuthType::OAuth2 => {
            let token = resolve_oauth_token(auth, timeout).await?;
            *auth_header = Some(token.clone());
            if let Some(param) = auth.query_param.as_ref() {
                *endpoint = add_query_param(endpoint, param, &token)?;
            }
        }
        AuthType::None => {}
    }

    Ok(())
}

fn add_query_param(endpoint: &str, key: &str, value: &str) -> AgentResult<String> {
    let mut url = reqwest::Url::parse(endpoint)
        .map_err(|err| AgentError::Mcp(format!("invalid endpoint URL: {err}")))?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair(key, value);
    }
    Ok(url.to_string())
}

async fn build_http_client(
    timeout: Duration,
    tls: Option<&TlsConfig>,
    headers: &HashMap<String, String>,
) -> AgentResult<reqwest::Client> {
    let mut default_headers = HeaderMap::new();
    for (key, value) in headers {
        let name = HeaderName::from_bytes(key.as_bytes())
            .map_err(|err| AgentError::Mcp(format!("invalid header name '{key}': {err}")))?;
        let header_value = HeaderValue::from_str(value)
            .map_err(|err| AgentError::Mcp(format!("invalid header value for '{key}': {err}")))?;
        default_headers.insert(name, header_value);
    }

    let mut builder = reqwest::Client::builder()
        .timeout(timeout)
        .default_headers(default_headers);

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

async fn resolve_oauth_token(auth: &AuthConfig, timeout: Duration) -> AgentResult<String> {
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

    let cache_key = format!(
        "{}|{}|{}|{}",
        token_url,
        client_id,
        auth.scope.clone().unwrap_or_default(),
        auth.audience.clone().unwrap_or_default()
    );

    if let Some(cached) = OAUTH_TOKEN_CACHE.lock().await.get(&cache_key).cloned() {
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

    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|err| AgentError::Mcp(format!("build oauth client failed: {err}")))?;

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

    let token_type = token.token_type.unwrap_or_else(|| "Bearer".to_string());
    if token_type.to_lowercase() != "bearer" {
        return Err(AgentError::Mcp(format!(
            "unsupported oauth2 token type: {token_type}"
        )));
    }

    let expires_at = token
        .expires_in
        .map(|secs| Instant::now() + Duration::from_secs(secs.saturating_sub(30)));

    OAUTH_TOKEN_CACHE.lock().await.insert(
        cache_key,
        OAuthToken {
            access_token: token.access_token.clone(),
            expires_at,
        },
    );

    Ok(token.access_token)
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

async fn connect_stdio_process(
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

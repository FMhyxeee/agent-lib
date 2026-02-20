use serde::de::{self, Deserializer, Unexpected, Visitor};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::error::{AgentError, AgentResult};

/// MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Unique server name
    pub name: String,

    /// Transport type
    #[serde(default)]
    pub transport: TransportType,

    /// Endpoint for stdio/http transports
    #[serde(default)]
    pub endpoint: String,

    /// Stdio command (used when transport=stdio)
    #[serde(default)]
    pub command: Option<String>,

    /// Stdio command arguments (used when transport=stdio)
    #[serde(default)]
    pub args: Vec<String>,

    /// Authentication configuration
    #[serde(default)]
    pub auth: Option<AuthConfig>,

    /// Custom HTTP headers
    #[serde(default)]
    pub headers: HashMap<String, String>,

    /// TLS configuration (for HTTPS)
    #[serde(default)]
    pub tls: Option<TlsConfig>,

    /// Server-specific timeout
    #[serde(
        default = "default_timeout",
        serialize_with = "duration_seconds::serialize",
        deserialize_with = "duration_seconds::deserialize"
    )]
    pub timeout: Duration,

    /// Whether this server is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Environment variables for this server
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// Supported MCP transport types in strict official mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransportType {
    /// Child process stdio transport
    #[default]
    Stdio,
    /// Streamable HTTP transport (http/https)
    StreamableHttp,
}

impl TransportType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::StreamableHttp => "streamable_http",
        }
    }
}

impl Serialize for TransportType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TransportType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TransportTypeVisitor;

        impl<'de> Visitor<'de> for TransportTypeVisitor {
            type Value = TransportType;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a transport type string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let normalized = value.trim().to_ascii_lowercase();
                match normalized.as_str() {
                    "stdio" => Ok(TransportType::Stdio),
                    "streamable_http" | "streamable-http" | "streamablehttp" | "http" | "https" => {
                        Ok(TransportType::StreamableHttp)
                    }
                    "tcp" | "websocket" | "ws" | "wss" | "sse" => {
                        Err(E::custom(unsupported_transport_message(value)))
                    }
                    _ => Err(E::invalid_value(Unexpected::Str(value), &self)),
                }
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_str(&value)
            }
        }

        deserializer.deserialize_any(TransportTypeVisitor)
    }
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Authentication type
    #[serde(rename = "type")]
    pub auth_type: AuthType,

    /// Bearer token (for Bearer auth)
    pub token: Option<String>,

    /// Username (for Basic auth)
    pub username: Option<String>,

    /// Password (for Basic auth)
    pub password: Option<String>,

    /// API key (for API Key auth)
    pub api_key: Option<String>,

    /// Custom header name for API key
    pub api_key_header: Option<String>,

    /// Token in query parameter
    pub query_param: Option<String>,

    /// OAuth2 token URL
    pub token_url: Option<String>,

    /// OAuth2 client ID
    pub client_id: Option<String>,

    /// OAuth2 client secret
    pub client_secret: Option<String>,

    /// OAuth2 scope (space separated)
    pub scope: Option<String>,

    /// OAuth2 audience (optional)
    pub audience: Option<String>,
}

/// Authentication types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum AuthType {
    /// No authentication
    #[default]
    None,
    /// Bearer token authentication
    Bearer,
    /// Basic username/password authentication
    Basic,
    /// API key authentication
    ApiKey,
    /// OAuth2 authentication
    #[serde(rename = "oauth2")]
    OAuth2,
}

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TlsConfig {
    /// CA certificate path (PEM)
    pub ca_cert_path: Option<String>,
    /// Client certificate path (PEM)
    pub client_cert_path: Option<String>,
    /// Client key path (PEM)
    pub client_key_path: Option<String>,
    /// Allow invalid certs (dangerous)
    #[serde(default)]
    pub danger_accept_invalid_certs: bool,
    /// Allow invalid hostnames (dangerous)
    #[serde(default)]
    pub danger_accept_invalid_hostnames: bool,
}

/// Complete MCP configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    /// General configuration
    #[serde(default)]
    pub general: GeneralConfig,

    /// List of MCP servers
    #[serde(default)]
    pub servers: Vec<ServerConfig>,
}

/// General configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// Default timeout for all MCP operations
    #[serde(
        default = "default_timeout",
        serialize_with = "duration_seconds::serialize",
        deserialize_with = "duration_seconds::deserialize"
    )]
    pub default_timeout: Duration,

    /// Maximum number of retries for failed operations
    #[serde(default = "default_max_retries")]
    pub max_retries: usize,

    /// Whether to enable logging
    #[serde(default = "default_enabled")]
    pub logging_enabled: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(30),
            max_retries: 3,
            logging_enabled: true,
        }
    }
}

/// Configuration loader
pub struct ConfigLoader;

impl ConfigLoader {
    /// Load configuration from a TOML file
    pub async fn from_toml_file(path: impl AsRef<Path>) -> AgentResult<McpConfig> {
        let path = path.as_ref();
        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            AgentError::Mcp(format!(
                "failed to read config file '{}': {}",
                path.display(),
                e
            ))
        })?;

        let mut config: McpConfig = toml::from_str(&content).map_err(|e| {
            AgentError::Mcp(format!(
                "failed to parse TOML config from '{}': {}",
                path.display(),
                e
            ))
        })?;

        config.expand_environment_variables()?;
        Ok(config)
    }

    /// Load configuration from a JSON file
    pub async fn from_json_file(path: impl AsRef<Path>) -> AgentResult<McpConfig> {
        let path = path.as_ref();
        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            AgentError::Mcp(format!(
                "failed to read config file '{}': {}",
                path.display(),
                e
            ))
        })?;

        let mut config: McpConfig = serde_json::from_str(&content).map_err(|e| {
            AgentError::Mcp(format!(
                "failed to parse JSON config from '{}': {}",
                path.display(),
                e
            ))
        })?;

        config.expand_environment_variables()?;
        Ok(config)
    }

    /// Load configuration from environment variables
    pub async fn from_env() -> AgentResult<McpConfig> {
        let mut config = McpConfig::default();

        if let Ok(json_config) = env::var("MCP_CONFIG_JSON") {
            config = serde_json::from_str(&json_config)
                .map_err(|e| AgentError::Mcp(format!("failed to parse MCP_CONFIG_JSON: {}", e)))?;
        }

        config.expand_environment_variables()?;
        Ok(config)
    }

    /// Try to load configuration from common locations
    pub async fn from_common_locations() -> AgentResult<McpConfig> {
        let locations = vec![
            PathBuf::from("/etc/agent-lib/mcp.toml"),
            PathBuf::from("/etc/agent-lib/mcp.json"),
            PathBuf::from(env::var("HOME").unwrap_or_else(|_| ".".to_string()))
                .join(".config/agent-lib/mcp.toml"),
            PathBuf::from(env::var("HOME").unwrap_or_else(|_| ".".to_string()))
                .join(".config/agent-lib/mcp.json"),
            PathBuf::from("./mcp.toml"),
            PathBuf::from("./mcp.json"),
        ];

        for location in locations {
            if location.exists() {
                match location.extension().and_then(|s| s.to_str()) {
                    Some("toml") => {
                        if let Ok(config) = Self::from_toml_file(&location).await {
                            tracing::info!("Loaded MCP config from: {}", location.display());
                            return Ok(config);
                        }
                    }
                    Some("json") => {
                        if let Ok(config) = Self::from_json_file(&location).await {
                            tracing::info!("Loaded MCP config from: {}", location.display());
                            return Ok(config);
                        }
                    }
                    _ => {}
                }
            }
        }

        Self::from_env().await
    }
}

impl McpConfig {
    /// Expand environment variables in configuration
    pub fn expand_environment_variables(&mut self) -> AgentResult<()> {
        for server in &mut self.servers {
            server.name = Self::expand_env_vars(&server.name);
            server.endpoint = Self::expand_env_vars(&server.endpoint);
            server.command = server
                .command
                .as_ref()
                .map(|cmd| Self::expand_env_vars(cmd));
            server.args = server
                .args
                .iter()
                .map(|arg| Self::expand_env_vars(arg))
                .collect();

            let mut expanded_headers = HashMap::new();
            for (key, value) in &server.headers {
                expanded_headers.insert(key.clone(), Self::expand_env_vars(value));
            }
            server.headers = expanded_headers;

            let mut expanded_env = HashMap::new();
            for (key, value) in &server.env {
                expanded_env.insert(key.clone(), Self::expand_env_vars(value));
            }
            server.env = expanded_env;

            if let Some(auth) = &mut server.auth {
                auth.token = auth.token.as_ref().map(|t| Self::expand_env_vars(t));
                auth.username = auth.username.as_ref().map(|u| Self::expand_env_vars(u));
                auth.password = auth.password.as_ref().map(|p| Self::expand_env_vars(p));
                auth.api_key = auth.api_key.as_ref().map(|k| Self::expand_env_vars(k));
                auth.token_url = auth.token_url.as_ref().map(|t| Self::expand_env_vars(t));
                auth.client_id = auth.client_id.as_ref().map(|c| Self::expand_env_vars(c));
                auth.client_secret = auth
                    .client_secret
                    .as_ref()
                    .map(|c| Self::expand_env_vars(c));
                auth.scope = auth.scope.as_ref().map(|s| Self::expand_env_vars(s));
                auth.audience = auth.audience.as_ref().map(|a| Self::expand_env_vars(a));
            }

            if let Some(tls) = &mut server.tls {
                tls.ca_cert_path = tls.ca_cert_path.as_ref().map(|p| Self::expand_env_vars(p));
                tls.client_cert_path = tls
                    .client_cert_path
                    .as_ref()
                    .map(|p| Self::expand_env_vars(p));
                tls.client_key_path = tls
                    .client_key_path
                    .as_ref()
                    .map(|p| Self::expand_env_vars(p));
            }
        }

        Ok(())
    }

    /// Expand environment variables in a string
    pub fn expand_env_vars(input: &str) -> String {
        let mut result = String::new();
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '$' && chars.peek() == Some(&'{') {
                chars.next();
                let mut var_name = String::new();

                for ch in chars.by_ref() {
                    if ch == '}' {
                        break;
                    }
                    var_name.push(ch);
                }

                if let Ok(env_value) = env::var(&var_name) {
                    result.push_str(&env_value);
                } else {
                    result.push('$');
                    result.push('{');
                    result.push_str(&var_name);
                    result.push('}');
                }
            } else if ch == '$' && chars.peek().is_some_and(|c| c.is_alphabetic()) {
                let mut var_name = String::from("$");

                while let Some(&next) = chars.peek() {
                    if !next.is_alphanumeric() && next != '_' {
                        break;
                    }
                    var_name.push(next);
                    chars.next();
                }

                if let Ok(env_value) = env::var(&var_name[1..]) {
                    result.push_str(&env_value);
                } else {
                    result.push_str(&var_name);
                }
            } else {
                result.push(ch);
            }
        }

        result
    }

    /// Get configuration for a specific server
    pub fn get_server_config(&self, name: &str) -> Option<&ServerConfig> {
        self.servers
            .iter()
            .find(|server| server.name == name && server.enabled)
    }

    /// Get all enabled servers
    pub fn get_enabled_servers(&self) -> Vec<&ServerConfig> {
        self.servers
            .iter()
            .filter(|server| server.enabled)
            .collect()
    }

    /// Validate configuration
    pub fn validate(&self) -> AgentResult<()> {
        let mut errors = Vec::new();

        let mut server_names = std::collections::HashSet::new();
        for server in &self.servers {
            if server_names.contains(&server.name) {
                errors.push(format!("Duplicate server name: '{}'", server.name));
            } else {
                server_names.insert(server.name.clone());
            }

            let endpoint = server.endpoint.trim();
            match server.transport {
                TransportType::Stdio => {
                    let has_command = server
                        .command
                        .as_ref()
                        .map(|cmd| !cmd.trim().is_empty())
                        .unwrap_or(false);
                    let has_endpoint = !endpoint.is_empty();
                    if !has_command && !has_endpoint {
                        errors.push(format!(
                            "Server '{}' requires either command or endpoint for stdio transport",
                            server.name
                        ));
                    }
                }
                TransportType::StreamableHttp => {
                    if endpoint.is_empty() {
                        errors.push(format!(
                            "Server '{}' requires endpoint for streamable_http transport",
                            server.name
                        ));
                    } else if !endpoint.starts_with("http://") && !endpoint.starts_with("https://")
                    {
                        errors.push(format!(
                            "Server '{}' endpoint must start with http:// or https:// for streamable_http transport",
                            server.name
                        ));
                    }
                }
            }

            if let Some(ref auth) = server.auth {
                match auth.auth_type {
                    AuthType::Bearer => {
                        if auth.token.is_none() {
                            errors.push(format!(
                                "Server '{}' requires token for Bearer auth",
                                server.name
                            ));
                        }
                    }
                    AuthType::Basic => {
                        if auth.username.is_none() || auth.password.is_none() {
                            errors.push(format!(
                                "Server '{}' requires username/password for Basic auth",
                                server.name
                            ));
                        }
                    }
                    AuthType::ApiKey => {
                        if auth.api_key.is_none() {
                            errors.push(format!(
                                "Server '{}' requires api_key for ApiKey auth",
                                server.name
                            ));
                        }
                    }
                    AuthType::None => {
                        if auth.token.is_some() || auth.api_key.is_some() {
                            errors.push(format!(
                                "Server '{}' has authentication configured but auth type is None",
                                server.name
                            ));
                        }
                    }
                    AuthType::OAuth2 => {
                        let has_static_token = auth.token.is_some();
                        let has_flow = auth.token_url.is_some()
                            && auth.client_id.is_some()
                            && auth.client_secret.is_some();
                        if !has_static_token && !has_flow {
                            errors.push(format!(
                                "Server '{}' requires token or token_url/client_id/client_secret for OAuth2",
                                server.name
                            ));
                        }
                    }
                }
            }
        }

        if !errors.is_empty() {
            return Err(AgentError::Mcp(format!(
                "Configuration validation failed: {}",
                errors.join("; ")
            )));
        }

        Ok(())
    }

    /// Convert to McpManager configuration
    pub async fn into_manager(self) -> AgentResult<Arc<crate::mcp::McpManager>> {
        use crate::mcp::McpManager;

        let manager = McpManager::with_timeout_and_retries(
            self.general.default_timeout,
            self.general.max_retries,
        );

        for server_config in self.servers {
            if !server_config.enabled {
                continue;
            }

            let server_name = server_config.name.clone();
            match manager.add_server_with_config(server_config).await {
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("Failed to add server '{}': {}", server_name, e);
                }
            }
        }

        Ok(manager)
    }
}

pub fn unsupported_transport_message(value: &str) -> String {
    format!(
        "Unsupported transport '{}'. Supported: stdio, streamable_http. http/https -> streamable_http; tcp/ws/wss/sse are removed in strict official mode.",
        value
    )
}

fn default_timeout() -> Duration {
    Duration::from_secs(30)
}

fn default_enabled() -> bool {
    true
}

fn default_max_retries() -> usize {
    3
}

mod duration_seconds {
    use super::*;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_secs())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DurationSecondsVisitor;

        impl<'de> Visitor<'de> for DurationSecondsVisitor {
            type Value = Duration;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(
                    "a duration in seconds (integer/float), a numeric string, or a struct with secs field",
                )
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Duration::from_secs(value))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if value < 0 {
                    return Err(E::invalid_value(Unexpected::Signed(value), &self));
                }
                Ok(Duration::from_secs(value as u64))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if !value.is_finite() || value < 0.0 {
                    return Err(E::invalid_value(Unexpected::Float(value), &self));
                }
                Ok(Duration::from_secs_f64(value))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    return Err(E::invalid_value(Unexpected::Str(value), &self));
                }
                let secs = trimmed.parse::<f64>().map_err(|_| {
                    E::invalid_value(Unexpected::Str(value), &"a numeric string in seconds")
                })?;
                if !secs.is_finite() || secs < 0.0 {
                    return Err(E::invalid_value(Unexpected::Str(value), &self));
                }
                Ok(Duration::from_secs_f64(secs))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_str(&value)
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                let mut secs: Option<u64> = None;
                let mut nanos: u32 = 0;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "secs" => secs = Some(map.next_value::<u64>()?),
                        "nanos" => nanos = map.next_value::<u32>()?,
                        _ => {
                            let _: de::IgnoredAny = map.next_value()?;
                        }
                    }
                }

                match secs {
                    Some(secs) => Ok(Duration::new(secs, nanos)),
                    None => Err(de::Error::missing_field("secs")),
                }
            }
        }

        deserializer.deserialize_any(DurationSecondsVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_server(name: &str) -> ServerConfig {
        ServerConfig {
            name: name.to_string(),
            transport: TransportType::StreamableHttp,
            endpoint: "https://example.com/mcp".to_string(),
            command: None,
            args: Vec::new(),
            auth: None,
            headers: HashMap::new(),
            tls: None,
            timeout: Duration::from_secs(30),
            enabled: true,
            env: HashMap::new(),
        }
    }

    #[test]
    fn test_expand_env_vars() {
        unsafe {
            env::set_var("TEST_VAR", "test_value");
            env::set_var("ANOTHER_VAR", "another_value");
        }

        let result = McpConfig::expand_env_vars("prefix-${TEST_VAR}-suffix");
        assert_eq!(result, "prefix-test_value-suffix");

        let result = McpConfig::expand_env_vars("prefix-$TEST_VAR-suffix");
        assert_eq!(result, "prefix-test_value-suffix");

        let result = McpConfig::expand_env_vars("$TEST_VAR and ${ANOTHER_VAR}");
        assert_eq!(result, "test_value and another_value");
    }

    #[test]
    fn test_config_validation_duplicate_name() {
        let mut config = McpConfig::default();
        config.servers.push(make_server("test"));
        config.servers.push(make_server("test"));

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_stdio_requires_command_or_endpoint() {
        let mut config = McpConfig::default();
        config.servers.push(ServerConfig {
            name: "stdio-invalid".to_string(),
            transport: TransportType::Stdio,
            endpoint: String::new(),
            command: None,
            args: Vec::new(),
            auth: None,
            headers: HashMap::new(),
            tls: None,
            timeout: Duration::from_secs(10),
            enabled: true,
            env: HashMap::new(),
        });

        assert!(config.validate().is_err());

        config.servers[0].command = Some("npx".to_string());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_streamable_http_requires_http_endpoint() {
        let mut config = McpConfig::default();
        config.servers.push(ServerConfig {
            name: "http-invalid".to_string(),
            transport: TransportType::StreamableHttp,
            endpoint: "localhost:8080".to_string(),
            command: None,
            args: Vec::new(),
            auth: None,
            headers: HashMap::new(),
            tls: None,
            timeout: Duration::from_secs(10),
            enabled: true,
            env: HashMap::new(),
        });

        assert!(config.validate().is_err());

        config.servers[0].endpoint = "https://example.com/mcp".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_parse_transport_aliases() {
        let http: TransportType = serde_json::from_str("\"http\"").expect("http alias");
        let https: TransportType = serde_json::from_str("\"https\"").expect("https alias");
        let streamable: TransportType =
            serde_json::from_str("\"streamable_http\"").expect("streamable");

        assert_eq!(http, TransportType::StreamableHttp);
        assert_eq!(https, TransportType::StreamableHttp);
        assert_eq!(streamable, TransportType::StreamableHttp);
    }

    #[test]
    fn test_parse_unsupported_transport() {
        let err = serde_json::from_str::<TransportType>("\"tcp\"")
            .expect_err("tcp should be unsupported")
            .to_string();
        assert!(err.contains("Unsupported transport 'tcp'"));
    }
}

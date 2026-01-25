use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::error::{AgentError, AgentResult};
use crate::mcp::TransportConfig;

/// MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Unique server name
    pub name: String,

    /// Transport type
    #[serde(default)]
    pub transport: TransportType,

    /// Transport-specific configuration
    #[serde(flatten)]
    pub transport_config: TransportConfig,

    /// Authentication configuration
    #[serde(default)]
    pub auth: Option<AuthConfig>,

    /// Custom HTTP headers
    #[serde(default)]
    pub headers: HashMap<String, String>,

    /// Server-specific timeout
    #[serde(default = "default_timeout")]
    pub timeout: Duration,

    /// Whether this server is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Environment variables for this server
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// Transport type configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TransportType {
    /// Standard input/output communication (child process)
    Stdio,
    /// TCP socket connection
    Tcp,
    /// HTTP connection
    Http,
    /// HTTPS connection
    Https,
    /// WebSocket connection
    WebSocket,
    /// Secure WebSocket connection
    Wss,
    /// Server-sent events
    Sse,
}

impl Default for TransportType {
    fn default() -> Self {
        TransportType::Stdio
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

    /// Token in query parameter (for some APIs)
    pub query_param: Option<String>,
}

/// Authentication types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AuthType {
    /// No authentication
    None,
    /// Bearer token authentication
    Bearer,
    /// Basic username/password authentication
    Basic,
    /// API key authentication
    ApiKey,
    /// OAuth 2.1 (for future implementation)
    #[serde(rename = "oauth2")]
    OAuth2,
}

impl Default for AuthType {
    fn default() -> Self {
        AuthType::None
    }
}

/// Complete MCP configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(default = "default_timeout")]
    pub default_timeout: Duration,

    /// Maximum number of retries for failed operations
    #[serde(default = "default_max_retries")]
    pub max_retries: usize,

    /// Whether to enable logging
    #[serde(default = "default_enabled")]
    pub logging_enabled: bool,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            servers: Vec::new(),
        }
    }
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

        // Expand environment variables
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

        // Expand environment variables
        config.expand_environment_variables()?;

        Ok(config)
    }

    /// Load configuration from environment variables
    pub async fn from_env() -> AgentResult<McpConfig> {
        let mut config = McpConfig::default();

        // Check for MCP-specific environment variables
        if let Ok(json_config) = env::var("MCP_CONFIG_JSON") {
            config = serde_json::from_str(&json_config)
                .map_err(|e| AgentError::Mcp(format!("failed to parse MCP_CONFIG_JSON: {}", e)))?;
        }

        // Expand environment variables
        config.expand_environment_variables()?;

        Ok(config)
    }

    /// Try to load configuration from common locations
    pub async fn from_common_locations() -> AgentResult<McpConfig> {
        // Try Claude Desktop config location first
        let locations = vec![
            // System-wide config
            PathBuf::from("/etc/agent-lib/mcp.toml"),
            PathBuf::from("/etc/agent-lib/mcp.json"),
            // User config
            PathBuf::from(env::var("HOME").unwrap_or_else(|_| ".".to_string()))
                .join(".config/agent-lib/mcp.toml"),
            PathBuf::from(env::var("HOME").unwrap_or_else(|_| ".".to_string()))
                .join(".config/agent-lib/mcp.json"),
            // Current directory
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

        // Fall back to environment variables
        Self::from_env().await
    }
}

impl McpConfig {
    /// Expand environment variables in configuration
    pub fn expand_environment_variables(&mut self) -> AgentResult<()> {
        // Expand environment variables in server configurations
        for server in &mut self.servers {
            // Expand server name
            server.name = Self::expand_env_vars(&server.name);

            // Expand environment variables in headers
            let mut expanded_headers = HashMap::new();
            for (key, value) in &server.headers {
                expanded_headers.insert(key.clone(), Self::expand_env_vars(value));
            }
            server.headers = expanded_headers;

            // Expand environment variables in env map
            let mut expanded_env = HashMap::new();
            for (key, value) in &server.env {
                expanded_env.insert(key.clone(), Self::expand_env_vars(value));
            }
            server.env = expanded_env;

            // Expand authentication tokens
            if let Some(auth) = &mut server.auth {
                auth.token = auth.token.as_ref().map(|t| Self::expand_env_vars(t));
                auth.username = auth.username.as_ref().map(|u| Self::expand_env_vars(u));
                auth.password = auth.password.as_ref().map(|p| Self::expand_env_vars(p));
                auth.api_key = auth.api_key.as_ref().map(|k| Self::expand_env_vars(k));
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
                // Start of ${VAR} pattern
                chars.next(); // Skip '{'
                let mut var_name = String::new();

                while let Some(ch) = chars.next() {
                    if ch == '}' {
                        break;
                    }
                    var_name.push(ch);
                }

                if let Ok(env_value) = env::var(&var_name) {
                    result.push_str(&env_value);
                } else {
                    // If variable not found, keep the original pattern
                    result.push('$');
                    result.push('{');
                    result.push_str(&var_name);
                    result.push('}');
                }
            } else if ch == '$' && chars.peek().map_or(false, |c| c.is_alphabetic()) {
                // Start of $VAR pattern (no braces)
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
                    // If variable not found, keep the original name
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

        // Check for duplicate server names
        let mut server_names = std::collections::HashSet::new();
        for server in &self.servers {
            if server_names.contains(&server.name) {
                errors.push(format!("Duplicate server name: '{}'", server.name));
            } else {
                server_names.insert(server.name.clone());
            }

            // Validate authentication configuration
            if let Some(ref auth) = server.auth {
                match auth.auth_type {
                    AuthType::Bearer | AuthType::Basic | AuthType::ApiKey => {
                        if auth.token.is_none() && auth.api_key.is_none() {
                            errors.push(format!(
                                "Server '{}' requires token/api_key for {:?} auth",
                                server.name, auth.auth_type
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
                        errors.push(format!(
                            "Server '{}' has OAuth2 auth not yet supported",
                            server.name
                        ));
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

        let manager = McpManager::with_timeout(self.general.default_timeout);

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

// Default value functions
fn default_timeout() -> Duration {
    Duration::from_secs(30)
}

fn default_enabled() -> bool {
    true
}

fn default_max_retries() -> usize {
    3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_env_vars() {
        // Set test environment variables
        unsafe {
            env::set_var("TEST_VAR", "test_value");
            env::set_var("ANOTHER_VAR", "another_value");
        }

        // Test ${VAR} pattern
        let result = McpConfig::expand_env_vars("prefix-${TEST_VAR}-suffix");
        assert_eq!(result, "prefix-test_value-suffix");

        // Test $VAR pattern
        let result = McpConfig::expand_env_vars("prefix-$TEST_VAR-suffix");
        assert_eq!(result, "prefix-test_value-suffix");

        // Test multiple variables
        let result = McpConfig::expand_env_vars("url: $BASE_URL/api/${VERSION}/end");
        if let Ok(base_url) = env::var("BASE_URL") {
            if let Ok(version) = env::var("VERSION") {
                assert_eq!(result, format!("url: {}/api/{}/end", base_url, version));
            }
        }

        // Test non-existent variable
        let result = McpConfig::expand_env_vars("prefix-${NONEXISTENT}-suffix");
        assert_eq!(result, "prefix-${NONEXISTENT}-suffix");

        // Test mixed patterns
        let result = McpConfig::expand_env_vars("$TEST_VAR and ${ANOTHER_VAR}");
        assert_eq!(result, "test_value and another_value");
    }

    #[test]
    fn test_config_validation() {
        let mut config = McpConfig::default();

        // Valid configuration
        config.servers.push(ServerConfig {
            name: "test".to_string(),
            transport: TransportType::Http,
            transport_config: TransportConfig {
                endpoint: "http://example.com".to_string(),
            },
            auth: None,
            headers: HashMap::new(),
            timeout: Duration::from_secs(30),
            enabled: true,
            env: HashMap::new(),
        });

        assert!(config.validate().is_ok());

        // Duplicate server name should fail
        config.servers.push(ServerConfig {
            name: "test".to_string(),
            transport: TransportType::Http,
            transport_config: TransportConfig {
                endpoint: "http://example2.com".to_string(),
            },
            auth: None,
            headers: HashMap::new(),
            timeout: Duration::from_secs(30),
            enabled: true,
            env: HashMap::new(),
        });

        assert!(config.validate().is_err());
    }
}

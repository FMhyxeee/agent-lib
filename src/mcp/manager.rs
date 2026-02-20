use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use rmcp::model::Tool;
use tokio::sync::Mutex;

use crate::error::{AgentError, AgentResult};
use crate::mcp::McpClient;
use crate::mcp::config::{
    ConfigLoader, McpConfig, ServerConfig, TransportType, unsupported_transport_message,
};

type ServerEntry = (Arc<McpClient>, Vec<Tool>);
type ServerMap = HashMap<String, ServerEntry>;

/// Manages multiple MCP server connections
/// Inspired by Codex's rmcp-client state management pattern
#[derive(Debug)]
pub struct McpManager {
    /// Map of server name to (client, tools)
    servers: Mutex<ServerMap>,
    /// Default timeout for MCP operations
    default_timeout: Option<Duration>,
    /// Default max retries for MCP operations
    max_retries: usize,
}

impl McpManager {
    /// Create a new MCP manager with default timeout (30 seconds)
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            servers: Mutex::new(HashMap::new()),
            default_timeout: Some(Duration::from_secs(30)),
            max_retries: 3,
        })
    }

    /// Create a new MCP manager with custom timeout
    pub fn with_timeout(timeout: Duration) -> Arc<Self> {
        Arc::new(Self {
            servers: Mutex::new(HashMap::new()),
            default_timeout: Some(timeout),
            max_retries: 3,
        })
    }

    /// Create a new MCP manager with custom timeout and retries
    pub fn with_timeout_and_retries(timeout: Duration, max_retries: usize) -> Arc<Self> {
        Arc::new(Self {
            servers: Mutex::new(HashMap::new()),
            default_timeout: Some(timeout),
            max_retries,
        })
    }

    /// Add an MCP server and discover its tools
    pub async fn add_server(
        &self,
        name: impl Into<String>,
        endpoint: impl Into<String>,
    ) -> AgentResult<Vec<Tool>> {
        let name = name.into();
        let endpoint = endpoint.into();
        let timeout = self.default_timeout.unwrap_or(Duration::from_secs(30));
        let config = Self::server_config_from_endpoint(name, endpoint, timeout)?;
        self.add_server_with_config(config).await
    }

    /// Add an MCP server with custom timeout
    pub async fn add_server_with_timeout(
        &self,
        name: impl Into<String>,
        endpoint: impl Into<String>,
        timeout: Duration,
    ) -> AgentResult<Vec<Tool>> {
        let name = name.into();
        let endpoint = endpoint.into();
        let config = Self::server_config_from_endpoint(name, endpoint, timeout)?;
        self.add_server_with_config(config).await
    }

    /// Get all tools from all registered servers
    pub async fn get_all_tools(&self) -> Vec<(String, Tool, Arc<McpClient>)> {
        let servers = self.servers.lock().await;
        let mut result = Vec::new();

        for (server_name, (client, tools)) in servers.iter() {
            for tool in tools {
                result.push((server_name.clone(), tool.clone(), Arc::clone(client)));
            }
        }

        result
    }

    /// Get client for a specific server
    pub async fn get_client(&self, name: &str) -> Option<Arc<McpClient>> {
        let servers = self.servers.lock().await;
        servers.get(name).map(|(client, _)| Arc::clone(client))
    }

    /// Get tools for a specific server
    pub async fn get_server_tools(&self, name: &str) -> Option<Vec<Tool>> {
        let servers = self.servers.lock().await;
        servers.get(name).map(|(_, tools)| tools.clone())
    }

    /// List all registered server names
    pub async fn list_servers(&self) -> Vec<String> {
        let servers = self.servers.lock().await;
        servers.keys().cloned().collect()
    }

    /// Remove a server by name
    pub async fn remove_server(&self, name: &str) -> Option<(Arc<McpClient>, Vec<Tool>)> {
        let mut servers = self.servers.lock().await;
        servers.remove(name)
    }

    /// Check if a server is registered
    pub async fn has_server(&self, name: &str) -> bool {
        let servers = self.servers.lock().await;
        servers.contains_key(name)
    }

    /// Get the total number of registered servers
    pub async fn server_count(&self) -> usize {
        let servers = self.servers.lock().await;
        servers.len()
    }

    /// Get the total number of tools across all servers
    pub async fn total_tools_count(&self) -> usize {
        let servers = self.servers.lock().await;
        servers.values().map(|(_, tools)| tools.len()).sum()
    }

    /// Get the default timeout used for MCP operations
    pub fn default_timeout(&self) -> Option<Duration> {
        self.default_timeout
    }

    /// Load configuration from a TOML file
    pub async fn from_config_file(path: &str) -> AgentResult<Arc<Self>> {
        let config = ConfigLoader::from_toml_file(path).await?;
        Self::from_config(config).await
    }

    /// Load configuration from a JSON file
    pub async fn from_config_json(path: &str) -> AgentResult<Arc<Self>> {
        let config = ConfigLoader::from_json_file(path).await?;
        Self::from_config(config).await
    }

    /// Load configuration from environment variables
    pub async fn from_env() -> AgentResult<Arc<Self>> {
        let config = ConfigLoader::from_env().await?;
        Self::from_config(config).await
    }

    /// Load configuration from common locations
    pub async fn from_common_locations() -> AgentResult<Arc<Self>> {
        let config = ConfigLoader::from_common_locations().await?;
        Self::from_config(config).await
    }

    /// Load from structured configuration
    pub async fn from_config(config: McpConfig) -> AgentResult<Arc<Self>> {
        // Validate configuration
        config.validate()?;

        // Create manager with default timeout from config
        let manager = Self::with_timeout_and_retries(
            config.general.default_timeout,
            config.general.max_retries,
        );

        // Add servers from configuration
        for server_config in config.servers {
            if server_config.enabled {
                let server_name = server_config.name.clone();
                match manager.add_server_with_config(server_config).await {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load server '{}' from config: {}",
                            server_name,
                            e
                        );
                    }
                }
            }
        }

        tracing::info!(
            "Loaded {} MCP servers from configuration",
            manager.server_count().await
        );

        Ok(manager)
    }

    /// Add server with full configuration
    pub async fn add_server_with_config(&self, config: ServerConfig) -> AgentResult<Vec<Tool>> {
        Self::validate_server_config(&config)?;

        let server_name = config.name.clone();
        let timeout = config.timeout;
        let max_attempts = self.max_retries.saturating_add(1).max(1);

        let client = Arc::new(McpClient::connect(config).await?);

        let mut last_error: Option<AgentError> = None;
        for attempt in 1..=max_attempts {
            match tokio::time::timeout(timeout, client.list_tools()).await {
                Ok(Ok(tools)) => {
                    let mut servers = self.servers.lock().await;
                    servers.insert(server_name.clone(), (Arc::clone(&client), tools.clone()));
                    tracing::info!(
                        "Added MCP server '{}' with {} tools from configuration",
                        server_name,
                        tools.len()
                    );
                    return Ok(tools);
                }
                Ok(Err(err)) => {
                    last_error = Some(err);
                }
                Err(_) => {
                    last_error = Some(AgentError::Mcp(format!(
                        "server '{}' timed out during tool discovery",
                        server_name
                    )));
                }
            }

            if attempt < max_attempts {
                tracing::warn!(
                    "Retrying tool discovery for MCP server '{}' ({}/{})",
                    server_name,
                    attempt + 1,
                    max_attempts
                );
            }
        }

        Err(last_error.unwrap_or_else(|| {
            AgentError::Mcp(format!(
                "failed to discover tools for MCP server '{}'",
                server_name
            ))
        }))
    }

    /// Reload configuration from file
    pub async fn reload_from_file(&self, path: &str) -> AgentResult<()> {
        let config = ConfigLoader::from_toml_file(path).await?;
        let new_manager = Self::from_config(config).await?;

        // Transfer servers from new manager to current one
        let new_servers = new_manager.get_all_servers().await;
        let mut servers = self.servers.lock().await;

        // Clear existing servers
        servers.clear();

        // Add new servers
        for (name, client, tools) in new_servers {
            servers.insert(name, (client, tools));
        }

        tracing::info!("Reloaded MCP configuration from: {}", path);
        Ok(())
    }

    /// Get all server information (name, client, tools)
    pub async fn get_all_servers(&self) -> Vec<(String, Arc<McpClient>, Vec<Tool>)> {
        let servers = self.servers.lock().await;
        servers
            .iter()
            .map(|(name, (client, tools))| (name.clone(), Arc::clone(client), tools.clone()))
            .collect()
    }

    /// Get configuration for a specific server
    pub async fn get_server_info(&self, name: &str) -> Option<(Arc<McpClient>, Vec<Tool>)> {
        let servers = self.servers.lock().await;
        servers
            .get(name)
            .map(|(client, tools)| (Arc::clone(client), tools.clone()))
    }

    fn validate_server_config(config: &ServerConfig) -> AgentResult<()> {
        let validation = McpConfig {
            general: Default::default(),
            servers: vec![config.clone()],
        };
        validation.validate()
    }

    fn server_config_from_endpoint(
        name: String,
        endpoint: String,
        timeout: Duration,
    ) -> AgentResult<ServerConfig> {
        let trimmed = endpoint.trim();
        if trimmed.is_empty() {
            return Err(AgentError::Mcp("endpoint cannot be empty".to_string()));
        }

        let transport = if trimmed.starts_with("stdio://") {
            TransportType::Stdio
        } else if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            TransportType::StreamableHttp
        } else if let Some((scheme, _)) = trimmed.split_once("://") {
            let normalized = scheme.to_ascii_lowercase();
            if matches!(
                normalized.as_str(),
                "tcp" | "websocket" | "ws" | "wss" | "sse"
            ) {
                return Err(AgentError::Mcp(unsupported_transport_message(scheme)));
            }
            return Err(AgentError::Mcp(format!(
                "Unsupported endpoint scheme '{}'. Supported endpoint prefixes: stdio://, http://, https://.",
                scheme
            )));
        } else {
            return Err(AgentError::Mcp(format!(
                "Unsupported endpoint '{}'. Supported endpoint prefixes: stdio://, http://, https://.",
                trimmed
            )));
        };

        Ok(ServerConfig {
            name,
            transport,
            endpoint: trimmed.to_string(),
            command: None,
            args: Vec::new(),
            auth: None,
            headers: HashMap::new(),
            tls: None,
            timeout,
            enabled: true,
            env: HashMap::new(),
        })
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self {
            servers: Mutex::new(HashMap::new()),
            default_timeout: Some(Duration::from_secs(30)),
            max_retries: 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_manager_creation() {
        let manager = McpManager::new();
        assert_eq!(manager.default_timeout, Some(Duration::from_secs(30)));

        let manager_with_timeout = McpManager::with_timeout(Duration::from_secs(60));
        assert_eq!(
            manager_with_timeout.default_timeout,
            Some(Duration::from_secs(60))
        );
    }

    #[tokio::test]
    async fn test_mcp_manager_concurrent_access() {
        let manager = McpManager::new();

        // Test concurrent access to server methods
        let handles = (0..5)
            .map(|_| {
                let manager_ref = Arc::downgrade(&manager);
                tokio::spawn(async move {
                    if let Some(manager) = manager_ref.upgrade() {
                        // These operations should not cause deadlocks
                        let _servers = manager.list_servers().await;
                        let _count = manager.server_count().await;
                        let _tools_count = manager.total_tools_count().await;
                    }
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_add_server_rejects_legacy_transport_endpoint() {
        let manager = McpManager::new();
        let err = manager
            .add_server("legacy", "tcp://localhost:9000")
            .await
            .expect_err("legacy transport must be rejected");

        let message = err.to_string();
        assert!(message.contains("Unsupported transport 'tcp'"));
        assert!(message.contains("Supported: stdio, streamable_http"));
    }

    #[tokio::test]
    async fn test_add_server_rejects_unknown_scheme() {
        let manager = McpManager::new();
        let err = manager
            .add_server("unknown", "ftp://localhost/resource")
            .await
            .expect_err("unknown scheme must be rejected");

        assert!(err.to_string().contains("Supported endpoint prefixes"));
    }
}

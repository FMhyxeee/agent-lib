use std::sync::Arc;
use std::time::Duration;

use crate::agent::AgentBuilder;
use crate::error::{AgentError, AgentResult};
use crate::mcp::{McpClient, McpManager, ServerConfig, TransportType};
use crate::tools::builtin::McpToolAdapter;

fn unsupported_transport_error(value: &str) -> AgentError {
    AgentError::Mcp(format!(
        "Unsupported transport '{}'. Supported: stdio, streamable_http. http/https -> streamable_http; tcp/ws/wss/sse are removed in strict official mode.",
        value
    ))
}

fn server_config_from_endpoint(name: String, endpoint: String) -> AgentResult<ServerConfig> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Err(AgentError::Mcp("endpoint cannot be empty".to_string()));
    }

    let transport = if endpoint.starts_with("stdio://") {
        TransportType::Stdio
    } else if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        TransportType::StreamableHttp
    } else if let Some((scheme, _)) = endpoint.split_once("://") {
        let normalized = scheme.to_ascii_lowercase();
        if matches!(
            normalized.as_str(),
            "tcp" | "websocket" | "ws" | "wss" | "sse"
        ) {
            return Err(unsupported_transport_error(scheme));
        }
        return Err(AgentError::Mcp(format!(
            "Unsupported endpoint scheme '{}'. Supported endpoint prefixes: stdio://, http://, https://.",
            scheme
        )));
    } else {
        return Err(AgentError::Mcp(format!(
            "Unsupported endpoint '{}'. Supported endpoint prefixes: stdio://, http://, https://.",
            endpoint
        )));
    };

    Ok(ServerConfig {
        name,
        transport,
        endpoint: endpoint.to_string(),
        command: None,
        args: Vec::new(),
        auth: None,
        headers: Default::default(),
        tls: None,
        timeout: Duration::from_secs(30),
        enabled: true,
        env: Default::default(),
    })
}

impl AgentBuilder {
    /// Connects to an MCP server and registers all its tools.
    pub async fn with_mcp_server(mut self, endpoint: impl Into<String>) -> Self {
        let endpoint_str = endpoint.into();
        let config = match server_config_from_endpoint("mcp-inline".to_string(), endpoint_str.clone()) {
            Ok(config) => config,
            Err(err) => {
                tracing::warn!(
                    "Failed to parse MCP endpoint '{}': {}",
                    endpoint_str,
                    err
                );
                return self;
            }
        };

        let client = match McpClient::connect(config).await {
            Ok(client) => Arc::new(client),
            Err(err) => {
                tracing::warn!(
                    "Failed to connect to MCP server '{}': {}",
                    endpoint_str,
                    err
                );
                return self;
            }
        };

        let tools = match client.list_tools().await {
            Ok(t) => t,
            Err(err) => {
                tracing::warn!(
                    "Failed to list tools from MCP server {}: {}",
                    endpoint_str,
                    err
                );
                return self;
            }
        };

        let tools_count = tools.len();
        for tool_def in tools {
            let adapter = McpToolAdapter::new(tool_def, Arc::clone(&client));
            self.registry.register(Arc::new(adapter));
        }

        tracing::info!(
            "Registered {} tools from MCP server: {}",
            tools_count,
            endpoint_str
        );

        self
    }

    /// Registers tools from a pre-configured MCP client.
    pub async fn with_mcp_client(mut self, client: Arc<McpClient>) -> AgentResult<Self> {
        let tools = client.list_tools().await?;
        let tools_count = tools.len();

        for tool_def in tools {
            let adapter = McpToolAdapter::new(tool_def, Arc::clone(&client));
            self.registry.register(Arc::new(adapter));
        }

        tracing::info!("Registered {} tools from MCP client", tools_count);
        Ok(self)
    }

    /// Uses an McpManager to register tools from multiple servers.
    pub async fn with_mcp_manager(mut self, manager: Arc<McpManager>) -> AgentResult<Self> {
        let all_tools = manager.get_all_tools().await;

        if all_tools.is_empty() {
            tracing::warn!("No MCP tools found in manager");
            return Ok(self);
        }

        let tools_count = all_tools.len();
        for (server_name, tool_def, client) in all_tools {
            let mut adapter = McpToolAdapter::new(tool_def, client);
            adapter.definition.name = format!("{}:{}", server_name, adapter.definition.name).into();
            self.registry.register(Arc::new(adapter));
        }

        tracing::info!(
            "Registered {} MCP tools from {} servers",
            tools_count,
            manager.server_count().await
        );

        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::McpManager;

    #[tokio::test]
    async fn test_with_mcp_server_connection_failure() {
        let builder = AgentBuilder::new();
        let builder = builder
            .with_mcp_server("invalid://nonexistent-server")
            .await;

        assert!(builder.registry.list().is_empty());
    }

    #[tokio::test]
    #[ignore = "requires MCP server running"]
    async fn test_with_mcp_client() {
        let config = ServerConfig {
            name: "echo".to_string(),
            transport: TransportType::Stdio,
            endpoint: "stdio://echo-server".to_string(),
            command: None,
            args: Vec::new(),
            auth: None,
            headers: Default::default(),
            tls: None,
            timeout: Duration::from_secs(30),
            enabled: true,
            env: Default::default(),
        };

        let client = Arc::new(McpClient::connect(config).await.unwrap());
        let builder = AgentBuilder::new();
        let result = builder.with_mcp_client(client).await;

        if let Ok(builder) = result {
            assert!(!builder.registry.list().is_empty());
        }
    }

    #[tokio::test]
    async fn test_with_mcp_manager() {
        let manager = McpManager::new();
        let builder = AgentBuilder::new();
        let result = builder.with_mcp_manager(manager).await;
        assert!(result.is_ok());
    }
}

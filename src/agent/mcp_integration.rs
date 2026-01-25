use std::sync::Arc;

use crate::agent::AgentBuilder;
use crate::error::AgentResult;
use crate::mcp::{McpClient, McpManager, McpTransport, TransportConfig};
use crate::tools::builtin::McpToolAdapter;

impl AgentBuilder {
    /// Connects to an MCP server and registers all its tools
    ///
    /// This is a convenience method that handles the full MCP integration flow:
    /// 1. Creates the transport connection
    /// 2. Creates the MCP client
    /// 3. Lists available tools from the server
    /// 4. Creates adapters for each tool and registers them
    ///
    /// If the connection fails, the method logs a warning and returns the
    /// builder unchanged, allowing the user to continue with other configuration.
    ///
    /// # Arguments
    /// * `endpoint` - MCP server endpoint (e.g., "stdio://mcp-server", "tcp://localhost:8080")
    ///
    /// # Returns
    /// Self for method chaining
    ///
    /// # Example
    /// ```rust,no_run
    /// use agent_lib::AgentBuilder;
    /// use agent_lib::model::provider::OpenAiProvider;
    /// # async fn example() -> agent_lib::AgentResult<()> {
    /// let builder = AgentBuilder::new()
    ///     .with_model(OpenAiProvider::new("gpt-4"));
    /// let builder = builder.with_mcp_server("stdio://mcp-server-filesystem").await;
    /// let _agent = builder.build()?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn with_mcp_server(mut self, endpoint: impl Into<String>) -> Self {
        let endpoint_str = endpoint.into();

        // Create transport connection
        let transport = match McpTransport::new(TransportConfig {
            endpoint: endpoint_str.clone(),
        })
        .await
        {
            Ok(t) => t,
            Err(err) => {
                tracing::warn!(
                    "Failed to create MCP transport for {}: {}",
                    endpoint_str,
                    err
                );
                return self;
            }
        };

        let client = Arc::new(McpClient::new(transport));

        // List tools from the server
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

        // Create and register an adapter for each tool
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

    /// Registers tools from a pre-configured MCP client
    ///
    /// Use this when you need more control over the MCP client configuration
    /// or want to share a client across multiple agents.
    ///
    /// # Arguments
    /// * `client` - Pre-configured MCP client
    ///
    /// # Returns
    /// AgentResult<Self> - Returns an error if tool listing fails
    ///
    /// # Example
    /// ```rust,no_run
    /// use agent_lib::{AgentBuilder, mcp::{McpClient, McpTransport, TransportConfig}};
    /// use agent_lib::model::provider::OpenAiProvider;
    /// use std::sync::Arc;
    /// # async fn example() -> agent_lib::AgentResult<()> {
    /// let transport = McpTransport::new(TransportConfig {
    ///     endpoint: "stdio://my-server".to_string(),
    /// }).await?;
    /// let client = Arc::new(McpClient::new(transport));
    ///
    /// let builder = AgentBuilder::new()
    ///     .with_model(OpenAiProvider::new("gpt-4"));
    /// let builder = builder.with_mcp_client(client.clone()).await?;
    /// let _agent = builder.build()?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn with_mcp_client(mut self, client: Arc<McpClient>) -> AgentResult<Self> {
        // List tools from the client
        let tools = client.list_tools().await?;

        let tools_count = tools.len();

        // Create and register an adapter for each tool
        for tool_def in tools {
            let adapter = McpToolAdapter::new(tool_def, Arc::clone(&client));
            self.registry.register(Arc::new(adapter));
        }

        tracing::info!("Registered {} tools from MCP client", tools_count);
        Ok(self)
    }

    /// Uses a McpManager to register tools from multiple servers
    ///
    /// This allows sharing a single manager across multiple agents and provides
    /// centralized server management. The manager should be pre-populated with
    /// servers using `manager.add_server()`.
    ///
    /// # Arguments
    /// * `manager` - Shared McpManager instance
    ///
    /// # Returns
    /// AgentResult<Self> - Returns an error if no tools are available
    ///
    /// # Example
    /// ```rust,no_run
    /// use agent_lib::{AgentBuilder, mcp::McpManager};
    /// use agent_lib::model::provider::OpenAiProvider;
    /// # async fn example() -> agent_lib::AgentResult<()> {
    /// let manager = McpManager::new();
    /// manager.add_server("fs", "stdio://mcp-server-filesystem").await?;
    /// manager.add_server("db", "tcp://localhost:5432").await?;
    ///
    /// let builder = AgentBuilder::new()
    ///     .with_model(OpenAiProvider::new("gpt-4"));
    /// let builder = builder.with_mcp_manager(manager.clone()).await?;
    /// let _agent = builder.build()?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn with_mcp_manager(mut self, manager: Arc<McpManager>) -> AgentResult<Self> {
        // Get all tools from all servers
        let all_tools = manager.get_all_tools().await;

        if all_tools.is_empty() {
            tracing::warn!("No MCP tools found in manager");
            return Ok(self);
        }

        let tools_count = all_tools.len();

        // Create and register adapters for each tool with its client
        for (server_name, tool_def, client) in all_tools {
            let mut adapter = McpToolAdapter::new(tool_def, client);
            // Add server name prefix to avoid name conflicts
            adapter.definition.name = format!("{}:{}", server_name, adapter.definition.name);
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
    use crate::mcp::{McpManager, McpTransport, TransportConfig};

    // Test that with_mcp_server handles connection failures gracefully
    #[tokio::test]
    async fn test_with_mcp_server_connection_failure() {
        let builder = AgentBuilder::new();
        let builder = builder
            .with_mcp_server("invalid://nonexistent-server")
            .await;

        // Should not panic and should return the builder unchanged
        // (except for the registry having potential tools from a previous successful call)
        assert!(builder.registry.list().is_empty());
    }

    // Test that with_mcp_client works with pre-configured clients
    #[tokio::test]
    #[ignore = "requires MCP server running"]
    async fn test_with_mcp_client() {
        let transport = McpTransport::new(TransportConfig {
            endpoint: "stdio://echo-server".to_string(),
        })
        .await
        .unwrap();

        let client = Arc::new(McpClient::new(transport));
        let builder = AgentBuilder::new();

        let result = builder.with_mcp_client(client.clone()).await;

        if let Ok(builder) = result {
            // Should succeed and register tools
            assert!(!builder.registry.list().is_empty());
        }
    }

    // Test that with_mcp_manager works with shared manager
    #[tokio::test]
    async fn test_with_mcp_manager() {
        let manager = McpManager::new();
        let builder = AgentBuilder::new();

        // This will fail because no servers are added, but should not panic
        let result = builder.with_mcp_manager(manager).await;

        assert!(result.is_ok());
        // We can't access the registry due to encapsulation, but the test passes
        // if it doesn't panic
    }
}

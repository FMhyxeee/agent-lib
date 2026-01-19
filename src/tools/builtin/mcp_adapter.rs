use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

use crate::error::{AgentError, AgentResult};
use crate::mcp::{McpClient, McpTool, McpToolCall};
use crate::tools::{Tool, ToolContext, ToolDef, ToolResult};

/// Adapter that wraps an MCP tool to implement the Tool trait
///
/// This allows MCP tools to be registered and used alongside builtin tools
/// in the agent's tool registry and executor.
///
/// Design inspired by Codex's tool runner pattern but simplified for agent-lib
#[derive(Debug, Clone)]
pub struct McpToolAdapter {
    /// The MCP tool definition (name, description, schema)
    pub definition: McpTool,

    /// Shared reference to the MCP client for executing tool calls
    /// Using Arc allows multiple adapters to share the same client connection
    /// This matches the pattern used in Codex for sharing clients across tools
    client: Arc<McpClient>,
}

impl McpToolAdapter {
    /// Creates a new MCP tool adapter
    ///
    /// # Arguments
    /// * `definition` - The MCP tool definition from list_tools()
    /// * `client` - Shared MCP client for executing this tool
    ///
    /// # Example
    /// ```rust,no_run
    /// use std::sync::Arc;
    /// use agent_lib::mcp::{McpTool, McpClient, McpTransport, TransportConfig};
    /// use agent_lib::tools::builtin::McpToolAdapter;
    ///
    /// # async fn example() -> agent_lib::AgentResult<()> {
    /// let tool = McpTool {
    ///     name: "test".to_string(),
    ///     description: "Test tool".to_string(),
    ///     schema: serde_json::json!({}),
    /// };
    /// let transport = McpTransport::new(TransportConfig {
    ///     endpoint: "stdio://mcp-server".to_string(),
    /// }).await?;
    /// let client = Arc::new(McpClient::new(transport));
    /// let _adapter = McpToolAdapter::new(tool, client);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(definition: McpTool, client: Arc<McpClient>) -> Self {
        Self {
            definition,
            client,
        }
    }

    /// Gets the tool name
    ///
    /// # Returns
    /// A string slice containing the tool name
    pub fn name(&self) -> &str {
        &self.definition.name
    }

    /// Gets the tool description
    ///
    /// # Returns
    /// A string slice containing the tool description
    pub fn description(&self) -> &str {
        &self.definition.description
    }

    /// Gets a reference to the underlying MCP tool definition
    pub fn definition_ref(&self) -> &McpTool {
        &self.definition
    }

    /// Gets a reference to the MCP client
    pub fn client(&self) -> &Arc<McpClient> {
        &self.client
    }
}

#[async_trait]
impl Tool for McpToolAdapter {
    /// Returns the tool definition for the MCP tool
    ///
    /// This converts the MCP tool definition to the agent-lib ToolDef format.
    /// The schema is already compatible between MCP and agent-lib.
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: self.definition.name.clone(),
            description: self.definition.description.clone(),
            schema: self.definition.schema.clone(),
        }
    }

    /// Executes the MCP tool call
    ///
    /// This method handles the actual MCP tool execution, wrapping the result
    /// in an agent-lib ToolResult. It follows Codex's pattern of:
    /// 1. Creating the tool call
    /// 2. Executing via MCP client
    /// 3. Wrapping the result
    ///
    /// # Arguments
    /// * `args` - JSON arguments for the tool
    /// * `_ctx` - Tool context (ignored for MCP tools)
    ///
    /// # Returns
    /// The tool result wrapped in an AgentResult
    ///
    /// # Note
    /// The ToolContext is ignored because MCP tools execute on the server,
    /// which is responsible for its own context handling (sandboxing, etc.)
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> AgentResult<ToolResult> {
        let tool_call = McpToolCall {
            name: self.definition.name.clone(),
            args,
        };

        // Execute the tool call with optional timeout
        // Using the enhanced client with timeout support (Codex pattern)
        let result = self
            .client
            .call_tool(tool_call)
            .await
            .map_err(|err| AgentError::Tool(format!("MCP tool call failed: {}", err)))?;

        // Wrap the raw MCP result in our ToolResult format
        Ok(ToolResult { output: result })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{McpClient, McpTransport, TransportConfig};

    #[tokio::test]
    #[ignore = "requires MCP server running"]
    async fn test_mcp_tool_adapter_definition() {
        let transport = McpTransport::new(TransportConfig {
            endpoint: "stdio://echo-server".to_string(),
        }).await.unwrap();

        let client = Arc::new(McpClient::new(transport));
        let tools = client.list_tools().await.unwrap();

        if !tools.is_empty() {
            let adapter = McpToolAdapter::new(tools[0].clone(), client);
            let def = adapter.definition();

            assert_eq!(def.name, tools[0].name);
            assert_eq!(def.description, tools[0].description);
            assert_eq!(def.schema, tools[0].schema);
        }
    }

    #[tokio::test]
    async fn test_mcp_tool_adapter_creation() {
        let tool = McpTool {
            name: "test-tool".to_string(),
            description: "A test tool".to_string(),
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "param1": { "type": "string" }
                },
                "required": ["param1"]
            }),
        };

        // Create mock client (we won't actually call it in this test)
        let transport = match McpTransport::new(TransportConfig {
            endpoint: "stdio://echo-server".to_string(),
        }).await {
            Ok(transport) => transport,
            Err(err) => {
                eprintln!("Skipping test: transport unavailable: {}", err);
                return;
            }
        };

        let client = Arc::new(McpClient::new(transport));

        let adapter = McpToolAdapter::new(tool.clone(), client);

        assert_eq!(adapter.name(), "test-tool");
        assert_eq!(adapter.description(), "A test tool");
        assert_eq!(adapter.definition_ref().name, "test-tool");
        // Check that client exists but don't access specific properties
        let _client = adapter.client();
    }
}
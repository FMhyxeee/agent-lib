use async_trait::async_trait;
use rmcp::model::{CallToolRequestParams, Tool};
use serde_json::Value;
use std::sync::Arc;

use crate::error::{AgentError, AgentResult};
use crate::mcp::McpClient;
use crate::tools::{Tool as AgentTool, ToolContext, ToolDef, ToolResult};

/// Adapter that wraps an MCP tool to implement the agent Tool trait.
#[derive(Debug, Clone)]
pub struct McpToolAdapter {
    /// MCP tool definition from rmcp.
    pub definition: Tool,
    client: Arc<McpClient>,
}

impl McpToolAdapter {
    pub fn new(definition: Tool, client: Arc<McpClient>) -> Self {
        Self { definition, client }
    }

    pub fn name(&self) -> &str {
        self.definition.name.as_ref()
    }

    pub fn description(&self) -> &str {
        self.definition.description.as_deref().unwrap_or_default()
    }

    pub fn definition_ref(&self) -> &Tool {
        &self.definition
    }

    pub fn client(&self) -> &Arc<McpClient> {
        &self.client
    }
}

#[async_trait]
impl AgentTool for McpToolAdapter {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: self.definition.name.to_string(),
            description: self
                .definition
                .description
                .as_deref()
                .unwrap_or_default()
                .to_string(),
            schema: Value::Object((*self.definition.input_schema).clone()),
        }
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> AgentResult<ToolResult> {
        let arguments = match args {
            Value::Object(arguments) => arguments,
            _ => {
                return Err(AgentError::Tool(
                    "MCP tool arguments must be a JSON object".to_string(),
                ));
            }
        };

        let request = CallToolRequestParams {
            meta: None,
            name: self.definition.name.clone(),
            arguments: Some(arguments),
            task: None,
        };

        let result = self
            .client
            .call_tool(request)
            .await
            .map_err(|err| AgentError::Tool(format!("MCP tool call failed: {}", err)))?;

        let output = serde_json::to_value(result).map_err(|err| {
            AgentError::Tool(format!("failed to serialize MCP tool result: {}", err))
        })?;

        Ok(ToolResult { output })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{McpClient, ServerConfig, TransportType};
    use std::time::Duration;

    #[tokio::test]
    #[ignore = "requires MCP server running"]
    async fn test_mcp_tool_adapter_definition() {
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
        let tools = client.list_tools().await.unwrap();

        if !tools.is_empty() {
            let adapter = McpToolAdapter::new(tools[0].clone(), client);
            let def = adapter.definition();
            assert_eq!(def.name, tools[0].name);
            assert_eq!(
                def.description,
                tools[0]
                    .description
                    .as_deref()
                    .unwrap_or_default()
                    .to_string()
            );
            assert_eq!(def.schema, Value::Object((*tools[0].input_schema).clone()));
        }
    }

    #[tokio::test]
    async fn test_mcp_tool_adapter_creation() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "param1": { "type": "string" }
            },
            "required": ["param1"]
        });
        let tool = Tool::new(
            "test-tool",
            "A test tool",
            schema.as_object().cloned().unwrap_or_default(),
        );

        let config = ServerConfig {
            name: "echo".to_string(),
            transport: TransportType::Stdio,
            endpoint: "stdio://echo-server".to_string(),
            command: None,
            args: Vec::new(),
            auth: None,
            headers: Default::default(),
            tls: None,
            timeout: Duration::from_secs(2),
            enabled: true,
            env: Default::default(),
        };

        let client = match McpClient::connect(config).await {
            Ok(client) => Arc::new(client),
            Err(err) => {
                eprintln!("Skipping test: client unavailable: {}", err);
                return;
            }
        };

        let adapter = McpToolAdapter::new(tool.clone(), client);
        assert_eq!(adapter.name(), "test-tool");
        assert_eq!(adapter.description(), "A test tool");
        assert_eq!(adapter.definition_ref().name, "test-tool");
        let _client = adapter.client();
    }
}

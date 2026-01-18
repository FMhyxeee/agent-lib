use serde_json::json;
use serde_json::Value;

use crate::error::{AgentError, AgentResult};
use crate::mcp::{McpRequest, McpResponse, McpTool, McpToolCall, McpTransport};

#[derive(Debug)]
pub struct McpClient {
    transport: McpTransport,
}

impl McpClient {
    pub fn new(transport: McpTransport) -> Self {
        Self { transport }
    }

    pub async fn list_tools(&self) -> AgentResult<Vec<McpTool>> {
        let response = self.send(McpRequest {
            id: "tools.list".to_string(),
            method: "tools/list".to_string(),
            params: json!({}),
        })
        .await?;

        let tools: Vec<McpTool> = serde_json::from_value(response.result)
            .map_err(|err| AgentError::Mcp(format!("invalid tools list: {err}")))?;
        Ok(tools)
    }

    pub async fn call_tool(&self, call: McpToolCall) -> AgentResult<Value> {
        let response = self.send(McpRequest {
            id: "tools.call".to_string(),
            method: "tools/call".to_string(),
            params: json!({
                "name": call.name,
                "args": call.args,
            }),
        })
        .await?;

        Ok(response.result)
    }

    async fn send(&self, _request: McpRequest) -> AgentResult<McpResponse> {
        self.transport.send(_request).await
    }
}
